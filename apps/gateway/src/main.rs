#![doc = "Logging Setup"]
use scraper_service::bootstrap::Application;

/// Global allocator: mimalloc.
///
/// The hot path of this service is allocation-heavy — html5ever builds a whole
/// DOM of `String`/`SmallVec` nodes per scrape, `serde_json` grows a buffer per
/// response, and reqwest hands back a fresh `Bytes` per fetch. The system
/// allocator (glibc malloc) pays a lock plus a size-class walk on every one of
/// those. mimalloc keeps per-thread free lists, so the same workload becomes a
/// pointer bump, and the multi-threaded runtime stops contending on malloc
/// arenas.
///
/// `default-features = false` (see Cargo.toml) keeps mimalloc from overriding
/// the C `malloc`/`free` symbols: the process only needs it as Rust's
/// `#[global_allocator]`, and symbol interposition would also change the
/// behaviour of the C dependencies (openssl, zlib).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // Application setup and server logic is now encapsulated in `startup.rs`
    // This makes the main function clean and the app easier to integration test.
    let app = Application::build().await?;
    app.run().await?;

    Ok(())
}
