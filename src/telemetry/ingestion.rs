use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use log::{debug, error, info, warn};
use prost::Message;
use std::net::SocketAddr;
use std::sync::Arc;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

use crate::telemetry::service::TelemetryBuffers;

/// Port for OTLP/HTTP as per OpenTelemetry specification.
pub const OTLP_HTTP_PORT: u16 = 4318;

/// State shared across axum handlers.
struct AppState {
    buffers: Arc<TelemetryBuffers>,
}

/// Start the OTLP Ingestion Service.
/// This service listens on all interfaces (0.0.0.0) to receive telemetry
/// from containers in the Dispenser network.
pub async fn start_ingestion_service(
    buffers: Arc<TelemetryBuffers>,
    shutdown_signal: Arc<tokio::sync::Notify>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = Arc::new(AppState { buffers });

    let app = Router::new()
        .route("/v1/logs", post(ingest_logs))
        .route("/v1/traces", post(ingest_traces))
        .route("/v1/metrics", post(ingest_metrics))
        .with_state(state);

    // Bind to all interfaces. In future versions, this may be restricted
    // to the dispenser bridge gateway only for security.
    let addr = SocketAddr::from(([0, 0, 0, 0], OTLP_HTTP_PORT));

    info!("Starting OTLP ingestion service on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal.notified().await;
            info!("OTLP ingestion service shutting down");
        })
        .await
        .map_err(|e| e.into())
}

/// Handler for OTLP logs.
async fn ingest_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<(), (StatusCode, String)> {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    debug!("Received OTLP log batch");

    let payload = if content_type.contains("application/json") {
        serde_json::from_slice(&bytes).map_err(|e| {
            error!("Failed to decode OTLP log batch from JSON: {}", e);
            (StatusCode::BAD_REQUEST, "Invalid JSON payload".to_string())
        })?
    } else if content_type.contains("application/x-protobuf") {
        ExportLogsServiceRequest::decode(bytes).map_err(|e| {
            error!("Failed to decode OTLP log batch: {}", e);
            (
                StatusCode::BAD_REQUEST,
                "Invalid protobuf payload".to_string(),
            )
        })?
    } else {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected application/x-protobuf or application/json".to_string(),
        ));
    };

    state.buffers.push_logs_event(payload).await;

    Ok(())
}

/// Handler for OTLP traces.
async fn ingest_traces(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<(), (StatusCode, String)> {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    debug!("Received OTLP trace batch");

    let payload = if content_type.contains("application/json") {
        serde_json::from_slice(&bytes).map_err(|e| {
            error!("Failed to decode OTLP trace batch from JSON: {}", e);
            (StatusCode::BAD_REQUEST, "Invalid JSON payload".to_string())
        })?
    } else if content_type.contains("application/x-protobuf") {
        ExportTraceServiceRequest::decode(bytes).map_err(|e| {
            error!("Failed to decode OTLP trace batch: {}", e);
            (
                StatusCode::BAD_REQUEST,
                "Invalid protobuf payload".to_string(),
            )
        })?
    } else {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected application/x-protobuf or application/json".to_string(),
        ));
    };

    state.buffers.push_span(payload).await;

    Ok(())
}

/// Handler for OTLP metrics.
async fn ingest_metrics(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
    _bytes: Bytes,
) -> Result<(), (StatusCode, String)> {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("application/x-protobuf")
        && !content_type.contains("application/json")
    {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected application/x-protobuf or application/json".to_string(),
        ));
    }

    // Metrics are mentioned as future work in the plan.
    warn!("Received metrics batch - metrics are not yet supported");
    Ok(())
}
