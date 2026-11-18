//! Per-endpoint telemetry backing the public status dashboard.
//!
//! The OTLP pipeline in [`super::metrics`] answers "what is the fleet doing" for
//! a metrics backend, but it is *off* unless `OTEL_EXPORTER_OTLP_ENDPOINT` is
//! set and it aggregates nothing locally — so the service itself could not
//! answer "which endpoints are alive right now". This module keeps that answer
//! in-process.
//!
//! Every request that reaches a real route is folded into a lock-free counter
//! set keyed by `METHOD /matched/path`. The key is the route *template*, not the
//! request URI: `/api/anime/detail/{slug}` is one series, not one per slug, so
//! the registry cannot grow with traffic. Recording costs a `DashMap` lookup and
//! a handful of relaxed atomic adds; nothing allocates on the request path after
//! the key is built.
//!
//! The dashboard's own plumbing (`/`, `/api/status`, `/docs*`, `/favicon.*`) is
//! skipped: it polls itself every few seconds and would otherwise dominate its
//! own traffic figures.
//!
//! # Lifetime totals
//!
//! Counters are seeded at boot from [`super::status_store`], which persists them
//! to Redis every second. Hydration is additive: the values restored from Redis
//! become the starting point and this process adds to them, so the dashboard
//! shows lifetime numbers across restarts while the request path never touches
//! the network.

use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use dashmap::DashMap;
use serde::Serialize;

/// Throughput is a 60-slot ring of one-second buckets.
const WINDOW_SLOTS: usize = 60;

/// Suffix used for requests that matched no route at all (404s).
const UNMATCHED_PATH: &str = "(unmatched)";

/// Process start, touched by [`init`] so uptime is measured from boot rather
/// than from the first request.
static STARTED_AT: LazyLock<Instant> = LazyLock::new(Instant::now);
static STARTED_AT_MS: LazyLock<u64> = LazyLock::new(unix_ms);

/// One counter set per route template.
static ENDPOINTS: LazyLock<DashMap<String, Arc<EndpointMetrics>>> = LazyLock::new(DashMap::new);

/// Where the counters came from, for the dashboard footer.
///
/// `memory` means Redis was unreachable or empty at boot and the numbers are
/// this process only; `redis` means they include history from earlier runs.
static PERSISTENCE: LazyLock<Mutex<&'static str>> = LazyLock::new(|| Mutex::new("memory"));

/// Ring of `(second, count)` buckets for the "requests in the last minute" tile.
static WINDOW_SECS: [AtomicU64; WINDOW_SLOTS] = [const { AtomicU64::new(0) }; WINDOW_SLOTS];
static WINDOW_COUNTS: [AtomicU64; WINDOW_SLOTS] = [const { AtomicU64::new(0) }; WINDOW_SLOTS];

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn unix_secs() -> u64 {
    unix_ms() / 1000
}

/// Current wall-clock time in milliseconds since the Unix epoch.
///
/// Exposed so the dashboard can stamp its snapshots with the same clock the
/// counters use.
pub fn now_secs() -> u64 {
    unix_secs()
}

pub fn now_ms() -> u64 {
    unix_ms()
}

/// Anchor uptime to process start. Call once during bootstrap.
pub fn init() {
    LazyLock::force(&STARTED_AT);
    LazyLock::force(&STARTED_AT_MS);
}

/// Seed the registry from the counters Redis kept from earlier runs.
///
/// Called once at boot, before the listener accepts traffic. Values are added to
/// whatever this process has already counted, so the dashboard reports lifetime
/// totals and never loses the tail of a run that ended between flushes.
///
/// `meta` only decides the label shown to the operator — a missing record still
/// hydrates, because the endpoint rows are the part that matters.
pub fn hydrate(
    persisted: std::collections::HashMap<String, super::status_store::PersistedEndpoint>,
    meta: Option<super::status_store::PersistedMeta>,
) {
    let restored = persisted.len();
    for (key, row) in persisted {
        entry_for(&key).seed(&row);
    }

    let label = if restored > 0 { "redis" } else { "memory" };
    // `try_lock` is enough: hydration runs before the server starts, so there is
    // no contention. If it somehow fails, the default label is the safe one.
    if let Ok(mut slot) = PERSISTENCE.try_lock() {
        *slot = label;
    }

    tracing::info!(
        endpoints = restored,
        started_at_ms = meta.as_ref().map(|m| m.started_at_ms).unwrap_or(0),
        previous_version = meta.as_ref().map(|m| m.version.as_str()).unwrap_or("-"),
        "telemetry: hydrated from persistent store"
    );
}

