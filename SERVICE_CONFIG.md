# Service Configuration Reference

This document describes all available configuration options for service definitions in `service.toml` files.

## Overview

Each service in Dispenser is configured using a `service.toml` file located in its own directory. This file defines how the Docker container should be created, what resources it should use, and how Dispenser should manage it.

## File Structure

A `service.toml` file has the following main sections:

```toml
[service]
# Core service configuration

[[port]]
# Port mapping (can have multiple)

[[volume]]
# Volume mount (can have multiple)

[env]
# Environment variables

[[network]]
# Network connection (can have multiple)

restart = "policy"

[dispenser]
# Dispenser-specific settings

[depends_on]
# Service dependencies

[proxy]
# Reverse proxy configuration
```

## Service Section

The `[service]` section defines the core container configuration.

### `name` (required)

The name of the container. Must be unique across all services.

```toml
[service]
name = "my-app"
```

### `image` (required)

The Docker image to use. Supports variable interpolation.

```toml
[service]
image = "nginx:latest"

# With registry and variables
image = "${registry_url}/my-org/my-app:${version}"
```

### `command` (optional)

Override the default command. Can be a string or array of strings.

```toml
[service]
name = "worker"
image = "python:3.11"

# Array format (recommended)
command = ["python", "worker.py", "--verbose"]

# String format
# command = "python worker.py --verbose"
```

### `entrypoint` (optional)

Override the default entrypoint. Can be a string or array of strings.

```toml
[service]
name = "custom-app"
image = "my-app:latest"

# Array format (recommended)
entrypoint = ["/bin/sh", "-c"]
command = ["echo hello && sleep 10"]

# String format
# entrypoint = "/bin/sh -c"
```

### `working_dir` (optional)

Set the working directory inside the container.

```toml
[service]
name = "app"
image = "node:18"
working_dir = "/app"
command = ["npm", "start"]
```

### `user` (optional)

Run the container as a specific user. Can be a username, UID, or UID:GID.

```toml
[service]
name = "app"
image = "my-app:latest"

# Run as specific UID
user = "1000"

# Run as specific UID:GID
user = "1000:1000"

# Run as named user
user = "appuser"
```

### `hostname` (optional)

Set the container's hostname.

```toml
[service]
name = "api"
image = "my-api:latest"
hostname = "api-server"
```

### `memory` (optional)

Set a memory limit for the container. Supports suffixes: `b`, `k`/`kb`, `m`/`mb`, `g`/`gb`.

```toml
[service]
name = "app"
image = "my-app:latest"

# 512 megabytes
memory = "512m"

# 2 gigabytes
memory = "2g"

# 256 megabytes
memory = "256mb"
```

### `cpus` (optional)

Set CPU limit for the container. Decimal values allowed.

```toml
[service]
name = "app"
image = "my-app:latest"

# Half a CPU
cpus = "0.5"

# Two CPUs
cpus = "2"

# One and a half CPUs
cpus = "1.5"
```

## Port Mappings

Map ports from the host to the container. Use `[[port]]` for each mapping.

```toml
[[port]]
host = 8080
container = 80

[[port]]
host = 8443
container = 443
```

### `host` (required)

The port on the host machine.

### `container` (required)

The port inside the container.

## Volume Mounts

Mount directories or files into the container. Use `[[volume]]` for each mount.

```toml
[[volume]]
source = "./data"
target = "/app/data"

[[volume]]
source = "./config"
target = "/app/config"
readonly = true
```

### `source` (required)

The source path on the host. Can be:
- Relative path (relative to the service directory)
- Absolute path
- Named volume

### `target` (required)

The target path inside the container. Must be an absolute path.

### `readonly` (optional)

If `true`, the volume is mounted as read-only.

**Default:** `false`

```toml
[[volume]]
source = "./config"
target = "/etc/app/config"
readonly = true
```

## Environment Variables

Define environment variables for the container.

```toml
[env]
NODE_ENV = "production"
DATABASE_URL = "postgres://user:pass@host:5432/db"
API_KEY = "${api_key}"
LOG_LEVEL = "info"
```

Variables support interpolation using `${variable}` syntax with values from `dispenser.vars` or `*.dispenser.vars` files.

## Network Connections

