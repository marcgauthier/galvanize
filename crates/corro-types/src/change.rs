use std::{iter::Peekable, ops::DerefMut};

use antithesis_sdk::assert_always;
pub use corro_api_types::SqliteValue;
use corro_api_types::{ColumnName, TableName};
use corro_base_types::{CrsqlDbVersion, CrsqlSeqRange};
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};
use serde_json::json;
use speedy::{Readable, Writable};
use tracing::{debug, trace, warn};

use crate::{
    actor::ActorId,
    agent::{Agent, BookedVersions, ChangeError},
    base::CrsqlSeq,
    broadcast::{ChangesetPerTable, Timestamp},
};

#[derive(Debug, Default, Clone, Serialize, Deserialize, Readable, Writable, PartialEq)]
pub struct Change {
    pub table: TableName,
    pub pk: Vec<u8>,
    pub cid: ColumnName,
    pub val: SqliteValue,
    pub col_version: i64,
    pub db_version: CrsqlDbVersion,
    pub seq: CrsqlSeq,
    pub site_id: [u8; 16],
    pub cl: i64,
}

impl Change {
    // this is an ESTIMATE, it should give a rough idea of how many bytes will
    // be required on the wire
    pub fn estimated_byte_size(&self) -> usize {
        self.table.len() + self.pk.len() + self.cid.len() + self.val.estimated_byte_size() +
        // db_version
        8 +
        self.estimated_column_byte_size() +
        // site_id
        16
    }

    pub fn estimated_column_byte_size(&self) -> usize {
        self.cid.len() + self.val.estimated_byte_size() +
        // col_version
        8 +
        // seq
        8 +
        // cl
        8
    }
}

pub fn row_to_change(row: &Row) -> Result<Change, rusqlite::Error> {
    Ok(Change {
        table: row.get(0)?,
        pk: row.get(1)?,
        cid: row.get(2)?,
        val: row.get(3)?,
        col_version: row.get(4)?,
        db_version: row.get(5)?,
        seq: row.get(6)?,
        site_id: row.get(7)?,
        cl: row.get(8)?,
    })
}

pub struct ChunkedChanges<I: Iterator> {
    iter: Peekable<I>,
    changes: ChangesetPerTable,
    last_pushed_seq: CrsqlSeq,
    last_start_seq: CrsqlSeq,
    last_seq: CrsqlSeq,
    max_buf_size: usize,
    buffered_size: usize,
    done: bool,
}

impl<I> ChunkedChanges<I>
where
    I: Iterator,
{
    pub fn new(iter: I, start_seq: CrsqlSeq, last_seq: CrsqlSeq, max_buf_size: usize) -> Self {
        Self {
            iter: iter.peekable(),
            changes: Default::default(),
            last_pushed_seq: CrsqlSeq(0),
            last_start_seq: start_seq,
            last_seq,
            max_buf_size,
            buffered_size: 0,
            done: false,
        }
    }

    pub fn max_buf_size(&self) -> usize {
        self.max_buf_size
    }

    pub fn set_max_buf_size(&mut self, size: usize) {
        self.max_buf_size = size;
    }
}

