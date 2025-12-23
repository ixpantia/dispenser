# Network Configuration Reference

This document describes how to configure Docker networks in Dispenser.

## Overview

Dispenser supports Docker networks to enable communication between services. Networks are declared in `dispenser.toml` and referenced in individual service configurations.

## Network Declaration

Networks must be declared in your `dispenser.toml` file before they can be referenced by services.

### Basic Network Declaration

```toml
[[network]]
name = "app-network"
driver = "bridge"
```

### Complete Network Configuration

```toml
[[network]]
name = "app-network"
driver = "bridge"
external = false
internal = false
attachable = true

[network.labels]
app = "myapp"
environment = "production"
```

## Configuration Fields

### `name` (required)

The name of the network. This is used to reference the network in service configurations.

**Example:**
```toml
[[network]]
name = "backend-network"
```

### `driver` (optional)

The network driver to use. 

**Default:** `bridge`

**Valid values:**
- `bridge` - Standard bridge network (default)
- `host` - Use the host's networking directly
- `overlay` - Multi-host network for Swarm
- `macvlan` - Assign a MAC address to containers
- `none` - Disable networking

**Example:**
```toml
[[network]]
name = "my-network"
driver = "overlay"
```

### `external` (optional)

If `true`, Dispenser will not create the network but expects it to already exist. This is useful for networks created outside of Dispenser.

**Default:** `false`

**Example:**
```toml
[[network]]
name = "existing-network"
external = true
```

### `internal` (optional)

If `true`, restricts external access to the network. Containers on the network can communicate with each other but cannot access external networks or the internet.

**Default:** `false`

**Example:**
```toml
[[network]]
name = "isolated-backend"
driver = "bridge"
internal = true
```

### `attachable` (optional)

If `true`, allows standalone containers to attach to the network. This is particularly useful for overlay networks in Swarm mode.

**Default:** `true`

**Example:**
```toml
[[network]]
name = "swarm-network"
driver = "overlay"
attachable = true
```

### `labels` (optional)

Key-value pairs to add as metadata labels to the network. These labels can be used for organization and filtering.

**Default:** Empty

**Example:**
```toml
[[network]]
name = "app-network"

[network.labels]
app = "myapp"
environment = "production"
team = "backend"
version = "2.0"
```

## Using Networks in Services

After declaring networks in `dispenser.toml`, reference them in your service configurations.

### Single Network

```toml
# my-app/service.toml
[service]
name = "my-app"
image = "ghcr.io/my-org/my-app:latest"

[[network]]
name = "app-network"

[dispenser]
watch = true
```

### Multiple Networks

A service can connect to multiple networks:

```toml
# api/service.toml
[service]
name = "api"
image = "my-api:latest"

[[network]]
name = "frontend-network"

[[network]]
name = "backend-network"

[dispenser]
watch = true
```

## Complete Example

### dispenser.toml

```toml
delay = 60

# Public-facing network
[[network]]
name = "frontend"
driver = "bridge"

[network.labels]
tier = "frontend"

# Internal backend network
[[network]]
name = "backend"
driver = "bridge"
internal = true

[network.labels]
tier = "backend"

# Database network (isolated)
[[network]]
name = "database"
driver = "bridge"
internal = true

[network.labels]
tier = "database"

[[service]]
path = "nginx"

[[service]]
path = "api"

[[service]]
path = "worker"

[[service]]
path = "postgres"
```

### nginx/service.toml

```toml
[service]
name = "nginx"
image = "nginx:latest"

[[port]]
host = 80
container = 80

[[port]]
host = 443
container = 443

# Only connected to frontend network
[[network]]
name = "frontend"

[dispenser]
watch = true
initialize = "immediately"
```

### api/service.toml

```toml
[service]
name = "api"
image = "my-api:latest"

# Connected to both frontend and backend
[[network]]
name = "frontend"

[[network]]
name = "backend"

[env]
DATABASE_URL = "postgres://postgres:5432/mydb"

[dispenser]
watch = true
initialize = "immediately"
```

### worker/service.toml

```toml
[service]
name = "worker"
image = "my-worker:latest"

# Connected to backend and database
[[network]]
name = "backend"

[[network]]
name = "database"

[env]
DATABASE_URL = "postgres://postgres:5432/mydb"

restart = "always"

[dispenser]
watch = true
initialize = "immediately"
```

### postgres/service.toml

```toml
[service]
name = "postgres"
image = "postgres:15"

# Only connected to database network (most isolated)
[[network]]
name = "database"

[env]
POSTGRES_PASSWORD = "secretpassword"
POSTGRES_DB = "mydb"

[[volume]]
source = "./data"
target = "/var/lib/postgresql/data"

restart = "unless-stopped"

[dispenser]
watch = false
initialize = "immediately"
```

## Network Communication

### Service Discovery

Services on the same network can communicate using their service names as hostnames. Docker provides built-in DNS resolution.

**Example:**
```toml
# api/service.toml
[service]
name = "api"
image = "my-api:latest"

[[network]]
name = "app-network"

[env]
# Reference the database by service name
DATABASE_URL = "postgres://postgres:5432/mydb"
```

```toml
# postgres/service.toml
[service]
name = "postgres"  # This becomes the hostname
image = "postgres:15"

[[network]]
name = "app-network"

[env]
POSTGRES_DB = "mydb"
```

### Network Isolation

Use internal networks to isolate sensitive services:

```toml
# dispenser.toml
[[network]]
name = "public"
driver = "bridge"
internal = false  # Can access internet

[[network]]
name = "private"
driver = "bridge"
internal = true   # Cannot access internet
```

## External Networks

To use a network created outside of Dispenser (e.g., manually created with `docker network create`):

```toml
[[network]]
name = "existing-network"
external = true
```

When `external = true`, Dispenser will not attempt to create or delete the network. It must already exist before starting services that reference it.

## Troubleshooting

### Network Already Exists

If you see an error that a network already exists, either:
1. Mark it as `external = true` in your configuration
2. Remove the existing network with `docker network rm <network-name>`

### Services Cannot Communicate

Ensure that:
1. Both services are connected to the same network
2. You're using the correct service name as the hostname
3. The network is not marked as `internal` if internet access is needed
4. Firewall rules are not blocking traffic

### Viewing Networks

```sh
# List all networks
docker network ls

# Inspect a specific network
docker network inspect app-network

# See which containers are connected
docker network inspect app-network --format '{{range .Containers}}{{.Name}} {{end}}'
```

## Best Practices

1. **Use multiple networks** for security isolation (frontend, backend, database tiers)
2. **Mark sensitive networks as internal** to prevent external access
3. **Use descriptive network names** that indicate their purpose
4. **Add labels** to networks for better organization and documentation
5. **Use the bridge driver** for single-host deployments (most common)
6. **Test connectivity** between services after configuration changes
