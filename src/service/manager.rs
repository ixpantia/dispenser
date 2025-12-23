use std::{sync::Arc, time::Duration};

use futures_util::future;
use tokio::{sync::Mutex, task::JoinSet};

use crate::service::{
    file::{EntrypointFile, ServiceFile},
    instance::{CronWatcher, ServiceInstance},
    manifest::ImageWatcher,
    vars::{render_template, ServiceConfigError, ServiceVarsMaterialized},
};

struct ServiceManagerInner {
    instances: Vec<Arc<Mutex<ServiceInstance>>>,
    delay: Duration,
}

pub struct ServicesManager {
    inner: Mutex<ServiceManagerInner>,
    cancel_tx: tokio::sync::broadcast::Sender<()>,
}

impl ServicesManager {
    pub async fn from_config(config: EntrypointFile) -> Result<Self, ServiceConfigError> {
        let mut instances = Vec::new();

        // Load and materialize variables once for all services
        let vars = ServiceVarsMaterialized::try_init().await?;

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
        let (cancel_tx, _) = tokio::sync::broadcast::channel(1);

        // Use a default delay of 60 seconds for polling images
        let delay = Duration::from_secs(60);

        let inner = ServiceManagerInner { instances, delay };

        Ok(ServicesManager {
            inner: Mutex::new(inner),
            cancel_tx,
        })
    }

    pub fn cancel(&self) {
        let _ = self.cancel_tx.send(());
    }

    pub async fn start_polling(&self) {
        log::info!("Starting polling task");
        let inner = self.inner.lock().await;
        let delay = inner.delay;
        let mut cancel_rx = self.cancel_tx.subscribe();

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

        tokio::select! {
            _ = polls.join_all() => {}
            _ = cancel_rx.recv() => {}
        }
    }
}
