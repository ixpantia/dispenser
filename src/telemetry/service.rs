use super::events::DispenserEvent;
use super::schema::{
    create_container_output_table, create_deployments_table, create_logs_table,
    create_status_table, create_traces_table,
};
use crate::service::file::TelemetryConfig;
use log::{error, info, warn};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Mutex;
use tokio::sync::mpsc::Receiver;
use uuid::Uuid;

const FLUSH_INTERVAL: Duration = Duration::from_secs(30); // 30 seconds

pub struct TelemetryService {
    config: TelemetryConfig,
    rx: Receiver<DispenserEvent>,
    writers: TelemetryWriters,
    telemetry_dir: PathBuf,
    last_maintenance: Instant,
}

struct TelemetryWriters {
    deployments: Mutex<Option<BufWriter<File>>>,
    status: Mutex<Option<BufWriter<File>>>,
    logs: Mutex<Option<BufWriter<File>>>,
    traces: Mutex<Option<BufWriter<File>>>,
    container_output: Mutex<Option<BufWriter<File>>>,
}

impl TelemetryWriters {
    fn new() -> Self {
        Self {
            deployments: Mutex::new(None),
            status: Mutex::new(None),
            logs: Mutex::new(None),
            traces: Mutex::new(None),
            container_output: Mutex::new(None),
        }
    }

    fn all(&self) -> [&Mutex<Option<BufWriter<File>>>; 5] {
        [
            &self.deployments,
            &self.status,
            &self.logs,
            &self.traces,
            &self.container_output,
        ]
    }
}

impl TelemetryService {
    pub async fn new(config: TelemetryConfig, rx: Receiver<DispenserEvent>) -> Self {
        let telemetry_dir = PathBuf::from("./.dispenser/telemetry");
        let active_dir = telemetry_dir.join("active");
        fs::create_dir_all(&active_dir)
            .await
            .expect("Failed to create telemetry active directory");

        Self {
            config,
            rx,
            writers: TelemetryWriters::new(),
            telemetry_dir,
            last_maintenance: Instant::now(),
        }
    }

    async fn get_or_open_writer<'a>(
        &self,
        writer_mutex: &'a Mutex<Option<BufWriter<File>>>,
        filename: &str,
    ) -> tokio::sync::MutexGuard<'a, Option<BufWriter<File>>> {
        let mut writer_opt = writer_mutex.lock().await;
        if writer_opt.is_none() {
            let path = self.telemetry_dir.join("active").join(filename);
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await
                .expect("Failed to open telemetry file");
            *writer_opt = Some(BufWriter::new(file));
        }
        writer_opt
    }

    pub async fn run(mut self) {
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

        let mut flush_interval = tokio::time::interval(FLUSH_INTERVAL);
        flush_interval.tick().await;

        loop {
            tokio::select! {
                maybe_event = self.rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            self.handle_event(event).await;
                        }
                        None => {
                            info!("Telemetry channel closed, flushing remaining events");
                            self.flush().await;
                            break;
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    self.flush().await;
                }
            }
        }
        info!("Telemetry service stopped");
    }

    async fn handle_event(&self, event: DispenserEvent) {
        let (writer_mutex, filename) = match &event {
            DispenserEvent::Deployment(_) => (&self.writers.deployments, "deployments.jsonl"),
            DispenserEvent::ContainerStatus(_) => (&self.writers.status, "status.jsonl"),
            DispenserEvent::LogBatch(_) => (&self.writers.logs, "logs.jsonl"),
            DispenserEvent::SpanBatch(_) => (&self.writers.traces, "traces.jsonl"),
            DispenserEvent::ContainerOutput(_) => {
                (&self.writers.container_output, "container-output.jsonl")
            }
        };

        let mut writer_opt = self.get_or_open_writer(writer_mutex, filename).await;
        if let Some(writer) = writer_opt.as_mut() {
            if let Ok(json) = serde_json::to_string(&event) {
                if let Err(e) = writer.write_all(json.as_bytes()).await {
                    error!("Failed to write telemetry event to disk: {}", e);
                } else if let Err(e) = writer.write_all(b"\n").await {
                    error!("Failed to write newline to disk: {}", e);
                }
            }
        }
    }

    async fn flush(&mut self) {
        let start = Instant::now();

        // 1. Acquire locks and close all writers by setting them to None
        let mut any_data = false;
        for writer_mutex in self.writers.all() {
            let mut writer_opt = writer_mutex.lock().await;
            if let Some(mut writer) = writer_opt.take() {
                if let Err(e) = writer.flush().await {
                    error!("Failed to flush telemetry writer: {}", e);
                }
                any_data = true;
                // Dropping the writer here closes the file
            }
        }

        if !any_data {
            // Check if there are any active files that weren't open but exist
            let active_dir = self.telemetry_dir.join("active");
            if let Ok(mut entries) = fs::read_dir(&active_dir).await {
                if entries.next_entry().await.unwrap().is_none() {
                    return;
                }
            } else {
                return;
            }
        }

        // 2. Prepare rotation
        let batch_uuid = Uuid::now_v7();
        let batch_dir = self
            .telemetry_dir
            .join("pending")
            .join(batch_uuid.to_string());

        if let Err(e) = fs::create_dir_all(&batch_dir).await {
            error!("Failed to create batch directory {:?}: {}", batch_dir, e);
            return;
        }

        // 3. Move active files to pending batch
        let active_dir = self.telemetry_dir.join("active");
        match fs::read_dir(&active_dir).await {
            Ok(mut entries) => {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    let dest = batch_dir.join(path.file_name().unwrap());
                    if let Err(e) = fs::rename(&path, &dest).await {
                        error!("Failed to move {:?} to {:?}: {}", path, dest, e);
                    }
                }
            }
            Err(e) => error!("Failed to read active telemetry directory: {}", e),
        }

        // 4. Spawn worker process
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to get current executable path: {}", e);
                return;
            }
        };

        let config_json = match serde_json::to_string(&self.config) {
            Ok(j) => j,
            Err(e) => {
                error!("Failed to serialize telemetry config: {}", e);
                return;
            }
        };

        let run_maintenance = self.config.maintenance.as_ref().and_then(|m_cfg| {
            (m_cfg.enabled && self.last_maintenance.elapsed().as_secs() >= m_cfg.interval_seconds)
                .then(|| {
                    self.last_maintenance = Instant::now();
                    true
                })
        }).unwrap_or(false);

        let mut cmd = tokio::process::Command::new(exe);
        cmd.arg("telemetry-flush")
            .arg("--batch-path")
            .arg(&batch_dir)
            .arg("--config")
            .arg(config_json);

        if run_maintenance {
            cmd.arg("--maintenance");
        }

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn telemetry worker: {}", e);
                return;
            }
        };

        info!(
            "Spawned telemetry worker (PID: {:?}) for batch {}",
            child.id(),
            batch_uuid
        );

        let duration = start.elapsed();
        if duration.as_secs() > 1 {
            warn!("Telemetry rotation took {:?}", duration);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    Deployments,
    Status,
    Logs,
    Traces,
    ContainerOutput,
}

pub type TableType = EventType;