Connect the service to Docker networks. Networks must be declared in `dispenser.toml` first.

```toml
[[network]]
name = "app-network"

[[network]]
name = "database-network"
```

### `name` (required)

The name of the network to connect to. Must match a network declared in `dispenser.toml`.

## Restart Policy

Define when Docker should restart the container.

```toml
restart = "always"
```

**Valid values:**
- `no` or `never` - Never restart (default)
- `always` - Always restart if stopped
- `on-failure` - Restart only if container exits with non-zero status
- `unless-stopped` - Always restart unless explicitly stopped

**Default:** `no`

**Examples:**

```toml
# Never restart
restart = "no"

# Always restart (for long-running services)
restart = "always"

# Restart on failure (for critical services)
restart = "on-failure"

# Restart unless stopped (for persistent services)
restart = "unless-stopped"
```

## Dispenser Section

The `[dispenser]` section controls how Dispenser manages the service.

### `watch` (required)

Whether to watch the image registry for updates. When `true`, Dispenser will poll the registry and automatically redeploy when a new version is detected.

```toml
[dispenser]
watch = true
```

### `initialize` (optional)

Controls when the service should be started.

**Valid values:**
- `immediately` - Start as soon as Dispenser starts (default)
- `on-trigger` - Start only when triggered (by cron or image update)

**Default:** `immediately`

```toml
[dispenser]
watch = true
initialize = "immediately"
```

```toml
# For scheduled tasks
[dispenser]
watch = false
initialize = "on-trigger"
cron = "0 3 * * *"
```

### `cron` (optional)

A cron expression for scheduled deployments. When specified, the service will be redeployed according to the schedule.

```toml
[dispenser]
watch = false
initialize = "on-trigger"
cron = "0 3 * * *"  # Every day at 3 AM
```

See [CRON.md](CRON.md) for more details on cron scheduling.

### `pull` (optional)

Controls when Dispenser should pull the Docker image from the registry. This is useful for ensuring that services (especially scheduled jobs) are always up-to-date with the latest image when they run, without necessarily triggering a redeployment on every image update if `watch` is `false`.

**Valid values:**
- `always` - Pull the image from the registry every time the container is started or recreated.
- `on-startup` - Pull the image only if the container does not exist. (default)

**Default:** `on-startup`

```toml
[dispenser]
# For a background scheduled job that should always run the latest image,
# but not necessarily restart if the image updates outside of its schedule.
watch = false
initialize = "on-trigger"
cron = "0 3 * * *" # Every day at 3 AM
pull = "always"
```

## Service Dependencies

The `[depends_on]` section defines dependencies between services.

```toml
[depends_on]
postgres = "service-started"
redis = "service-started"
migration = "service-completed"
```

**Valid conditions:**
- `service-started` or `started` - Wait for service to start
- `service-completed` or `completed` - Wait for service to complete

## Proxy Configuration

The `[proxy]` section configures the built-in reverse proxy to route traffic to this service. These settings only take effect if the proxy is enabled globally in your main `dispenser.toml` file (enabled by default). Note that enabling/disabling the proxy globally requires a full process restart.

### `host` (required)

The domain name (FQDN) that this service should respond to.

```toml
[proxy]
host = "app.example.com"
```

### `service_port` (required)

The port the application is listening on inside the container. This is where the proxy will forward traffic.

```toml
[proxy]
host = "app.example.com"
service_port = 8080
```

### `cert_file` (optional)

Path to a custom SSL certificate file (PEM format). If not provided, Dispenser uses Let's Encrypt only if the `[certbot]` section is explicitly defined in your main `dispenser.toml`. If `[certbot]` is missing, Dispenser expects manual certificates here (or will use simulation mode if running the `dev` command).

### `key_file` (optional)

Path to the private key file for the custom certificate.

```toml
[proxy]
host = "internal.example.com"
service_port = 80
cert_file = "/etc/ssl/certs/internal.crt"
key_file = "/etc/ssl/certs/internal.key"
```

See [PROXY.md](PROXY.md) for more details on the reverse proxy.

## Complete Examples

### Basic Web Application

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

[[volume]]
source = "./html"
target = "/usr/share/nginx/html"
readonly = true

[[volume]]
source = "./nginx.conf"
target = "/etc/nginx/nginx.conf"
readonly = true

