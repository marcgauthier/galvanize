//! Setup main agent state

// External crates
use arc_swap::ArcSwap;
use camino::Utf8PathBuf;
use parking_lot::RwLock;
use rusqlite::{Connection, OptionalExtension};
use std::{
    io::{self, Read, Seek, SeekFrom},
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{
        mpsc::{channel as tokio_channel, Receiver as TokioReceiver},
        RwLock as TokioRwLock, Semaphore,
    },
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use tripwire::Tripwire;
use zstd;

// Internals
use crate::{
    api::{
        peer::gossip_server_endpoint,
        public::{
            pubsub::{process_sub_channel, MatcherBroadcastCache, SharedMatcherBroadcastCache},
            update::SharedUpdateBroadcastCache,
        },
    },
    transport::Transport,
};
use corro_types::{
    actor::ActorId,
    agent::{migrate, Agent, AgentConfig, ApplyTrigger, BookedVersions, Bookie, SplitPool},
    base::CrsqlDbVersionRange,
    broadcast::{
        BroadcastInput, BroadcastV1, ChangeSource, ChangeV1, FocaInput, PlumtreeInput,
        PlumtreeUpdates,
    },
    channel::{bounded, CorroReceiver},
    compress::ZstdDicts,
    config::{CompressionConfig, Config},
    members::Members,
    metrics_tracker::MetricsTracker,
    pubsub::{Matcher, SubsManager},
    schema::{init_schema, Schema},
    sqlite::CrConn,
    updates::UpdatesManager,
};

/// Runtime state for the Corrosion agent
pub struct AgentOptions {
    pub gossip_server_endpoint: quinn::Endpoint,
    pub transport: Transport,
    pub api_listeners: Vec<TcpListener>,
    pub highlow_control_listener: Option<TcpListener>,
    pub rx_bcast: CorroReceiver<BroadcastInput>,
    pub rx_apply: CorroReceiver<ApplyTrigger>,
    pub rx_clear_buf: CorroReceiver<(ActorId, CrsqlDbVersionRange)>,
    pub rx_changes: CorroReceiver<(ChangeV1, ChangeSource, Option<BroadcastV1>)>,
    pub rx_foca: CorroReceiver<FocaInput>,
    // pub tx_plumtree: CorroSender<PlumtreeInput>,
    pub rx_plumtree: CorroReceiver<PlumtreeInput>,
    pub rx_plumtree_updates: CorroReceiver<PlumtreeUpdates>,
    pub rtt_rx: TokioReceiver<(SocketAddr, Duration)>,
    pub subs_manager: SubsManager,
    pub subs_bcast_cache: SharedMatcherBroadcastCache,
    pub updates_bcast_cache: SharedUpdateBroadcastCache,
    pub tripwire: Tripwire,
}

pub struct UnlockedDbState {
    pub actor_id: ActorId,
    pub pool: SplitPool,
    pub clock: Arc<uhlc::HLC>,
    pub schema: Schema,
    pub bookie: Bookie,
    pub booked: corro_types::agent::Booked,
    pub cluster_id: corro_types::actor::ClusterId,
    pub subs_manager: SubsManager,
    pub subs_bcast_cache: SharedMatcherBroadcastCache,
}

/// Initialize database state (SQLite connection, pool, migrations, schemas, bookie, subscriptions)
pub async fn init_database_state(
    conf: &Config,
    write_sema: Arc<Semaphore>,
    tripwire: &Tripwire,
) -> eyre::Result<UnlockedDbState> {
    if let Some(parent) = conf.db.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let actor_id = {
        // we need to set auto_vacuum before any tables are created
        let mut db_conn = Connection::open(&conf.db.path)?;
        corro_types::sqlite::apply_key_if_present(&mut db_conn, None)?;
        db_conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL")?;
        if conf.highlow.enabled {
            // Deliberately create these before CR-SQLite initializes user
            // tables: high/low bookkeeping is local agent state and must not
            // enter ordinary mesh replication.
            galv_highlow::initialize_store(&db_conn)?;
        }
        db_conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS __galv_files_local (
                uuid TEXT PRIMARY KEY NOT NULL,
                status TEXT NOT NULL,
                downloaded_at TEXT NOT NULL,
                error TEXT
            ) WITHOUT ROWID;",
        )?;

        let conn = CrConn::init(db_conn)?;
        conn.query_row("SELECT crsql_site_id();", [], |row| {
            row.get::<_, ActorId>(0)
        })
    }?;

    info!("Actor ID: {actor_id}");

    let pool = SplitPool::create(
        &conf.db.path,
        write_sema.clone(),
        conf.db.cache_size_kib,
        conf.db.mmap_size_bytes,
        conf.db.journal_size_limit_bytes,
    )
    .await?;

    let clock = Arc::new(
        uhlc::HLCBuilder::default()
            .with_id(actor_id.try_into().unwrap())
            .with_max_delta(Duration::from_millis(300))
            .build(),
    );

    let schema = {
        let mut conn = pool.write_priority().await?;
        migrate(clock.clone(), &mut conn)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                uuid TEXT PRIMARY KEY NOT NULL,
                filename TEXT NOT NULL DEFAULT '',
                size INTEGER NOT NULL DEFAULT 0,
                sha256 TEXT NOT NULL DEFAULT '',
                content_type TEXT,
                created_at TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT '',
                metadata JSON
            );
            SELECT crsql_as_crr('files');
            DELETE FROM __corro_schema WHERE tbl_name = 'files';
            INSERT INTO __corro_schema SELECT tbl_name, type, name, sql, 'system' AS source FROM sqlite_schema WHERE tbl_name = 'files' AND type IN ('table', 'index') AND name IS NOT NULL AND sql IS NOT NULL;",
        )?;

        let mut schema = init_schema(&conn)?;
        schema.constrain()?;

        schema
    };

    let subs_manager = SubsManager::default();

    let subs_bcast_cache = setup_spawn_subscriptions(
        &subs_manager,
        conf.db.subscriptions_path(),
        &pool,
        &schema,
        tripwire,
    )
    .await?;

    let cluster_id = {
        let conn = pool.read().await?;
        conn.query_row(
            "SELECT value FROM __corro_state WHERE key = 'cluster_id'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default()
    };

    info!("Cluster ID: {cluster_id}");

    // Load all actors' bookie state synchronously.
    let start = Instant::now();
    let all_booked = {
        let conn = pool.read().await?;
        BookedVersions::load_all_from_conn(&conn)?
    };
    info!("Loaded booked versions in {:?}", start.elapsed());

    let bookie = Bookie::new(all_booked);
    let booked = bookie.ensure(actor_id);

    Ok(UnlockedDbState {
        actor_id,
        pool,
        clock,
        schema,
        bookie,
        booked,
        cluster_id,
        subs_manager,
        subs_bcast_cache,
    })
}

