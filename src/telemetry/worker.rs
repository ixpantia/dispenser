use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use deltalake::DeltaTable;
use deltalake::datafusion::catalog::Session;
use deltalake::datafusion::execution::runtime_env::RuntimeEnvBuilder;
use deltalake::delta_datafusion::DeltaSessionContext;
use serde_json;
use tokio::io::AsyncReadExt;

use crate::service::file::TelemetryConfig;
use crate::telemetry::buffer::{
    ContainerOutputBuffer, DeploymentsBuffer, HostCpuBuffer, HostDiskBuffer, HostMemoryBuffer,
    LogsBuffer, SpansBuffer, StatusBuffer,
};
use crate::telemetry::events::DispenserEvent;
use crate::telemetry::service::TableType;

pub async fn run_worker(config: TelemetryConfig, maintenance: bool) -> ExitCode {
    let mut stdin = tokio::io::stdin();
    let mut input = String::new();
    if let Err(e) = stdin.read_to_string(&mut input).await {
        log::error!("Failed to read telemetry batch paths from stdin: {}", e);
        return ExitCode::FAILURE;
    }

    let batch_paths = parse_batch_paths(&input);
    if batch_paths.is_empty() {
        log::info!("No telemetry batches to process.");
        return ExitCode::SUCCESS;
    }

    log::info!(
        "Telemetry worker started for {} batch(es), maintenance: {}",
        batch_paths.len(),
        maintenance
    );

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

    let batch_timeout = Duration::from_secs(config.worker_timeout_secs);
    let mut all_succeeded = true;

    // Batches are assigned oldest-first. Stop at the first failure so the
    // failed batch stays at the head of the queue for the next worker to
    // retry, preserving the oldest-first ordering.
    for batch_path in &batch_paths {
        let result = tokio::time::timeout(
            batch_timeout,
            process_batch(batch_path, &config, &session_state),
        )
        .await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                log::error!(
                    "Failed to process telemetry batch {:?}: {}. Batch kept for retry.",
                    batch_path,
                    e
                );
                all_succeeded = false;
                break;
            }
            Err(_) => {
                log::error!(
                    "Telemetry batch {:?} timed out after {:?}. Batch kept for retry.",
                    batch_path,
                    batch_timeout
                );
                all_succeeded = false;
                break;
            }
        }
    }

    if all_succeeded {
        log::info!("Successfully processed all assigned telemetry batches.");

        if maintenance {
            if let Some(m_cfg) = &config.maintenance {
                log::info!("Running maintenance operations on telemetry tables.");
                let retention_hours = std::cmp::max(m_cfg.retention_hours, 168);
                let retention_duration = chrono::Duration::hours(retention_hours as i64);

                let tables = vec![
                    ("deployments", config.table_uri_deployments()),
                    ("status", config.table_uri_status()),
                    ("logs", config.table_uri_logs()),
                    ("traces", config.table_uri_traces()),
                    ("container_output", config.table_uri_container_output()),
                    ("host_cpu", config.table_uri_host_cpu()),
                    ("host_disk", config.table_uri_host_disk()),
                    ("host_memory", config.table_uri_host_memory()),
                ];

                for (name, table_uri) in tables {
                    log::info!("Starting maintenance for table: {}", name);
                    match deltalake::open_table(table_uri).await {
                        Ok(table) => {
                            // Optimize
                            let table = match table.optimize().await {
                                Ok((t, metrics)) => {
                                    log::info!("Optimize successful for {}: {:?}", name, metrics);
                                    t
                                }
                                Err(e) => {
                                    log::error!("Optimize failed for {}: {}", name, e);
                                    continue;
                                }
                            };

                            // Vacuum
                            match table
                                .vacuum()
                                .with_retention_period(retention_duration)
                                .with_enforce_retention_duration(true)
                                .await
                            {
                                Ok((_t, metrics)) => {
                                    log::info!("Vacuum successful for {}: {:?}", name, metrics);
                                }
                                Err(e) => {
                                    log::error!("Vacuum failed for {}: {}", name, e);
                                }
                            }
                        }
                        Err(e) => {
                            log::info!(
                                "Table {} may not exist yet, skipping maintenance: {}",
                                name,
                                e
                            );
                        }
                    }
                }
            }
        }

        ExitCode::SUCCESS
    } else {
        log::error!("Some telemetry batches were not processed. Batch directories NOT deleted.");
        ExitCode::FAILURE
    }
}

