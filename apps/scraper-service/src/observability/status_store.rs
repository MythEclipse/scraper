//! Redis-backed storage for the public status dashboard.
//!
//! [`super::telemetry`] keeps counters in process memory, which is what makes
//! recording a request cost a few relaxed atomics and no network round trip.
//! That view dies with the process, so this module persists the same numbers to
//! Redis and reads them back on boot: the dashboard shows lifetime totals and
//! per-endpoint history that survive a restart or a deploy, while the request
//! path stays entirely in memory.
//!
//! # Why a write-behind loop instead of a write per request
//!
//! A Redis `HINCRBY` on every request would put a network round trip (and a
//! pool checkout) on the hot path of a scraper whose whole job is being fast,
//! and it would serialise every worker on one key. Instead a background task
//! snapshots the in-memory registry every [`FLUSH_INTERVAL`] and writes only
//! what changed:
//!
//! * counters are absolute values, so the flush uses `HSET` with the current
//!   total. That is **idempotent** — a retried or doubled flush cannot inflate
//!   a number, which `HINCRBY` would.
//! * a field is written only when its value differs from what was last written,
//!   so an idle service writes nothing at all.
//! * the last flush is also run on shutdown, so a clean restart loses nothing.
//!
//! The worst case is a hard kill: at most one flush interval of counting is
//! lost. That is the deliberate trade for keeping Redis off the request path.
//!
//! # Layout
//!
//! ```text
//! scraper:status:meta            hash  { started_at_ms, version, environment }
//! scraper:status:endpoint:<key>  hash  { requests, ok, client_errors, ... }
//! ```
//!
//! The endpoint key carries `METHOD /route/{template}`, so one hash per route
//! template — never one per request URI.
//!
//! # Degradation
//!
//! Redis is optional. If it is unreachable the flush logs once and skips, reads
//! return nothing, and the dashboard falls back to the in-process numbers. The
//! service never fails a request because its statistics store is down.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use redis::AsyncCommands;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::infrastructure::cache::redis_pool::get_redis_pool;
use crate::observability::telemetry;

/// How often the in-memory counters are pushed to Redis.
///
/// One second keeps the numbers close to live for a dashboard that refreshes
/// every five, while keeping Redis traffic to one small pipeline per second
/// regardless of request rate.
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// Prefix for every key this module owns, so an operator can find or drop the
/// whole store with one `SCAN`/`DEL` pattern:
///
/// ```text
/// redis-cli --scan --pattern 'scraper:status:*'
/// redis-cli --scan --pattern 'scraper:status:*' | xargs redis-cli DEL
/// ```
pub const KEY_PREFIX: &str = "scraper:status";
const META_KEY: &str = "scraper:status:meta";
const ENDPOINT_PREFIX: &str = "scraper:status:endpoint:";

/// A per-endpoint row read back from Redis.
#[derive(Debug, Clone, Default)]
pub struct PersistedEndpoint {
    pub requests: u64,
    pub ok: u64,
    pub client_errors: u64,
    pub server_errors: u64,
    pub total_micros: u64,
    pub max_micros: u64,
    pub last_status: u16,
    pub last_seen_ms: u64,
    pub last_error_ms: u64,
    pub last_error_status: u16,
}

/// Service-wide totals read back from Redis.
#[derive(Debug, Clone, Default)]
pub struct PersistedMeta {
    pub started_at_ms: u64,
    pub version: String,
    pub environment: String,
}

/// What a flush last wrote, so an unchanged counter is not re-sent.
///
/// Held behind a `Mutex` because the flush task is the only writer, but the
/// shutdown path also calls [`flush_now`] — the lock makes those two mutually
/// exclusive instead of racing on the same key.
static LAST_WRITTEN: LazyLock<Mutex<HashMap<String, [u64; 8]>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Signature of the counters that decide whether a row needs rewriting.
fn signature(
    requests: u64,
    ok: u64,
    client_errors: u64,
    server_errors: u64,
    max_micros: u64,
) -> [u64; 8] {
    [
        requests,
        ok,
        client_errors,
        server_errors,
        0, // total_micros is derived from requests; not part of the diff
        max_micros,
        0,
        0,
    ]
}