/// Setup an agent runtime and state with a configuration
pub async fn setup(conf: Config, tripwire: Tripwire) -> eyre::Result<(Agent, AgentOptions)> {
    debug!("setting up corrosion @ {}", conf.db.path);

    if let Some(parent) = conf.db.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // do this early to error earlier
    let members = Members::new(conf.gossip.member_id);
    let write_sema = Arc::new(Semaphore::new(1));

    let maybe_unlocked = if conf.db.await_unlock && !galv_rekey_cli::is_unlocked() {
        info!("Database configured with await_unlock = true. Booting in AwaitingUnlock state.");
        None
    } else if galv_rekey_cli::is_unlocked() {
        Some(init_database_state(&conf, write_sema.clone(), &tripwire).await?)
    } else {
        match init_database_state(&conf, write_sema.clone(), &tripwire).await {
            Ok(state) => Some(state),
            Err(e) => {
                info!("Database locked or requires unlock: {e}. Booting in AwaitingUnlock state.");
                None
            }
        }
    };

    let lock_state = if maybe_unlocked.is_some() {
        corro_types::agent::AgentLockState::Unlocked
    } else {
        corro_types::agent::AgentLockState::AwaitingUnlock
    };

    let (actor_id, pool, clock, schema, bookie, booked, cluster_id, subs_manager, subs_bcast_cache) =
        match maybe_unlocked {
            Some(state) => (
                state.actor_id,
                state.pool,
                state.clock,
                state.schema,
                state.bookie,
                state.booked,
                state.cluster_id,
                state.subs_manager,
                state.subs_bcast_cache,
            ),
            None => {
                let default_actor_id = ActorId::default();
                let clock = Arc::new(
                    uhlc::HLCBuilder::default()
                        .with_id(
                            default_actor_id
                                .try_into()
                                .unwrap_or_else(|_| uhlc::ID::rand()),
                        )
                        .with_max_delta(Duration::from_millis(300))
                        .build(),
                );
                let bookie = Bookie::new(Default::default());
                let booked = bookie.ensure(default_actor_id);
                (
                    default_actor_id,
                    SplitPool::dummy(),
                    clock,
                    Schema::default(),
                    bookie,
                    booked,
                    Default::default(),
                    SubsManager::default(),
                    Arc::new(TokioRwLock::new(MatcherBroadcastCache::default())),
                )
            }
        };

    let updates_manager = UpdatesManager::default();
    let updates_bcast_cache = SharedUpdateBroadcastCache::default();

    let (tx_apply, rx_apply) = bounded(conf.perf.apply_channel_len, "apply");
    let (tx_clear_buf, rx_clear_buf) = bounded(conf.perf.clearbuf_channel_len, "clear_buf");

    let gossip_server_endpoint = gossip_server_endpoint(&conf.gossip).await?;
    let gossip_addr = gossip_server_endpoint.local_addr()?;

    let external_addr = conf.gossip.external_addr;

    // RTT handling interacts with the tokio ReceiverStream and as
    let (rtt_tx, rtt_rx) = tokio_channel(1024);

    let transport = Transport::new(&conf.gossip, rtt_tx).await?;

    let mut api_listeners = Vec::with_capacity(conf.api.bind_addr.len());
    for addr in conf.api.bind_addr.iter() {
        api_listeners.push(TcpListener::bind(addr).await?);
    }
    let api_addr = api_listeners.first().unwrap().local_addr()?;
    let highlow_control_listener = match &conf.highlow.control_api {
        Some(control) if conf.highlow.enabled => Some(TcpListener::bind(control.addr).await?),
        _ => None,
    };

    let (tx_bcast, rx_bcast) = bounded(conf.perf.bcast_channel_len, "bcast");
    let (tx_changes, rx_changes) = bounded(conf.perf.changes_channel_len, "changes");
    let (tx_foca, rx_foca) = bounded(conf.perf.foca_channel_len, "foca");
    let (tx_plumtree, rx_plumtree) = bounded(conf.perf.bcast_channel_len, "plumtree");
    let (tx_plumtree_updates, rx_plumtree_updates) =
        bounded(conf.perf.foca_channel_len, "plumtree_updates");

    let metrics_tracker = MetricsTracker::new(Duration::from_secs(120), 5)?;
    let change_dict = load_change_dicts(&conf.gossip.compression_config())?;

    let opts = AgentOptions {
        gossip_server_endpoint,
        transport: transport.clone(),
        api_listeners,
        highlow_control_listener,
        rx_bcast,
        rx_apply,
        rx_clear_buf,
        rx_changes,
        rx_foca,
        rx_plumtree,
        rx_plumtree_updates,
        rtt_rx,
        subs_manager: subs_manager.clone(),
        subs_bcast_cache,
        updates_bcast_cache,
        tripwire: tripwire.clone(),
    };

    let agent = Agent::new(AgentConfig {
        actor_id,
        pool: pool.clone(),
        gossip_addr,
        external_addr,
        api_addr,
        members: RwLock::new(members),
        config: ArcSwap::from_pointee(conf),
        clock,
        booked,
        bookie,
        tx_bcast,
        tx_apply,
        tx_clear_buf,
        tx_changes,
        tx_foca,
        tx_plumtree,
        tx_plumtree_updates,
        change_dict,
        write_sema,
        schema: RwLock::new(schema),
        cluster_id,
        subs_manager,
        updates_manager,
        metrics_tracker,
        tripwire,
        fatal_issue: Default::default(),
        shutdown_token: CancellationToken::new(),
        lock_state,
    });

    Ok((agent, opts))
}

