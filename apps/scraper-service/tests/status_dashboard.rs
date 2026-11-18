//! The homepage and the public status dashboard, driven through the real
//! router.
//!
//! These assert the two things a unit test cannot: that the routes are actually
//! mounted, and that the telemetry middleware records a request made through
//! the full stack (routing → layer → handler). A monitor that reports nothing
//! because the middleware was never wired is indistinguishable from a healthy
//! idle service, so the wiring itself is what is under test here.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

use scraper_service::presentation::router::build_router;

/// Send one request through the router and return `(status, headers, body)`.
async fn get(uri: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let response = build_router()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router is infallible");

    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .expect("body reads");
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

async fn get_json(uri: &str) -> Value {
    let (status, _, body) = get(uri).await;
    assert_eq!(status, StatusCode::OK, "{uri} returned {status}: {body}");
    // `assert!` rather than `panic!`/`expect`: this crate denies
    // `clippy::panic` in tests, and the body must appear to be diagnosable.
    let parsed = serde_json::from_str(&body);
    assert!(parsed.is_ok(), "{uri} is not JSON: {body}");
    parsed.unwrap_or_default()
}

#[tokio::test]
async fn homepage_is_served_as_html() {
    let (status, headers, body) = get("/").await;

    assert_eq!(status, StatusCode::OK);
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        content_type.starts_with("text/html"),
        "homepage content type was {content_type}"
    );
    // Never cached: the page embeds live counters.
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert!(body.starts_with("<!doctype html>"));
    assert!(body.contains("Monitor per endpoint"));
    // The monitor renders one row per documented endpoint.
    assert!(body.contains("data-key=\"GET /api/anime/detail/{slug}\""));
    assert!(body.contains("data-key=\"GET /health\""));
}

#[tokio::test]
async fn status_json_describes_the_whole_public_surface() {
    let json = get_json("/api/status").await;

    assert_eq!(json["service"]["name"], "scraper");
    assert!(
        json["service"]["documented_operations"]
            .as_u64()
            .unwrap_or(0)
            >= 80
    );
    assert!(json["counts"]["total"].as_u64().unwrap_or(0) >= 80);

    // The dashboard tells the operator where the numbers live, so a restart
    // that silently loses history is visible rather than inferred.
    let persistence = json["persistence"].as_str().unwrap_or("");
    assert!(
        persistence == "redis" || persistence == "memory",
        "unexpected persistence backend: {persistence}"
    );

    // `stale` is always present as a bucket, even when zero, so the rendered
    // detail line never reads `undefined`.
    assert!(json["counts"]["stale"].is_u64());

    let groups = json["groups"].as_array().expect("groups is an array");
    let tags: Vec<&str> = groups
        .iter()
        .filter_map(|group| group["tag"].as_str())
        .collect();
    for expected in [
        "anime", "anime2", "komik", "download", "image", "misc", "stalk", "search", "tool",
        "service",
    ] {
        assert!(tags.contains(&expected), "status is missing `{expected}`");
    }

    // Every endpoint carries the fields the dashboard renders.
    let first = &groups[0]["endpoints"][0];
    for field in [
        "key",
        "method",
        "path",
        "sample",
        "health",
        "requests",
        "error_rate",
        "avg_latency_ms",
    ] {
        assert!(!first[field].is_null(), "endpoint is missing `{field}`");
    }
}

