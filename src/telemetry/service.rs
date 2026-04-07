use super::buffer::{
    ContainerOutputBuffer, DeploymentsBuffer, LogsBuffer, SpansBuffer, StatusBuffer,
};
use super::schema::{
    create_container_output_table, create_deployments_table, create_logs_table,
    create_status_table, create_traces_table,
};
use crate::service::file::TelemetryConfig;
use crate::telemetry::events::{ContainerOutputEvent, ContainerStatusEvent, DeploymentEvent};
use deltalake::datafusion::catalog::Session;
use deltalake::datafusion::execution::disk_manager::DiskManagerMode;
use deltalake::datafusion::execution::memory_pool::FairSpillPool;
use deltalake::datafusion::execution::runtime_env::RuntimeEnvBuilder;
use deltalake::datafusion::execution::DiskManager;
use deltalake::delta_datafusion::DeltaSessionContext;
use deltalake::{DeltaTable, DeltaTableError};
use log::{error, info, warn};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use url::Url;

const FLUSH_INTERVAL: Duration = Duration::from_secs(30); // 30 seconds

pub struct TelemetryBuffers {
    deployments_buffer: Mutex<DeploymentsBuffer>,
    status_buffer: Mutex<StatusBuffer>,
    logs_buffer: Mutex<LogsBuffer>,
    spans_buffer: Mutex<SpansBuffer>,
    container_output_buffer: Mutex<ContainerOutputBuffer>,
}

impl std::fmt::Debug for TelemetryBuffers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TelemetryBuffers")
    }
}

impl TelemetryBuffers {
    pub fn new() -> Self {
        TelemetryBuffers {
            deployments_buffer: Mutex::new(DeploymentsBuffer::new(64)),
            status_buffer: Mutex::new(StatusBuffer::new(64)),
            logs_buffer: Mutex::new(LogsBuffer::new(64)), // Small initial capacity to prevent fragmentation
            spans_buffer: Mutex::new(SpansBuffer::new(64)),
            container_output_buffer: Mutex::new(ContainerOutputBuffer::new(64)),
        }
    }
}

impl TelemetryBuffers {
    pub async fn push_deployments_event<'a>(&self, event: DeploymentEvent<'a>) {
        self.deployments_buffer.lock().await.push(event);
    }
    pub async fn push_status_event<'a>(&self, event: ContainerStatusEvent<'a>) {
        self.status_buffer.lock().await.push(event);
    }
    pub async fn push_logs_event(&self, events: ExportLogsServiceRequest) {
        self.logs_buffer.lock().await.push_logs_data(events);
    }
    pub async fn push_span(&self, event: ExportTraceServiceRequest) {
        self.spans_buffer.lock().await.push_traces_data(event);
    }
    pub async fn push_container_output<'a>(&self, event: ContainerOutputEvent<'a>) {
        self.container_output_buffer.lock().await.push(event);
    }
}

pub struct TelemetryService {
    config: TelemetryConfig,
    datafusion_session_state: Arc<dyn Session>,
    buffers: Arc<TelemetryBuffers>,
}

/// 10MB
const POOL_SIZE: usize = 10 * 1024 * 1024;

impl TelemetryService {
    pub fn new(config: TelemetryConfig, buffers: Arc<TelemetryBuffers>) -> Self {
        let memory_pool = Arc::new(FairSpillPool::new(POOL_SIZE));

        let disk_manager = DiskManager::builder().with_mode(DiskManagerMode::OsTmpDirectory);

        let runtime_env = RuntimeEnvBuilder::new()
            .with_memory_pool(memory_pool)
            .with_disk_manager_builder(disk_manager)
            .build_arc()
            .expect("Unable to buld memory pool");

        let datafusion_session_state =
            Arc::new(DeltaSessionContext::with_runtime_env(runtime_env.into()).state())
                as Arc<dyn Session>;

        Self {
            config,
            datafusion_session_state,
            buffers,
        }
    }

    pub async fn flush_deployments_buffer(&self) {
        let mut deployments_buffer = self.buffers.deployments_buffer.lock().await;
        if !deployments_buffer.is_empty() {
            let count = deployments_buffer.len();

            match deployments_buffer.into_record_batch() {
                Ok(batch) => {
                    drop(deployments_buffer);
                    if let Err(e) = self
                        .write_to_delta(
                            &self.config.table_uri_deployments(),
                            batch,
                            TableType::Deployments,
                        )
                        .await
                    {
                        error!("Failed to write deployment events to Delta Lake: {:?}", e);
                    } else {
                        info!("Flushed {} deployment events to Delta Lake", count);
                    }
                }
                Err(e) => error!("Failed to create record batch for deployments: {:?}", e),
            }
        }
    }

    pub async fn flush_status_buffer(&self) {
        let mut status_buffer = self.buffers.status_buffer.lock().await;
        if !status_buffer.is_empty() {
            let count = status_buffer.len();

            match status_buffer.into_record_batch() {
                Ok(batch) => {
                    drop(status_buffer);
                    if let Err(e) = self
                        .write_to_delta(&self.config.table_uri_status(), batch, TableType::Status)
                        .await
                    {
                        error!("Failed to write status events to Delta Lake: {:?}", e);
                    } else {
                        info!("Flushed {} status events to Delta Lake", count);
                    }
                }
                Err(e) => error!("Failed to create record batch for status: {:?}", e),
            }
        }
    }