impl<I> Iterator for ChunkedChanges<I>
where
    I: Iterator<Item = rusqlite::Result<Change>>,
{
    type Item = Result<(ChangesetPerTable, CrsqlSeqRange), rusqlite::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // previously marked as done because the Rows iterator returned None
        if self.done {
            return None;
        }

        let details = json!({});
        assert_always!(
            self.changes.is_empty(),
            "iterator for ChunkedChanges still has changes when next() is called",
            &details
        );

        // reset the buffered size
        self.buffered_size = 0;

        loop {
            trace!("chunking through the rows iterator");
            match self.iter.next() {
                Some(Ok(change)) => {
                    trace!("got change: {change:?}");

                    self.last_pushed_seq = change.seq;

                    let size = self.changes.insert(change);
                    self.buffered_size += size;

                    if self.last_pushed_seq == self.last_seq {
                        // this was the last seq! break early
                        break;
                    }

                    if self.buffered_size >= self.max_buf_size {
                        // chunking it up
                        let start_seq = self.last_start_seq;

                        if self.iter.peek().is_none() {
                            // no more rows, break early
                            break;
                        }

                        // prepare for next round! we're not done...
                        self.last_start_seq = self.last_pushed_seq + 1;

                        return Some(Ok((
                            self.changes.drain(),
                            CrsqlSeqRange::new(start_seq, self.last_pushed_seq),
                        )));
                    }
                }
                None => {
                    // probably not going to happen since we peek at the next and end early
                    // break out of the loop, don't return, there might be buffered changes
                    trace!("no more changes to iterate on");
                    break;
                }
                Some(Err(e)) => return Some(Err(e)),
            }
        }

        self.done = true;

        // return buffered changes
        Some(Ok((
            self.changes.clone(), // no need to drain here like before
            CrsqlSeqRange::new(self.last_start_seq, self.last_seq), // even if empty, this is all we have still applied
        )))
    }
}

pub const MAX_CHANGES_BYTE_SIZE: usize = 8 * 1024;

pub struct InsertChangesInfo {
    pub db_version: CrsqlDbVersion,
    pub last_seq: CrsqlSeq,
    pub ts: Timestamp,
}

pub fn insert_local_changes(
    agent: &Agent,
    tx: &Connection,
    book_writer: &mut impl DerefMut<Target = BookedVersions>,
) -> Result<Option<InsertChangesInfo>, ChangeError> {
    let actor_id = agent.actor_id();

    let db_version: CrsqlDbVersion = tx
        .prepare_cached("SELECT crsql_peek_next_db_version()")
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: None,
        })?
        .query_row((), |row| row.get(0))
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: None,
        })?;

    let version_info: (Option<CrsqlSeq>, Option<Timestamp>) = tx
        .prepare_cached(
            "SELECT MAX(seq), MAX(ts) FROM crsql_changes WHERE site_id = ? AND db_version = ?;",
        )
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: None,
        })?
        .query_row((agent.actor_id(), db_version), |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: None,
        })?;

    match version_info {
        (None, None) => Ok(None),
        (None, Some(ts)) => {
            warn!("found db_version {db_version} without seq, last ts: {ts:?})");
            Ok(None)
        }
        (Some(last_seq), ts) => {
            let ts = ts.unwrap_or_else(|| {
                warn!("found db_version {db_version} without seq, last ts: {ts:?}");
                Timestamp::from(agent.clock().new_timestamp())
            });

            debug!("found db_version {db_version} (last seq: {last_seq}, last ts: {ts})");

            let db_versions = db_version..=db_version;

            book_writer
                .insert_db(tx, [db_versions].into())
                .map_err(|source| ChangeError::Rusqlite {
                    source,
                    actor_id: Some(actor_id),
                    version: Some(db_version),
                })?;

            if agent.config().highlow.enabled && agent.config().highlow.low.is_some() {
                if let Err(e) = capture_highlow_events_for_actor(agent, tx, actor_id, db_version) {
                    tracing::error!(
                        "failed to capture highlow events for db_version {db_version}: {e}"
                    );
                }
            }

            Ok(Some(InsertChangesInfo {
                db_version,
                last_seq,
                ts,
            }))
        }
    }
}

