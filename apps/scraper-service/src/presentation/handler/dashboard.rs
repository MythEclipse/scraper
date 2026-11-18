//! Public homepage and live status dashboard.
//!
//! Two routes, one data model:
//!
//! * `GET /`           — server-rendered HTML: service overview, public
//!   statistics, and a per-endpoint monitor.
//! * `GET /api/status` — the same snapshot as JSON, which the page polls to
//!   stay live and which any external uptime checker can consume.
//!
//! The endpoint catalog is *derived from the OpenAPI document* rather than
//! hard-coded, so a handler that is documented but never called still shows up
//! as `idle`, and a new endpoint appears on the homepage the moment it is added
//! to `openapi_modules.rs`. Live counters come from
//! [`crate::observability::telemetry`].

use std::sync::LazyLock;

use axum::Json;
use axum::Router;
use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use serde::Serialize;

use crate::observability::openapi;
use crate::observability::telemetry::{self, Health, ServiceStats};

/// Same set the telemetry middleware skips, from the one place that defines it.
use crate::observability::telemetry::is_dashboard_path;

// ============================================================================
// Snapshot model
// ============================================================================

/// Everything the page and the JSON endpoint expose.
#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub service: ServiceInfo,
    pub stats: ServiceStats,
    /// Worst health across every monitored endpoint.
    pub overall: Health,
    pub counts: HealthCounts,
    pub groups: Vec<EndpointGroup>,
    /// `true` when no endpoint has been called since the process started.
    pub cold_start: bool,
    /// Where the counters live: `redis` (survives restarts) or `memory`.
    pub persistence: &'static str,
    /// Per-second request counts for the trailing minute, oldest first. Drives
    /// the sparkline; dense so a quiet second plots as zero, not as a gap.
    pub throughput: Vec<ThroughputPoint>,
    pub generated_at_ms: u64,
}

/// One second of the trailing-minute throughput window.
#[derive(Debug, Clone, Serialize)]
pub struct ThroughputPoint {
    pub t: u64,
    pub requests: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub name: String,
    pub version: String,
    pub environment: String,
    pub uptime_secs: u64,
    pub started_at_ms: u64,
    /// Number of operations in the OpenAPI document.
    pub documented_operations: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct HealthCounts {
    pub total: usize,
    pub ok: usize,
    pub warn: usize,
    pub error: usize,
    pub idle: usize,
    /// Endpoints whose last observation is too old to judge.
    pub stale: usize,
}

impl HealthCounts {
    fn tally(&mut self, health: Health) {
        self.total += 1;
        match health {
            Health::Ok => self.ok += 1,
            Health::Warn => self.warn += 1,
            Health::Error => self.error += 1,
            Health::Idle => self.idle += 1,
            Health::Stale => self.stale += 1,
        }
    }

    /// Worst verdict across every endpoint, for the header badge.
    ///
    /// `stale` deliberately does not escalate: it means "unknown", and an
    /// unknown must not raise the same alarm as a known failure. It only wins
    /// when nothing has been observed at all.
    fn overall(&self) -> Health {
        if self.error > 0 {
            Health::Error
        } else if self.warn > 0 {
            Health::Warn
        } else if self.ok > 0 {
            Health::Ok
        } else if self.stale > 0 {
            Health::Stale
        } else {
            Health::Idle
        }
    }
}

/// Endpoints sharing an OpenAPI tag, in dashboard order.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointGroup {
    pub tag: String,
    pub description: String,
    pub endpoints: Vec<EndpointView>,
}

/// One documented operation plus its live counters.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointView {
    /// `METHOD /route/{template}` — also the telemetry registry key.
    pub key: String,
    pub method: String,
    pub path: String,
    pub summary: String,
    /// Ready-to-call path with path parameters filled in.
    pub sample: String,
    pub health: Health,
    pub requests: u64,
    pub ok: u64,
    pub client_errors: u64,
    pub server_errors: u64,
    pub in_flight: u64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
    pub max_latency_ms: f64,
    pub last_status: Option<u16>,
    pub last_seen_ms: Option<u64>,
    pub last_error_ms: Option<u64>,
    pub last_error_status: Option<u16>,
}

// ============================================================================
// Catalog, derived from the OpenAPI document
// ============================================================================

struct CatalogEntry {
    tag: String,
    method: &'static str,
    path: String,
    summary: String,
}

/// Dashboard ordering. Anything not listed is appended alphabetically, so a new
/// module still shows up without touching this list.
const TAG_ORDER: [&str; 10] = [
    "anime", "anime2", "komik", "download", "image", "misc", "stalk", "search", "tool", "service",
];

