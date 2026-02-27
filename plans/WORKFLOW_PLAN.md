# Workflow Implementation Plan: DAG Support via `on_finished`

This document outlines the design and implementation plan for enabling DAG-style workflows in Dispenser. The goal is to allow one service to trigger other services upon successful completion, enabling complex pipelines (e.g., ETL jobs, backups) while maintaining explicit configuration semantics.

## 1. Conceptual Design

The system will move from a purely "pull/wait" model to a hybrid model that supports "push" notifications.

*   **Explicit Triggers**: Instead of inferring dependencies, services will explicitly declare which downstream services to trigger using a new `on_finished` configuration key.
*   **Semantics**: The `initialize = "on-trigger"` setting retains its current meaning: "Do not start automatically; wait for a signal." That signal can now come from another service in addition to Cron or Image Watchers.
*   **Mechanism**: Services will communicate via asynchronous channels. When Service A completes successfully (exit code 0), it sends a "start" signal to the channels of services defined in `on_finished`.

## 2. Configuration Changes

### `service.toml`

We will add a new optional field `on_finished` to the `[dispenser]` section.

```toml
# upstream-service/service.toml
[service]
name = "data-import"
# ...

[dispenser]
watch = false
initialize = "immediately"
# NEW: List of service names to trigger when this service exits with code 0
on_finished = ["data-processing", "notification-service"]
```

```toml
# downstream-service/service.toml
[service]
name = "data-processing"
# ...

[dispenser]
# This service waits. It starts when triggered by "data-import"
initialize = "on-trigger"
```

## 3. Implementation Details

### A. Data Structures (`src/service/file.rs`)

Update the `DispenserConfig` struct to include the new field.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DispenserConfig {
    pub watch: bool,
    #[serde(default)]
    pub initialize: Initialize,
    pub cron: Option<Schedule>,
    #[serde(default)]
    pub pull: PullOptions,
    
    // NEW FIELD
    #[serde(default)]
    pub on_finished: Vec<String>,
}
```

### B. Service Communication (`src/service/manager.rs`)

The `ServicesManager` is responsible for wiring the services together.

1.  **Channel Creation**: For every service, create a generic MPSC channel (`trigger_tx`, `trigger_rx`) used to receive start signals.
2.  **Router Map**: Build a `HashMap<String, Sender>` that maps service names to their trigger senders.
3.  **Distribution**:
    *   Pass the specific `trigger_rx` to the `ServiceInstance` so it can listen.
    *   Pass the full `HashMap<String, Sender>` (wrapped in an `Arc`) to the `ServiceInstance` so it can send triggers to others.

### C. Instance Logic (`src/service/instance.rs`)

The `ServiceInstance` needs updates in two places: the polling loop (receiving) and the execution logic (sending).

#### 1. Receiving Triggers (The Poll Loop)

The `poll` method currently sleeps for a delay interval. It needs to become reactive.

```rust
// Pseudo-code concept
pub async fn poll(&self, poll_images: bool, init: bool) {
    // ... existing init logic ...

    tokio::select! {
        // Existing periodic check
        _ = sleep(delay) => {
             // ... check cron ...
             // ... check image updates ...
        }
        
        // NEW: Listen for triggers
        _ = self.trigger_rx.recv() => {
             log::info!("Received trigger from upstream service");
             if self.config.dispenser.initialize == Initialize::OnTrigger {
                 self.run_container().await;
             }
        }
    }
}
```

#### 2. Sending Triggers (Post-Execution)

The `run_container` method currently fires off the start command. We need to monitor the container if it has downstream triggers.

```rust
// Pseudo-code concept
pub async fn run_container(&self) -> Result<(), ServiceConfigError> {
    // ... wait for depends_on ...
    // ... start container ...

    // NEW: Spawn a background task to wait for completion
    if !self.config.dispenser.on_finished.is_empty() {
        let docker = get_docker();
        let triggers = self.config.dispenser.on_finished.clone();
        let router = self.trigger_router.clone();
        let name = self.config.service.name.clone();

        tokio::spawn(async move {
            // Wait for the container to stop
            let wait_stream = docker.wait_container(&name, ...);
            
            if let Ok(exit_code) = wait_stream.await {
                if exit_code == 0 {
                    log::info!("Service {} finished. Triggering: {:?}", name, triggers);
                    for target in triggers {
                         if let Some(sender) = router.get(&target) {
                             let _ = sender.send(()); 
                         }
                    }
                }
            }
        });
    }
    
    Ok(())
}
```

## 4. Edge Cases & Considerations

1.  **Startup Order**: Triggers rely on channels established during `ServicesManager::from_config`. Since all channels are created before any polling starts, the wiring is safe.
2.  **Loops**: If A triggers B, and B triggers A, they will loop indefinitely. This is valid behavior (e.g., a continuous processing cycle), but we should ensure it doesn't crash the stack (using async tasks prevents stack overflow).
3.  **Duplicate Triggers**: If Service A and Service B both trigger Service C simultaneously, the channel receiver will pick up two messages.
    *   *Mitigation*: We should ensure `run_container` checks if the container is *already* running or queued to run to prevent double-starting or errors.
4.  **Fan-in Dependencies**: If C depends on A and B (`depends_on`), and both A and B trigger C (`on_finished`):
    *   A finishes -> Triggers C. C wakes up, checks `depends_on`. B is not ready. C goes back to sleep or aborts run.
    *   B finishes -> Triggers C. C wakes up, checks `depends_on`. A and B are ready. C runs.
    *   *Refinement*: The `run_container` logic must robustly handle "dependencies not met" by simply logging and returning, rather than retrying indefinitely in a blocking manner, so it doesn't clog the trigger channel processing.

## 5. Next Steps

1.  Modify `DispenserConfig` struct in `src/service/file.rs`.
2.  Refactor `ServiceInstance` struct to hold `trigger_rx` and `trigger_router`.
3.  Update `ServicesManager` in `src/service/manager.rs` to initialize and distribute channels.
4.  Implement the `select!` logic in `poll()` and the wait-notify logic in `run_container()`.