[[network]]
name = "web"

restart = "unless-stopped"

[dispenser]
watch = true
initialize = "immediately"
```

### API Service with Database

```toml
[service]
name = "api"
image = "ghcr.io/my-org/api:latest"
memory = "1g"
cpus = "1.0"

[[port]]
host = 3000
container = 3000

[env]
NODE_ENV = "production"
DATABASE_URL = "postgres://postgres:5432/mydb"
REDIS_URL = "redis://redis:6379"
LOG_LEVEL = "info"

[[network]]
name = "frontend"

[[network]]
name = "backend"

restart = "always"

[dispenser]
watch = true
initialize = "immediately"

[depends_on]
postgres = "service-started"
redis = "service-started"
```

### Background Worker

```toml
[service]
name = "worker"
image = "python:3.11"
command = ["python", "worker.py"]
working_dir = "/app"
user = "1000:1000"
memory = "512m"
cpus = "0.5"

[[volume]]
source = "./src"
target = "/app"

[[volume]]
source = "./logs"
target = "/app/logs"

[env]
PYTHONUNBUFFERED = "1"
DATABASE_URL = "postgres://postgres:5432/mydb"
QUEUE_URL = "redis://redis:6379"

[[network]]
name = "backend"

restart = "always"

[dispenser]
watch = true
initialize = "immediately"

[depends_on]
postgres = "service-started"
redis = "service-started"
```

### Scheduled Backup Job

```toml
[service]
name = "backup-job"
image = "my-backup:latest"
command = ["/backup.sh"]
working_dir = "/backups"

[[volume]]
source = "./backups"
target = "/backups"

[[volume]]
source = "/var/lib/docker/volumes"
target = "/source"
readonly = true

[env]
BACKUP_RETENTION_DAYS = "30"
BACKUP_DESTINATION = "s3://my-bucket/backups"

restart = "no"

[dispenser]
watch = false
initialize = "on-trigger"
cron = "0 2 * * *"  # Every day at 2 AM
```

### Database Service

```toml
[service]
name = "postgres"
image = "postgres:15"
hostname = "postgres-db"
memory = "2g"

[env]
POSTGRES_PASSWORD = "secretpassword"
POSTGRES_USER = "myapp"
POSTGRES_DB = "myapp"
PGDATA = "/var/lib/postgresql/data/pgdata"

[[volume]]
source = "./data"
target = "/var/lib/postgresql/data"

[[network]]
name = "database"

restart = "unless-stopped"

[dispenser]
watch = false
initialize = "immediately"
```

### Custom Entrypoint Example

```toml
[service]
name = "init-service"
image = "alpine:latest"
entrypoint = ["/bin/sh", "-c"]
command = ["apk add --no-cache curl && curl https://example.com/setup.sh | sh"]
working_dir = "/workspace"

[[volume]]
source = "./workspace"
target = "/workspace"

restart = "no"

[dispenser]
watch = false
initialize = "immediately"
```

## Validation

Before applying your configuration, validate it with:

```sh
dispenser --test
```

This will check for:
- Syntax errors
- Missing required fields
- Undefined variables
- Network references to non-existent networks

## Best Practices

1. **Use meaningful service names** that describe the service's purpose
2. **Pin image versions** in production instead of using `latest`
3. **Set resource limits** (`memory`, `cpus`) to prevent resource exhaustion
4. **Use readonly volumes** for configuration files
5. **Use restart policies** appropriate for the service type:
   - `always` for critical services
   - `on-failure` for services that should recover from crashes
   - `no` for one-time jobs
6. **Use environment variables** for configuration instead of hardcoding
7. **Connect to appropriate networks** based on security requirements
8. **Define dependencies** when services rely on each other
9. **Use `initialize = "on-trigger"`** for scheduled or batch jobs
10. **Test configuration changes** with `dispenser --test` before deployment

## See Also

- [CLI Reference](CLI.md) - Command-line options
- [Reverse Proxy](PROXY.md) - Proxy and SSL configuration
- [Network Configuration](NETWORKS.md) - Detailed network setup
- [CRON Documentation](CRON.md) - Scheduling reference
- [Migration Guide](MIGRATION_GUIDE.md) - Migrating from Docker Compose