/// Tag → description, taken from the `tags(...)` block of the OpenAPI document.
static TAG_DESCRIPTIONS: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    openapi::combined()
        .tags
        .as_ref()
        .map(|tags| {
            tags.iter()
                .map(|tag| {
                    (
                        tag.name.clone(),
                        tag.description.clone().unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
});

/// Every documented operation, flattened out of the OpenAPI paths map.
static CATALOG: LazyLock<Vec<CatalogEntry>> = LazyLock::new(|| {
    let mut entries = Vec::new();
    for (path, item) in &openapi::combined().paths.paths {
        let operations = [
            ("GET", &item.get),
            ("PUT", &item.put),
            ("POST", &item.post),
            ("DELETE", &item.delete),
            ("PATCH", &item.patch),
        ];
        for (method, operation) in operations {
            let Some(operation) = operation else {
                continue;
            };
            entries.push(CatalogEntry {
                tag: operation
                    .tags
                    .as_ref()
                    .and_then(|tags| tags.first().cloned())
                    .unwrap_or_else(|| "other".to_string()),
                method,
                path: path.clone(),
                summary: operation
                    .summary
                    .clone()
                    .or_else(|| operation.description.clone())
                    .unwrap_or_default(),
            });
        }
    }
    entries
});

/// Fill path parameters with a plausible value so a row links somewhere real.
fn sample_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let param = &rest[open + 1..open + close];
        out.push_str(match param {
            "page" => "1",
            "slug" => "one-piece",
            "filename" => "merged.mp4",
            _ => "example",
        });
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

fn view_for(entry: &CatalogEntry) -> EndpointView {
    let key = format!("{} {}", entry.method, entry.path);
    let metrics = telemetry::snapshot_of(&key);

    let health = metrics.as_ref().map(|m| m.health).unwrap_or(Health::Idle);
    EndpointView {
        sample: sample_path(&entry.path),
        key,
        method: entry.method.to_string(),
        path: entry.path.clone(),
        summary: entry.summary.clone(),
        health,
        requests: metrics.as_ref().map_or(0, |m| m.requests),
        ok: metrics.as_ref().map_or(0, |m| m.ok),
        client_errors: metrics.as_ref().map_or(0, |m| m.client_errors),
        server_errors: metrics.as_ref().map_or(0, |m| m.server_errors),
        in_flight: metrics.as_ref().map_or(0, |m| m.in_flight),
        error_rate: metrics.as_ref().map_or(0.0, |m| m.error_rate),
        avg_latency_ms: metrics.as_ref().map_or(0.0, |m| m.avg_latency_ms),
        max_latency_ms: metrics.as_ref().map_or(0.0, |m| m.max_latency_ms),
        last_status: metrics.as_ref().map(|m| m.last_status).filter(|s| *s != 0),
        last_seen_ms: metrics.as_ref().and_then(|m| m.last_seen_ms),
        last_error_ms: metrics.as_ref().and_then(|m| m.last_error_ms),
        last_error_status: metrics.as_ref().and_then(|m| m.last_error_status),
    }
}

/// Build the complete snapshot: catalog + counters, grouped by tag.
pub fn snapshot() -> StatusResponse {
    let mut groups: Vec<EndpointGroup> = Vec::new();
    let mut counts = HealthCounts::default();
    let mut seen_keys: Vec<String> = Vec::with_capacity(CATALOG.len());

    for entry in CATALOG.iter() {
        // The dashboard's own plumbing is not part of the public surface it
        // reports on; listing it would also make it the busiest endpoint.
        if is_dashboard_path(&entry.path) {
            continue;
        }
        let view = view_for(entry);
        seen_keys.push(view.key.clone());

        counts.tally(view.health);

        match groups.iter_mut().find(|group| group.tag == entry.tag) {
            Some(group) => group.endpoints.push(view),
            None => groups.push(EndpointGroup {
                tag: entry.tag.clone(),
                description: tag_description(&entry.tag),
                endpoints: vec![view],
            }),
        }
    }

    // Traffic on routes the document does not describe (a 404 handler hit, or a
    // route added without a `#[utoipa::path]`). Surfaced rather than dropped:
    // silently ignoring them would hide real errors from the monitor.
    let undocumented: Vec<EndpointView> = telemetry::snapshot_all()
        .into_iter()
        .filter(|snap| !seen_keys.iter().any(|key| key == &snap.key))
        .map(|snap| EndpointView {
            key: snap.key.clone(),
            method: snap.key.split(' ').next().unwrap_or("GET").to_string(),
            path: snap
                .key
                .split_once(' ')
                .map(|(_, path)| path.to_string())
                .unwrap_or_else(|| snap.key.clone()),
            summary: "Not described in the OpenAPI document".to_string(),
            sample: String::new(),
            health: snap.health,
            requests: snap.requests,
            ok: snap.ok,
            client_errors: snap.client_errors,
            server_errors: snap.server_errors,
            in_flight: snap.in_flight,
            error_rate: snap.error_rate,
            avg_latency_ms: snap.avg_latency_ms,
            max_latency_ms: snap.max_latency_ms,
            last_status: Some(snap.last_status).filter(|s| *s != 0),
            last_seen_ms: snap.last_seen_ms,
            last_error_ms: snap.last_error_ms,
            last_error_status: snap.last_error_status,
        })
        .collect();

    if !undocumented.is_empty() {
        for view in &undocumented {
            counts.tally(view.health);
        }
        groups.push(EndpointGroup {
            tag: "other".to_string(),
            description: "Traffic on routes the OpenAPI document does not describe".to_string(),
            endpoints: undocumented,
        });
    }

    groups.sort_by_key(|group| {
        TAG_ORDER
            .iter()
            .position(|tag| *tag == group.tag)
            .unwrap_or(TAG_ORDER.len())
    });

    let overall = counts.overall();

    let stats = telemetry::service_stats();
    StatusResponse {
        service: ServiceInfo {
            name: "scraper".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: environment_label(),
            uptime_secs: stats.uptime_secs,
            started_at_ms: stats.started_at_ms,
            documented_operations: CATALOG.len(),
        },
        cold_start: stats.requests_total == 0,
        persistence: telemetry::persistence(),
        throughput: telemetry::throughput_series(telemetry::now_secs())
            .into_iter()
            .map(|(t, requests)| ThroughputPoint { t, requests })
            .collect(),
        stats,
        overall,
        counts,
        groups,
        generated_at_ms: telemetry::now_ms(),
    }
}

/// Deployment label shown in the page header.
///
/// Read straight from the environment instead of through [`crate::config::CONFIG`]:
/// that static is fail-fast by design and calls `process::exit` when a required
/// variable is missing, which is right for the server but wrong for a label
/// rendered by a test. `APP_ENVIRONMENT` is the same key `AppConfig` reads, and
/// `RUN_MODE` is the file-layer selector, so the label agrees with the config
/// in every real deployment.
fn environment_label() -> String {
    std::env::var("APP_ENVIRONMENT")
        .or_else(|_| std::env::var("RUN_MODE"))
        .unwrap_or_else(|_| "development".to_string())
}

fn tag_description(tag: &str) -> String {
    TAG_DESCRIPTIONS
        .iter()
        .find(|(name, _)| name == tag)
        .map(|(_, description)| description.clone())
        .unwrap_or_default()
}

// ============================================================================
// Handlers
// ============================================================================

/// `GET /api/status` — the live snapshot as JSON.
///
/// Public on purpose: an external uptime checker should be able to read the
/// same numbers the page shows without scraping HTML.
pub async fn status_json() -> Response {
    let mut response = Json(snapshot()).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

/// `GET /` — the public homepage and dashboard.
pub async fn homepage() -> Response {
    let mut response = Html(render_page(&snapshot())).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

/// The same `S` mark the page header draws, as an SVG.
const FAVICON_SVG: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">"##,
    r##"<rect width="64" height="64" rx="16" fill="#4f9cf9"/>"##,
    r##"<text x="32" y="45" font-family="system-ui,sans-serif" font-size="40" "##,
    r##"font-weight="800" fill="#ffffff" text-anchor="middle">S</text></svg>"##,
);

/// `GET /favicon.svg` — the icon declared by the page.
pub async fn favicon_svg() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        FAVICON_SVG,
    )
        .into_response()
}

/// `GET /favicon.ico` — what a browser asks for unprompted.
///
/// Serving the SVG here keeps the access log free of a 404 on every first visit;
/// browsers that ignore the type sniff the SVG fine, and the `<link>` tags in
/// the page already name the SVG explicitly for the ones that do not.
pub async fn favicon_ico() -> Response {
    favicon_svg().await
}

/// Routes served by this module. Mounted at the root by
/// [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/", get(homepage))
        .route("/api/status", get(status_json))
        .route("/favicon.svg", get(favicon_svg))
        .route("/favicon.ico", get(favicon_ico))
}

// ============================================================================
// HTML rendering
// ============================================================================

fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn health_class(health: Health) -> &'static str {
    match health {
        Health::Ok => "ok",
        Health::Warn => "warn",
        Health::Error => "error",
        Health::Idle => "idle",
        Health::Stale => "stale",
    }
}

fn health_label(health: Health) -> &'static str {
    match health {
        Health::Ok => "ok",
        Health::Warn => "warn",
        Health::Error => "error",
        Health::Idle => "idle",
        Health::Stale => "stale",
    }
}

