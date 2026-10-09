pub mod setup;

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::CONFIG;
use crate::infrastructure::cache::redis_pool::get_redis_conn;

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
        if let Err(e) = crate::infrastructure::scheduler::start_scheduler() {
            tracing::error!("[scheduler] failed to start daily cleanup: {e}");
        }

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

        // Listener
        let port = CONFIG.server_port;
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("Server listening on {}", listener.local_addr()?);

        Ok(Self {
            port,
            router: app,
            listener,
        })
    }

    pub async fn run(self) -> std::io::Result<()> {
        axum::serve(self.listener, self.router.into_make_service()).await
    }
}
