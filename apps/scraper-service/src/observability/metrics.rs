//! OpenTelemetry metrics initialization for the scraper service.
//!
//! Provides:
//! - Global `MeterProvider` connected via OTLP gRPC to the metrics backend
//! - Standard HTTP server metrics middleware
//!
//! #![allow(clippy::all)]
//!
//! Environment:
//!   OTEL_EXPORTER_OTLP_ENDPOINT  — default: http://localhost:4317
//!   OTEL_SERVICE_NAME             — default: scraper-api
//!   OTEL_METRICS_EXPORT_INTERVAL  — default: 5000 (ms)

use axum::{extract::Request, middleware::Next, response::Response};
use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Histogram, Meter, UpDownCounter},
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{Resource, metrics::MeterProviderBuilder, metrics::PeriodicReader};
use std::sync::OnceLock;
use std::time::Instant;

static METER: OnceLock<Meter> = OnceLock::new();
static PROVIDER: OnceLock<opentelemetry_sdk::metrics::SdkMeterProvider> = OnceLock::new();

fn meter() -> &'static Meter {
    METER
        .get()
        .expect("OTel meter not initialized — call init_otel_metrics first")
}

/// Initialize the global OTLP MeterProvider.
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn init_otel_metrics() {
    if METER.get().is_some() {
        return;
    }

    let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT");
    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "scraper-api".into());

    let m = if let Ok(endpoint) = otel_endpoint {
        let export_interval_ms: u64 = std::env::var("OTEL_METRICS_EXPORT_INTERVAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5000);

        // Build the gRPC OTLP exporter
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(endpoint.clone())
            .build()
            .expect("Failed to create OTLP metric exporter");

        let reader = PeriodicReader::builder(exporter)
            .with_interval(std::time::Duration::from_millis(export_interval_ms))
            .build();

        let resource = Resource::builder()
            .with_service_name(service_name.clone())
            .build();

        let provider = MeterProviderBuilder::default()
            .with_resource(resource)
            .with_reader(reader)
            .build();

        let _ = PROVIDER.set(provider.clone());
        global::set_meter_provider(provider);

        tracing::info!(otel_endpoint = %endpoint, service_name, "OTel metrics initialized");
        global::meter("scraper-http-server")
    } else {
        tracing::info!("OTEL_EXPORTER_OTLP_ENDPOINT not set — metrics disabled");
        global::meter("scraper-http-server")
    };

    let _ = METER.set(m);
}

/// Shut down the global MeterProvider, flushing pending exports.
pub async fn shutdown_otel_metrics() {
    if let Some(provider) = PROVIDER.get() {
        if let Err(e) = provider.shutdown() {
            tracing::warn!(error = %e, "OTel metrics shutdown error");
        } else {
            tracing::info!("OTel metrics shut down");
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP metrics middleware
// ---------------------------------------------------------------------------

/// Axum middleware that records standard HTTP server metrics for every request.
///
/// This middleware must be added as a layer **after** `init_otel_metrics()` has been called.
///
/// Metrics emitted:
///   - `http.server.request_count`      — Counter { method, path, status }
///   - `http.server.request_duration_ms` — Histogram { method, path, status }
///   - `http.server.request_in_flight`   — UpDownCounter { method, path }
pub async fn otel_metrics_middleware(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    let in_flight = get_in_flight_counter();
    let counter = get_request_counter();
    let duration = get_duration_histogram();

    // Record in-flight
    in_flight.add(
        1,
        &[
            KeyValue::new("method", method.clone()),
            KeyValue::new("path", path.clone()),
        ],
    );

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed_ms = start.elapsed().as_millis() as f64;

    let status = response.status().as_u16().to_string();

    // Record request count + duration
    let attrs = [
        KeyValue::new("method", method),
        KeyValue::new("path", path),
        KeyValue::new("status", status),
    ];
    counter.add(1, &attrs);
    duration.record(elapsed_ms, &attrs);

    // Decrement in-flight
    in_flight.add(-1, &[]);

    response
}

/// Lazily-initialised instruments guarded by OnceLock.
fn get_in_flight_counter() -> UpDownCounter<i64> {
    static INST: OnceLock<UpDownCounter<i64>> = OnceLock::new();
    INST.get_or_init(|| {
        meter()
            .i64_up_down_counter("http.server.request_in_flight")
            .with_description("Number of HTTP requests currently in flight")
            .build()
    })
    .clone()
}

fn get_request_counter() -> Counter<u64> {
    static INST: OnceLock<Counter<u64>> = OnceLock::new();
    INST.get_or_init(|| {
        meter()
            .u64_counter("http.server.request_count")
            .with_description("Total number of HTTP requests received")
            .build()
    })
    .clone()
}

fn get_duration_histogram() -> Histogram<f64> {
    static INST: OnceLock<Histogram<f64>> = OnceLock::new();
    INST.get_or_init(|| {
        meter()
            .f64_histogram("http.server.request_duration_ms")
            .with_description("Duration of HTTP requests in milliseconds")
            .with_unit("ms")
            .build()
    })
    .clone()
}