fn fmt_duration(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;
    if days > 0 {
        format!("{days}h {hours}j {minutes}m")
    } else if hours > 0 {
        format!("{hours}j {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn fmt_percent(fraction: f64) -> String {
    format!("{:.2}%", fraction * 100.0)
}

fn render_page(snapshot: &StatusResponse) -> String {
    let mut html = String::with_capacity(96 * 1024);
    html.push_str(PAGE_HEAD);
    html.push_str(&render_hero(snapshot));
    html.push_str(&render_cards(snapshot));
    html.push_str(&render_monitor(snapshot));
    html.push_str(PAGE_FOOT);
    html
}

fn render_hero(snapshot: &StatusResponse) -> String {
    let overall = health_class(snapshot.overall);
    let status_text = match snapshot.overall {
        Health::Ok => "All endpoints healthy",
        Health::Warn => "Some endpoints are rejecting requests",
        Health::Error => "An endpoint is failing",
        Health::Idle => "Waiting for the first request",
        Health::Stale => "Last observation is old",
    };
    let env = escape_html(&snapshot.service.environment);
    let version = escape_html(&snapshot.service.version);
    // Computed once: `spark_paths` walks the whole series.
    let (spark_line, spark_area) = spark_paths(&snapshot.throughput);

    format!(
        r#"<header class="hero">
  <div class="hero-top">
    <div class="brand">
      <span class="logo">S</span>
      <div>
        <h1>Scraper API</h1>
        <p class="tagline">Scraping &amp; media download service — one API for anime, comics, social media, and utilities.</p>
      </div>
    </div>
    <div class="overall overall-{overall}" id="overall">
      <span class="dot"></span>
      <span id="overall-text">{status_text}</span>
    </div>
  </div>
  <div class="meta">
    <span class="chip">v{version}</span>
    <span class="chip">{env}</span>
    <span class="chip">uptime <b id="uptime">{uptime}</b></span>
    <span class="chip">{ops} documented endpoints</span>
    <span class="chip">stats <b id="persistence">{persistence_label}</b></span>
    <span class="chip chip-live">live · <b id="updated">just now</b></span>
  </div>
  <div class="pulse-panel">
    <div class="pulse-head">
      <span class="pulse-title">Traffic, last 60 seconds</span>
      <span class="pulse-now"><b id="pulse-peak">{peak}</b> peak/s · <b id="pulse-total">{pulse_total}</b> total</span>
    </div>
    <svg id="spark" viewBox="0 0 600 56" preserveAspectRatio="none" role="img"
         aria-label="Per-second traffic chart for the last 60 seconds">
      <defs>
        <linearGradient id="sparkGrad" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color='#5aa2ff' stop-opacity=".38" />
          <stop offset="100%" stop-color='#5aa2ff' stop-opacity="0" />
        </linearGradient>
      </defs>
      <path class="spark-area" id="spark-area" d="{spark_area}" />
      <path class="spark-line" id="spark-line" d="{spark_line}" />
    </svg>
  </div>
  <nav class="links">
    <a href="/docs">Swagger UI</a>
    <a href="/api-docs/openapi.json">OpenAPI JSON</a>
    <a href="/api/status">Status JSON</a>
    <a href="/health">Health</a>
  </nav>
</header>"#,
        uptime = fmt_duration(snapshot.service.uptime_secs),
        ops = snapshot.service.documented_operations,
        persistence_label = persistence_label(snapshot.persistence),
        peak = snapshot
            .throughput
            .iter()
            .map(|p| p.requests)
            .max()
            .unwrap_or(0),
        pulse_total = snapshot.throughput.iter().map(|p| p.requests).sum::<u64>(),
    )
}

/// Human wording for the storage backend.
///
/// `memory` is not an error — it is the honest label for a deployment with no
/// Redis, where the numbers restart with the process.
fn persistence_label(persistence: &str) -> &'static str {
    match persistence {
        "redis" => "persisted in Redis",
        _ => "in-process",
    }
}

fn render_cards(snapshot: &StatusResponse) -> String {
    let stats = &snapshot.stats;
    let counts = &snapshot.counts;
    // Pre-formatted so the markup below stays a single `format!` call with no
    // nested `format!` arguments.
    let err_detail = format!(
        "{} ok · {} 4xx · {} 5xx",
        stats.ok, stats.client_errors, stats.server_errors
    );
    let ep_detail = format!(
        "{} ok · {} warn · {} error · {} stale · {} idle",
        counts.ok, counts.warn, counts.error, counts.stale, counts.idle
    );
    format!(
        r#"<section class="cards">
  <article class="card" style="--card-accent: var(--accent)">
    <div class="card-head">
      <span class="card-label">Total requests</span>
      <span class="card-icon" aria-hidden="true">Σ</span>
    </div>
    <strong class="card-value" id="stat-total">{total}</strong>
    <span class="card-note">since process start</span>
  </article>
  <article class="card" style="--card-accent: var(--accent-2)">
    <div class="card-head">
      <span class="card-label">Requests / minute</span>
      <span class="card-icon" aria-hidden="true">⟳</span>
    </div>
    <strong class="card-value" id="stat-rpm">{rpm}</strong>
    <span class="card-note">last 60 seconds</span>
  </article>
  <article class="card" style="--card-accent: var(--ok)">
    <div class="card-head">
      <span class="card-label">In flight</span>
      <span class="card-icon" aria-hidden="true">◉</span>
    </div>
    <strong class="card-value" id="stat-inflight">{inflight}</strong>
    <span class="card-note">requests being handled</span>
  </article>
  <article class="card" style="--card-accent: var(--error)">
    <div class="card-head">
      <span class="card-label">Error rate</span>
      <span class="card-icon" aria-hidden="true">!</span>
    </div>
    <strong class="card-value" id="stat-errors">{errors}</strong>
    <span class="card-note" id="stat-error-detail">{err_detail}</span>
  </article>
  <article class="card" style="--card-accent: var(--warn)">
    <div class="card-head">
      <span class="card-label">Endpoints monitored</span>
      <span class="card-icon" aria-hidden="true">⌗</span>
    </div>
    <strong class="card-value" id="stat-endpoints">{endpoints}</strong>
    <span class="card-note" id="stat-endpoint-detail">{ep_detail}</span>
  </article>
</section>"#,
        total = stats.requests_total,
        rpm = stats.requests_last_minute,
        inflight = stats.in_flight,
        errors = fmt_percent(stats.error_rate),
        err_detail = err_detail,
        endpoints = counts.total,
        ep_detail = ep_detail,
    )
}

fn render_monitor(snapshot: &StatusResponse) -> String {
    let mut html = String::with_capacity(64 * 1024);
    html.push_str(
        r#"<section class="monitor">
  <div class="monitor-head">
    <div>
      <h2>Monitor per endpoint</h2>
      <p class="section-note">Health is derived from the last response and the 5xx ratio since process start.</p>
    </div>
    <div class="controls">
      <input id="search" type="search" placeholder="Search path or endpoint name…" autocomplete="off" />
      <div class="filters" id="filters">
        <button class="filter active" data-filter="all">All</button>
        <button class="filter" data-filter="error"><span class="fdot"></span>Error</button>
        <button class="filter" data-filter="warn"><span class="fdot"></span>Warn</button>
        <button class="filter" data-filter="stale"><span class="fdot"></span>Stale</button>
        <button class="filter" data-filter="idle"><span class="fdot"></span>Idle</button>
      </div>
    </div>
  </div>
"#,
    );

    // Rendered hidden when traffic already exists; the poller toggles it, so a
    // service that goes quiet does not keep a stale "no traffic yet" claim.
    html.push_str(&format!(
        r#"  <p class="cold-start" id="cold-start"{}>No requests recorded since process start — every endpoint shows <b>idle</b> until it is called.</p>
"#,
        if snapshot.cold_start { "" } else { " hidden" }
    ));

    for group in &snapshot.groups {
        html.push_str(&format!(
            r#"  <details class="group" open data-tag="{tag}">
    <summary>
      <span class="group-name">{tag}</span>
      <span class="group-desc">{desc}</span>
      <span class="group-badges" data-group-badges="{tag}">{badges}</span>
      <span class="group-count" data-group-count="{tag}">{n} endpoint</span>
    </summary>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th class="col-status">Status</th>
            <th class="col-method">Method</th>
            <th class="col-path">Path</th>
            <th class="col-num">Request</th>
            <th class="col-num">Error</th>
            <th class="col-num">Avg</th>
            <th class="col-num">Max</th>
            <th class="col-num">Last</th>
          </tr>
        </thead>
        <tbody data-tbody="{tag}">
"#,
            tag = escape_html(&group.tag),
            desc = escape_html(&group.description),
            n = group.endpoints.len(),
            badges = group_badges(&group.endpoints),
        ));
        for endpoint in &group.endpoints {
            html.push_str(&render_row(endpoint));
        }
        html.push_str("        </tbody>\n      </table>\n    </div>\n  </details>\n");
    }

    html.push_str("</section>\n");
    html
}

/// SVG path data for the throughput sparkline, as `(line, area)`.
///
/// Computed server-side so the panel is drawn on first paint instead of staying
/// blank until the first poll five seconds later. Mirrors `renderSpark` in the
/// page script, including the fixed 600×56 viewBox.
fn spark_paths(points: &[ThroughputPoint]) -> (String, String) {
    const W: f64 = 600.0;
    const H: f64 = 56.0;
    const PAD: f64 = 4.0;

    if points.is_empty() {
        return (String::new(), String::new());
    }

    let peak = points.iter().map(|p| p.requests).max().unwrap_or(0);
    // A flat-zero minute needs a visible baseline, not a divide by zero.
    let scale = if peak > 0 { peak as f64 } else { 1.0 };
    let step = if points.len() > 1 {
        W / (points.len() - 1) as f64
    } else {
        W
    };
    let usable = H - PAD * 2.0;

    let mut line = String::with_capacity(points.len() * 16);
    for (index, point) in points.iter().enumerate() {
        let x = index as f64 * step;
        let y = PAD + usable - (point.requests as f64 / scale) * usable;
        line.push_str(&format!(
            "{}{x:.1} {y:.1} ",
            if index == 0 { "M" } else { "L" }
        ));
    }
    let line = line.trim_end().to_string();
    let area = format!("{line} L{W:.0} {H:.0} L0 {H:.0} Z");
    (line, area)
}

/// A latency figure with a proportional bar underneath the number's own scale.
///
/// `slow_ms` is the threshold above which the bar turns amber: past that, the
/// endpoint is slow enough that the colour is the useful signal.
fn latency_cell(value_ms: f64, max_ms: f64) -> String {
    const SLOW_MS: f64 = 1500.0;
    let ratio = if max_ms > 0.0 {
        (value_ms / max_ms).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // 34px is the widest bar that still leaves room for the number on a
    // narrow column; the minimum keeps a sub-millisecond value visible.
    let width = (ratio * 34.0).max(2.0);
    let class = if value_ms >= SLOW_MS {
        "lat-bar is-slow"
    } else {
        "lat-bar"
    };
    format!(
        r#"<span class="lat"><span class="{class}" style="width:{width:.0}px"></span>{value_ms:.1}ms</span>"#
    )
}

/// Compact per-group health tally (`2 error`, `1 warn`) for the summary line.
///
/// Rendered server-side so a collapsed group still tells you it contains a
/// failure without expanding it; the poller rebuilds it from the JSON.
fn group_badges(endpoints: &[EndpointView]) -> String {
    let mut error = 0;
    let mut warn = 0;
    let mut stale = 0;
    for endpoint in endpoints {
        match endpoint.health {
            Health::Error => error += 1,
            Health::Warn => warn += 1,
            Health::Stale => stale += 1,
            _ => {}
        }
    }
    let mut out = String::new();
    for (count, class) in [(error, "error"), (warn, "warn"), (stale, "stale")] {
        if count > 0 {
            out.push_str(&format!(
                r#"<span class="gbadge gbadge-{class}">{count}</span>"#
            ));
        }
    }
    out
}

fn render_row(endpoint: &EndpointView) -> String {
    let health = endpoint.health;
    let class = health_class(health);
    let label = health_label(health);
    let last = match endpoint.last_status {
        Some(status) => format!("<span class=\"code code-{class}\">{status}</span>"),
        None => "<span class=\"muted\">—</span>".to_string(),
    };
    // Latency renders as a number plus a proportional bar, so the column reads
    // as a chart instead of a list of figures. The bar is relative to the
    // endpoint's own max: this is a within-row comparison, not a cross-row one,
    // which would need a page-wide scale to mean anything.
    let avg = if endpoint.requests == 0 {
        "—".to_string()
    } else {
        latency_cell(endpoint.avg_latency_ms, endpoint.max_latency_ms)
    };
    let max = if endpoint.requests == 0 {
        "—".to_string()
    } else {
        format!("{:.1}ms", endpoint.max_latency_ms)
    };
    let error = if endpoint.requests == 0 {
        "—".to_string()
    } else {
        fmt_percent(endpoint.error_rate)
    };

    format!(
        r#"          <tr data-key="{key}" data-health="{class}">
            <td><span class="pill pill-{class}"><span class="dot"></span>{label}</span></td>
            <td><span class="method">{method}</span></td>
            <td class="path-cell">
              <a class="path" href="{sample}">{path}</a>
              <span class="summary">{summary}</span>
            </td>
            <td class="num" data-field="requests">{requests}</td>
            <td class="num" data-field="error_rate">{error}</td>
            <td class="num" data-field="avg" data-avg-ms="{avg_ms}">{avg}</td>
            <td class="num" data-field="max">{max}</td>
            <td class="num" data-field="last">{last}</td>
          </tr>
"#,
        key = escape_html(&endpoint.key),
        method = escape_html(&endpoint.method),
        path = escape_html(&endpoint.path),
        sample = escape_html(&endpoint.sample),
        summary = escape_html(&endpoint.summary),
        requests = endpoint.requests,
        avg_ms = endpoint.avg_latency_ms,
    )
}

// ============================================================================
// Static assets
// ============================================================================

const PAGE_HEAD: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<meta name="color-scheme" content="dark light" />
<meta name="description" content="Scraper API — public statistics and per-endpoint status monitor." />
<title>Scraper API — Status &amp; Statistics</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml" />
<link rel="icon" href="/favicon.ico" sizes="any" />
<style>
:root {
  --bg: #070b11;
  --bg-glow: #12233a;
  --panel: #0f1620;
  --panel-2: #0b121a;
  --panel-3: #141d29;
  --border: #1d2836;
  --border-soft: #16202c;
  --text: #e8eff8;
  --muted: #8494a8;
  --faint: #5c6b7d;
  --accent: #5aa2ff;
  --accent-2: #8b5cf6;
  --ok: #34d399;
  --warn: #fbbf24;
  --error: #fb7185;
  --idle: #55647a;
  --stale: #a78bfa;
  --radius: 14px;
  --radius-sm: 10px;
  --mono: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  --shadow: 0 1px 2px rgba(0,0,0,.4), 0 8px 24px -12px rgba(0,0,0,.6);
}
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; }
html { scroll-behavior: smooth; }
body {
  background:
    radial-gradient(900px 520px at 12% -8%, rgba(90,162,255,.13) 0%, transparent 62%),
    radial-gradient(760px 460px at 88% 2%, rgba(139,92,246,.10) 0%, transparent 60%),
    var(--bg);
  color: var(--text);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, Roboto, sans-serif;
  line-height: 1.55;
  min-height: 100vh;
  padding: 28px clamp(14px, 4vw, 44px) 56px;
  -webkit-font-smoothing: antialiased;
}
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
.wrap, header.hero, section.cards, section.monitor, footer { max-width: 1240px; margin-inline: auto; }

/* ── hero ─────────────────────────────────────────────────────────── */
.hero {
  position: relative; margin-bottom: 22px; padding: 22px 24px;
  border: 1px solid var(--border); border-radius: 18px;
  background:
    linear-gradient(135deg, rgba(90,162,255,.07) 0%, transparent 45%),
    linear-gradient(180deg, var(--panel) 0%, var(--panel-2) 100%);
  box-shadow: var(--shadow); overflow: hidden;
}
/* A hairline of accent along the top edge: cheap, and it stops the panel
   from reading as a plain rectangle. */
.hero::before {
  content: ""; position: absolute; inset: 0 0 auto 0; height: 1px;
  background: linear-gradient(90deg, transparent, var(--accent), var(--accent-2), transparent);
  opacity: .7;
}
.hero-top { display: flex; flex-wrap: wrap; gap: 18px; align-items: flex-start; justify-content: space-between; }
.brand { display: flex; gap: 15px; align-items: center; }
.logo {
  width: 50px; height: 50px; border-radius: 15px; flex: 0 0 auto;
  display: grid; place-items: center; font-weight: 800; font-size: 23px;
  background: linear-gradient(140deg, #5aa2ff, #8b5cf6);
  color: #fff; box-shadow: 0 10px 28px -8px rgba(90,162,255,.6);
}
h1 { margin: 0; font-size: clamp(23px, 2.6vw, 29px); letter-spacing: -.025em; font-weight: 700; }
.tagline { margin: 3px 0 0; color: var(--muted); font-size: 13.5px; max-width: 62ch; }
.overall {
  display: inline-flex; align-items: center; gap: 9px;
  padding: 9px 15px; border-radius: 999px; font-weight: 600; font-size: 13.5px;
  border: 1px solid var(--border); background: var(--panel-2);
  white-space: nowrap;
}
.overall .dot { width: 8px; height: 8px; border-radius: 50%; background: currentColor; }
.overall-ok { color: var(--ok); border-color: rgba(52,211,153,.35); background: rgba(52,211,153,.08); }
.overall-warn { color: var(--warn); border-color: rgba(251,191,36,.35); background: rgba(251,191,36,.08); }
.overall-error { color: var(--error); border-color: rgba(251,113,133,.4); background: rgba(251,113,133,.09); }
.overall-idle { color: var(--muted); }
.overall-stale { color: var(--stale); border-color: rgba(167,139,250,.35); background: rgba(167,139,250,.08); }
.overall-ok .dot, .overall-warn .dot, .overall-error .dot { animation: pulse 2s ease-in-out infinite; }
@keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: .3; } }

