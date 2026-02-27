use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use log::{debug, error, info, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use super::events::DispenserEvent;
use super::otlp::{LogsData, TracesData};

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
    Json(payload): Json<LogsData>,
) -> Result<(), (StatusCode, String)> {
    debug!("Received OTLP log batch");
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
    Json(payload): Json<TracesData>,
) -> Result<(), (StatusCode, String)> {
    debug!("Received OTLP trace batch");
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
    Json(_payload): Json<serde_json::Value>,
) -> Result<(), (StatusCode, String)> {
    // Metrics are mentioned as future work in the plan.
    warn!("Received metrics batch - metrics are not yet supported");
    Ok(())
}