/// Which store the counters are backed by: `redis` or `memory`.
pub fn persistence() -> &'static str {
    PERSISTENCE.try_lock().map(|slot| *slot).unwrap_or("memory")
}

/// How long a stored observation stays authoritative.
///
/// Counters now survive restarts, so "the last call failed" can be days old. A
/// monitor that keeps reporting `error` for an endpoint nothing has touched
/// since is worse than useless — it trains the operator to ignore the page. Past
/// this age the verdict becomes [`Health::Stale`]: we do not know, and we say so
/// rather than guessing. Override with `STATUS_STALE_AFTER_SECS`.
const DEFAULT_STALE_AFTER_SECS: u64 = 3600;

fn stale_after_ms() -> u64 {
    static CACHED: LazyLock<u64> = LazyLock::new(|| {
        std::env::var("STATUS_STALE_AFTER_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|secs| *secs > 0)
            .unwrap_or(DEFAULT_STALE_AFTER_SECS)
            * 1000
    });
    *CACHED
}

/// Health verdict shown on the dashboard.
///
/// The rule is deliberately blunt, because the only signals available here are
/// the status codes the service itself produced:
///
/// * `idle`  — the route has never been called since the store was created.
/// * `error` — the last call returned 5xx, or at least half of all calls did
///   (a flapping upstream: one 200 does not hide a broken endpoint).
/// * `warn`  — the last call returned 4xx: the route is alive but the request
///   was rejected (usually a missing query parameter).
/// * `ok`    — the last call succeeded.
/// * `stale` — the last observation is older than [`DEFAULT_STALE_AFTER_SECS`],
///   so none of the above can be asserted any more. The row still shows the
///   historical error rate and status code; only the live verdict is withheld.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Ok,
    Warn,
    Error,
    Idle,
    Stale,
}

/// Mutable counters for one route template.
///
/// Every field is an atomic, so a request never waits on the registry and a
/// snapshot never blocks a request.
#[derive(Debug, Default)]
pub struct EndpointMetrics {
    requests: AtomicU64,
    ok: AtomicU64,
    client_errors: AtomicU64,
    server_errors: AtomicU64,
    in_flight: AtomicU64,
    total_micros: AtomicU64,
    max_micros: AtomicU64,
    last_micros: AtomicU64,
    last_status: AtomicU16,
    last_seen_ms: AtomicU64,
    last_error_ms: AtomicU64,
    last_error_status: AtomicU16,
}

impl EndpointMetrics {
    /// A request entered the handler. Paired with [`Self::finish`].
    pub fn start(&self) {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
    }

