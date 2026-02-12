use super::buffer::{
    ContainerOutputBuffer, DeploymentsBuffer, LogsBuffer, SpansBuffer, StatusBuffer,
};
use super::events::DispenserEvent;
use super::schema::{
    create_container_output_table, create_deployments_table, create_logs_table,
    create_status_table, create_traces_table,
};
use crate::service::file::TelemetryConfig;
use deltalake::{DeltaOps, DeltaTableError};
use log::{error, info, warn};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Receiver;

const DEFAULT_BUFFER_SIZE: usize = 1000;
const FLUSH_INTERVAL: Duration = Duration::from_secs(60); // 1 minute

pub struct TelemetryService {
    config: TelemetryConfig,
    rx: Receiver<DispenserEvent>,
    deployments_buffer: DeploymentsBuffer,
    status_buffer: StatusBuffer,
    logs_buffer: LogsBuffer,
    spans_buffer: SpansBuffer,
    container_output_buffer: ContainerOutputBuffer,
    buffer_limit: usize,
}

impl TelemetryService {
    pub fn new(config: TelemetryConfig, rx: Receiver<DispenserEvent>) -> Self {
        let buffer_limit = config.buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
        Self {
            config,
            rx,
            deployments_buffer: DeploymentsBuffer::new(buffer_limit),
            status_buffer: StatusBuffer::new(buffer_limit),
            logs_buffer: LogsBuffer::new(buffer_limit * 10), // Logs and spans can be higher volume
            spans_buffer: SpansBuffer::new(buffer_limit * 10),
            container_output_buffer: ContainerOutputBuffer::new(buffer_limit * 10),
            buffer_limit,
        }
    }

    pub async fn run(mut self) {
        info!("Telemetry service started");
        // Start with a tick so we don't wait 5 mins for the first check if needed,
        // but actually we only want to flush if time passes.
        // interval.tick() completes immediately the first time.
        let mut flush_interval = tokio::time::interval(FLUSH_INTERVAL);
        // Consume the first immediate tick
        flush_interval.tick().await;

        loop {
            tokio::select! {
                maybe_event = self.rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            self.handle_event(event);
                            if self.should_flush() {
                                self.flush().await;
                            }
                        }
                        None => {
                            info!("Telemetry channel closed, flushing remaining events");
                            self.flush().await;
                            break;
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    if !self.deployments_buffer.is_empty()
                        || !self.status_buffer.is_empty()
                        || !self.logs_buffer.is_empty()
                        || !self.spans_buffer.is_empty()
                        || !self.container_output_buffer.is_empty()
                    {
                        self.flush().await;
                    }
                }
            }
        }
        info!("Telemetry service stopped");
    }

    fn handle_event(&mut self, event: DispenserEvent) {
        match event {
            DispenserEvent::Deployment(e) => self.deployments_buffer.push(&e),
            DispenserEvent::ContainerStatus(e) => self.status_buffer.push(&e),
            DispenserEvent::LogBatch(data) => self.logs_buffer.push_logs_data(&data),
            DispenserEvent::SpanBatch(data) => self.spans_buffer.push_traces_data(&data),
            DispenserEvent::ContainerOutput(e) => self.container_output_buffer.push(&e),
        }
    }

    fn should_flush(&self) -> bool {
        self.deployments_buffer.len() >= self.buffer_limit
            || self.status_buffer.len() >= self.buffer_limit
            || self.logs_buffer.len() >= self.buffer_limit * 10
            || self.spans_buffer.len() >= self.buffer_limit * 10
            || self.container_output_buffer.len() >= self.buffer_limit * 10
    }

    async fn flush(&mut self) {
        let start = Instant::now();

        // Flush Deployments
        if !self.deployments_buffer.is_empty() {
            let count = self.deployments_buffer.len();
            let old_buffer = std::mem::replace(
                &mut self.deployments_buffer,
                DeploymentsBuffer::new(self.buffer_limit),
            );

            match old_buffer.into_record_batch() {
                Ok(batch) => {
                    if let Err(e) = self
                        .write_to_delta(
                            &self.config.table_uri_deployments,
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

        // Flush Status
        if !self.status_buffer.is_empty() {
            let count = self.status_buffer.len();
            let old_buffer = std::mem::replace(
                &mut self.status_buffer,
                StatusBuffer::new(self.buffer_limit),
            );

            match old_buffer.into_record_batch() {
                Ok(batch) => {
                    if let Err(e) = self
                        .write_to_delta(&self.config.table_uri_status, batch, TableType::Status)
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

        // Flush Logs
        if !self.logs_buffer.is_empty() {
            let count = self.logs_buffer.len();
            let old_buffer = std::mem::replace(
                &mut self.logs_buffer,
                LogsBuffer::new(self.buffer_limit * 10),
            );

            match old_buffer.into_record_batch() {
                Ok(batch) => {
                    if let Err(e) = self
                        .write_to_delta(&self.config.table_uri_logs, batch, TableType::Logs)
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

        // Flush Spans
        if !self.spans_buffer.is_empty() {
            let count = self.spans_buffer.len();
            let old_buffer = std::mem::replace(
                &mut self.spans_buffer,
                SpansBuffer::new(self.buffer_limit * 10),
            );

            match old_buffer.into_record_batch() {
                Ok(batch) => {
                    if let Err(e) = self
                        .write_to_delta(&self.config.table_uri_traces, batch, TableType::Traces)
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

        // Flush Container Output
        if !self.container_output_buffer.is_empty() {
            let count = self.container_output_buffer.len();
            let old_buffer = std::mem::replace(
                &mut self.container_output_buffer,
                ContainerOutputBuffer::new(self.buffer_limit * 10),
            );

            match old_buffer.into_record_batch() {
                Ok(batch) => {
                    if let Err(e) = self
                        .write_to_delta(
                            &self.config.table_uri_container_output,
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

        let duration = start.elapsed();
        if duration.as_secs() > 1 {
            warn!("Telemetry flush took {:?}", duration);
        }
    }

    async fn write_to_delta(
        &self,
        table_uri: &str,
        batch: arrow::record_batch::RecordBatch,
        table_type: TableType,
    ) -> Result<(), DeltaTableError> {
        let table = match deltalake::open_table(table_uri).await {
            Ok(table) => table,
            Err(DeltaTableError::NotATable(_)) => match table_type {
                TableType::Deployments => create_deployments_table(table_uri).await?,
                TableType::Status => create_status_table(table_uri).await?,
                TableType::Logs => create_logs_table(table_uri).await?,
                TableType::Traces => create_traces_table(table_uri).await?,
                TableType::ContainerOutput => create_container_output_table(table_uri).await?,
            },
            Err(e) => return Err(e),
        };

        let ops = DeltaOps(table);
        ops.write(vec![batch])
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
