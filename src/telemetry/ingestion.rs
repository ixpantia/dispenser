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
use tokio::sync::mpsc::Sender;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

use super::events::DispenserEvent;

/// Port for OTLP/HTTP as per OpenTelemetry specification.
pub const OTLP_HTTP_PORT: u16 = 4318;

/// State shared across axum handlers.
struct AppState {
    tx: Sender<DispenserEvent>,
}

/// Start the OTLP Ingestion Service.
/// This service listens on all interfaces (0.0.0.0) to receive telemetry
/// from containers in the Dispenser network.
pub async fn start_ingestion_service(
    tx: Sender<DispenserEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = Arc::new(AppState { tx });

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

    axum::serve(listener, app).await.map_err(|e| e.into())
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

    state
        .tx
        .try_send(DispenserEvent::LogBatch(payload))
        .map_err(|_| {
            error!("Failed to queue OTLP log batch: buffer full");
            (
                StatusCode::TOO_MANY_REQUESTS,
                "Telemetry buffer full".to_string(),
            )
        })?;
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

    state
        .tx
        .try_send(DispenserEvent::SpanBatch(payload))
        .map_err(|_| {
            error!("Failed to queue OTLP trace batch: buffer full");
            (
                StatusCode::TOO_MANY_REQUESTS,
                "Telemetry buffer full".to_string(),
            )
        })?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[tokio::test]
    async fn test_ingest_logs() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let state = Arc::new(AppState { tx });

        let payload = ExportLogsServiceRequest::default();
        let mut buf = Vec::new();
        payload.encode(&mut buf).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-protobuf"),
        );

        let result = ingest_logs(State(state), headers, Bytes::from(buf)).await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        match event {
            DispenserEvent::LogBatch(_) => {}
            _ => panic!("Expected LogBatch"),
        }
    }

    #[tokio::test]
    async fn test_ingest_traces() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let state = Arc::new(AppState { tx });

        let payload = ExportTraceServiceRequest::default();
        let mut buf = Vec::new();
        payload.encode(&mut buf).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-protobuf"),
        );

        let result = ingest_traces(State(state), headers, Bytes::from(buf)).await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        match event {
            DispenserEvent::SpanBatch(_) => {}
            _ => panic!("Expected SpanBatch"),
        }
    }

    #[tokio::test]
    async fn test_ingest_metrics() {
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let state = Arc::new(AppState { tx });

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-protobuf"),
        );

        let result = ingest_metrics(State(state), headers, Bytes::new()).await;
        assert!(result.is_ok());
    }
}
