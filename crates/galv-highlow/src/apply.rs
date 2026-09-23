use crate::{Bundle, Error, Event, Operation, Result, Value};
use rusqlite::{ToSql, Transaction};
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct ApplyStats {
    pub upserts: usize,
    pub deletes: usize,
}

fn to_sql_value(val: &Value) -> Result<rusqlite::types::Value> {
    match val {
        Value::Null => Ok(rusqlite::types::Value::Null),
        Value::Integer(i) => Ok(rusqlite::types::Value::Integer(*i)),
        Value::Real(r) => Ok(rusqlite::types::Value::Real(*r)),
        Value::Text(s) => Ok(rusqlite::types::Value::Text(s.clone())),
        Value::Blob(b64) => {
            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD_NO_PAD, b64)
                    .map_err(|e| {
                        Error::Configuration(format!("invalid base64 in blob value: {e}"))
                    })?;
            Ok(rusqlite::types::Value::Blob(bytes))
        }
    }
}

fn json_to_sql_value(val: &serde_json::Value) -> Result<rusqlite::types::Value> {
    match val {
        serde_json::Value::Null => Ok(rusqlite::types::Value::Null),
        serde_json::Value::Bool(b) => Ok(rusqlite::types::Value::Integer(if *b { 1 } else { 0 })),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(rusqlite::types::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(rusqlite::types::Value::Real(f))
            } else {
                Ok(rusqlite::types::Value::Text(n.to_string()))
            }
        }
        serde_json::Value::String(s) => Ok(rusqlite::types::Value::Text(s.clone())),
        serde_json::Value::Object(obj) => {
            // Check for typed value format: {"type": "integer", "value": 123}
            if let (Some(t), Some(v)) = (obj.get("type"), obj.get("value")) {
                if let Some(type_str) = t.as_str() {
                    match type_str {
                        "null" => return Ok(rusqlite::types::Value::Null),
                        "integer" => {
                            if let Some(i) = v.as_i64() {
                                return Ok(rusqlite::types::Value::Integer(i));
                            }
                        }
                        "real" => {
                            if let Some(f) = v.as_f64() {
                                return Ok(rusqlite::types::Value::Real(f));
                            }
                        }
                        "text" => {
                            if let Some(s) = v.as_str() {
                                return Ok(rusqlite::types::Value::Text(s.to_string()));
                            }
                        }
                        "blob" => {
                            if let Some(b64) = v.as_str() {
                                let bytes = base64::Engine::decode(
                                    &base64::engine::general_purpose::STANDARD_NO_PAD,
                                    b64,
                                )
                                .map_err(|e| {
                                    Error::Configuration(format!(
                                        "invalid base64 in blob value: {e}"
                                    ))
                                })?;
                                return Ok(rusqlite::types::Value::Blob(bytes));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(rusqlite::types::Value::Text(
                serde_json::to_string(obj).unwrap_or_default(),
            ))
        }
        serde_json::Value::Array(arr) => Ok(rusqlite::types::Value::Text(
            serde_json::to_string(arr).unwrap_or_default(),
        )),
    }
}

pub fn apply_bundle(tx: &Transaction<'_>, bundle: &Bundle) -> Result<ApplyStats> {
    bundle.validate()?;
    let mut stats = ApplyStats::default();

    for event in &bundle.events {
        event.validate()?;
        match event.operation {
            Operation::Upsert => {
                apply_upsert(tx, event)?;
                stats.upserts += 1;
            }
            Operation::Delete => {
                apply_delete(tx, event)?;
                stats.deletes += 1;
            }
        }
    }

    Ok(stats)
}

fn apply_upsert(tx: &Transaction<'_>, event: &Event) -> Result<()> {
    // Combine primary key columns and updated columns
    let mut cols_map: BTreeMap<String, rusqlite::types::Value> = BTreeMap::new();

    for (pk_name, pk_val) in &event.primary_key {
        cols_map.insert(pk_name.clone(), json_to_sql_value(pk_val)?);
    }

    for col in &event.columns {
        cols_map.insert(col.name.clone(), to_sql_value(&col.value)?);
    }

    if cols_map.is_empty() {
        return Ok(());
    }

    let col_names: Vec<&str> = cols_map.keys().map(String::as_str).collect();
    if col_names.iter().any(|column| !valid_identifier(column)) || !valid_identifier(&event.table) {
        return Err(Error::Configuration("unsafe table or column name".into()));
    }
    let col_list = col_names
        .iter()
        .map(|c| quote(c))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders: Vec<String> = (1..=col_names.len()).map(|i| format!("?{i}")).collect();
    let placeholder_list = placeholders.join(", ");

    let owned: std::collections::HashSet<String> =
        crate::high_owned_fields(tx, &event.table, &event.primary_key)?
            .into_iter()
            .collect();
    let updates: Vec<String> = col_names
        .iter()
        .filter(|name| !event.primary_key.contains_key(**name) && !owned.contains(**name))
        .map(|name| format!("{} = excluded.{}", quote(name), quote(name)))
        .collect();
    let pk = event
        .primary_key
        .keys()
        .map(|name| quote(name))
        .collect::<Vec<_>>()
        .join(", ");
    let conflict = if updates.is_empty() {
        format!("ON CONFLICT ({pk}) DO NOTHING")
    } else {
        format!("ON CONFLICT ({pk}) DO UPDATE SET {}", updates.join(", "))
    };
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({}) {conflict}",
        quote(&event.table),
        col_list,
        placeholder_list
    );

    let values: Vec<&dyn ToSql> = cols_map.values().map(|v| v as &dyn ToSql).collect();

    tx.execute(&sql, values.as_slice()).map_err(|e| {
        Error::Configuration(format!("failed to execute upsert on {}: {e}", event.table))
    })?;

    crate::mark_low_origin(tx, event)
}

fn apply_delete(tx: &Transaction<'_>, event: &Event) -> Result<()> {
    if event.primary_key.is_empty() {
        return Err(Error::Configuration(
            "delete event has empty primary key".into(),
        ));
    }

    let mut where_clauses = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();

    if !valid_identifier(&event.table) {
        return Err(Error::Configuration("unsafe table name".into()));
    }
    for (i, (pk_name, pk_val)) in event.primary_key.iter().enumerate() {
        if !valid_identifier(pk_name) {
            return Err(Error::Configuration("unsafe primary-key name".into()));
        }
        where_clauses.push(format!("{} = ?{}", quote(pk_name), i + 1));
        values.push(json_to_sql_value(pk_val)?);
    }

    let where_clause = where_clauses.join(" AND ");
    let sql = format!("DELETE FROM {} WHERE {}", quote(&event.table), where_clause);

    let params_refs: Vec<&dyn ToSql> = values.iter().map(|v| v as &dyn ToSql).collect();

    tx.execute(&sql, params_refs.as_slice()).map_err(|e| {
        Error::Configuration(format!("failed to execute delete on {}: {e}", event.table))
    })?;

    crate::remove_provenance(tx, &event.table, &event.primary_key)
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(i, c)| {
            c == b'_' || (c.is_ascii_alphanumeric() && (i > 0 || !c.is_ascii_digit()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Column;
    use rusqlite::Connection;

    #[test]
    fn test_apply_bundle_upsert_and_delete() -> Result<()> {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT
            );",
        )
        .unwrap();
        crate::initialize_store(&conn)?;

        let event1 = Event {
            stream_id: "stream-1".into(),
            sequence: 1,
            transaction_id: "tx-1".into(),
            table: "users".into(),
            primary_key: serde_json::Map::from_iter([(
                "id".to_string(),
                serde_json::json!({"type": "integer", "value": 42}),
            )]),
            operation: Operation::Upsert,
            columns: vec![
                Column {
                    name: "name".into(),
                    value: Value::Text("Alice".into()),
                },
                Column {
                    name: "email".into(),
                    value: Value::Text("alice@example.com".into()),
                },
            ],
            source_actor: "actor-1".into(),
            committed_at_ms: 1000,
        };

        let event2 = Event {
            stream_id: "stream-1".into(),
            sequence: 2,
            transaction_id: "tx-2".into(),
            table: "users".into(),
            primary_key: serde_json::Map::from_iter([(
                "id".to_string(),
                serde_json::json!({"type": "integer", "value": 42}),
            )]),
            operation: Operation::Delete,
            columns: vec![],
            source_actor: "actor-1".into(),
            committed_at_ms: 2000,
        };

        let bundle = Bundle::new(vec![event1], "a".repeat(64), false)?;
        let tx = conn.transaction().unwrap();
        let stats = apply_bundle(&tx, &bundle)?;
        tx.commit().unwrap();
        assert_eq!(stats.upserts, 1);

        let name: String = conn
            .query_row("SELECT name FROM users WHERE id = 42", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "Alice");

        let bundle2 = Bundle::new(vec![event2], "a".repeat(64), false)?;
        let tx = conn.transaction().unwrap();
        let stats2 = apply_bundle(&tx, &bundle2)?;
        tx.commit().unwrap();
        assert_eq!(stats2.deletes, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM users WHERE id = 42", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);

        Ok(())
    }

    #[test]
    fn low_updates_do_not_overwrite_high_owned_fields() -> Result<()> {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT);")
            .unwrap();
        crate::initialize_store(&conn)?;
        let mut event = Event {
            stream_id: "low".into(),
            sequence: 1,
            transaction_id: "a:1".into(),
            table: "users".into(),
            primary_key: serde_json::Map::from_iter([(String::from("id"), serde_json::json!(1))]),
            operation: Operation::Upsert,
            columns: vec![
                Column {
                    name: "name".into(),
                    value: Value::Text("low".into()),
                },
                Column {
                    name: "email".into(),
                    value: Value::Text("old".into()),
                },
            ],
            source_actor: "a".into(),
            committed_at_ms: 1,
        };
        let bundle = Bundle::new(vec![event.clone()], "a".repeat(64), false)?;
        let tx = conn.transaction().unwrap();
        apply_bundle(&tx, &bundle)?;
        tx.commit().unwrap();
        let tx = conn.transaction().unwrap();
        crate::mark_high_ownership(&tx, "users", &event.primary_key, &["email".into()])?;
        tx.execute("UPDATE users SET email='high' WHERE id=1", [])
            .unwrap();
        tx.commit().unwrap();
        event.sequence = 2;
        event.columns[0].value = Value::Text("new-low".into());
        event.columns[1].value = Value::Text("new-low-email".into());
        let bundle = Bundle::new(vec![event], "a".repeat(64), false)?;
        let tx = conn.transaction().unwrap();
        apply_bundle(&tx, &bundle)?;
        tx.commit().unwrap();
        let row: (String, String) = conn
            .query_row("SELECT name,email FROM users WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(row, ("new-low".into(), "high".into()));
        Ok(())
    }
}