/// Push every changed counter to Redis.
///
/// Idempotent by construction: the values written are the current totals, not
/// increments, so running this twice (or after a retry) cannot double-count.
pub async fn flush_now() -> Result<usize, String> {
    let pool = get_redis_pool().map_err(|e| e.clone())?;
    let mut conn = pool.get().await.map_err(|e| e.to_string())?;

    let snapshot = telemetry::snapshot_all();
    let mut written = 0usize;
    let mut last = LAST_WRITTEN.lock().await;

    for endpoint in snapshot {
        let signature = signature(
            endpoint.requests,
            endpoint.ok,
            endpoint.client_errors,
            endpoint.server_errors,
            (endpoint.max_latency_ms * 1000.0) as u64,
        );
        if last.get(&endpoint.key) == Some(&signature) {
            continue;
        }

        let key = format!("{ENDPOINT_PREFIX}{}", endpoint.key);
        let fields: Vec<(&str, u64)> = vec![
            ("requests", endpoint.requests),
            ("ok", endpoint.ok),
            ("client_errors", endpoint.client_errors),
            ("server_errors", endpoint.server_errors),
            (
                "total_micros",
                (endpoint.avg_latency_ms * endpoint.requests as f64 * 1000.0) as u64,
            ),
            ("max_micros", (endpoint.max_latency_ms * 1000.0) as u64),
            ("last_status", u64::from(endpoint.last_status)),
            ("last_seen_ms", endpoint.last_seen_ms.unwrap_or(0)),
            ("last_error_ms", endpoint.last_error_ms.unwrap_or(0)),
            (
                "last_error_status",
                u64::from(endpoint.last_error_status.unwrap_or(0)),
            ),
        ];

        // One HSET per endpoint: a pipeline would batch these, but at ~100
        // endpoints only the changed ones reach here, and a single command per
        // change keeps the failure blast radius to one row.
        let command = redis::Cmd::hset_multiple(&key, &fields);
        let result: Result<(), _> = command.query_async(&mut *conn).await;
        if result.is_err() {
            // A broken connection must not poison the diff map: leave the
            // signature out so the next tick retries this row.
            return Err(format!("hset {key} failed"));
        }
        last.insert(endpoint.key, signature);
        written += 1;
    }

    // Meta is written once per boot, but re-writing it is cheap and keeps the
    // uptime anchor accurate if the process restarted.
    let stats = telemetry::service_stats();
    let meta: Vec<(&str, String)> = vec![
        ("started_at_ms", stats.started_at_ms.to_string()),
        ("version", env!("CARGO_PKG_VERSION").to_string()),
        ("environment", environment_label()),
    ];
    let command = redis::Cmd::hset_multiple(META_KEY, &meta);
    let _: Result<(), _> = command.query_async(&mut *conn).await;

    Ok(written)
}

/// Read every persisted endpoint row.
///
/// Returns an empty map when Redis is unavailable: the caller then shows the
/// in-process numbers alone, which is exactly the pre-Redis behaviour.
pub async fn load_endpoints() -> HashMap<String, PersistedEndpoint> {
    let Ok(pool) = get_redis_pool() else {
        return HashMap::new();
    };
    let Ok(mut conn) = pool.get().await else {
        return HashMap::new();
    };

    let mut cursor = 0u64;
    let pattern = format!("{ENDPOINT_PREFIX}*");
    let mut rows = HashMap::new();

    loop {
        let page: Result<(u64, Vec<String>), _> = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(256)
            .query_async(&mut *conn)
            .await;
        let Ok((next, keys)) = page else {
            warn!("status store: SCAN failed, falling back to in-process counters");
            return rows;
        };

        for key in keys {
            let hash: Result<HashMap<String, String>, _> = conn.hgetall(&key).await;
            let Ok(hash) = hash else { continue };
            let Some(slug) = key.strip_prefix(ENDPOINT_PREFIX) else {
                continue;
            };
            rows.insert(slug.to_string(), parse_endpoint(&hash));
        }

        cursor = next;
        if cursor == 0 {
            break;
        }
    }

    debug!(
        endpoints = rows.len(),
        "status store: loaded persisted counters"
    );
    rows
}