    /// A request left the handler with `status` after `micros` microseconds.
    pub fn finish(&self, status: u16, micros: u64) {
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        self.requests.fetch_add(1, Ordering::Relaxed);
        match status {
            500..=599 => {
                self.server_errors.fetch_add(1, Ordering::Relaxed);
                self.last_error_ms.store(unix_ms(), Ordering::Relaxed);
                self.last_error_status.store(status, Ordering::Relaxed);
            }
            400..=499 => {
                self.client_errors.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.ok.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.total_micros.fetch_add(micros, Ordering::Relaxed);
        self.max_micros.fetch_max(micros, Ordering::Relaxed);
        self.last_micros.store(micros, Ordering::Relaxed);
        self.last_status.store(status, Ordering::Relaxed);
        self.last_seen_ms.store(unix_ms(), Ordering::Relaxed);
    }

    /// Fold counters restored from the persistent store into this set.
    ///
    /// Additive rather than replacing: a request may already have been recorded
    /// between process start and hydration, and `fetch_add` keeps both. The
    /// latencies are restored as totals so the average stays a lifetime average
    /// instead of restarting from this process's first request.
    fn seed(&self, row: &super::status_store::PersistedEndpoint) {
        self.requests.fetch_add(row.requests, Ordering::Relaxed);
        self.ok.fetch_add(row.ok, Ordering::Relaxed);
        self.client_errors
            .fetch_add(row.client_errors, Ordering::Relaxed);
        self.server_errors
            .fetch_add(row.server_errors, Ordering::Relaxed);
        self.total_micros
            .fetch_add(row.total_micros, Ordering::Relaxed);
        self.max_micros.fetch_max(row.max_micros, Ordering::Relaxed);
        if row.last_status != 0 {
            self.last_status.store(row.last_status, Ordering::Relaxed);
        }
        self.last_seen_ms.store(row.last_seen_ms, Ordering::Relaxed);
        if row.last_error_ms != 0 {
            self.last_error_ms
                .store(row.last_error_ms, Ordering::Relaxed);
        }
        if row.last_error_status != 0 {
            self.last_error_status
                .store(row.last_error_status, Ordering::Relaxed);
        }
    }

    /// Point-in-time copy of the counters.
    pub fn snapshot(&self, key: &str) -> EndpointSnapshot {
        let requests = self.requests.load(Ordering::Relaxed);
        let ok = self.ok.load(Ordering::Relaxed);
        let client_errors = self.client_errors.load(Ordering::Relaxed);
        let server_errors = self.server_errors.load(Ordering::Relaxed);
        let total_micros = self.total_micros.load(Ordering::Relaxed);
        let last_status = self.last_status.load(Ordering::Relaxed);
        let last_seen = self.last_seen_ms.load(Ordering::Relaxed);
        let last_error = self.last_error_ms.load(Ordering::Relaxed);
        let last_error_status = self.last_error_status.load(Ordering::Relaxed);

        let error_rate = if requests == 0 {
            0.0
        } else {
            server_errors as f64 / requests as f64
        };
        let avg_micros = if requests == 0 {
            0.0
        } else {
            total_micros as f64 / requests as f64
        };

        // A verdict is only asserted while the observation is fresh. Staleness
        // is checked first: an old 500 must not outrank a newer, healthy state
        // that this process simply has not observed yet.
        let age_ms = unix_ms().saturating_sub(last_seen);
        let is_stale = last_seen > 0 && age_ms > stale_after_ms();

        let health = if requests == 0 {
            Health::Idle
        } else if is_stale {
            Health::Stale
        } else if last_status >= 500 || (requests >= 4 && server_errors * 2 >= requests) {
            Health::Error
        } else if last_status >= 400 {
            Health::Warn
        } else {
            Health::Ok
        };

        EndpointSnapshot {
            key: key.to_owned(),
            requests,
            ok,
            client_errors,
            server_errors,
            in_flight: self.in_flight.load(Ordering::Relaxed),
            error_rate,
            avg_latency_ms: avg_micros / 1000.0,
            max_latency_ms: self.max_micros.load(Ordering::Relaxed) as f64 / 1000.0,
            last_latency_ms: self.last_micros.load(Ordering::Relaxed) as f64 / 1000.0,
            last_status,
            last_seen_ms: (last_seen > 0).then_some(last_seen),
            last_error_ms: (last_error > 0).then_some(last_error),
            last_error_status: (last_error_status > 0).then_some(last_error_status),
            health,
        }
    }
}

/// Frozen view of one route template, ready to serialize or render.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointSnapshot {
    /// `METHOD /route/{template}` — matches the dashboard row key.
    pub key: String,
    pub requests: u64,
    pub ok: u64,
    pub client_errors: u64,
    pub server_errors: u64,
    pub in_flight: u64,
    /// Fraction of requests that ended in 5xx, `0.0..=1.0`.
    pub error_rate: f64,
    pub avg_latency_ms: f64,
    pub max_latency_ms: f64,
    pub last_latency_ms: f64,
    pub last_status: u16,
    pub last_seen_ms: Option<u64>,
    pub last_error_ms: Option<u64>,
    pub last_error_status: Option<u16>,
    pub health: Health,
}

/// Service-wide totals, aggregated over every route template.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceStats {
    pub uptime_secs: u64,
    pub started_at_ms: u64,
    pub requests_total: u64,
    pub requests_last_minute: u64,
    pub in_flight: u64,
    pub ok: u64,
    pub client_errors: u64,
    pub server_errors: u64,
    /// Fraction of requests that ended in 5xx, `0.0..=1.0`.
    pub error_rate: f64,
}

/// Resolve (or create) the counter set for `key`.
fn entry_for(key: &str) -> Arc<EndpointMetrics> {
    if let Some(existing) = ENDPOINTS.get(key) {
        return Arc::clone(&existing);
    }
    let fresh = Arc::new(EndpointMetrics::default());
    Arc::clone(ENDPOINTS.entry(key.to_owned()).or_insert(fresh).value())
}

/// Route templates the dashboard itself serves and polls.
///
/// Recording them would make the monitor's own five-second polling the busiest
/// "endpoint" in the service and drown out real traffic. `/health` is
/// deliberately *not* in this set: it is a real public endpoint and belongs in
/// the monitor. Shared with the dashboard handler so both sides agree on
/// exactly which routes are invisible.
pub fn is_dashboard_path(path: &str) -> bool {
    path == "/"
        || path == "/api/status"
        || path.starts_with("/docs")
        || path == "/favicon.svg"
        || path == "/favicon.ico"
}

