use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use deltalake::DeltaTable;
use deltalake::datafusion::catalog::Session;
use deltalake::datafusion::execution::runtime_env::RuntimeEnvBuilder;
use deltalake::delta_datafusion::DeltaSessionContext;
use serde_json;

use crate::service::file::TelemetryConfig;
use crate::telemetry::buffer::{
    ContainerOutputBuffer, DeploymentsBuffer, LogsBuffer, SpansBuffer, StatusBuffer,
};
use crate::telemetry::events::DispenserEvent;
use crate::telemetry::service::TableType;

pub async fn run_worker(batch_path: PathBuf, config: TelemetryConfig) -> ExitCode {
    log::info!("Telemetry worker started for batch: {:?}", batch_path);

    let entries = match fs::read_dir(&batch_path) {
        Ok(entries) => entries,
        Err(e) => {
            log::error!("Failed to read batch directory: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let runtime_env = match RuntimeEnvBuilder::new()
        .with_memory_limit(64 * 1024 * 1024, 1.0) // 64MB limit
        .build_arc()
    {
        Ok(env) => env,
        Err(e) => {
            log::error!("Failed to build DataFusion runtime environment: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let session_state = Arc::new(DeltaSessionContext::with_runtime_env(runtime_env.into()).state())
        as Arc<dyn Session>;

    let mut success = true;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }

        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let table_type = match filename {
            "deployments.jsonl" => TableType::Deployments,
            "status.jsonl" => TableType::Status,
            "logs.jsonl" => TableType::Logs,
            "traces.jsonl" => TableType::Traces,
            "container-output.jsonl" => TableType::ContainerOutput,
            _ => {
                log::warn!("Unknown telemetry file type: {:?}", path);
                continue;
            }
        };

        let table_uri = match table_type {
            TableType::Deployments => config.table_uri_deployments(),
            TableType::Status => config.table_uri_status(),
            TableType::Logs => config.table_uri_logs(),
            TableType::Traces => config.table_uri_traces(),
            TableType::ContainerOutput => config.table_uri_container_output(),
        };

        if let Err(e) = process_file(&path, &table_uri, table_type, &session_state).await {
            log::error!("Failed to process file {:?}: {}", path, e);
            success = false;
        }
    }

    if success {
        log::info!("Successfully processed all telemetry files in batch.");
        if let Err(e) = fs::remove_dir_all(&batch_path) {
            log::error!("Failed to cleanup batch directory {:?}: {}", batch_path, e);
            // We still return success as data was written
        }
        ExitCode::SUCCESS
    } else {
        log::error!("Some telemetry writes failed. Batch directory NOT deleted.");
        ExitCode::FAILURE
    }
}

async fn process_file(
    path: &PathBuf,
    table_uri: &url::Url,
    table_type: TableType,
    session_state: &Arc<dyn deltalake::datafusion::catalog::Session>,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let mut count = 0;

    match table_type {
        TableType::Deployments => {
            let mut buffer = DeploymentsBuffer::new(100);
            for line in content.lines() {
                if let Ok(DispenserEvent::Deployment(e)) = serde_json::from_str(line) {
                    buffer.push(&e);
                    count += 1;
                }
            }
            if !buffer.is_empty() {
                let batch = buffer.into_record_batch()?;
                write_to_delta(table_uri, batch, session_state).await?;
            }
        }
        TableType::Status => {
            let mut buffer = StatusBuffer::new(100);
            for line in content.lines() {
                if let Ok(DispenserEvent::ContainerStatus(e)) = serde_json::from_str(line) {
                    buffer.push(&e);
                    count += 1;
                }
            }
            if !buffer.is_empty() {
                let batch = buffer.into_record_batch()?;
                write_to_delta(table_uri, batch, session_state).await?;
            }
        }
        TableType::Logs => {
            let mut buffer = LogsBuffer::new(100);
            for line in content.lines() {
                if let Ok(DispenserEvent::LogBatch(e)) = serde_json::from_str(line) {
                    buffer.push_logs_data(&e);
                    count += 1;
                }
            }
            if !buffer.is_empty() {
                let batch = buffer.into_record_batch()?;
                write_to_delta(table_uri, batch, session_state).await?;
            }
        }
        TableType::Traces => {
            let mut buffer = SpansBuffer::new(100);
            for line in content.lines() {
                if let Ok(DispenserEvent::SpanBatch(e)) = serde_json::from_str(line) {
                    buffer.push_traces_data(&e);
                    count += 1;
                }
            }
            if !buffer.is_empty() {
                let batch = buffer.into_record_batch()?;
                write_to_delta(table_uri, batch, session_state).await?;
            }
        }
        TableType::ContainerOutput => {
            let mut buffer = ContainerOutputBuffer::new(100);
            for line in content.lines() {
                if let Ok(DispenserEvent::ContainerOutput(e)) = serde_json::from_str(line) {
                    buffer.push(&e);
                    count += 1;
                }
            }
            if !buffer.is_empty() {
                let batch = buffer.into_record_batch()?;
                write_to_delta(table_uri, batch, session_state).await?;
            }
        }
    }

    log::info!("Processed {} events from {:?}", count, path);
    Ok(())
}

async fn write_to_delta(
    table_uri: &url::Url,
    batch: arrow::record_batch::RecordBatch,
    session_state: &Arc<dyn deltalake::datafusion::catalog::Session>,
) -> Result<(), deltalake::DeltaTableError> {
    let table = DeltaTable::try_from_url(table_uri.clone()).await?;

    table
        .write(vec![batch])
        .with_save_mode(deltalake::protocol::SaveMode::Append)
        .with_session_fallback_policy(
            deltalake::delta_datafusion::SessionFallbackPolicy::RequireSessionState,
        )
        .with_session_state(Arc::clone(session_state))
        .with_configuration([
            ("delta.autoOptimize.autoCompact", Some("false")),
            ("delta.autoOptimize.optimizeWrite", Some("false")),
        ])
        .await?;
    Ok(())
}