/// Load trained zstd dictionaries from `gossip.compression.dict_dir`.
///
/// Every valid dictionary file in the directory is registered for decoding.
/// When `dict_file` is set, that file is also used as the encoder dictionary.
pub fn load_change_dicts(
    compression_config: &CompressionConfig,
) -> eyre::Result<Option<Arc<ZstdDicts>>> {
    let Some(dir) = &compression_config.dict_dir else {
        return Ok(None);
    };

    let encode_name = compression_config.dict_file.as_deref();
    let mut encoder_bytes: Option<Vec<u8>> = None;
    let mut decoder_dicts = Vec::new();

    let entries = std::fs::read_dir(dir)
        .map_err(|e| eyre::eyre!("could not read compression dict dir {dir}: {e}"))?;
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let entry_path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();

        let mut file = match std::fs::File::open(&entry_path) {
            Ok(file) => file,
            Err(e) => {
                warn!("could not open candidate compression dict {entry_path:?}: {e}");
                continue;
            }
        };

        match load_dictionary(&mut file) {
            Ok(Some(b)) => {
                if encode_name == Some(file_name.as_ref()) {
                    info!(
                        path = %entry_path.display(),
                        dict_id = ?zstd::zstd_safe::get_dict_id_from_dict(&b),
                        "using compression dictionary for encoding and decoding"
                    );
                    encoder_bytes = Some(b.clone());
                } else {
                    info!(
                        path = %entry_path.display(),
                        dict_id = ?zstd::zstd_safe::get_dict_id_from_dict(&b),
                        "using compression dictionary for decoding"
                    );
                }
                decoder_dicts.push(b);
            }
            _ => {
                warn!("could not read candidate compression dict {entry_path:?}");
            }
        }
    }

    if let Some(name) = encode_name {
        if encoder_bytes.is_none() {
            eyre::bail!(
                "gossip.compression.dict_file `{name}` not found in {dir} \
                 or is not a valid zstd dictionary"
            );
        }
    }

    Ok(Some(Arc::new(ZstdDicts::new(
        encoder_bytes.as_deref(),
        compression_config.level,
        decoder_dicts,
    ))))
}