.meta { display: flex; flex-wrap: wrap; gap: 7px; margin-top: 16px; }
.chip {
  font-size: 12px; color: var(--muted); background: rgba(20,29,41,.7);
  border: 1px solid var(--border-soft); border-radius: 999px; padding: 4px 11px;
  display: inline-flex; align-items: center; gap: 6px;
}
.chip b { color: var(--text); font-weight: 600; }
.chip-live::before {
  content: ""; width: 6px; height: 6px; border-radius: 50%;
  background: var(--ok); box-shadow: 0 0 0 3px rgba(52,211,153,.16);
  animation: pulse 2s ease-in-out infinite;
}
.links { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 16px; }
.links a {
  font-size: 12.5px; font-weight: 500; color: var(--muted);
  border: 1px solid var(--border-soft); border-radius: 8px; padding: 6px 12px;
  background: rgba(20,29,41,.5); transition: color .15s, border-color .15s, background .15s;
}
.links a:hover { color: var(--text); border-color: var(--accent); background: rgba(90,162,255,.1); text-decoration: none; }

/* ── throughput sparkline ─────────────────────────────────────────── */
.pulse-panel {
  margin-top: 18px; padding: 14px 16px 10px;
  border: 1px solid var(--border-soft); border-radius: var(--radius);
  background: rgba(9,14,20,.55);
}
.pulse-head { display: flex; flex-wrap: wrap; gap: 8px; align-items: baseline; justify-content: space-between; margin-bottom: 6px; }
.pulse-title { font-size: 11.5px; text-transform: uppercase; letter-spacing: .08em; color: var(--muted); font-weight: 600; }
.pulse-now { font-size: 12px; color: var(--faint); font-variant-numeric: tabular-nums; }
.pulse-now b { color: var(--accent); font-weight: 700; }
#spark { display: block; width: 100%; height: 56px; overflow: visible; }
.spark-area { fill: url(#sparkGrad); }
.spark-line { fill: none; stroke: var(--accent); stroke-width: 1.6; stroke-linejoin: round; stroke-linecap: round; }
.spark-empty { fill: var(--faint); font-size: 11px; }

/* ── cards ────────────────────────────────────────────────────────── */
.cards {
  display: grid; gap: 12px; margin-bottom: 26px;
  grid-template-columns: repeat(auto-fit, minmax(178px, 1fr));
}
.card {
  position: relative; overflow: hidden;
  background: linear-gradient(180deg, var(--panel) 0%, var(--panel-2) 100%);
  border: 1px solid var(--border); border-radius: var(--radius);
  padding: 15px 16px 14px; display: flex; flex-direction: column; gap: 3px;
  box-shadow: var(--shadow); transition: border-color .18s, transform .18s;
}
.card:hover { border-color: #2a3a4e; transform: translateY(-1px); }
/* Per-card accent stripe: each KPI gets its own colour so the row is not five
   identical grey boxes. */
.card::after {
  content: ""; position: absolute; inset: 0 auto 0 0; width: 2px;
  background: var(--card-accent, var(--accent)); opacity: .85;
}
.card-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.card-label { font-size: 11.5px; color: var(--muted); text-transform: uppercase; letter-spacing: .07em; font-weight: 600; }
.card-icon { font-size: 13px; opacity: .5; line-height: 1; }
.card-value { font-size: 27px; font-weight: 700; letter-spacing: -.03em; font-variant-numeric: tabular-nums; line-height: 1.15; }
.card-note { font-size: 11.5px; color: var(--faint); }
.card-note b { color: var(--muted); font-weight: 600; }

/* ── monitor ──────────────────────────────────────────────────────── */
.monitor-head { display: flex; flex-wrap: wrap; gap: 14px; align-items: flex-end; justify-content: space-between; margin-bottom: 14px; }
h2 { margin: 0; font-size: 18.5px; letter-spacing: -.015em; font-weight: 680; }
.section-note { margin: 3px 0 0; color: var(--muted); font-size: 12.5px; }
.controls { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
#search {
  background: var(--panel); border: 1px solid var(--border); color: var(--text);
  border-radius: var(--radius-sm); padding: 8px 13px; font-size: 13px; min-width: 230px;
  transition: border-color .15s, box-shadow .15s;
}
#search::placeholder { color: var(--faint); }
#search:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px rgba(90,162,255,.14); }
.filters { display: flex; gap: 5px; flex-wrap: wrap; }
.filter {
  background: var(--panel); border: 1px solid var(--border); color: var(--muted);
  border-radius: 8px; padding: 7px 11px; font-size: 12px; cursor: pointer; font-weight: 600;
  transition: color .15s, border-color .15s, background .15s;
  display: inline-flex; align-items: center; gap: 6px;
}
.filter:hover { color: var(--text); border-color: #2a3a4e; }
.filter.active { color: var(--text); border-color: var(--accent); background: rgba(90,162,255,.13); }
/* A coloured dot on each filter so the control maps to the pill colours. */
.filter .fdot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; opacity: .9; }
.filter[data-filter="all"] .fdot { display: none; }
.filter[data-filter="error"] .fdot { color: var(--error); }
.filter[data-filter="warn"] .fdot { color: var(--warn); }
.filter[data-filter="stale"] .fdot { color: var(--stale); }
.filter[data-filter="idle"] .fdot { color: var(--idle); }

.cold-start {
  background: rgba(251,191,36,.07); border: 1px solid rgba(251,191,36,.28);
  color: var(--warn); border-radius: var(--radius-sm); padding: 11px 15px;
  font-size: 13px; margin: 0 0 14px;
}
.group {
  border: 1px solid var(--border); border-radius: var(--radius); margin-bottom: 11px;
  background: var(--panel-2); overflow: hidden;
  box-shadow: 0 1px 2px rgba(0,0,0,.3);
}
.group > summary {
  cursor: pointer; padding: 12px 16px; display: flex; flex-wrap: wrap; gap: 9px;
  align-items: center; list-style: none; user-select: none;
  transition: background .15s;
}
.group > summary:hover { background: rgba(90,162,255,.045); }
.group > summary::-webkit-details-marker { display: none; }
.group > summary::before {
  content: "▸"; color: var(--faint); font-size: 10px; transition: transform .18s;
  display: inline-block;
}
.group[open] > summary::before { transform: rotate(90deg); }
.group-name {
  font-weight: 700; text-transform: uppercase; letter-spacing: .09em; font-size: 11.5px;
  color: var(--text);
}
.group-desc { color: var(--faint); font-size: 12px; flex: 1 1 auto; min-width: 0; }
.group-count {
  color: var(--muted); font-size: 11.5px; font-variant-numeric: tabular-nums;
  background: rgba(90,162,255,.08); border: 1px solid rgba(90,162,255,.16);
  border-radius: 999px; padding: 2px 9px; white-space: nowrap;
}
/* Live health dots per group, so a collapsed group still reports its state. */
.group-badges { display: inline-flex; gap: 5px; align-items: center; }
.gbadge {
  font-size: 10.5px; font-weight: 700; font-variant-numeric: tabular-nums;
  padding: 1px 6px; border-radius: 5px; border: 1px solid transparent;
}
.gbadge-error { color: var(--error); background: rgba(251,113,133,.12); border-color: rgba(251,113,133,.3); }
.gbadge-warn { color: var(--warn); background: rgba(251,191,36,.12); border-color: rgba(251,191,36,.3); }
.gbadge-stale { color: var(--stale); background: rgba(167,139,250,.12); border-color: rgba(167,139,250,.3); }

.table-wrap { overflow-x: auto; border-top: 1px solid var(--border-soft); }
table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
th {
  text-align: left; padding: 8px 14px; font-size: 10.5px; letter-spacing: .08em;
  text-transform: uppercase; color: var(--faint); font-weight: 700;
  background: var(--panel); position: sticky; top: 0; white-space: nowrap;
  border-bottom: 1px solid var(--border-soft);
}
td { padding: 8px 14px; border-top: 1px solid rgba(29,40,54,.55); vertical-align: middle; }
tbody tr { transition: background .12s; }
tbody tr:hover { background: rgba(90,162,255,.055); }
/* The left edge carries the health colour, so a scan of the table reads as a
   status strip rather than a wall of identical rows. */
tbody tr td:first-child { position: relative; }
tbody tr td:first-child::before {
  content: ""; position: absolute; inset: 0 auto 0 0; width: 2px;
  background: currentColor; opacity: 0;
}
tbody tr[data-health="error"] td:first-child::before { background: var(--error); opacity: .9; }
tbody tr[data-health="warn"] td:first-child::before { background: var(--warn); opacity: .85; }
tbody tr[data-health="stale"] td:first-child::before { background: var(--stale); opacity: .8; }
tbody tr[data-health="ok"] td:first-child::before { background: var(--ok); opacity: .55; }
tr.hidden { display: none; }
.col-num, .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
.col-status { width: 92px; }
.col-method { width: 72px; }
.method { font-family: var(--mono); font-size: 10.5px; color: var(--accent); font-weight: 700; letter-spacing: .03em; }
.path-cell { display: flex; flex-direction: column; gap: 1px; min-width: 270px; }
.path { font-family: var(--mono); font-size: 12px; word-break: break-all; }
.summary { color: var(--faint); font-size: 11px; }
.pill {
  display: inline-flex; align-items: center; gap: 5px; padding: 2px 8px;
  border-radius: 999px; font-size: 11px; font-weight: 650; border: 1px solid transparent;
}
.pill .dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }
.pill-ok { color: var(--ok); background: rgba(52,211,153,.1); border-color: rgba(52,211,153,.28); }
.pill-warn { color: var(--warn); background: rgba(251,191,36,.1); border-color: rgba(251,191,36,.28); }
.pill-error { color: var(--error); background: rgba(251,113,133,.1); border-color: rgba(251,113,133,.3); }
.pill-idle { color: var(--idle); background: rgba(85,100,122,.14); border-color: rgba(85,100,122,.3); }
.pill-stale { color: var(--stale); background: rgba(167,139,250,.1); border-color: rgba(167,139,250,.28); }
.code { font-family: var(--mono); font-size: 11px; font-weight: 700; }
.code-ok { color: var(--ok); }
.code-warn { color: var(--warn); }
.code-error { color: var(--error); }
.code-stale { color: var(--stale); }
.muted { color: var(--faint); }
/* Latency bar: a cheap in-cell bar chart so the latency columns are not just
   numbers. Width is set inline from the max observed latency. */
.lat { display: inline-flex; align-items: center; gap: 7px; justify-content: flex-end; }
.lat-bar { height: 3px; border-radius: 2px; background: var(--accent); opacity: .55; min-width: 2px; }
.lat-bar.is-slow { background: var(--warn); opacity: .8; }

/* ── footer ───────────────────────────────────────────────────────── */
footer {
  margin-top: 32px; padding-top: 18px; border-top: 1px solid var(--border-soft);
  color: var(--faint); font-size: 12px; display: flex; flex-wrap: wrap;
  gap: 10px; justify-content: space-between;
}
footer code { font-family: var(--mono); color: var(--muted); }
@media (max-width: 720px) {
  body { padding: 18px 12px 40px; }
  .hero { padding: 16px; border-radius: 14px; }
  .col-num:last-child, td:last-child { display: none; }
  .path-cell { min-width: 170px; }
  #search { min-width: 100%; }
  .card-value { font-size: 23px; }
}
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation: none !important; transition: none !important; }
}
</style>
</head>
<body>
"#;

