use std::{sync::Arc, time::Duration};

use futures_util::future;
use tokio::{sync::Mutex, task::JoinSet};

use crate::service::{
    file::{EntrypointFile, ServiceFile},
    instance::{CronWatcher, ServiceInstance},
    manifest::ImageWatcher,
    network::NetworkInstance,
    vars::{render_template, ServiceConfigError, ServiceVarsMaterialized},
};

struct ServiceManagerInner {
    instances: Vec<Arc<Mutex<ServiceInstance>>>,
    networks: Vec<NetworkInstance>,
    delay: Duration,
}

pub struct ServicesManager {
    pub service_names: Vec<String>,
    inner: Mutex<ServiceManagerInner>,
    cancel_tx: tokio::sync::mpsc::Sender<()>,
    cancel_rx: Mutex<tokio::sync::mpsc::Receiver<()>>,
}

impl ServicesManager {
    pub async fn from_config(config: EntrypointFile) -> Result<Self, ServiceConfigError> {
        // Get the delay from config (in seconds)
        let delay = Duration::from_secs(config.delay);
        let mut instances = Vec::new();
        let mut networks = Vec::new();
        let mut service_names = Vec::new();

        // Load and materialize variables once for all services
        let vars = ServiceVarsMaterialized::try_init().await?;

        // Process networks first - create NetworkInstance objects
        for network_entry in config.networks {
            let network = NetworkInstance::from(network_entry);
            networks.push(network);
        }

        // Ensure all networks exist before creating services
        for network in &networks {
            if let Err(e) = network.ensure_exists().await {
                log::error!("Failed to ensure network {} exists: {}", network.name, e);
                return Err(ServiceConfigError::Io(e));
            }
        }

        // Iterate through each service entry in the config
        for entry in config.services {
            // Construct the path to service.toml
            let service_toml_path = entry.path.join("service.toml");

            // Read the service.toml file
            let service_file_content = tokio::fs::read_to_string(&service_toml_path).await?;

            // Render the template with variables
            let rendered_service = render_template(&service_file_content, &vars)?;

            // Parse the rendered config as TOML
            let service_file: ServiceFile = toml::from_str(&rendered_service)?;

            // Initialize the image watcher if watch is enabled
            let image_watcher = if service_file.dispenser.watch {
                Some(ImageWatcher::initialize(&service_file.service.image).await)
            } else {
                None
            };

            // Create cron watcher if cron schedule is specified
            let cron_watcher = service_file
                .dispenser
                .cron
                .as_ref()
                .map(|schedule| CronWatcher::new(schedule));

            service_names.push(service_file.service.name.clone());

            // Create the ServiceInstance
            let instance = ServiceInstance {
                dir: entry.path.clone(),
                service: service_file.service,
                ports: service_file.ports,
                volume: service_file.volume,
                env: service_file.env,
                restart: service_file.restart,
                network: service_file.network,
                dispenser: service_file.dispenser,
                depends_on: service_file.depends_on,
                cron_watcher,
                image_watcher,
            };

            instances.push(Arc::new(Mutex::new(instance)));
        }

        // Create the broadcast channel for cancellation
        let (cancel_tx, cancel_rx) = tokio::sync::mpsc::channel(1);
        let cancel_rx = Mutex::new(cancel_rx);

        let inner = ServiceManagerInner {
            instances,
            networks,
            delay,
        };

        Ok(ServicesManager {
            service_names,
            inner: Mutex::new(inner),
            cancel_tx,
            cancel_rx,
        })
    }

    pub async fn cancel(&self) {
        let _ = self.cancel_tx.send(());
    }

    pub async fn start_polling(&self) {
        log::info!("Starting polling task");
        let inner = self.inner.lock().await;
        let delay = inner.delay;

        let polls = inner
            .instances
            .iter()
            .map(|instance| {
                let instance = Arc::clone(instance);
                async move {
                    let mut last_image_poll = std::time::Instant::now();
                    let mut init = true;
                    loop {
                        let poll_images = last_image_poll.elapsed() >= delay;
                        if poll_images {
                            last_image_poll = std::time::Instant::now();
                        }
                        let poll_start = std::time::Instant::now();
                        let mut instance = instance.lock().await;
                        instance.poll(poll_images, init).await;
                        let poll_duration = poll_start.elapsed();
                        log::debug!(
                            "Polling for {} took {:?}",
                            instance.service.name,
                            poll_duration
                        );
                        init = false;
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            })
            .collect::<JoinSet<_>>();

        let mut cancel_rx = self.cancel_rx.lock().await;

        tokio::select! {
            _ = polls.join_all() => {}
            _ = cancel_rx.recv() => {
                log::warn!("CANCELLED");
            }
        }
    }

    /// Clean up networks created by this manager
    /// This should be called on shutdown to remove non-external networks
    pub async fn cleanup_networks(&self) {
        log::info!("Cleaning up networks");
        let inner = self.inner.lock().await;

        for network in &inner.networks {
            if let Err(e) = network.remove_network().await {
                log::warn!("Failed to remove network {}: {}", network.name, e);
            }
        }
    }

    pub async fn remove_containers(&self, names: Vec<String>) {
        let instances = self.inner.lock().await;
        for instance in &instances.instances {
            let instance = instance.lock().await;
            if names.contains(&instance.service.name) {
                let _ = instance.stop_container().await;
                let _ = instance.remove_container().await;
            }
        }
    }
    pub async fn shutdown(&self) {
        self.remove_containers(self.service_names.clone()).await;
        self.cleanup_networks().await;
    }
}
