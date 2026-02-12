# Observability Plan

Currently, Dispenser monitors container lifecycle events (deployments) and health status. The next step is to allow services running *inside* Dispenser to emit high-volume telemetry data (Logs, Traces, and Metrics) using standard OpenTelemetry (OTel) SDKs.

This data will be ingested by Dispenser and stored asynchronously in Delta Lake, allowing users to query logs and traces using tools like Athena, Trino, or Spark.

## Architecture

### 1. The Ingestion Service (Sidecar Host)
Dispenser will spawn a new async task (the **Ingestion Service**) that listens on a **TCP Socket**. While currently listening on all interfaces (`0.0.0.0`) for simplicity, it is reached by containers via the host gateway.

*   **Interface**: `0.0.0.0` (Currently all interfaces, reachable via host gateway)
*   **Port**: `4318` (Standard OTLP/HTTP port)
*   **Protocol**: OpenTelemetry Protocol (OTLP) over HTTP (`POST`).

This design ensures:
1.  **Robustness**: Using `host.docker.internal` (mapped to `host-gateway`) allows containers to reach the host-bound ingestion service regardless of the specific bridge subnet.
2.  **Compatibility**: Standard OTel SDKs (Java, Node, Python, Go, Rust) work out-of-the-box by setting the exporter endpoint.

### 2. Container Integration
All containers managed by Dispenser are automatically attached to the `dispenser` bridge network and configured with an `extra_hosts` entry for `host.docker.internal:host-gateway`.

*Example (Environment Variable):*
`OTEL_EXPORTER_OTLP_ENDPOINT="http://host.docker.internal:4318"`

Dispenser automatically injects this environment variable into all containers if telemetry is enabled.

### 3. Data Processing Pipeline
1.  **Ingest**: The Ingestion Service (built with `axum`) receives OTLP/JSON batches.
2.  **Convert**: JSON payloads are deserialized into internal Rust structs.
3.  **Buffer**: Data is pushed into `LogsBuffer`, `SpansBuffer`, or `MetricsBuffer` (new buffers in `src/telemetry`).
4.  **Flush**: On interval or size limit, buffers are written to Delta Lake as Parquet files.

### 4. Container Output Capture
Dispenser will capture `stdout` and `stderr` from all managed containers.
*   **Mechanism**: Streaming via Docker API (`docker logs --follow`).
*   **Integration**: `ServiceInstance` spawns a background task for each container to read the stream line-by-line.
*   **Storage**: Stored in a separate Delta table (`dispenser-container-output`) to avoid polluting structured OTel logs.

## API Endpoints

The Ingestion Service will support the standard OTLP/HTTP endpoints:

| Endpoint | Method | Content-Type | Description |
| :--- | :--- | :--- | :--- |
| `/v1/logs` | `POST` | `application/json` | Ingests log records. |
| `/v1/traces` | `POST` | `application/json` | Ingests spans/traces. |
| `/v1/metrics` | `POST` | `application/json` | (Future) Ingests metrics. |

## Data Model & Schema

Dispenser will create new Delta tables for these telemetry types.

### 1. Logs Table (`dispenser-logs`)
Optimized for searching logs by service, severity, and time.

| Column | Type | Description |
| :--- | :--- | :--- |
| `date` | `DATE` | Partition column (UTC). |
| `timestamp` | `TIMESTAMP` | Exact time of the log entry. |
| `service` | `STRING` | Service name (from OTel resource). |
| `severity` | `STRING` | INFO, WARN, ERROR, etc. |
| `body` | `STRING` | The log message. |
| `trace_id` | `STRING` | Associated trace ID (hex). |
| `span_id` | `STRING` | Associated span ID (hex). |
| `attributes` | `MAP<STRING, STRING>` | Flattened attributes. |
| `resource` | `MAP<STRING, STRING>` | Resource attributes (pod, node, etc). |

### 2. Traces Table (`dispenser-traces`)
Optimized for visualizing distributed traces (waterfall charts).

| Column | Type | Description |
| :--- | :--- | :--- |
| `date` | `DATE` | Partition column. |
| `trace_id` | `STRING` | Trace ID (32-char hex). |
| `span_id` | `STRING` | Span ID (16-char hex). |
| `parent_span_id` | `STRING` | Parent Span ID. |
| `name` | `STRING` | Span name (e.g., "GET /api/users"). |
| `kind` | `STRING` | SERVER, CLIENT, PRODUCER, etc. |
| `start_time` | `TIMESTAMP` | Start time. |
| `end_time` | `TIMESTAMP` | End time. |
| `duration_ms` | `LONG` | Calculated duration. |
| `status_code` | `STRING` | OK, ERROR. |
| `status_message` | `STRING` | Error description. |
| `service` | `STRING` | Service name. |
| `attributes` | `MAP<STRING, STRING>` | Span attributes. |
| `events` | `ARRAY<STRUCT>` | Span events (exceptions, logs inside spans). |

### 3. Container Output Table (`dispenser-container-output`)
Captures raw `stdout` and `stderr` streams from containers.

| Column | Type | Description |
| :--- | :--- | :--- |
| `date` | `DATE` | Partition column. |
| `timestamp` | `TIMESTAMP` | Exact time of the log line. |
| `service` | `STRING` | Service name. |
| `container_id` | `STRING` | Container ID. |
| `stream` | `STRING` | `stdout` or `stderr`. |
| `message` | `STRING` | The raw log line. |

## Implementation Plan

### Phase 1: Dependencies & HTTP Server
- Add `axum` (lightweight, robust HTTP server).
- Create `src/telemetry/ingestion.rs`.
- Implement the HTTP listener bound to `0.0.0.0:4318`.

### Phase 2: Data Structures & Buffers
- Define Rust structs matching the OTLP JSON schema in `src/telemetry/otlp.rs`.
- Update `TelemetryService` to handle `LogBatch` and `SpanBatch` events.
- Implement `LogsBuffer` and `SpansBuffer` with higher capacity than deployment buffers.

### Phase 3: Wiring
- Modify `src/main.rs` to start the Ingestion Service task if telemetry is enabled.
- Ensure the `dispenser` network gateway IP is stable (it is hardcoded to `172.28.0.1` in `src/service/network.rs`).

### Phase 4: Delta Lake Integration
- Create table schemas in `src/telemetry/schema.rs`.
- Implement the flush logic to write OTLP batches to Parquet.

### Phase 5: Container Output Capture
- Implement a streaming log watcher in `src/service/instance.rs`.
- Create `ContainerOutputBuffer` and the corresponding Delta table schema.
- Wire up the log watcher to push events to `TelemetryService`.

## Open Questions / Future Work

1.  **Protobuf Support**: OTLP/gRPC or OTLP/HTTP+Protobuf is more efficient. However, supporting it requires significant additional dependencies (`prost`, `opentelemetry-proto`) and complexity. **Decision**: Protobuf support is deferred and is not in the current roadmap. We will launch with JSON-only support.
2.  **Backpressure**: If the write buffer is full, should we block the HTTP request (slowing the app) or return 503/429?
    *   *Decision*: Return HTTP 429 (Too Many Requests) to signal the SDK to retry or drop.
3.  **Sampling**: Should Dispenser implement head-based sampling?
    *   *Decision*: V1 will accept all data. Sampling is complex to coordinate across distributed services.