fn parse_batch_paths(input: &str) -> Vec<PathBuf> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

async fn process_batch(
    batch_path: &Path,
    config: &TelemetryConfig,
    session_state: &Arc<dyn Session>,
) -> Result<(), String> {
    let entries = match fs::read_dir(batch_path) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // The batch was evicted or cleaned up while this worker was
            // starting; there is nothing left to do for it.
            log::warn!(
                "Telemetry batch {:?} no longer exists, skipping.",
                batch_path
            );
            return Ok(());
        }
        Err(e) => {
            return Err(format!("failed to read batch directory: {}", e));
        }
    };

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
            "host-cpu.jsonl" => TableType::HostCpu,
            "host-disk.jsonl" => TableType::HostDisk,
            "host-memory.jsonl" => TableType::HostMemory,
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
            TableType::HostCpu => config.table_uri_host_cpu(),
            TableType::HostDisk => config.table_uri_host_disk(),
            TableType::HostMemory => config.table_uri_host_memory(),
        };

        if let Err(e) = process_file(&path, &table_uri, table_type, session_state).await {
            log::error!("Failed to process file {:?}: {}", path, e);
            success = false;
        }
    }

    if !success {
        return Err("some telemetry writes failed".to_string());
    }

    log::info!("Successfully processed all telemetry files in batch.");
    if let Err(e) = fs::remove_dir_all(batch_path) {
        log::error!("Failed to cleanup batch directory {:?}: {}", batch_path, e);
        // We still return success as data was written
    }

    Ok(())
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
        TableType::HostCpu => {
            let mut buffer = HostCpuBuffer::new(100);
            for line in content.lines() {
                if let Ok(DispenserEvent::HostCpu(e)) = serde_json::from_str(line) {
                    buffer.push(&e);
                    count += 1;
                }
            }
            if !buffer.is_empty() {
                let batch = buffer.into_record_batch()?;
                write_to_delta(table_uri, batch, session_state).await?;
            }
        }
        TableType::HostMemory => {
            let mut buffer = HostMemoryBuffer::new(100);
            for line in content.lines() {
                if let Ok(DispenserEvent::HostMemory(e)) = serde_json::from_str(line) {
                    buffer.push(&e);
                    count += 1;
                }
            }
            if !buffer.is_empty() {
                let batch = buffer.into_record_batch()?;
                write_to_delta(table_uri, batch, session_state).await?;
            }
        }
        TableType::HostDisk => {
            let mut buffer = HostDiskBuffer::new(100);
            for line in content.lines() {
                if let Ok(DispenserEvent::HostDisk(e)) = serde_json::from_str(line) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_paths_in_order() {
        let input = "/tmp/batch-1\n/tmp/batch-2\n/tmp/batch-3\n";
        assert_eq!(
            parse_batch_paths(input),
            vec![
                PathBuf::from("/tmp/batch-1"),
                PathBuf::from("/tmp/batch-2"),
                PathBuf::from("/tmp/batch-3"),
            ]
        );
    }

    #[test]
    fn skips_blank_lines_and_trims_whitespace() {
        let input = "  /tmp/a  \n\n\t\n/tmp/b\n   ";
        assert_eq!(
            parse_batch_paths(input),
            vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]
        );
    }

    #[test]
    fn empty_input_yields_no_paths() {
        assert!(parse_batch_paths("").is_empty());
        assert!(parse_batch_paths("\n\n  \n").is_empty());
    }
}
