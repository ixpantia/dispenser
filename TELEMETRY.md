# Telemetry Configuration

Dispenser includes a built-in, high-performance telemetry system powered by [Delta Lake](https://delta.io/). It allows you to automatically collect deployment events and container health status, writing them directly to data lakes (S3, GCS, Azure) or local filesystems in Parquet format.

## Overview

The telemetry system runs in a dedicated, isolated thread to ensure that heavy I/O operations never block the main orchestration loop. It provides:

1.  **Deployment Tracking**: Every time a container is created, updated, or restarted, a detailed event is logged.
2.  **Health Monitoring**: Periodically samples the status of all managed containers (CPU, memory, uptime, health checks).
3.  **Delta Lake Integration**: Writes data using the Delta Lake protocol, enabling ACID transactions, scalable metadata handling, and direct compatibility with tools like Spark, Trino, Athena, and Databricks.

## Configuration

Telemetry is disabled by default. To enable it, add a `[telemetry]` section to your global `dispenser.toml` configuration file.

```toml
# dispenser.toml

[telemetry]
enabled = true

# URIs for the Delta tables. 
# Supported schemes: file://, s3://, gs://, az://, adls://
table_uri_deployments = "s3://my-data-lake/dispenser/deployments"
table_uri_status = "s3://my-data-lake/dispenser/status"

# Optional: How often to sample container status (default: 60 seconds)
status_interval = 60

# Optional: Number of events to buffer in memory before flushing to storage (default: 1000)
# Lower values write more frequently (lower latency) but create more small files.
buffer_size = 1000
```

### Supported Storage Backends

Dispenser uses the `deltalake` Rust crate, which supports multiple storage backends natively. The backend is determined by the URI scheme.

#### 1. Local Filesystem (`file://`)
Writes to a local directory. Useful for development or single-node setups where an external agent (like Fluentbit) ships the logs.

```toml
table_uri_deployments = "file:///var/log/dispenser/deployments"
```

#### 2. AWS S3 (`s3://`)
Writes directly to Amazon S3.

**Authentication:**
Dispenser automatically looks for credentials in the environment:
*   Environment variables: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_REGION`.
*   IAM Instance Profiles (if running on EC2).
*   IAM Roles for Service Accounts (IRSA) (if running on EKS).

#### 3. Google Cloud Storage (`gs://`)
Writes directly to Google Cloud Storage.

**Authentication:**
*   Environment variable: `GOOGLE_APPLICATION_CREDENTIALS` pointing to your Service Account JSON key file.
*   Workload Identity (if running on GKE/GCE).

#### 4. Azure Blob Storage / Data Lake Gen2 (`az://` or `adls://`)
Writes directly to Azure Storage.

**Authentication:**
Dispenser supports several authentication methods via environment variables:
*   Storage Account Key: `AZURE_STORAGE_ACCOUNT`, `AZURE_STORAGE_KEY`.
*   Service Principal: `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`.
*   Managed Identity (if running on Azure VMs/AKS).

## Data Schemas

Dispenser automatically manages two Delta tables. It will create them if they do not exist.

### Deployments Table (`dispenser-deployments`)

Records every lifecycle event that results in a container change (creation, recreation, update).

| Column | Type | Description |
| :--- | :--- | :--- |
| `date` | `DATE` | Partition column. Derived from timestamp. |
| `timestamp` | `TIMESTAMP (UTC)` | Exact time of the event. |
| `service` | `STRING` | Service name defined in `service.toml`. |
| `image` | `STRING` | Image name and tag (e.g., `nginx:latest`). |
| `image_sha` | `STRING` | SHA256 digest of the image. |
| `image_size_mb` | `LONG` | Size of the image in MB. |
| `container_id` | `STRING` | Docker container ID. |
| `container_created_at` | `TIMESTAMP (UTC)` | Creation time reported by Docker. |
| `trigger_type` | `STRING` | Cause of deployment (`startup`, `cron`, `image_update`, `manual_reload`). |
| `dispenser_version` | `STRING` | Version of the Dispenser binary. |
| `restart_policy` | `STRING` | Configured restart policy. |
| `memory_limit` | `STRING` | Configured memory limit. |
| `cpu_limit` | `STRING` | Configured CPU limit. |
| `proxy_enabled` | `BOOLEAN` | Whether the service uses the reverse proxy. |
| `proxy_host` | `STRING` | Hostname configured for the proxy. |
| `port_mappings_count` | `INTEGER` | Number of exposed ports. |
| `volume_count` | `INTEGER` | Number of mounted volumes. |
| `network_count` | `INTEGER` | Number of connected networks. |

### Container Status Table (`dispenser-container-status`)

Records periodic snapshots of the runtime state of containers.

| Column | Type | Description |
| :--- | :--- | :--- |
| `date` | `DATE` | Partition column. Derived from timestamp. |
| `timestamp` | `TIMESTAMP (UTC)` | Exact time of the sample. |
| `service` | `STRING` | Service name. |
| `container_id` | `STRING` | Short container ID. |
| `state` | `STRING` | Execution state (e.g., `running`, `exited`, `dead`). |
| `health_status` | `STRING` | Docker healthcheck status (`healthy`, `unhealthy`, `starting`, `none`). |
| `exit_code` | `INTEGER` | Exit code (only relevant if state is `exited`). |
| `restart_count` | `INTEGER` | Number of times Docker has restarted this container. |
| `uptime_seconds` | `LONG` | Seconds since the container started. |
| `failing_streak` | `INTEGER` | Consecutive healthcheck failures. |
| `last_health_output` | `STRING` | Output of the last failed healthcheck (truncated). |

## Performance Tuning

### Buffering & Latency

The `buffer_size` setting controls the trade-off between latency and file fragmentation.

*   **Small Buffer (e.g., 1-10)**: Events appear in the data lake almost instantly. However, this generates many small Parquet files ("small file problem"), which can degrade query performance and increase storage costs.
*   **Large Buffer (e.g., 1000-5000)**: Events are batched into larger files. This is much more efficient for query engines (Presto/Trino, Spark) but introduces a delay before data is visible.

Dispenser also enforces a time-based flush every **5 minutes** to ensure data is not held in memory indefinitely during periods of low activity.

### Resource Isolation

The telemetry service runs on a dedicated Tokio runtime spawned in a separate OS thread. This design ensures that network latency when talking to S3/GCS or CPU-intensive compression of Parquet files does not impact the responsiveness of the main Dispenser loop or the reverse proxy.

## Data Management

### Partitioning & Optimization

*   **Partitioning**: Data is automatically partitioned by `date`. Query engines should always filter by date (e.g., `WHERE date = CURRENT_DATE`) for optimal performance.
*   **Target File Size**: Dispenser aims for **32MB** Parquet files to balance ingestion latency with query efficiency.
*   **Data Skipping**: Key columns like `service` and `container_id` are indexed in Delta Lake stats to allow engines to skip irrelevant files.

### Retention Policies

To prevent indefinite storage growth, Dispenser applies the following default retention policies during table creation:

*   **Log Retention**: 30 days (Deployments), 7 days (Status). Delta log history is kept for time-travel queries.
*   **Deleted Files**: 7 days (Deployments), 1 day (Status). Vacuum operations can reclaim space after this period.