pub fn capture_highlow_events_for_actor(
    agent: &Agent,
    tx: &Connection,
    actor_id: ActorId,
    db_version: CrsqlDbVersion,
) -> Result<(), ChangeError> {
    let config = agent.config();
    let low = match &config.highlow.low {
        Some(low) if config.highlow.enabled => low,
        _ => return Ok(()),
    };

    let mut prepped = tx
        .prepare_cached(
            r#"
                SELECT "table", pk, cid, val, col_version, db_version, seq, site_id, cl
                    FROM crsql_changes
                    WHERE db_version = ?
                    AND site_id = ?
                    ORDER BY seq ASC
            "#,
        )
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;

    let rows = prepped
        .query_map(
            rusqlite::params![db_version, actor_id.as_bytes()],
            row_to_change,
        )
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;

    let mut table_pk_groups: indexmap::IndexMap<(String, Vec<u8>), Vec<Change>> =
        indexmap::IndexMap::new();
    for row in rows {
        let change = row.map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;
        if change.table.starts_with("__galv_")
            || change.table.starts_with("__corro_")
            || change.table.starts_with("crsql_")
        {
            continue;
        }
        table_pk_groups
            .entry((change.table.to_string(), change.pk.clone()))
            .or_default()
            .push(change);
    }

    if table_pk_groups.is_empty() {
        return Ok(());
    }

    let mut current_seq: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) FROM __galv_highlow_events WHERE stream_id = ?1",
            [&low.stream_id],
            |r| r.get(0),
        )
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let mut stmt = tx
        .prepare_cached(
            "INSERT INTO __galv_highlow_events (stream_id, sequence, event_json, committed_at_ms) VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;

    let schema = agent.schema().read();
    let pk_names_by_table: std::collections::HashMap<String, Vec<String>> = schema
        .tables
        .iter()
        .map(|(name, table)| (name.clone(), table.pk.iter().cloned().collect()))
        .collect();
    drop(schema);

    for ((table, pk_bytes), changes) in table_pk_groups {
        current_seq += 1;
        let is_delete = changes.iter().any(|c| c.cid.as_str() == "-1");
        let operation = if is_delete {
            galv_highlow::Operation::Delete
        } else {
            galv_highlow::Operation::Upsert
        };

        let mut columns = Vec::new();
        if !is_delete {
            for c in &changes {
                if c.cid.as_str() != "-1" {
                    let val = match &c.val {
                        SqliteValue::Null => galv_highlow::Value::Null,
                        SqliteValue::Integer(i) => galv_highlow::Value::Integer(*i),
                        SqliteValue::Real(r) => galv_highlow::Value::Real(r.0),
                        SqliteValue::Text(t) => galv_highlow::Value::Text(t.to_string()),
                        SqliteValue::Blob(b) => galv_highlow::Value::blob(b),
                    };
                    columns.push(galv_highlow::Column {
                        name: c.cid.to_string(),
                        value: val,
                    });
                }
            }
        }

        let mut pk_map = serde_json::Map::new();
        let pk_cols = pk_names_by_table.get(&table);
        if let Ok(unpacked) = crate::pubsub::unpack_columns(&pk_bytes) {
            for (idx, val_ref) in unpacked.into_iter().enumerate() {
                let col_name = pk_cols
                    .and_then(|cols| cols.get(idx).cloned())
                    .unwrap_or_else(|| {
                        if table == "files" && idx == 0 {
                            "uuid".to_string()
                        } else {
                            format!("pk_{idx}")
                        }
                    });
                let json_val = match val_ref.0 {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(i) => serde_json::json!(i),
                    rusqlite::types::ValueRef::Real(r) => serde_json::json!(r),
                    rusqlite::types::ValueRef::Text(t) => {
                        serde_json::Value::String(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(b) => {
                        let hex_str = hex::encode(b);
                        serde_json::Value::String(hex_str)
                    }
                };
                pk_map.insert(col_name, json_val);
            }
        } else if let Ok(s) = std::str::from_utf8(&pk_bytes) {
            pk_map.insert("id".to_string(), serde_json::Value::String(s.to_string()));
        } else {
            pk_map.insert(
                "id".to_string(),
                serde_json::Value::String(hex::encode(&pk_bytes)),
            );
        }

        let event = galv_highlow::Event {
            stream_id: low.stream_id.clone(),
            sequence: current_seq,
            transaction_id: format!("{}:{}", actor_id, db_version),
            table,
            primary_key: pk_map,
            operation,
            columns,
            source_actor: actor_id.to_string(),
            committed_at_ms: now_ms,
        };

        let json_bytes = serde_json::to_vec(&event).map_err(|e| ChangeError::Rusqlite {
            source: rusqlite::Error::ToSqlConversionFailure(Box::new(e)),
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;

        stmt.execute(rusqlite::params![
            event.stream_id,
            event.sequence,
            json_bytes,
            event.committed_at_ms
        ])
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;
    }

    Ok(())
}

/// Marks fields changed locally on High as High-owned, but only for rows that
/// already have Low provenance. This is kept separate from Low capture so a
/// received Low bundle can never accidentally claim ownership of itself.
pub fn capture_highlow_ownership_for_actor(
    agent: &Agent,
    tx: &Connection,
    actor_id: ActorId,
    db_version: CrsqlDbVersion,
) -> Result<(), ChangeError> {
    if !agent.config().highlow.enabled || agent.config().highlow.high.is_none() {
        return Ok(());
    }
    let schema = agent.schema().read();
    let pk_names: std::collections::HashMap<String, Vec<String>> = schema
        .tables
        .iter()
        .map(|(name, table)| (name.clone(), table.pk.iter().cloned().collect()))
        .collect();
    drop(schema);
    let mut stmt = tx.prepare_cached("SELECT \"table\", pk, cid FROM crsql_changes WHERE db_version=?1 AND site_id=?2 ORDER BY seq").map_err(|source| ChangeError::Rusqlite { source, actor_id:Some(actor_id), version:Some(db_version) })?;
    let rows = stmt
        .query_map(rusqlite::params![db_version, actor_id.as_bytes()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;
    let mut groups: indexmap::IndexMap<(String, Vec<u8>), Vec<String>> = indexmap::IndexMap::new();
    for row in rows {
        let (table, pk, cid) = row.map_err(|source| ChangeError::Rusqlite {
            source,
            actor_id: Some(actor_id),
            version: Some(db_version),
        })?;
        if !table.starts_with("__galv_")
            && !table.starts_with("__corro_")
            && !table.starts_with("crsql_")
        {
            groups.entry((table, pk)).or_default().push(cid);
        }
    }
    for ((table, bytes), columns) in groups {
        let mut key = serde_json::Map::new();
        if let Ok(values) = crate::pubsub::unpack_columns(&bytes) {
            for (index, value) in values.into_iter().enumerate() {
                let name = pk_names
                    .get(&table)
                    .and_then(|v| v.get(index))
                    .cloned()
                    .unwrap_or_else(|| {
                        if table == "files" && index == 0 {
                            "uuid".to_string()
                        } else {
                            format!("pk_{index}")
                        }
                    });
                let json = match value.0 {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(v) => json!(v),
                    rusqlite::types::ValueRef::Real(v) => json!(v),
                    rusqlite::types::ValueRef::Text(v) => {
                        serde_json::Value::String(String::from_utf8_lossy(v).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(v) => serde_json::Value::String(hex::encode(v)),
                };
                key.insert(name, json);
            }
        }
        if key.is_empty() {
            continue;
        }
        if columns.iter().any(|v| v == "-1") {
            galv_highlow::remove_provenance(tx, &table, &key).map_err(|source| {
                ChangeError::Rusqlite {
                    source: rusqlite::Error::ToSqlConversionFailure(Box::new(source)),
                    actor_id: Some(actor_id),
                    version: Some(db_version),
                }
            })?;
        } else {
            galv_highlow::mark_high_ownership(tx, &table, &key, &columns).map_err(|source| {
                ChangeError::Rusqlite {
                    source: rusqlite::Error::ToSqlConversionFailure(Box::new(source)),
                    actor_id: Some(actor_id),
                    version: Some(db_version),
                }
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::dbsr;

    #[test]
    fn test_change_chunker() {
        // empty interator
        let mut chunker = ChunkedChanges::new(vec![].into_iter(), CrsqlSeq(0), CrsqlSeq(100), 50);

        assert_eq!(
            chunker.next(),
            Some(Ok((ChangesetPerTable::default(), dbsr!(0, 100))))
        );
        assert_eq!(chunker.next(), None);

        let changes: Vec<Change> = (0..100)
            .map(|seq| Change {
                seq: CrsqlSeq(seq),
                ..Default::default()
            })
            .collect();

        let (changeset, size) =
            mapped_changeset_from_changes(vec![changes[0].clone(), changes[1].clone()]);
        // 2 iterations
        let mut chunker = ChunkedChanges::new(
            vec![
                Ok(changes[0].clone()),
                Ok(changes[1].clone()),
                Ok(changes[2].clone()),
            ]
            .into_iter(),
            CrsqlSeq(0),
            CrsqlSeq(100),
            size,
        );

        assert_eq!(chunker.next(), Some(Ok((changeset, dbsr!(0, 1)))));

        let (changeset, _) = mapped_changeset_from_changes(vec![changes[2].clone()]);
        assert_eq!(chunker.next(), Some(Ok((changeset, dbsr!(2, 100)))));
        assert_eq!(chunker.next(), None);

        let (changeset, size) = mapped_changeset_from_changes(vec![changes[0].clone()]);
        let mut chunker = ChunkedChanges::new(
            vec![Ok(changes[0].clone()), Ok(changes[1].clone())].into_iter(),
            CrsqlSeq(0),
            CrsqlSeq(0),
            size,
        );

        assert_eq!(chunker.next(), Some(Ok((changeset, dbsr!(0, 0)))));
        assert_eq!(chunker.next(), None);

        let (changeset, size) =
            mapped_changeset_from_changes(vec![changes[0].clone(), changes[2].clone()]);
        // gaps
        let mut chunker = ChunkedChanges::new(
            vec![Ok(changes[0].clone()), Ok(changes[2].clone())].into_iter(),
            CrsqlSeq(0),
            CrsqlSeq(100),
            size,
        );

        assert_eq!(chunker.next(), Some(Ok((changeset, dbsr!(0, 100)))));

        assert_eq!(chunker.next(), None);

        // gaps
        let (changeset, _) = mapped_changeset_from_changes(vec![
            changes[2].clone(),
            changes[4].clone(),
            changes[7].clone(),
            changes[8].clone(),
        ]);
        let mut chunker = ChunkedChanges::new(
            vec![
                Ok(changes[2].clone()),
                Ok(changes[4].clone()),
                Ok(changes[7].clone()),
                Ok(changes[8].clone()),
            ]
            .into_iter(),
            CrsqlSeq(0),
            CrsqlSeq(100),
            100000, // just send them all!
        );

        assert_eq!(chunker.next(), Some(Ok((changeset, dbsr!(0, 100)))));

        assert_eq!(chunker.next(), None);

        // gaps
        let (changeset, size) =
            mapped_changeset_from_changes(vec![changes[2].clone(), changes[4].clone()]);
        let mut chunker = ChunkedChanges::new(
            vec![
                Ok(changes[2].clone()),
                Ok(changes[4].clone()),
                Ok(changes[7].clone()),
                Ok(changes[8].clone()),
            ]
            .into_iter(),
            CrsqlSeq(0),
            CrsqlSeq(10),
            size,
        );

        assert_eq!(chunker.next(), Some(Ok((changeset, dbsr!(0, 4)))));

        let (changeset, _) =
            mapped_changeset_from_changes(vec![changes[7].clone(), changes[8].clone()]);
        assert_eq!(chunker.next(), Some(Ok((changeset, dbsr!(5, 10)))));

        assert_eq!(chunker.next(), None);
    }

    fn mapped_changeset_from_changes(changes: Vec<Change>) -> (ChangesetPerTable, usize) {
        let mut changeset = ChangesetPerTable::default();
        let mut size = 0;
        for change in changes {
            size += changeset.insert(change);
        }
        (changeset, size)
    }
}
