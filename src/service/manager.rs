use std::{path::PathBuf, sync::Arc, time::Duration};

use futures_util::future;
use tokio::{sync::Mutex, task::JoinSet};

use crate::service::{
    file::{EntrypointFile, ServiceFile},
    instance::{CronWatcher, ServiceInstance},
    manifest::ImageWatcher,
    network::NetworkInstance,
    vars::{render_template, ServiceConfigError, ServiceVarsMaterialized},
};

pub struct ServiceMangerConfig {
    entrypoint_file: EntrypointFile,
    services: Vec<(PathBuf, ServiceFile)>,
}

impl ServiceMangerConfig {
    pub async fn try_init() -> Result<Self, ServiceConfigError> {
        // Load and materialize variables
        let vars = ServiceVarsMaterialized::try_init().await?;
        let entrypoint_file = EntrypointFile::try_init(&vars).await?;

        let mut services = Vec::new();

        for entry in &entrypoint_file.services {
            // Construct the path to service.toml
            let service_toml_path = entry.path.join("service.toml");

            // Read the service.toml file
            let service_file_content = tokio::fs::read_to_string(&service_toml_path).await?;

            // Render the template with variables
            let rendered_service = render_template(&service_file_content, &vars)
                .map_err(|e| ServiceConfigError::Template((service_toml_path.clone(), e)))?;

            // Parse the rendered config as TOML
            let service_file: ServiceFile = toml::from_str(&rendered_service)?;

            services.push((entry.path.clone(), service_file));
        }
        Ok(Self {
            services,
            entrypoint_file,
        })
    }
}

struct ServiceManagerInner {
    // These two are craeted together. We can zip them
    pub service_names: Vec<String>,
    instances: Vec<Arc<Mutex<ServiceInstance>>>,
    networks: Vec<NetworkInstance>,
    delay: Duration,
}

pub struct ServicesManager {
    inner: ServiceManagerInner,
    cancel_tx: tokio::sync::mpsc::Sender<()>,
    cancel_rx: Mutex<tokio::sync::mpsc::Receiver<()>>,
}

impl ServicesManager {
    // We should ensure that the containers don't exist before start up.
    // This is to make 100% sure that dispenser controls these containers
    // and they don't exist previously.
    pub async fn validate_containers_not_present(&self) -> Result<(), String> {
        let mut join_set = JoinSet::new();

        for instance in &self.inner.instances {
            let instance_clone = Arc::clone(instance);
            join_set.spawn(async move {
                let instance = instance_clone.lock().await;
                match instance.container_does_not_exist().await {
                    true => Ok(()),
                    false => Err(format!(
                        "Container {} already exists",
                        instance.service.name
                    )),
                }
            });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(_)) => {
                    // Container validation succeeded
                }
                Ok(Err(e)) => {
                    log::error!("Container validation failed: {}", e);
                    return Err(e);
                }
                Err(e) => {
                    let error_msg = format!("Task join error: {}", e);
                    log::error!("{}", error_msg);
                    return Err(error_msg);
                }
            }
        }

        Ok(())
    }
    pub async fn recreate_if_changed_and_cleanup(&self, other: &Self) {
        let mut join_set = JoinSet::new();

        for this_instance in &self.inner.instances {
            let this_instance_clone = Arc::clone(this_instance);
            let other_service_names = other.inner.service_names.clone();
            let other_instances = other.inner.instances.clone();

            join_set.spawn(async move {
                let this_instance = this_instance_clone.lock().await;
                // If the instance is present in other we check for recreation
                match other_service_names
                    .iter()
                    .zip(other_instances.iter())
                    .filter(|(o, _)| *o == &this_instance.service.name)
                    .next()
                {
                    Some((_, other_instance)) => {
                        let other_instance = other_instance.lock().await;
                        this_instance.recreate_if_required(&other_instance).await;
                    }
                    None => {
                        // If the new container does not exist in other
                        // it will be recreated uppon startup
                    }
                };
            });
        }

        // Wait for all recreation tasks to complete
        while let Some(result) = join_set.join_next().await {
            if let Err(e) = result {
                log::error!("Failed to recreate instance: {}", e);
            }
        }

        // Cleanup: remove containers that exist in self but not in other
        let removed_services = self
            .inner
            .service_names
            .iter()
            .filter(|s| !other.inner.service_names.contains(s))
            .cloned()
            .collect();

        self.remove_containers(removed_services).await;
    }
    pub async fn from_config(config: ServiceMangerConfig) -> Result<Self, ServiceConfigError> {
        // Get the delay from config (in seconds)
        let delay = Duration::from_secs(config.entrypoint_file.delay);
        let mut instances = Vec::new();
        let mut networks = Vec::new();
        let mut service_names = Vec::new();

        // Process networks first - create NetworkInstance objects
        for network_entry in config.entrypoint_file.networks {
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
        let mut join_set = JoinSet::new();

        for (entry_path, service_file) in config.services {
            join_set.spawn(async move {
                log::debug!("Initializing config for {entry_path:?}");

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

                let service_name = service_file.service.name.clone();

                // Create the ServiceInstance
                let instance = ServiceInstance {
                    dir: entry_path,
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

                (service_name, Arc::new(Mutex::new(instance)))
            });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((service_name, instance)) => {
                    service_names.push(service_name);
                    instances.push(instance);
                }
                Err(e) => {
                    log::error!("Failed to initialize service: {}", e);
                }
            }
        }

        // Create the broadcast channel for cancellation
        let (cancel_tx, cancel_rx) = tokio::sync::mpsc::channel(1);
        let cancel_rx = Mutex::new(cancel_rx);

        let inner = ServiceManagerInner {
            service_names,
            instances,
            networks,
            delay,
        };

        Ok(ServicesManager {
            inner: inner,
            cancel_tx,
            cancel_rx,
        })
    }

    pub async fn cancel(&self) {
        let _ = self.cancel_tx.send(());
    }

    pub async fn start_polling(&self) {
        log::info!("Starting polling task");
        let delay = self.inner.delay;

        let polls = self
            .inner
            .instances
            .iter()
            .cloned()
            .map(|instance| async move {
                let mut last_image_poll = std::time::Instant::now();
                let mut init = true;
                loop {
                    let poll_images = last_image_poll.elapsed() >= delay;
                    if poll_images {
                        last_image_poll = std::time::Instant::now();
                    }
                    // Scope to release the lock
                    {
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
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
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

        for network in &self.inner.networks {
            if let Err(e) = network.remove_network().await {
                log::warn!("Failed to remove network {}: {}", network.name, e);
            }
        }
    }

    pub async fn remove_containers(&self, names: Vec<String>) {
        let mut join_set = JoinSet::new();

        for instance in &self.inner.instances {
            let instance_clone = Arc::clone(instance);
            let names_clone = names.clone();

            join_set.spawn(async move {
                let instance = instance_clone.lock().await;
                if names_clone.contains(&instance.service.name) {
                    let _ = instance.stop_container().await;
                    let _ = instance.remove_container().await;
                }
            });
        }

        while let Some(result) = join_set.join_next().await {
            if let Err(e) = result {
                log::error!("Failed to remove container: {}", e);
            }
        }
    }

    pub async fn shutdown(&self) {
        self.remove_containers(self.inner.service_names.clone())
            .await;
        self.cleanup_networks().await;
    }
}