    pub async fn flush_logs_buffer(&self) {
        let mut logs_buffer = self.buffers.logs_buffer.lock().await;
        if !logs_buffer.is_empty() {
            let count = logs_buffer.len();

            match logs_buffer.into_record_batch() {
                Ok(batch) => {
                    drop(logs_buffer);
                    if let Err(e) = self
                        .write_to_delta(&self.config.table_uri_logs(), batch, TableType::Logs)
                        .await
                    {
                        error!("Failed to write log events to Delta Lake: {:?}", e);
                    } else {
                        info!("Flushed {} log events to Delta Lake", count);
                    }
                }
                Err(e) => error!("Failed to create record batch for logs: {:?}", e),
            }
        }
    }

    pub async fn flush_spans_buffer(&self) {
        let mut spans_buffer = self.buffers.spans_buffer.lock().await;
        if !spans_buffer.is_empty() {
            let count = spans_buffer.len();

            match spans_buffer.into_record_batch() {
                Ok(batch) => {
                    drop(spans_buffer);
                    if let Err(e) = self
                        .write_to_delta(&self.config.table_uri_traces(), batch, TableType::Traces)
                        .await
                    {
                        error!("Failed to write trace events to Delta Lake: {:?}", e);
                    } else {
                        info!("Flushed {} trace events to Delta Lake", count);
                    }
                }
                Err(e) => error!("Failed to create record batch for traces: {:?}", e),
            }
        }
    }

    pub async fn flush_container_output_buffer(&self) {
        let mut container_output_buffer = self.buffers.container_output_buffer.lock().await;
        if !container_output_buffer.is_empty() {
            let count = container_output_buffer.len();

            match container_output_buffer.into_record_batch() {
                Ok(batch) => {
                    drop(container_output_buffer);
                    if let Err(e) = self
                        .write_to_delta(
                            &self.config.table_uri_container_output(),
                            batch,
                            TableType::ContainerOutput,
                        )
                        .await
                    {
                        error!(
                            "Failed to write container output events to Delta Lake: {:?}",
                            e
                        );
                    } else {
                        info!("Flushed {} container output events to Delta Lake", count);
                    }
                }
                Err(e) => error!(
                    "Failed to create record batch for container output: {:?}",
                    e
                ),
            }
        }
    }

    pub async fn run(self, shutdown_signal: Arc<tokio::sync::Notify>) {
        info!("Telemetry service started");

        // Ensure tables exist on startup
        if let Err(e) = create_deployments_table(&self.config.table_uri_deployments()).await {
            error!("Failed to initialize deployments table: {}", e);
        }
        if let Err(e) = create_status_table(&self.config.table_uri_status()).await {
            error!("Failed to initialize status table: {}", e);
        }
        if let Err(e) = create_logs_table(&self.config.table_uri_logs()).await {
            error!("Failed to initialize logs table: {}", e);
        }
        if let Err(e) = create_traces_table(&self.config.table_uri_traces()).await {
            error!("Failed to initialize traces table: {}", e);
        }
        if let Err(e) =
            create_container_output_table(&self.config.table_uri_container_output()).await
        {
            error!("Failed to initialize container output table: {}", e);
        }

        // Start with a tick so we don't wait 5 mins for the first check if needed,
        // but actually we only want to flush if time passes.
        // interval.tick() completes immediately the first time.
        let mut flush_interval = tokio::time::interval(FLUSH_INTERVAL);
        // Consume the first immediate tick
        flush_interval.tick().await;

        loop {
            tokio::select! {
                _ = shutdown_signal.notified() => {
                    info!("Telemetry service received shutdown signal");
                    break;
                }
                _ = flush_interval.tick() => {
                    self.flush().await;
                }
            }
        }

        info!("Telemetry service performing final flush...");
        self.flush().await;
        info!("Telemetry service stopped");
    }

    async fn flush(&self) {
        let start = Instant::now();

        self.flush_deployments_buffer().await;
        self.flush_status_buffer().await;
        self.flush_logs_buffer().await;
        self.flush_spans_buffer().await;
        self.flush_container_output_buffer().await;

        let duration = start.elapsed();
        if duration.as_secs() > 1 {
            warn!("Telemetry flush took {:?}", duration);
        }
    }

    async fn write_to_delta(
        &self,
        table_uri: &Url,
        batch: arrow::record_batch::RecordBatch,
        table_type: TableType,
    ) -> Result<(), DeltaTableError> {
        let table = match DeltaTable::try_from_url(table_uri.clone()).await {
            Ok(table) => table,
            Err(DeltaTableError::NotATable(_)) | Err(DeltaTableError::InvalidTableLocation(_)) => {
                match table_type {
                    TableType::Deployments => create_deployments_table(table_uri).await?,
                    TableType::Status => create_status_table(table_uri).await?,
                    TableType::Logs => create_logs_table(table_uri).await?,
                    TableType::Traces => create_traces_table(table_uri).await?,
                    TableType::ContainerOutput => create_container_output_table(table_uri).await?,
                }
            }
            Err(e) => return Err(e),
        };

        let _ops = table
            .write(vec![batch])
            .with_session_state(self.datafusion_session_state.clone())
            .with_save_mode(deltalake::protocol::SaveMode::Append)
            .await?;
        Ok(())
    }
}

enum TableType {
    Deployments,
    Status,
    Logs,
    Traces,
    ContainerOutput,
}
