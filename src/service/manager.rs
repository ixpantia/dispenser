use std::{
    collections::{HashMap, HashSet},
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use tokio::{sync::Mutex, task::JoinSet};

use crate::service::{
    cron_watcher::CronWatcher,
    file::{CertbotSettings, EntrypointFile, GlobalProxyConfig, ProxySettings, ServiceFile},
    instance::{ServiceInstance, ServiceInstanceConfig},
    manifest::ImageWatcher,
    network::{ensure_default_network, remove_default_network, NetworkInstance},
    vars::{render_template, ServiceConfigError, ServiceVarsMaterialized},
};

pub struct ServiceMangerConfig {
    entrypoint_file: EntrypointFile,
    services: Vec<(PathBuf, ServiceFile)>,
}

impl ServiceMangerConfig {
    pub async fn try_init(filter: Option<&[String]>) -> Result<Self, ServiceConfigError> {
        // Load and materialize variables
        let vars = ServiceVarsMaterialized::try_init().await?;
        let entrypoint_file = EntrypointFile::try_init(&vars).await?;

        let mut services = Vec::new();

        for entry in &entrypoint_file.services {
            if let Some(filter) = filter {
                let path_str = entry.path.to_string_lossy();
                let matches = filter
                    .iter()
                    .any(|f| path_str == *f || path_str.ends_with(&format!("/{}", f)));
                if !matches {
                    continue;
                }
            }

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

struct HostRoute {
    path: String,
    service_index: usize,
}

struct ServiceManagerInner {
    // These two are craeted together. We can zip them
    pub service_names: Vec<String>,
    instances: Vec<Arc<ServiceInstance>>,
    router: HashMap<String, Vec<HostRoute>>,
    networks: Vec<NetworkInstance>,
    delay: Duration,
    certbot: Option<CertbotSettings>,
    proxy: GlobalProxyConfig,
}

pub struct ServicesManager {
    inner: ServiceManagerInner,
    cancel_tx: tokio::sync::mpsc::Sender<()>,
    cancel_rx: Mutex<tokio::sync::mpsc::Receiver<()>>,
}

impl ServicesManager {
    pub fn proxy_enabled(&self) -> bool {
        self.inner.proxy.enabled
    }
    pub fn resolve_route(&self, host: &str, path: &str) -> Option<SocketAddrV4> {
        let path = if path.is_empty() { "/" } else { path };
        if let Some(routes) = self.inner.router.get(host) {
            for route in routes {
                // Longest prefix match
                let is_match = if route.path == "/" || path == route.path {
                    true
                } else if path.starts_with(&route.path) {
                    // Ensure it matches a full path segment, e.g. /api matches /api/v1 but not /api-v2
                    path.as_bytes().get(route.path.len()) == Some(&b'/')
                } else {
                    false
                };

                if is_match {
                    return self.inner.instances[route.service_index].get_socket_addr();
                }
            }
        }
        None
    }
    // We should ensure that the containers don't exist before start up.
    // This is to make 100% sure that dispenser controls these containers
    // and they don't exist previously.
    pub async fn validate_containers_not_present(&self) -> Result<(), String> {
        let mut join_set = JoinSet::new();

        for instance in &self.inner.instances {
            let instance = Arc::clone(instance);
            join_set.spawn(async move {
                match instance.container_does_not_exist().await {
                    true => Ok(()),
                    false => Err(format!(
                        "Container {} already exists",
                        instance.config.service.name
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
            let this_instance = Arc::clone(this_instance);
            let other_service_names = other.inner.service_names.clone();
            let other_instances = other.inner.instances.clone();

            join_set.spawn(async move {
                // If the instance is present in other we check for recreation
                match other_service_names
                    .iter()
                    .zip(other_instances.iter())
                    .find(|(o, _)| *o == &this_instance.config.service.name)
                {
                    Some((_, other_instance)) => {
                        this_instance.recreate_if_required(other_instance).await;
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
    /// Get a map of service names to their assigned IP addresses.
    /// This is used during reload to preserve IP assignments for existing services.
    pub fn get_ip_map(&self) -> HashMap<String, Ipv4Addr> {
        self.inner
            .service_names
            .iter()
            .zip(self.inner.instances.iter())
            .map(|(name, instance)| (name.clone(), instance.config.assigned_ip))
            .collect()
    }

    pub fn get_proxy_configs(&self) -> Vec<ProxySettings> {
        let mut configs: HashMap<String, ProxySettings> = HashMap::new();
        for instance in &self.inner.instances {
            if let Some(proxy) = &instance.config.proxy {
                let existing = configs.get(&proxy.host);
                let is_better = match existing {
                    None => true,
                    Some(e) => e.cert_file.is_none() && proxy.cert_file.is_some(),
                };
                if is_better {
                    configs.insert(proxy.host.clone(), proxy.clone());
                }
            }
        }
        configs.into_values().collect()
    }

    pub fn get_certbot_settings(&self) -> Option<CertbotSettings> {
        self.inner.certbot.clone()
    }

    pub fn get_proxy_strategy(&self) -> crate::service::file::ProxyStrategy {
        self.inner.proxy.strategy
    }

    pub fn get_trust_forwarded_headers(&self) -> bool {
        self.inner.proxy.trust_forwarded_headers
    }

    pub async fn from_config(
        mut config: ServiceMangerConfig,
        existing_ips: Option<HashMap<String, Ipv4Addr>>,
    ) -> Result<Self, ServiceConfigError> {
        // Get the delay from config (in seconds)
        let delay = Duration::from_secs(config.entrypoint_file.delay);
        let mut instances = Vec::new();
        let mut networks = Vec::new();
        let mut service_names = Vec::new();
        let mut router = HashMap::new();
        let proxy = config.entrypoint_file.proxy;

        // Ensure the default dispenser network exists first
        // This network is used by all containers for inter-container communication
        if let Err(e) = ensure_default_network().await {
            log::error!("Failed to ensure default dispenser network exists: {}", e);
            return Err(e);
        }

        // Process user-defined networks - create NetworkInstance objects
        for network_entry in config.entrypoint_file.networks {
            let network = NetworkInstance::from(network_entry);
            networks.push(network);
        }

        // Ensure all user-defined networks exist before creating services
        for network in &networks {
            if let Err(e) = network.ensure_exists().await {
                log::error!("Failed to ensure network {} exists: {}", network.name, e);
                return Err(e);
            }
        }

        // Prune dependencies: Remove dependencies on services that are not being loaded
        let loaded_service_names: std::collections::HashSet<String> = config
            .services
            .iter()
            .map(|(_, s)| s.service.name.clone())
            .collect();

        for (_, service_file) in &mut config.services {
            service_file.depends_on.retain(|name, _| {
                let exists = loaded_service_names.contains(name);
                if !exists {
                    log::debug!(
                        "Pruning dependency '{}' from service '{}' as it's not being loaded.",
                        name,
                        service_file.service.name
                    );
                }
                exists
            });
        }

        // Allocate IP addresses using "Reserve then Fill" strategy
        let assigned_ips = allocate_ips(&config.services, existing_ips);

        // Iterate through each service entry in the config
        let mut join_set = JoinSet::new();

        for (entry_path, service_file) in config.services {
            // Get the assigned IP for this service
            let assigned_ip = assigned_ips
                .get(&service_file.service.name)
                .copied()
                .expect("IP should have been allocated for all services");
            join_set.spawn(async move {
                log::debug!("Initializing config for {entry_path:?}");

                // Initialize the image watcher if watch is enabled
                let image_watcher = if service_file.dispenser.watch {
                    Some(ImageWatcher::initialize(&service_file.service.image).await)
                } else {
                    None
                };

                // Create cron watcher if cron schedule is specified
                let cron_watcher = service_file.dispenser.cron.as_ref().map(CronWatcher::new);

                let service_name = service_file.service.name.clone();

                // Create the ServiceInstance
                let config = ServiceInstanceConfig {
                    dir: entry_path,
                    service: service_file.service,
                    ports: service_file.ports,
                    volume: service_file.volume,
                    env: service_file.env,
                    restart: service_file.restart,
                    network: service_file.network,
                    dispenser: service_file.dispenser,
                    depends_on: service_file.depends_on,
                    proxy: service_file.proxy,
                    assigned_ip,
                };

                let instance = ServiceInstance {
                    config: Arc::new(config),
                    cron_watcher,
                    image_watcher,
                };

                (service_name, Arc::new(instance))
            });
        }

        let mut index = 0;
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((service_name, instance)) => {
                    if let Some(proxy_config) = &instance.config.proxy {
                        let host = proxy_config.host.clone();
                        let path = normalize_path(proxy_config.path.as_deref());

                        let routes = router.entry(host).or_insert_with(Vec::new);
                        routes.push(HostRoute {
                            path,
                            service_index: index,
                        });
                    }
                    service_names.push(service_name);
                    instances.push(instance);
                    index += 1;
                }
                Err(e) => {
                    log::error!("Failed to initialize service: {}", e);
                }
            }
        }

        // Sort routes by path length descending to ensure longest prefix match
        for routes in router.values_mut() {
            routes.sort_by(|a, b| b.path.len().cmp(&a.path.len()));
        }

        // Create the broadcast channel for cancellation
        let (cancel_tx, cancel_rx) = tokio::sync::mpsc::channel(1);
        let cancel_rx = Mutex::new(cancel_rx);

        let inner = ServiceManagerInner {
            service_names,
            instances,
            networks,
            delay,
            router,
            proxy,
            certbot: config.entrypoint_file.certbot,
        };

        Ok(ServicesManager {
            inner,
            cancel_tx,
            cancel_rx,
        })
    }

    pub async fn cancel(&self) {
        let _ = self.cancel_tx.send(()).await;
    }

    pub async fn start_polling(&self) {
        log::info!("Starting polling task");
        let delay = self.inner.delay;

        let polls = self
            .inner
            .instances
            .iter()
            .map(|instance| {
                let instance = instance.clone();
                async move {
                    let mut last_image_poll = std::time::Instant::now();
                    let mut init = true;
                    loop {
                        let poll_images = last_image_poll.elapsed() >= delay;
                        if poll_images {
                            last_image_poll = std::time::Instant::now();
                        }
                        // Scope to release the lock
                        let poll_start = std::time::Instant::now();
                        instance.poll(poll_images, init).await;
                        let poll_duration = poll_start.elapsed();
                        log::debug!(
                            "Polling for {} took {:?}",
                            instance.config.service.name,
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

        for network in &self.inner.networks {
            if let Err(e) = network.remove_network().await {
                log::warn!("Failed to remove network {}: {}", network.name, e);
            }
        }
    }

    pub async fn remove_containers(&self, names: Vec<String>) {
        let mut join_set = JoinSet::new();

        for instance in &self.inner.instances {
            let instance = Arc::clone(instance);
            let names_clone = names.clone();

            join_set.spawn(async move {
                if names_clone.contains(&instance.config.service.name) {
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

        // Remove the default dispenser network after all containers and user networks are cleaned up
        if let Err(e) = remove_default_network().await {
            log::warn!("Failed to remove default dispenser network: {}", e);
        }
    }
}

/// Allocate IP addresses to services using "Reserve then Fill" strategy.
///
/// This ensures that:
/// 1. Existing services keep their IP addresses (Reserve phase)
/// 2. New services get the lowest available IP addresses (Fill phase)
///
/// The subnet is 172.28.0.0/16 with gateway at 172.28.0.1, so we start from 172.28.0.2.
fn normalize_path(path: Option<&str>) -> String {
    let mut path = path.unwrap_or("/").to_string();
    if path.is_empty() {
        return "/".to_string();
    }
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    if path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    path
}

fn allocate_ips(
    services: &[(PathBuf, crate::service::file::ServiceFile)],
    existing_ips: Option<HashMap<String, Ipv4Addr>>,
) -> HashMap<String, Ipv4Addr> {
    let mut assigned: HashMap<String, Ipv4Addr> = HashMap::new();
    let mut used_ips: HashSet<Ipv4Addr> = HashSet::new();

    // Base IP: 172.28.0.0
    let base_ip: u32 = u32::from(Ipv4Addr::new(172, 28, 0, 0));

    // Reserve the gateway IP (172.28.0.1)
    used_ips.insert(Ipv4Addr::new(172, 28, 0, 1));

    let existing = existing_ips.unwrap_or_default();

    // Reserve Phase: Preserve IPs for existing services
    for (_, service_file) in services {
        let service_name = &service_file.service.name;
        if let Some(&existing_ip) = existing.get(service_name) {
            assigned.insert(service_name.clone(), existing_ip);
            used_ips.insert(existing_ip);
            log::debug!(
                "Reserved existing IP {} for service {}",
                existing_ip,
                service_name
            );
        }
    }

    // Fill Phase: Assign new IPs to services that don't have one
    // Start from 172.28.0.2 (offset 2 from base)
    let mut next_offset: u32 = 2;

    for (_, service_file) in services {
        let service_name = &service_file.service.name;
        if assigned.contains_key(service_name) {
            continue; // Already assigned in reserve phase
        }

        // Find the next available IP
        loop {
            let candidate_ip = Ipv4Addr::from(base_ip + next_offset);
            next_offset += 1;

            // Check if we've exceeded the subnet (unlikely with /16)
            if next_offset > 65534 {
                panic!("Exhausted all available IPs in the dispenser subnet");
            }

            if !used_ips.contains(&candidate_ip) {
                assigned.insert(service_name.clone(), candidate_ip);
                used_ips.insert(candidate_ip);
                log::debug!(
                    "Assigned new IP {} to service {}",
                    candidate_ip,
                    service_name
                );
                break;
            }
        }
    }

    assigned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::file::{DispenserConfig, PullOptions, Restart, ServiceEntry, ServiceFile};

    fn make_service_file(name: &str) -> ServiceFile {
        ServiceFile {
            service: ServiceEntry {
                name: name.to_string(),
                image: "test:latest".to_string(),
                hostname: None,
                user: None,
                working_dir: None,
                command: None,
                entrypoint: None,
                memory: None,
                cpus: None,
            },
            ports: vec![],
            volume: vec![],
            env: HashMap::new(),
            restart: Restart::No,
            network: vec![],
            dispenser: DispenserConfig {
                watch: false,
                cron: None,
                pull: PullOptions::OnStartup,
                initialize: crate::service::file::Initialize::default(),
            },
            depends_on: HashMap::new(),
            proxy: None,
        }
    }

    #[test]
    fn test_allocate_ips_new_services() {
        let services = vec![
            (PathBuf::from("/a"), make_service_file("service-a")),
            (PathBuf::from("/b"), make_service_file("service-b")),
            (PathBuf::from("/c"), make_service_file("service-c")),
        ];

        let assigned = allocate_ips(&services, None);

        assert_eq!(assigned.len(), 3);
        assert_eq!(
            assigned.get("service-a"),
            Some(&Ipv4Addr::new(172, 28, 0, 2))
        );
        assert_eq!(
            assigned.get("service-b"),
            Some(&Ipv4Addr::new(172, 28, 0, 3))
        );
        assert_eq!(
            assigned.get("service-c"),
            Some(&Ipv4Addr::new(172, 28, 0, 4))
        );
    }

    #[test]
    fn test_allocate_ips_preserves_existing() {
        let services = vec![
            (PathBuf::from("/a"), make_service_file("service-a")),
            (PathBuf::from("/b"), make_service_file("service-b")),
            (PathBuf::from("/c"), make_service_file("service-c")),
        ];

        let mut existing = HashMap::new();
        existing.insert("service-b".to_string(), Ipv4Addr::new(172, 28, 0, 10));

        let assigned = allocate_ips(&services, Some(existing));

        assert_eq!(assigned.len(), 3);
        // service-a gets the first available IP
        assert_eq!(
            assigned.get("service-a"),
            Some(&Ipv4Addr::new(172, 28, 0, 2))
        );
        // service-b keeps its existing IP
        assert_eq!(
            assigned.get("service-b"),
            Some(&Ipv4Addr::new(172, 28, 0, 10))
        );
        // service-c gets the next available IP
        assert_eq!(
            assigned.get("service-c"),
            Some(&Ipv4Addr::new(172, 28, 0, 3))
        );
    }

    #[test]
    fn test_allocate_ips_skips_used_ips() {
        let services = vec![
            (PathBuf::from("/a"), make_service_file("service-a")),
            (PathBuf::from("/b"), make_service_file("service-b")),
            (PathBuf::from("/c"), make_service_file("service-c")),
        ];

        let mut existing = HashMap::new();
        // Reserve IP .2 for service-b (which is processed second)
        existing.insert("service-b".to_string(), Ipv4Addr::new(172, 28, 0, 2));

        let assigned = allocate_ips(&services, Some(existing));

        assert_eq!(assigned.len(), 3);
        // service-a should skip .2 (used by service-b) and get .3
        assert_eq!(
            assigned.get("service-a"),
            Some(&Ipv4Addr::new(172, 28, 0, 3))
        );
        // service-b keeps its reserved IP
        assert_eq!(
            assigned.get("service-b"),
            Some(&Ipv4Addr::new(172, 28, 0, 2))
        );
        // service-c gets .4
        assert_eq!(
            assigned.get("service-c"),
            Some(&Ipv4Addr::new(172, 28, 0, 4))
        );
    }

    #[test]
    fn test_allocate_ips_ignores_stale_existing() {
        // Test that existing IPs for services no longer in the config are ignored
        let services = vec![(PathBuf::from("/a"), make_service_file("service-a"))];

        let mut existing = HashMap::new();
        existing.insert("service-removed".to_string(), Ipv4Addr::new(172, 28, 0, 5));

        let assigned = allocate_ips(&services, Some(existing));

        assert_eq!(assigned.len(), 1);
        // service-a gets .2 (the removed service's IP is not reserved)
        assert_eq!(
            assigned.get("service-a"),
            Some(&Ipv4Addr::new(172, 28, 0, 2))
        );
    }

    #[test]
    fn test_allocate_ips_gateway_reserved() {
        // Ensure gateway IP (172.28.0.1) is never assigned
        let services = vec![(PathBuf::from("/a"), make_service_file("service-a"))];

        let assigned = allocate_ips(&services, None);

        assert_eq!(assigned.len(), 1);
        // Should start from .2, not .1 (gateway)
        assert_eq!(
            assigned.get("service-a"),
            Some(&Ipv4Addr::new(172, 28, 0, 2))
        );
    }

    #[test]
    fn test_resolve_route_path_matching() {
        let mut router = HashMap::new();
        router.insert(
            "example.com".to_string(),
            vec![
                HostRoute {
                    path: "/api/v1".to_string(),
                    service_index: 1,
                },
                HostRoute {
                    path: "/api".to_string(),
                    service_index: 2,
                },
                HostRoute {
                    path: "/".to_string(),
                    service_index: 0,
                },
            ],
        );

        // Mock instances with dummy socket addresses
        let mut instances = Vec::new();
        for i in 0..3 {
            let config = ServiceInstanceConfig {
                dir: PathBuf::from(format!("/service-{}", i)),
                service: ServiceEntry {
                    name: format!("service-{}", i),
                    image: "test:latest".to_string(),
                    hostname: None,
                    user: None,
                    working_dir: None,
                    command: None,
                    entrypoint: None,
                    memory: None,
                    cpus: None,
                },
                ports: vec![],
                volume: vec![],
                env: HashMap::new(),
                restart: Restart::No,
                network: vec![],
                dispenser: DispenserConfig {
                    watch: false,
                    cron: None,
                    pull: PullOptions::OnStartup,
                    initialize: crate::service::file::Initialize::default(),
                },
                depends_on: HashMap::new(),
                proxy: Some(ProxySettings {
                    host: "example.com".to_string(),
                    path: None,
                    service_port: 8080,
                    cert_file: None,
                    key_file: None,
                }),
                assigned_ip: Ipv4Addr::new(172, 28, 0, i as u8 + 10),
            };
            instances.push(Arc::new(ServiceInstance {
                config: Arc::new(config),
                cron_watcher: None,
                image_watcher: None,
            }));
        }

        let manager = ServicesManager {
            inner: ServiceManagerInner {
                service_names: vec![],
                instances,
                networks: vec![],
                delay: Duration::from_secs(60),
                router,
                proxy: GlobalProxyConfig::default(),
                certbot: None,
            },
            cancel_tx: tokio::sync::mpsc::channel(1).0,
            cancel_rx: Mutex::new(tokio::sync::mpsc::channel(1).1),
        };

        // 1. Test exact match
        assert_eq!(
            manager.resolve_route("example.com", "/api").unwrap().ip(),
            &Ipv4Addr::new(172, 28, 0, 12)
        );

        // 2. Test longest prefix match
        assert_eq!(
            manager
                .resolve_route("example.com", "/api/v1/users")
                .unwrap()
                .ip(),
            &Ipv4Addr::new(172, 28, 0, 11)
        );

        // 3. Test fallback to root
        assert_eq!(
            manager
                .resolve_route("example.com", "/dashboard")
                .unwrap()
                .ip(),
            &Ipv4Addr::new(172, 28, 0, 10)
        );

        // 4. Test safety (preventing partial segment match)
        // "/api-v2" should NOT match "/api", it should fall back to "/"
        assert_eq!(
            manager
                .resolve_route("example.com", "/api-v2")
                .unwrap()
                .ip(),
            &Ipv4Addr::new(172, 28, 0, 10)
        );

        // 5. Test trailing slash behavior
        assert_eq!(
            manager.resolve_route("example.com", "/api/").unwrap().ip(),
            &Ipv4Addr::new(172, 28, 0, 12)
        );

        // 6. Test empty path defaults to /
        assert_eq!(
            manager.resolve_route("example.com", "").unwrap().ip(),
            &Ipv4Addr::new(172, 28, 0, 10)
        );
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path(None), "/");
        assert_eq!(normalize_path(Some("")), "/");
        assert_eq!(normalize_path(Some("/")), "/");
        assert_eq!(normalize_path(Some("/api")), "/api");
        assert_eq!(normalize_path(Some("/api/")), "/api");
        assert_eq!(normalize_path(Some("api")), "/api");
        assert_eq!(normalize_path(Some("api/")), "/api");
    }
}