/// Count one request in the last-minute throughput ring.
fn record_throughput(sec: u64) {
    let idx = (sec % WINDOW_SLOTS as u64) as usize;
    if WINDOW_SECS[idx].load(Ordering::Relaxed) != sec {
        // A stale slot: overwrite the second first so a concurrent reader sees
        // the old count against the old second, never the new second with it.
        WINDOW_SECS[idx].store(sec, Ordering::Relaxed);
        WINDOW_COUNTS[idx].store(1, Ordering::Relaxed);
    } else {
        WINDOW_COUNTS[idx].fetch_add(1, Ordering::Relaxed);
    }
}

/// Requests recorded in the trailing 60 seconds.
fn throughput_last_minute(now: u64) -> u64 {
    let mut total = 0;
    for idx in 0..WINDOW_SLOTS {
        let sec = WINDOW_SECS[idx].load(Ordering::Relaxed);
        if sec != 0 && now.saturating_sub(sec) < WINDOW_SLOTS as u64 {
            total += WINDOW_COUNTS[idx].load(Ordering::Relaxed);
        }
    }
    total
}

/// Per-second request counts for the last [`WINDOW_SLOTS`] seconds, oldest
/// first, as `(unix_second, count)`.
///
/// Empty seconds are reported as `0` rather than omitted: the chart needs a
/// dense series so a quiet minute draws a flat line instead of a misleading
/// straight segment between two distant samples.
pub fn throughput_series(now: u64) -> Vec<(u64, u64)> {
    let mut series = Vec::with_capacity(WINDOW_SLOTS);
    // Walk the window oldest → newest so the caller can plot left to right.
    for offset in (0..WINDOW_SLOTS as u64).rev() {
        let sec = now.saturating_sub(offset);
        let idx = (sec % WINDOW_SLOTS as u64) as usize;
        let stored = WINDOW_SECS[idx].load(Ordering::Relaxed);
        let count = if stored == sec {
            WINDOW_COUNTS[idx].load(Ordering::Relaxed)
        } else {
            0
        };
        series.push((sec, count));
    }
    series
}

/// Snapshot of every route template that has seen traffic.
pub fn snapshot_all() -> Vec<EndpointSnapshot> {
    ENDPOINTS
        .iter()
        .map(|entry| entry.value().snapshot(entry.key()))
        .collect()
}

/// Snapshot of one route template, if it has seen traffic.
pub fn snapshot_of(key: &str) -> Option<EndpointSnapshot> {
    ENDPOINTS
        .get(key)
        .map(|entry| entry.value().snapshot(entry.key()))
}

/// Aggregate totals across every route template.
pub fn service_stats() -> ServiceStats {
    let mut stats = ServiceStats {
        uptime_secs: STARTED_AT.elapsed().as_secs(),
        started_at_ms: *STARTED_AT_MS,
        requests_total: 0,
        requests_last_minute: 0,
        in_flight: 0,
        ok: 0,
        client_errors: 0,
        server_errors: 0,
        error_rate: 0.0,
    };

    for entry in ENDPOINTS.iter() {
        let metrics = entry.value();
        stats.requests_total += metrics.requests.load(Ordering::Relaxed);
        stats.ok += metrics.ok.load(Ordering::Relaxed);
        stats.client_errors += metrics.client_errors.load(Ordering::Relaxed);
        stats.server_errors += metrics.server_errors.load(Ordering::Relaxed);
        stats.in_flight += metrics.in_flight.load(Ordering::Relaxed);
    }

    stats.requests_last_minute = throughput_last_minute(unix_secs());
    stats.error_rate = if stats.requests_total == 0 {
        0.0
    } else {
        stats.server_errors as f64 / stats.requests_total as f64
    };
    stats
}

/// Axum middleware: time every request and fold it into the registry.
///
/// Applied to the whole router, so it must stay allocation-light on the happy
/// path — one key `String`, one `DashMap` probe, relaxed atomics.
pub async fn telemetry_middleware(req: Request, next: Next) -> Response {
    let method = req.method().as_str();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
        .unwrap_or_else(|| UNMATCHED_PATH.to_owned());

    if is_dashboard_path(&path) {
        return next.run(req).await;
    }

    let key = format!("{method} {path}");
    let metrics = entry_for(&key);

    metrics.start();
    let start = Instant::now();
    let response = next.run(req).await;
    let micros = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
    metrics.finish(response.status().as_u16(), micros);
    record_throughput(unix_secs());

    response
}
