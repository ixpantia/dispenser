use super::events::DispenserEvent;
use super::schema::{
    create_container_output_table, create_deployments_table, create_host_cpu_table,
    create_host_disk_table, create_host_memory_table, create_logs_table, create_status_table,
    create_traces_table,
};
use crate::service::file::TelemetryConfig;
use log::{error, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{Mutex, Notify};
use tokio::sync::mpsc::Receiver;
use uuid::Uuid;

const FLUSH_INTERVAL: Duration = Duration::from_secs(30); // 30 seconds

// ENOSPC is 28 on both Linux and macOS.
const ENOSPC: i32 = 28;

fn is_disk_full(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(ENOSPC)
}

pub struct TelemetryService {
    config: TelemetryConfig,
    rx: Receiver<DispenserEvent>,
    writers: TelemetryWriters,
    telemetry_dir: PathBuf,
    last_maintenance: Instant,
    worker_active: Arc<AtomicBool>,
    drain_notify: Arc<Notify>,
}

struct TelemetryWriters {
    deployments: Mutex<Option<BufWriter<File>>>,
    status: Mutex<Option<BufWriter<File>>>,
    logs: Mutex<Option<BufWriter<File>>>,
    traces: Mutex<Option<BufWriter<File>>>,
    container_output: Mutex<Option<BufWriter<File>>>,
    host_cpu: Mutex<Option<BufWriter<File>>>,
    host_disk: Mutex<Option<BufWriter<File>>>,
    host_memory: Mutex<Option<BufWriter<File>>>,
}

impl TelemetryWriters {
    fn new() -> Self {
        Self {
            deployments: Mutex::new(None),
            status: Mutex::new(None),
            logs: Mutex::new(None),
            traces: Mutex::new(None),
            container_output: Mutex::new(None),
            host_cpu: Mutex::new(None),
            host_disk: Mutex::new(None),
            host_memory: Mutex::new(None),
        }
    }

    fn all(&self) -> [&Mutex<Option<BufWriter<File>>>; 8] {
        [
            &self.deployments,
            &self.status,
            &self.logs,
            &self.traces,
            &self.container_output,
            &self.host_cpu,
            &self.host_disk,
            &self.host_memory,
        ]
    }
}

impl TelemetryService {
    pub async fn new(config: TelemetryConfig, rx: Receiver<DispenserEvent>) -> Self {
        let telemetry_dir = PathBuf::from("./.dispenser/telemetry");
        let active_dir = telemetry_dir.join("active");
        if let Err(e) = fs::create_dir_all(&active_dir).await {
            if is_disk_full(&e) {
                error!(
                    "Telemetry disk full: cannot create active directory {:?}: {}",
                    active_dir, e
                );
            } else {
                error!(
                    "Failed to create telemetry active directory {:?}: {}",
                    active_dir, e
                );
            }
        }

        Self {
            config,
            rx,
            writers: TelemetryWriters::new(),
            telemetry_dir,
            last_maintenance: Instant::now(),
            worker_active: Arc::new(AtomicBool::new(false)),
            drain_notify: Arc::new(Notify::new()),
        }
    }

    async fn get_or_open_writer<'a>(
        &self,
        writer_mutex: &'a Mutex<Option<BufWriter<File>>>,
        filename: &str,
    ) -> tokio::sync::MutexGuard<'a, Option<BufWriter<File>>> {
        let mut writer_opt = writer_mutex.lock().await;
        if writer_opt.is_none() {
            let active_dir = self.telemetry_dir.join("active");
            let path = active_dir.join(filename);
            if let Err(e) = fs::create_dir_all(&active_dir).await {
                if is_disk_full(&e) {
                    error!(
                        "Telemetry disk full: cannot create active directory {:?}: {}",
                        active_dir, e
                    );
                } else {
                    error!(
                        "Failed to create telemetry active directory {:?}: {}",
                        active_dir, e
                    );
                }
            } else {
                match OpenOptions::new().create(true).append(true).open(&path).await {
                    Ok(file) => *writer_opt = Some(BufWriter::new(file)),
                    Err(e) => {
                        if is_disk_full(&e) {
                            error!(
                                "Telemetry disk full: cannot open telemetry file {:?}: {}",
                                path, e
                            );
                        } else {
                            error!("Failed to open telemetry file {:?}: {}", path, e);
                        }
                    }
                }
            }
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
        if let Err(e) = create_host_cpu_table(&self.config.table_uri_host_cpu()).await {
            error!("Failed to initialize host CPU table: {}", e);
        }
        if let Err(e) = create_host_memory_table(&self.config.table_uri_host_memory()).await {
            error!("Failed to initialize host memory table: {}", e);
        }
        if let Err(e) = create_host_disk_table(&self.config.table_uri_host_disk()).await {
            error!("Failed to initialize host disk table: {}", e);
        }

        let mut flush_interval = tokio::time::interval(FLUSH_INTERVAL);
        flush_interval.tick().await;

        // Startup recovery: enforce the pending cap on leftovers from a
        // previous run, then start draining any orphaned batches.
        self.enforce_pending_cap().await;
        self.maybe_spawn_worker().await;

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
                    self.enforce_pending_cap().await;
                    self.maybe_spawn_worker().await;
                }
                _ = self.drain_notify.notified() => {
                    self.maybe_spawn_worker().await;
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
            DispenserEvent::HostCpu(_) => (&self.writers.host_cpu, "host-cpu.jsonl"),
            DispenserEvent::HostDisk(_) => (&self.writers.host_disk, "host-disk.jsonl"),
            DispenserEvent::HostMemory(_) => (&self.writers.host_memory, "host-memory.jsonl"),
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

    async fn flush(&self) {
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
            let has_files = match fs::read_dir(&active_dir).await {
                Ok(mut entries) => matches!(entries.next_entry().await, Ok(Some(_))),
                Err(_) => false,
            };
            if !has_files {
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
            if is_disk_full(&e) {
                error!(
                    "Telemetry disk full: cannot create batch directory {:?}: {}",
                    batch_dir, e
                );
            } else {
                error!("Failed to create batch directory {:?}: {}", batch_dir, e);
            }
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

        let duration = start.elapsed();
        if duration.as_secs() > 1 {
            warn!("Telemetry rotation took {:?}", duration);
        }
    }

    async fn enforce_pending_cap(&self) {
        let pending_dir = self.telemetry_dir.join("pending");
        let max_size_bytes = self.config.max_pending_size_mb.saturating_mul(1024 * 1024);
        if let Err(e) = enforce_pending_cap_on_dir(
            &pending_dir,
            self.config.max_pending_batches,
            max_size_bytes,
        )
        .await
        {
            error!("Failed to enforce telemetry pending cap: {}", e);
        }
    }

    async fn maybe_spawn_worker(&mut self) {
        if self.worker_active.load(Ordering::SeqCst) {
            return;
        }

        let pending_dir = self.telemetry_dir.join("pending");
        let oldest = match list_pending_batches(&pending_dir).await {
            Ok(batches) => batches.into_iter().next(),
            Err(e) => {
                error!("Failed to list pending telemetry batches: {}", e);
                None
            }
        };
        let Some(batch) = oldest else {
            return;
        };

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

        let run_maintenance = self
            .config
            .maintenance
            .as_ref()
            .and_then(|m_cfg| {
                (m_cfg.enabled
                    && self.last_maintenance.elapsed().as_secs() >= m_cfg.interval_seconds)
                    .then(|| {
                        self.last_maintenance = Instant::now();
                        true
                    })
            })
            .unwrap_or(false);

        let mut cmd = tokio::process::Command::new(exe);
        cmd.arg("telemetry-flush")
            .arg("--batch-path")
            .arg(&batch.path)
            .arg("--config")
            .arg(config_json);

        if run_maintenance {
            cmd.arg("--maintenance");
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn telemetry worker: {}", e);
                return;
            }
        };

        info!(
            "Spawned telemetry worker (PID: {:?}) for batch {:?}",
            child.id(),
            batch.path
        );

        self.worker_active.store(true, Ordering::SeqCst);

        let worker_active = Arc::clone(&self.worker_active);
        let drain_notify = Arc::clone(&self.drain_notify);
        let worker_timeout = Duration::from_secs(self.config.worker_timeout_secs);

        tokio::spawn(async move {
            let result = tokio::time::timeout(worker_timeout, child.wait()).await;
            match result {
                Ok(Ok(status)) if status.success() => {
                    info!("Telemetry worker finished successfully");
                    // Drain the next pending batch right away so the backlog
                    // is processed as fast as the downstream allows.
                    drain_notify.notify_one();
                }
                Ok(Ok(status)) => {
                    error!(
                        "Telemetry worker failed with status: {}. Batch kept for retry.",
                        status
                    );
                }
                Ok(Err(e)) => {
                    error!("Failed to wait for telemetry worker: {}", e);
                }
                Err(_) => {
                    error!(
                        "Telemetry worker timed out after {:?}; killing it. Batch kept for retry.",
                        worker_timeout
                    );
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
            }
            worker_active.store(false, Ordering::SeqCst);
        });
    }
}

struct PendingBatch {
    path: PathBuf,
    size: u64,
}

async fn list_pending_batches(pending_dir: &Path) -> std::io::Result<Vec<PendingBatch>> {
    let mut batches = Vec::new();
    let mut entries = fs::read_dir(pending_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let is_dir = entry
            .file_type()
            .await
            .map(|ft| ft.is_dir())
            .unwrap_or(false);
        if !is_dir {
            continue;
        }

        let path = entry.path();
        let mut size = 0;
        if let Ok(mut files) = fs::read_dir(&path).await {
            while let Ok(Some(file)) = files.next_entry().await {
                if let Ok(meta) = file.metadata().await
                    && meta.is_file()
                {
                    size += meta.len();
                }
            }
        }

        batches.push(PendingBatch { path, size });
    }

    // Batch directories are named after UUIDv7s, so path order is creation order.
    batches.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(batches)
}

async fn enforce_pending_cap_on_dir(
    pending_dir: &Path,
    max_batches: u32,
    max_size_bytes: u64,
) -> std::io::Result<()> {
    let batches = match list_pending_batches(pending_dir).await {
        Ok(batches) => batches,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    let mut total_size: u64 = batches.iter().map(|b| b.size).sum();
    let mut count = batches.len();

    for batch in &batches {
        if count <= max_batches as usize && total_size <= max_size_bytes {
            break;
        }

        if let Err(e) = fs::remove_dir_all(&batch.path).await {
            error!("Failed to evict telemetry batch {:?}: {}", batch.path, e);
            continue;
        }

        warn!(
            "Evicted telemetry batch {:?} (pending cap exceeded: {} batches, {} bytes on disk)",
            batch.path, count, total_size
        );
        count -= 1;
        total_size = total_size.saturating_sub(batch.size);
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    Deployments,
    Status,
    Logs,
    Traces,
    ContainerOutput,
    HostCpu,
    HostDisk,
    HostMemory,
}

pub type TableType = EventType;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pending_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dispenser-telemetry-test-{}",
            Uuid::now_v7()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    async fn create_batch(pending: &Path, name: &str, content: &str) {
        let batch = pending.join(name);
        std::fs::create_dir_all(&batch).unwrap();
        std::fs::write(batch.join("status.jsonl"), content).unwrap();
    }

    fn batch_names(pending: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(pending)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn cap_evicts_oldest_batches_first() {
        let pending = temp_pending_dir();
        for i in 0..5 {
            create_batch(&pending, &format!("batch-{i:04}"), "x".repeat(10).as_str()).await;
        }

        enforce_pending_cap_on_dir(&pending, 3, u64::MAX)
            .await
            .unwrap();

        assert_eq!(
            batch_names(&pending),
            vec!["batch-0002", "batch-0003", "batch-0004"]
        );
        std::fs::remove_dir_all(&pending).unwrap();
    }

    #[tokio::test]
    async fn cap_evicts_by_total_size() {
        let pending = temp_pending_dir();
        for i in 0..3 {
            create_batch(&pending, &format!("batch-{i:04}"), &"x".repeat(100)).await;
        }

        enforce_pending_cap_on_dir(&pending, u32::MAX, 250)
            .await
            .unwrap();

        assert_eq!(batch_names(&pending), vec!["batch-0001", "batch-0002"]);
        std::fs::remove_dir_all(&pending).unwrap();
    }

    #[tokio::test]
    async fn cap_noop_under_limits() {
        let pending = temp_pending_dir();
        create_batch(&pending, "batch-0000", "hello").await;

        enforce_pending_cap_on_dir(&pending, 10, 1024 * 1024)
            .await
            .unwrap();

        assert_eq!(batch_names(&pending), vec!["batch-0000"]);
        std::fs::remove_dir_all(&pending).unwrap();
    }

    #[tokio::test]
    async fn cap_handles_missing_dir() {
        let pending = temp_pending_dir().join("does-not-exist");
        enforce_pending_cap_on_dir(&pending, 10, 1024)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cap_ignores_non_directory_entries() {
        let pending = temp_pending_dir();
        std::fs::write(pending.join("stray.txt"), "junk").unwrap();
        create_batch(&pending, "batch-0000", "x").await;

        enforce_pending_cap_on_dir(&pending, 1, u64::MAX)
            .await
            .unwrap();

        assert_eq!(batch_names(&pending), vec!["batch-0000"]);
        assert!(pending.join("stray.txt").exists());
        std::fs::remove_dir_all(&pending).unwrap();
    }
}
