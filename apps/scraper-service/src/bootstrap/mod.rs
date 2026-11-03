pub mod setup;

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use mytheclipse::cron::CronJob;
use mytheclipse::panic_tracker::{PanicGuard, PanicTracker};
use mytheclipse::shutdown::{ShutdownManager, ShutdownSignal};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::CONFIG;
use crate::infrastructure::cache::redis_pool::get_redis_conn;

/// Grace window for in-flight requests and tracked tasks after SIGTERM.
///
/// The systemd unit keeps its default `TimeoutStopSec` (90s), so this leaves
/// room before the supervisor escalates to SIGKILL.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(20);

/// Open the SeaORM pool, returning `None` instead of failing startup.
///
/// No request path issues a query today — every handler reads from upstream
/// HTTP providers and Redis — so an unreachable database degrades the service
/// rather than taking it down.
async fn connect_database() -> Option<DatabaseConnection> {
    let mut options = ConnectOptions::new(CONFIG.database_url.clone());
    options
        .max_connections(20)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(CONFIG.db.connect_timeout_seconds))
        .idle_timeout(Duration::from_secs(CONFIG.db.idle_timeout_seconds))
        .acquire_timeout(Duration::from_secs(CONFIG.db.acquire_timeout_seconds))
        .max_lifetime(Duration::from_secs(CONFIG.db.max_lifetime_seconds))
        .sqlx_logging(CONFIG.log_level == "debug")
        .map_sqlx_postgres_opts(|opts| opts.extra_float_digits(None));

    match Database::connect(options).await {
        Ok(connection) => {
            tracing::info!("✓ SeaORM database connection established");
            Some(connection)
        }
        Err(e) => {
            tracing::error!("Failed to connect to database (continuing): {e}");
            None
        }
    }
}

pub struct Application {
    pub port: u16,
    router: Router,
    listener: TcpListener,
    shutdown: ShutdownManager,
    scheduler: Option<CronJob>,
    /// Restores the previous panic hook when the process exits.
    _panic_guard: PanicGuard,
}

impl Application {
    pub async fn build() -> anyhow::Result<Self> {
        // Initialize tracing. Default to warn/error globally unless RUST_LOG is explicitly set.
        let env_filter = match std::env::var("RUST_LOG") {
            Ok(filter) => EnvFilter::new(filter).add_directive("html5ever=error".parse()?),
            Err(_) => EnvFilter::new("warn,html5ever=error"),
        };

        // Use mytheclipse-tracing's formatted layer, composed with the
        // scraper's own env filter (default: warn + html5ever=error).
        use tracing_subscriber::layer::{Layer, SubscriberExt};
        use tracing_subscriber::util::SubscriberInitExt;
        let _ = tracing_subscriber::registry()
            .with(mytheclipse_tracing::TracingLayer::layer().with_filter(env_filter))
            .try_init();

        // Log panics through tracing before the default hook prints them, so a
        // panic in a detached task leaves a trace instead of only stderr.
        let panic_guard = PanicTracker::install();

        // Initialize OpenTelemetry metrics
        crate::observability::metrics::init_otel_metrics();

        tracing::info!("🚀 Scraper starting up...");
        tracing::info!("   Environment: {}", CONFIG.environment);

        // Thread configuration from mytheclipse runtime_auto
        let runtime_cfg = mytheclipse::runtime_auto::RuntimeConfig::auto();
        tracing::info!(
            "   Tokio Worker Threads: {} (auto from CPU cores)",
            runtime_cfg.worker_threads
        );
        tracing::info!(
            "   Max Blocking Threads: {} | Compute Threads: {} | IO Threads: {}",
            runtime_cfg.max_blocking_threads,
            runtime_cfg.compute_threads,
            runtime_cfg.io_threads
        );

        // Redis
        let _ = get_redis_conn().await;

        // Job queue + daily scheduler (mytheclipse-queue + mytheclipse::cron)
        crate::infrastructure::queue::init_global_job_queue();
        let scheduler = match crate::infrastructure::scheduler::start_scheduler() {
            Ok(job) => Some(job),
            Err(e) => {
                tracing::error!("[scheduler] failed to start daily cleanup: {e}");
                None
            }
        };

        // Database. No request path issues a query today (every handler reads
        // from upstream HTTP providers and Redis), so a failed connection is
        // logged rather than fatal — the scraper is fully functional without
        // it, and refusing to boot here would take the API down for a
        // dependency nothing uses.
        let db = connect_database().await;
        if let Some(db) = db.as_ref() {
            if let Err(e) = crate::bootstrap::setup::init(db).await {
                tracing::error!("Failed to init DB schema: {}", e);
            }
        }

        let app = crate::presentation::router::build_router();

        // Shutdown coordination: one watcher turns SIGTERM/SIGINT into a
        // broadcast that tracked tasks observe; `run` drains them afterwards.
        let shutdown = ShutdownManager::new();
        let signal_manager = shutdown.clone();
        shutdown.register(tokio::spawn(async move {
            signal_manager.wait_for_os_signal().await;
            tracing::info!("termination signal received");
            signal_manager.request();
        }));

        // Listener
        let port = CONFIG.server_port;
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("Server listening on {}", listener.local_addr()?);

        Ok(Self {
            port,
            router: app,
            listener,
            shutdown,
            scheduler,
            _panic_guard: panic_guard,
        })
    }

    /// Serves until a termination signal arrives, then finishes in-flight
    /// requests and drains tracked tasks.
    ///
    /// The job queue's worker pool exposes no stop hook upstream, so queued
    /// work is abandoned on exit; the only producer today enqueues a no-op
    /// cache-repair job.
    pub async fn run(self) -> std::io::Result<()> {
        let signal = self.shutdown.handle();
        let served = serve_until_shutdown(self.listener, self.router, signal).await;

        if let Some(scheduler) = self.scheduler {
            scheduler.cancel();
        }
        self.shutdown.drain(SHUTDOWN_GRACE).await;
        tracing::info!("shutdown complete");
        served
    }
}

/// Serves `router` on `listener` until `signal` fires, then stops accepting
/// connections and finishes the requests already in flight.
async fn serve_until_shutdown(
    listener: TcpListener,
    router: Router,
    mut signal: ShutdownSignal,
) -> std::io::Result<()> {
    axum::serve(listener, router.into_make_service())
        .with_graceful_shutdown(async move { signal.wait().await })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    /// A request already being handled finishes before `serve` returns, and the
    /// listener stops accepting new ones.
    #[tokio::test]
    async fn in_flight_request_completes_after_shutdown_signal() {
        let router = Router::new().route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                "done"
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("listener address");

        let manager = ShutdownManager::new();
        let serve = tokio::spawn(serve_until_shutdown(listener, router, manager.handle()));

        let mut client = TcpStream::connect(addr).await.expect("connect");
        client
            .write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .expect("send request");

        // Let the handler start, then request shutdown while it sleeps.
        tokio::time::sleep(Duration::from_millis(100)).await;
        manager.request();

        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .await
            .expect("read response");
        assert!(
            response.contains("done"),
            "in-flight request was cut short: {response}"
        );

        let served = tokio::time::timeout(Duration::from_secs(5), serve)
            .await
            .expect("serve returns after draining")
            .expect("serve task joins");
        assert!(served.is_ok(), "serve returned an error: {served:?}");

        assert!(
            TcpStream::connect(addr).await.is_err(),
            "listener still accepting after shutdown"
        );
    }
}