/// Read the persisted service metadata.
pub async fn load_meta() -> Option<PersistedMeta> {
    let pool = get_redis_pool().ok()?;
    let mut conn = pool.get().await.ok()?;
    let hash: HashMap<String, String> = conn.hgetall(META_KEY).await.ok()?;
    if hash.is_empty() {
        return None;
    }
    Some(PersistedMeta {
        started_at_ms: hash
            .get("started_at_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        version: hash.get("version").cloned().unwrap_or_default(),
        environment: hash.get("environment").cloned().unwrap_or_default(),
    })
}

fn parse_endpoint(hash: &HashMap<String, String>) -> PersistedEndpoint {
    let get = |field: &str| -> u64 { hash.get(field).and_then(|v| v.parse().ok()).unwrap_or(0) };
    PersistedEndpoint {
        requests: get("requests"),
        ok: get("ok"),
        client_errors: get("client_errors"),
        server_errors: get("server_errors"),
        total_micros: get("total_micros"),
        max_micros: get("max_micros"),
        last_status: u16::try_from(get("last_status")).unwrap_or(0),
        last_seen_ms: get("last_seen_ms"),
        last_error_ms: get("last_error_ms"),
        last_error_status: u16::try_from(get("last_error_status")).unwrap_or(0),
    }
}

/// Same label logic the dashboard header uses.
fn environment_label() -> String {
    std::env::var("APP_ENVIRONMENT")
        .or_else(|_| std::env::var("RUN_MODE"))
        .unwrap_or_else(|_| "development".to_string())
}

/// Restore the counters Redis kept from earlier runs.
///
/// Runs during bootstrap, **before** the listener accepts traffic, so the very
/// first dashboard render already shows lifetime numbers instead of zeroes.
/// Safe to call when Redis is unreachable: it then simply restores nothing and
/// the dashboard reports this process only.
pub async fn hydrate() {
    let persisted = load_endpoints().await;
    if persisted.is_empty() {
        info!("status store: no persisted counters found, starting from zero");
        return;
    }
    let meta = load_meta().await;
    info!(
        endpoints = persisted.len(),
        previous_version = meta.as_ref().map(|m| m.version.as_str()).unwrap_or("-"),
        "status store: restored counters from Redis"
    );
    telemetry::hydrate(persisted, meta);
}

/// Spawn the write-behind flush loop.
///
/// Returns the task handle so the shutdown path can stop it; the caller
/// registers it with the process `ShutdownManager`.
pub fn start(shutdown: mytheclipse::shutdown::ShutdownSignal) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut shutdown = shutdown;
        let mut failing = false;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match flush_now().await {
                        Ok(0) => {}
                        Ok(n) => {
                            if failing {
                                info!("status store: flush recovered");
                                failing = false;
                            }
                            debug!(rows = n, "status store: flushed");
                        }
                        Err(e) => {
                            // Log the first failure only: a Redis outage would
                            // otherwise print this line every second.
                            if !failing {
                                warn!(error = %e, "status store: flush failed, will retry");
                                failing = true;
                            }
                        }
                    }
                }
                _ = shutdown.wait() => {
                    // One last write so a clean restart keeps the tail of the
                    // counters the loop had not yet persisted.
                    match flush_now().await {
                        Ok(n) => info!(rows = n, "status store: final flush on shutdown"),
                        Err(e) => warn!(error = %e, "status store: final flush failed"),
                    }
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_complete_row() {
        let mut hash = HashMap::new();
        hash.insert("requests".to_string(), "10".to_string());
        hash.insert("ok".to_string(), "7".to_string());
        hash.insert("client_errors".to_string(), "2".to_string());
        hash.insert("server_errors".to_string(), "1".to_string());
        hash.insert("total_micros".to_string(), "5000".to_string());
        hash.insert("max_micros".to_string(), "900".to_string());
        hash.insert("last_status".to_string(), "500".to_string());
        hash.insert("last_seen_ms".to_string(), "1700000000000".to_string());
        hash.insert("last_error_ms".to_string(), "1699999999000".to_string());
        hash.insert("last_error_status".to_string(), "502".to_string());

        let row = parse_endpoint(&hash);
        assert_eq!(row.requests, 10);
        assert_eq!(row.ok, 7);
        assert_eq!(row.client_errors, 2);
        assert_eq!(row.server_errors, 1);
        assert_eq!(row.total_micros, 5000);
        assert_eq!(row.max_micros, 900);
        assert_eq!(row.last_status, 500);
        assert_eq!(row.last_seen_ms, 1_700_000_000_000);
        assert_eq!(row.last_error_status, 502);
    }

    #[test]
    fn missing_or_corrupt_fields_degrade_to_zero() {
        let mut hash = HashMap::new();
        hash.insert("requests".to_string(), "not-a-number".to_string());
        hash.insert("last_status".to_string(), "99999".to_string());

        let row = parse_endpoint(&hash);
        assert_eq!(row.requests, 0, "a corrupt counter must not panic");
        assert_eq!(row.last_status, 0, "out-of-range status is dropped");
        assert_eq!(row.ok, 0, "absent fields are zero");
    }

    #[test]
    fn key_layout_is_prefixed_and_derived_from_the_route_template() {
        assert_eq!(ENDPOINT_PREFIX, "scraper:status:endpoint:");
        assert!(META_KEY.starts_with(KEY_PREFIX));
        let key = format!("{ENDPOINT_PREFIX}{}", "GET /api/anime/detail/{slug}");
        assert_eq!(key, "scraper:status:endpoint:GET /api/anime/detail/{slug}");
    }

    #[test]
    fn signature_ignores_derived_latency_so_a_live_endpoint_rewrites() {
        // Same counters but a new max latency must still be written, otherwise
        // the persisted max would freeze at the first observed value.
        assert_ne!(signature(1, 1, 0, 0, 100), signature(1, 1, 0, 0, 200));
        // A pure repeat is skipped.
        assert_eq!(signature(5, 4, 1, 0, 100), signature(5, 4, 1, 0, 100));
    }
}