#[tokio::test]
async fn a_call_is_recorded_against_its_route_template() {
    // A real handler, with its query parameter missing: it answers 400 without
    // touching any upstream, which is exactly the "alive but rejected" case the
    // monitor must surface as `warn`.
    let (status, _, _) = get("/tool/whois").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let json = get_json("/api/status").await;
    let endpoint = find_endpoint(&json, "GET /tool/whois")
        .expect("the whois route is monitored even though it is not documented by tag");

    assert!(
        endpoint["requests"].as_u64().unwrap_or(0) >= 1,
        "the request was not counted: {endpoint}"
    );
    assert_eq!(endpoint["last_status"], 400);
    assert_eq!(endpoint["health"], "warn");
    assert_eq!(endpoint["client_errors"].as_u64(), Some(1));

    let stats = &json["stats"];
    assert!(stats["requests_total"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(json["cold_start"], false);
}

#[tokio::test]
async fn two_different_path_parameters_share_one_series() {
    // Route-template keying is what stops the registry growing with traffic.
    // `/file/yt_merge/{filename}` rejects a traversal attempt before touching
    // any upstream or config, so this exercises the middleware alone.
    for name in ["a.mp4", "b.mp4"] {
        let (status, _, _) = get(&format!("/file/yt_merge/{name}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    let json = get_json("/api/status").await;
    let endpoint = find_endpoint(&json, "GET /file/yt_merge/{filename}")
        .expect("the merged-file route is monitored");
    assert!(
        endpoint["requests"].as_u64().unwrap_or(0) >= 2,
        "both filenames must fold into one series: {endpoint}"
    );
    assert!(
        find_endpoint(&json, "GET /file/yt_merge/a.mp4").is_none(),
        "the registry keyed on the raw URI instead of the route template"
    );
}

#[tokio::test]
async fn the_dashboards_own_traffic_is_not_counted() {
    // Poll the dashboard the way the page does, then prove it did not show up.
    for _ in 0..3 {
        let (status, _, _) = get("/api/status").await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, _, _) = get("/").await;
    assert_eq!(status, StatusCode::OK);

    let json = get_json("/api/status").await;
    assert!(
        find_endpoint(&json, "GET /api/status").is_none(),
        "the status endpoint counted its own polling"
    );
    assert!(
        find_endpoint(&json, "GET /").is_none(),
        "the homepage counted its own renders"
    );
}

#[tokio::test]
async fn health_stays_a_monitored_public_endpoint() {
    let (status, _, body) = get("/health").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"status\":\"ok\""));

    let json = get_json("/api/status").await;
    let endpoint = find_endpoint(&json, "GET /health").expect("/health is monitored");
    assert_eq!(endpoint["health"], "ok");
    assert_eq!(endpoint["last_status"], 200);
}

#[tokio::test]
async fn openapi_document_covers_every_mounted_module() {
    let json = get_json("/api-docs/openapi.json").await;
    let paths = json["paths"].as_object().expect("paths is an object");

    for path in [
        "/api/anime",
        "/api/anime2",
        "/api/komik/genre_list",
        "/download",
        "/download/terabox",
        "/proxy/terabox",
        "/misc/server-info",
        "/stalk/github",
        "/search/bmkg",
        "/tool/whois",
        "/image/brat",
        "/health",
    ] {
        assert!(paths.contains_key(path), "OpenAPI is missing `{path}`");
    }
}

#[tokio::test]
async fn favicon_is_served_so_the_page_makes_no_failing_request() {
    for path in ["/favicon.svg", "/favicon.ico"] {
        let (status, headers, body) = get(path).await;
        assert_eq!(status, StatusCode::OK, "{path} must not 404");
        assert!(
            headers
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .starts_with("image/svg+xml"),
            "{path} has the wrong content type"
        );
        assert!(body.contains("<svg"), "{path} did not return an icon");
    }

    // An icon fetch is browser plumbing, not API traffic.
    let json = get_json("/api/status").await;
    assert!(find_endpoint(&json, "GET /favicon.ico").is_none());
    assert!(find_endpoint(&json, "GET /favicon.svg").is_none());
}

#[tokio::test]
async fn homepage_declares_the_icon_it_serves() {
    let (_, _, body) = get("/").await;
    assert!(body.contains("rel=\"icon\" href=\"/favicon.svg\""));
    assert!(body.contains("rel=\"icon\" href=\"/favicon.ico\""));
}

/// Find one endpoint row in the status snapshot, searching every group.
fn find_endpoint<'a>(snapshot: &'a Value, key: &str) -> Option<&'a Value> {
    snapshot["groups"]
        .as_array()?
        .iter()
        .flat_map(|group| {
            group["endpoints"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[])
        })
        .find(|endpoint| endpoint["key"] == key)
}