const PAGE_FOOT: &str = r#"<footer>
  <span>Scraper API — <code>Rust</code> + <code>Axum</code>. Counters are kept in-process and flushed to Redis every second, so totals survive a restart.</span>
  <span>Auto-refresh 5s · <a href="/api/status">status JSON</a></span>
</footer>
<script>
(function () {
  "use strict";

  var REFRESH_MS = 5000;
  var overallEl = document.getElementById("overall");
  var overallText = document.getElementById("overall-text");
  var uptimeEl = document.getElementById("uptime");
  var updatedEl = document.getElementById("updated");

  var LABELS = { ok: "ok", warn: "warn", error: "error", idle: "idle", stale: "stale" };

  function pct(fraction) {
    return (fraction * 100).toFixed(2) + "%";
  }

  function duration(secs) {
    var d = Math.floor(secs / 86400);
    var h = Math.floor((secs % 86400) / 3600);
    var m = Math.floor((secs % 3600) / 60);
    var s = secs % 60;
    if (d > 0) return d + "d " + h + "h " + m + "m";
    if (h > 0) return h + "h " + m + "m " + s + "s";
    if (m > 0) return m + "m " + s + "s";
    return s + "s";
  }

  function setText(id, value) {
    var el = document.getElementById(id);
    if (el) el.textContent = value;
  }

  // Sparkline over the trailing-minute series. Drawn as two paths (area + line)
  // in a fixed 600x56 viewBox with `preserveAspectRatio="none"`, so it stretches
  // to whatever width the panel has without needing a resize listener.
  var SPARK_W = 600;
  var SPARK_H = 56;
  var SPARK_PAD = 4;

  function renderSpark(series) {
    var line = document.getElementById("spark-line");
    var area = document.getElementById("spark-area");
    if (!line || !area) return;

    if (!series || series.length === 0) {
      line.setAttribute("d", "");
      area.setAttribute("d", "");
      return;
    }

    var peak = 0;
    series.forEach(function (p) { if (p.requests > peak) peak = p.requests; });
    // A flat-zero minute still needs a visible baseline, not a divide by zero.
    var scale = peak > 0 ? peak : 1;
    var step = series.length > 1 ? SPARK_W / (series.length - 1) : SPARK_W;
    var usable = SPARK_H - SPARK_PAD * 2;

    var points = series.map(function (p, i) {
      var x = i * step;
      var y = SPARK_PAD + usable - (p.requests / scale) * usable;
      return [x, y];
    });

    var d = points.map(function (pt, i) {
      return (i === 0 ? "M" : "L") + pt[0].toFixed(1) + " " + pt[1].toFixed(1);
    }).join(" ");

    line.setAttribute("d", d);
    // Close the area down to the baseline so the gradient has something to fill.
    area.setAttribute("d", d + " L" + SPARK_W + " " + SPARK_H + " L0 " + SPARK_H + " Z");
  }

  // Mirrors the server-rendered <td> exactly, including the `data-field` hook,
  // so a row is the same whether it arrived in the HTML or over the poller.
  function makeCell(text, className, field) {
    var td = document.createElement("td");
    td.textContent = text;
    if (className) td.className = className;
    if (field) td.dataset.field = field;
    return td;
  }

  function renderRow(ep) {
    var tr = document.createElement("tr");
    tr.dataset.key = ep.key;
    tr.dataset.health = ep.health;

    var statusTd = document.createElement("td");
    var pill = document.createElement("span");
    pill.className = "pill pill-" + ep.health;
    var dot = document.createElement("span");
    dot.className = "dot";
    pill.appendChild(dot);
    pill.appendChild(document.createTextNode(LABELS[ep.health] || ep.health));
    statusTd.appendChild(pill);
    tr.appendChild(statusTd);

    var methodTd = document.createElement("td");
    var method = document.createElement("span");
    method.className = "method";
    method.textContent = ep.method;
    methodTd.appendChild(method);
    tr.appendChild(methodTd);

    var pathTd = document.createElement("td");
    pathTd.className = "path-cell";
    if (ep.sample) {
      var link = document.createElement("a");
      link.className = "path";
      link.href = ep.sample;
      link.textContent = ep.path;
      pathTd.appendChild(link);
    } else {
      var span = document.createElement("span");
      span.className = "path";
      span.textContent = ep.path;
      pathTd.appendChild(span);
    }
    if (ep.summary) {
      var summary = document.createElement("span");
      summary.className = "summary";
      summary.textContent = ep.summary;
      pathTd.appendChild(summary);
    }
    tr.appendChild(pathTd);

    var called = ep.requests > 0;
    tr.appendChild(makeCell(String(ep.requests), "num", "requests"));
    tr.appendChild(makeCell(called ? pct(ep.error_rate) : "—", "num", "error_rate"));
    // Same shape as the server-rendered cell: bar + number, so a polled row is
    // indistinguishable from one that arrived in the initial HTML.
    var avgTd = document.createElement("td");
    avgTd.className = "num";
    avgTd.dataset.field = "avg";
    if (called) {
      avgTd.dataset.avgMs = String(ep.avg_latency_ms);
      var lat = document.createElement("span");
      lat.className = "lat";
      var bar = document.createElement("span");
      bar.className = ep.avg_latency_ms >= 1500 ? "lat-bar is-slow" : "lat-bar";
      var ratio = ep.max_latency_ms > 0
        ? Math.min(Math.max(ep.avg_latency_ms / ep.max_latency_ms, 0), 1)
        : 0;
      bar.style.width = Math.max(ratio * 34, 2).toFixed(0) + "px";
      lat.appendChild(bar);
      lat.appendChild(document.createTextNode(ep.avg_latency_ms.toFixed(1) + "ms"));
      avgTd.appendChild(lat);
    } else {
      var dashAvg = document.createElement("span");
      dashAvg.className = "muted";
      dashAvg.textContent = "—";
      avgTd.appendChild(dashAvg);
    }
    tr.appendChild(avgTd);
    tr.appendChild(makeCell(called ? ep.max_latency_ms.toFixed(1) + "ms" : "—", "num", "max"));

    var lastTd = document.createElement("td");
    lastTd.className = "num";
    lastTd.dataset.field = "last";
    if (ep.last_status) {
      var code = document.createElement("span");
      code.className = "code code-" + ep.health;
      code.textContent = String(ep.last_status);
      lastTd.appendChild(code);
    } else {
      var dash = document.createElement("span");
      dash.className = "muted";
      dash.textContent = "—";
      lastTd.appendChild(dash);
    }
    tr.appendChild(lastTd);

    return tr;
  }

  // The server renders one <details> per group it knows about. A group can
  // appear later — the `other` group exists only once traffic hits an
  // undocumented route — so the poller builds the shell on demand instead of
  // dropping those rows on the floor.
  function ensureGroup(group) {
    var tbody = document.querySelector('tbody[data-tbody="' + group.tag + '"]');
    if (tbody) return tbody;

    var monitor = document.querySelector("section.monitor");
    if (!monitor) return null;

    var details = document.createElement("details");
    details.className = "group";
    details.open = true;
    details.dataset.tag = group.tag;

    var summary = document.createElement("summary");
    var name = document.createElement("span");
    name.className = "group-name";
    name.textContent = group.tag;
    var desc = document.createElement("span");
    desc.className = "group-desc";
    desc.textContent = group.description || "";
    var count = document.createElement("span");
    count.className = "group-count";
    count.dataset.groupCount = group.tag;
    summary.appendChild(name);
    summary.appendChild(desc);
    summary.appendChild(count);

    var wrap = document.createElement("div");
    wrap.className = "table-wrap";
    var table = document.createElement("table");

    var thead = document.createElement("thead");
    var headRow = document.createElement("tr");
    ["Status", "Method", "Path", "Request", "Error", "Avg", "Max", "Last"].forEach(function (label, index) {
      var th = document.createElement("th");
      th.textContent = label;
      th.className = index === 0 ? "col-status" : index === 1 ? "col-method" : index === 2 ? "col-path" : "col-num";
      headRow.appendChild(th);
    });
    thead.appendChild(headRow);

    var body = document.createElement("tbody");
    body.dataset.tbody = group.tag;

    table.appendChild(thead);
    table.appendChild(body);
    wrap.appendChild(table);
    details.appendChild(summary);
    details.appendChild(wrap);
    monitor.appendChild(details);

    return body;
  }

  function applyFilters() {
    var query = (document.getElementById("search").value || "").toLowerCase().trim();
    var active = document.querySelector(".filter.active");
    var filter = active ? active.dataset.filter : "all";

    document.querySelectorAll("tr[data-key]").forEach(function (row) {
      var matchesText = !query || row.dataset.key.toLowerCase().indexOf(query) !== -1 ||
        row.textContent.toLowerCase().indexOf(query) !== -1;
      var matchesFilter = filter === "all" || row.dataset.health === filter;
      row.classList.toggle("hidden", !(matchesText && matchesFilter));
    });

    document.querySelectorAll("details.group").forEach(function (group) {
      var rows = group.querySelectorAll("tr[data-key]");
      var visible = group.querySelectorAll("tr[data-key]:not(.hidden)").length;
      group.style.display = rows.length > 0 && visible === 0 ? "none" : "";
    });
  }

  function refresh() {
    fetch("/api/status", { headers: { accept: "application/json" }, cache: "no-store" })
      .then(function (res) { return res.ok ? res.json() : null; })
      .then(function (data) {
        if (!data) return;

        setText("stat-total", String(data.stats.requests_total));
        setText("stat-rpm", String(data.stats.requests_last_minute));
        setText("stat-inflight", String(data.stats.in_flight));
        setText("stat-errors", pct(data.stats.error_rate));
        setText("stat-error-detail",
          data.stats.ok + " ok · " + data.stats.client_errors + " 4xx · " +
          data.stats.server_errors + " 5xx");
        setText("stat-endpoints", String(data.counts.total));
        setText("stat-endpoint-detail",
          data.counts.ok + " ok · " + data.counts.warn + " warn · " +
          data.counts.error + " error · " + data.counts.stale + " stale · " +
          data.counts.idle + " idle");
        setText("uptime", duration(data.service.uptime_secs));
        setText("persistence", data.persistence === "redis" ? "persisted in Redis" : "in-process");

        if (data.throughput) {
          renderSpark(data.throughput);
          var peak = 0;
          var total = 0;
          data.throughput.forEach(function (p) {
            total += p.requests;
            if (p.requests > peak) peak = p.requests;
          });
          setText("pulse-peak", String(peak));
          setText("pulse-total", String(total));
        }

        if (overallEl) {
          overallEl.className = "overall overall-" + data.overall;
        }
        if (overallText) {
          overallText.textContent = data.overall === "ok" ? "All endpoints healthy"
            : data.overall === "warn" ? "Some endpoints are rejecting requests"
            : data.overall === "error" ? "An endpoint is failing"
            : data.overall === "stale" ? "Last observation is old"
            : "Waiting for the first request";
        }
        if (updatedEl) {
          var when = new Date(data.generated_at_ms);
          updatedEl.textContent = when.toLocaleTimeString("id-ID");
        }

        var coldStart = document.getElementById("cold-start");
        if (coldStart) coldStart.hidden = !data.cold_start;

        data.groups.forEach(function (group) {
          var tbody = ensureGroup(group);
          if (!tbody) return;
          tbody.replaceChildren();
          group.endpoints.forEach(function (ep) { tbody.appendChild(renderRow(ep)); });
          var countEl = document.querySelector('[data-group-count="' + group.tag + '"]');
          if (countEl) countEl.textContent = group.endpoints.length + " endpoint";

          // Rebuild the health badges: a group can gain its first failure after
          // the page loaded, and a stale badge would hide it.
          var badgesEl = document.querySelector('[data-group-badges="' + group.tag + '"]');
          if (badgesEl) {
            var tally = { error: 0, warn: 0, stale: 0 };
            group.endpoints.forEach(function (ep) {
              if (tally[ep.health] !== undefined) tally[ep.health] += 1;
            });
            badgesEl.replaceChildren();
            ["error", "warn", "stale"].forEach(function (kind) {
              if (tally[kind] > 0) {
                var badge = document.createElement("span");
                badge.className = "gbadge gbadge-" + kind;
                badge.textContent = String(tally[kind]);
                badgesEl.appendChild(badge);
              }
            });
          }
        });

        applyFilters();
      })
      .catch(function () { /* keep the last good snapshot on screen */ });
  }

  var search = document.getElementById("search");
  if (search) search.addEventListener("input", applyFilters);

  document.querySelectorAll(".filter").forEach(function (button) {
    button.addEventListener("click", function () {
      document.querySelectorAll(".filter").forEach(function (other) {
        other.classList.remove("active");
      });
      button.classList.add("active");
      applyFilters();
    });
  });

  // Poll only while the tab is visible: a background tab refreshing every 5s
  // is pure noise in the numbers it is reading.
  var timer = null;
  function start() {
    if (timer) return;
    timer = setInterval(refresh, REFRESH_MS);
  }
  function stop() {
    if (timer) { clearInterval(timer); timer = null; }
  }
  document.addEventListener("visibilitychange", function () {
    if (document.hidden) { stop(); } else { start(); refresh(); }
  });
  start();
})();
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_path_fills_known_parameters() {
        assert_eq!(
            sample_path("/api/anime/detail/{slug}"),
            "/api/anime/detail/one-piece"
        );
        assert_eq!(
            sample_path("/api/komik/genre/{slug}/{page}"),
            "/api/komik/genre/one-piece/1"
        );
        assert_eq!(sample_path("/download"), "/download");
        assert_eq!(
            sample_path("/file/yt_merge/{filename}"),
            "/file/yt_merge/merged.mp4"
        );
        assert_eq!(sample_path("/x/{unknown}/y"), "/x/example/y");
    }

    #[test]
    fn sample_path_leaves_unbalanced_braces_alone() {
        assert_eq!(sample_path("/broken/{slug"), "/broken/{slug");
    }

    #[test]
    fn escaping_neutralises_markup() {
        assert_eq!(
            escape_html(r#"<script>alert("x")</script>"#),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;"
        );
    }

    #[test]
    fn dashboard_paths_are_excluded_from_the_monitor() {
        assert!(is_dashboard_path("/"));
        assert!(is_dashboard_path("/api/status"));
        assert!(is_dashboard_path("/docs"));
        assert!(is_dashboard_path("/docs/"));
        assert!(is_dashboard_path("/favicon.ico"));
        assert!(!is_dashboard_path("/api/anime"));
        // `/health` is a real endpoint: monitoring it is the point.
        assert!(!is_dashboard_path("/health"));
    }

    #[test]
    fn catalog_covers_every_documented_module() {
        let tags: Vec<&str> = CATALOG.iter().map(|entry| entry.tag.as_str()).collect();
        for tag in [
            "anime", "anime2", "komik", "download", "image", "misc", "stalk", "search", "tool",
        ] {
            assert!(tags.contains(&tag), "catalog is missing the `{tag}` module");
        }
        assert!(
            CATALOG.len() >= 80,
            "catalog looks truncated: {}",
            CATALOG.len()
        );
    }

    #[test]
    fn snapshot_reports_every_documented_endpoint_once() {
        let snap = snapshot();
        let listed: usize = snap.groups.iter().map(|group| group.endpoints.len()).sum();
        assert_eq!(listed, snap.counts.total);
        assert!(
            snap.counts.ok + snap.counts.warn + snap.counts.error + snap.counts.idle
                == snap.counts.total
        );
        assert!(
            snap.counts.total >= 80,
            "monitor is missing endpoints: {}",
            snap.counts.total
        );
    }

    #[test]
    fn the_page_is_entirely_in_english() {
        // The dashboard is public-facing, so its copy is English. This guards
        // against a stray Indonesian string creeping back in during a redesign:
        // it checks the rendered page, not the source, so a value that only
        // reaches the HTML through a helper is covered too.
        let html = render_page(&snapshot());
        let mut checked = 0;
        for word in [
            "sejak",
            "Semua",
            "aktif",
            "tersimpan",
            "diperbarui",
            "baru saja",
            "Menunggu",
            "Sebagian",
            "Trafik",
            "puncak",
            "diproses",
            "Belum ada",
            "dihitung",
            "dipantau",
            "tersedia",
            "Cari ",
            "detik",
            "menit",
            "dan ",
            "untuk ",
            "yang ",
        ] {
            checked += 1;
            assert!(
                !html.contains(word),
                "rendered page still contains Indonesian text `{word}`"
            );
        }
        assert!(checked >= 20, "the guard should cover a real word list");

        // The document language must match the copy.
        assert!(html.contains(r#"<html lang="en">"#), "lang should be `en`");

        // The <title> is easy to miss because it is not body text, and it is
        // what a browser tab and a search result show.
        let title = html
            .split("<title>")
            .nth(1)
            .and_then(|rest| rest.split("</title>").next())
            .unwrap_or_default();
        assert!(!title.is_empty(), "the page needs a <title>");
        assert!(
            !title.contains("Statistik"),
            "the <title> is still Indonesian: {title}"
        );
    }

    #[test]
    fn page_markup_is_structurally_balanced() {
        // Regression: a bulk CSS replacement once dropped the opening `<style>`
        // tag, so every rule was served as visible body text. Nothing failed,
        // because no test looked at the document's structure.
        let html = render_page(&snapshot());

        for tag in [
            "html", "head", "body", "style", "script", "header", "footer",
        ] {
            // Match `<tag>`/`<tag ` exactly: a bare `<head` would also match
            // `<header>`, and `<style` would match nothing useful here.
            let open = html.matches(&format!("<{tag}>")).count()
                + html.matches(&format!("<{tag} ")).count();
            let close = html.matches(&format!("</{tag}>")).count();
            assert_eq!(
                open, close,
                "unbalanced <{tag}>: {open} open, {close} close"
            );
            assert!(open > 0, "<{tag}> is missing entirely");
        }

        // CSS must be inside a <style> element, not loose in the body.
        let style_open = html.find("<style>").expect("opening <style> tag");
        let style_close = html.find("</style>").expect("closing </style> tag");
        let body = html.find("<body>").expect("body tag");
        assert!(
            style_open < style_close && style_close < body,
            "the stylesheet must sit inside <head>, before <body>"
        );

        // And the page must not leak CSS at the start of the body.
        let body_text = &html[body..];
        assert!(
            !body_text.starts_with("<body>\n:root"),
            "CSS variables are being rendered as page text"
        );
    }

    #[test]
    fn sparkline_paths_stay_valid_for_edge_cases() {
        // No data at all must not produce a path, and must not divide by zero.
        let (line, area) = spark_paths(&[]);
        assert!(line.is_empty());
        assert!(area.is_empty());

        // A completely idle minute: peak is 0, so the scale falls back to 1 and
        // the line sits flat on the baseline rather than vanishing.
        let idle: Vec<ThroughputPoint> = (0..60)
            .map(|t| ThroughputPoint { t, requests: 0 })
            .collect();
        let (line, area) = spark_paths(&idle);
        assert!(line.starts_with('M'));
        assert!(!line.contains("NaN"), "idle series produced NaN: {line}");
        assert!(area.ends_with('Z'));

        // A real spike must move the line off the baseline.
        let mut spiky = idle.clone();
        spiky[30].requests = 10;
        let (line, _) = spark_paths(&spiky);
        assert!(line.starts_with('M'));
        assert!(!line.contains("NaN"));
        // The spike's y is above the baseline, so the path is not a single y.
        let ys: Vec<&str> = line.split(' ').skip(1).step_by(2).collect();
        assert!(
            ys.iter().any(|y| *y != ys[0]),
            "a spike should change the y coordinate"
        );
    }

    #[test]
    fn latency_cell_scales_and_flags_slow_endpoints() {
        // A fast call inside a fast endpoint: small bar, no slow flag.
        let fast = latency_cell(10.0, 20.0);
        assert!(fast.contains("10.0ms"));
        assert!(!fast.contains("is-slow"));
        assert!(fast.contains("width:17px"), "half of 34px: {fast}");

        // Past the slow threshold the bar turns amber regardless of ratio.
        let slow = latency_cell(2000.0, 2000.0);
        assert!(slow.contains("is-slow"));

        // A zero max must not divide by zero or produce NaN in the style.
        let degenerate = latency_cell(5.0, 0.0);
        assert!(!degenerate.contains("NaN"));
        assert!(
            degenerate.contains("width:2px"),
            "floor width: {degenerate}"
        );
    }

    #[test]
    fn group_badges_only_list_states_that_occurred() {
        let mut counts = HealthCounts::default();
        counts.tally(Health::Ok);
        assert_eq!(group_badges(&[]), "");
    }

    #[test]
    fn a_stale_failure_does_not_raise_a_permanent_alarm() {
        // Regression: once counters survive restarts, an endpoint that 500'd
        // days ago and was never called again used to keep the whole dashboard
        // in `error` forever. The verdict must decay to `stale` instead.
        let mut counts = HealthCounts::default();
        counts.tally(Health::Stale);
        counts.tally(Health::Ok);
        assert_eq!(counts.error, 0);
        assert_eq!(counts.overall(), Health::Ok, "a stale row must not alarm");

        // With nothing fresh observed at all, `stale` is the honest verdict.
        let mut only_stale = HealthCounts::default();
        only_stale.tally(Health::Stale);
        assert_eq!(only_stale.overall(), Health::Stale);

        // A real failure still outranks everything.
        let mut failing = HealthCounts::default();
        failing.tally(Health::Error);
        failing.tally(Health::Ok);
        assert_eq!(failing.overall(), Health::Error);
    }

    #[test]
    fn every_health_variant_has_a_class_and_a_label() {
        for health in [
            Health::Ok,
            Health::Warn,
            Health::Error,
            Health::Idle,
            Health::Stale,
        ] {
            assert!(!health_class(health).is_empty());
            assert!(!health_label(health).is_empty());
        }
    }

    #[test]
    fn homepage_renders_without_a_single_request() {
        let html = render_page(&snapshot());
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Monitor per endpoint"));
        assert!(html.contains("data-tbody=\"anime\""));
        assert!(html.contains("/api/status"));
    }
}