/// Rescan `gossip.compression.dict_dir` and swap the agent's dictionaries.
pub fn reload_change_dicts(agent: &Agent) -> eyre::Result<usize> {
    let dicts = load_change_dicts(&agent.config().gossip.compression_config())?;
    let decoder_count = dicts.as_ref().map(|d| d.decoder_count()).unwrap_or(0);
    agent.set_change_dict(dicts);
    Ok(decoder_count)
}

/// Initialise subscription state and tasks
///
/// 1. Get subscriptions state directory from config
/// 2. Load existing subscriptions and restore them in SubsManager
/// 3. Spawn subscription processor task
async fn setup_spawn_subscriptions(
    subs_manager: &SubsManager,
    subs_path: Utf8PathBuf,
    pool: &SplitPool,
    schema: &Schema,
    tripwire: &Tripwire,
) -> eyre::Result<SharedMatcherBroadcastCache> {
    let mut subs_bcast_cache = MatcherBroadcastCache::default();
    let mut to_cleanup = vec![];

    if let Ok(mut dir) = tokio::fs::read_dir(&subs_path).await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            let path_str = entry.path().display().to_string();
            if let Some(sub_id_str) = path_str.strip_prefix(subs_path.as_str()) {
                if let Ok(sub_id) = sub_id_str.trim_matches('/').parse() {
                    let (_, created) = match subs_manager.restore(
                        sub_id,
                        &subs_path,
                        schema,
                        pool,
                        tripwire.clone(),
                    ) {
                        Ok(res) => res,
                        Err(e) => {
                            error!(%sub_id, "could not restore subscription: {e}");
                            to_cleanup.push(sub_id);
                            continue;
                        }
                    };

                    info!(%sub_id, "Restored subscription");

                    let (sub_tx, _) = tokio::sync::broadcast::channel(10240);

                    tokio::spawn(process_sub_channel(
                        subs_manager.clone(),
                        sub_id,
                        sub_tx.clone(),
                        created.evt_rx,
                    ));

                    subs_bcast_cache.insert(sub_id, sub_tx);
                }
            }
        }
    }

    for id in to_cleanup {
        info!(sub_id = %id, "Cleaning up unclean subscription");
        Matcher::cleanup(id, Matcher::sub_path(subs_path.as_path(), id))?;
    }

    Ok(Arc::new(TokioRwLock::new(subs_bcast_cache)))
}

fn load_dictionary(file: &mut std::fs::File) -> io::Result<Option<Vec<u8>>> {
    let mut prefix = [0u8; 4];
    let peeked = file.read(&mut prefix)?;
    let is_dict = peeked == 4 && u32::from_le_bytes(prefix) == zstd::zstd_safe::MAGIC_DICTIONARY;
    if !is_dict {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}
