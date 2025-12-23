# Migration Guide: Docker Compose to service.toml

This guide helps you migrate from the older Dispenser repository structure using `docker-compose.yaml` files to the new structure using `service.toml` files.

## Overview

The new structure replaces Docker Compose YAML files with TOML-based service configuration files. The key changes are:

1. **Per-service configuration**: Each service now has its own `service.toml` file instead of `docker-compose.yaml`
2. **Simplified dispenser.toml**: The main configuration file is simplified - it only lists services by path
3. **Service-level settings**: Image tracking, cron schedules, and initialization behavior are now defined in each `service.toml`
4. **Same interpolation syntax**: Variable interpolation using `${variable_name}` works exactly the same way

## File Structure Comparison

### Old Structure
```
project/
├── dispenser.toml          # Contains service paths, images, cron, initialize settings
├── dispenser.vars          # Variable definitions
└── service-name/
    └── docker-compose.yaml # Docker Compose service definition
```

### New Structure
```
project/
├── dispenser.toml          # Only contains service paths and polling delay
├── dispenser.vars          # Variable definitions (unchanged)
└── service-name/
    └── service.toml        # Complete service configuration
```

## Main Configuration File Migration

### dispenser.toml

**Old format:**
```toml
delay = 60

[[instance]]
path = "nginx"
images = [{ registry = "${docker_io}", name = "nginx", tag = "latest" }]

[[instance]]
path = "hello-world"
cron = "*/10 * * * * *"
initialize = "on-trigger"
```

**New format:**
```toml
# Delay in seconds between polling for new images (default: 60)
delay = 60

[[service]]
path = "nginx"

[[service]]
path = "hello-world"
```

**Key changes:**
- `[[instance]]` → `[[service]]`
- Remove `images`, `cron`, and `initialize` fields (they move to `service.toml`)
- Keep only `path` to indicate service location

### dispenser.vars

**No changes required** - the variable file format remains the same:

```toml
docker_io="docker.io"
nginx_port="8080"
```

Variable interpolation using `${variable_name}` syntax works identically in both formats.

## Service Configuration Migration

Each service directory needs a `service.toml` file to replace its `docker-compose.yaml`.

### Example 1: Basic Web Service (nginx)

**Old (docker-compose.yaml):**
```yaml
version: "3.8"
services:
  nginx:
    image: ${docker_io}/nginx:latest
    ports:
      - "8080:80"
```

**New (service.toml):**
```toml
# Service metadata (required)
[service]
name = "nginx-service"
image = "${docker_io}/nginx:latest"

# Port mappings (optional)
[[port]]
host = 8080
container = 80

# Restart policy (optional, defaults to "no")
restart = "always"

# Dispenser-specific configuration (required)
[dispenser]
# Watch for image updates
watch = true

# Initialize immediately on startup (default behavior)
initialize = "immediately"
```

### Example 2: Scheduled Job (hello-world)

**Old (docker-compose.yaml):**
```yaml
version: "3.8"
services:
  hello-world:
    image: hello-world
    restart: no
```

**Old (dispenser.toml entry):**
```toml
[[instance]]
path = "hello-world"
cron = "*/10 * * * * *"
initialize = "on-trigger"
```

**New (service.toml):**
```toml
# Service metadata (required)
[service]
name = "hello-world-job"
image = "hello-world"

# Restart policy (optional, defaults to "no")
restart = "no"

# Dispenser-specific configuration (required)
[dispenser]
# Don't watch for image updates
watch = false

# Initialize only when triggered (by cron in this case)
initialize = "on-trigger"

# Run every 10 seconds
cron = "*/10 * * * * *"
```

## Field Mapping Reference

### Service-Level Fields

| Docker Compose | service.toml | Notes |
|----------------|--------------|-------|
| `services.<name>.image` | `[service] image` | Same interpolation syntax |
| `services.<name>.ports` | `[[port]]` sections | One `[[port]]` per mapping |
| `services.<name>.volumes` | `[[volume]]` sections | One `[[volume]]` per mount |
| `services.<name>.environment` | `[[env]]` sections | One `[[env]]` per variable |
| `services.<name>.restart` | `restart` | Values: "no", "always", "on-failure", "unless-stopped" |
| `services.<name>.command` | `command` | String or array of strings |
| `services.<name>.entrypoint` | `entrypoint` | String or array of strings |
| `services.<name>.working_dir` | `working_dir` | String path |
| `services.<name>.user` | `user` | String (UID or UID:GID) |
| `services.<name>.hostname` | `hostname` | String |
| `services.<name>.networks` | `networks` | Array of network names |
| N/A | `memory` | New: Resource limits (e.g., "256m", "1g") |
| N/A | `cpus` | New: CPU limits (e.g., "0.5", "1.0") |

### Dispenser-Specific Fields

| Old Location | New Location | Notes |
|--------------|--------------|-------|
| `dispenser.toml: [[instance]].images` | `service.toml: [dispenser].watch` | `images` list → `watch = true/false` |
| `dispenser.toml: [[instance]].cron` | `service.toml: [dispenser].cron` | Same cron syntax |
| `dispenser.toml: [[instance]].initialize` | `service.toml: [dispenser].initialize` | Values: "immediately" or "on-trigger" |

## Complete Migration Examples

### Example 3: Service with Volumes and Environment Variables

**Old (docker-compose.yaml):**
```yaml
version: "3.8"
services:
  webapp:
    image: ${registry}/myapp:${version}
    ports:
      - "${app_port}:3000"
    environment:
      - NODE_ENV=production
      - API_KEY=${api_key}
    volumes:
      - ./data:/app/data
      - ./config:/app/config:ro
    restart: unless-stopped
```

**New (service.toml):**
```toml
[service]
name = "webapp"
image = "${registry}/myapp:${version}"
memory = "512m"
cpus = "1.0"

[[port]]
host = "${app_port}"
container = 3000

[[env]]
name = "NODE_ENV"
value = "production"

[[env]]
name = "API_KEY"
value = "${api_key}"

[[volume]]
source = "./data"
target = "/app/data"

[[volume]]
source = "./config"
target = "/app/config"
readonly = true

restart = "unless-stopped"

[dispenser]
watch = true
initialize = "immediately"
```

### Example 4: Database Service with Networks

**Old (docker-compose.yaml):**
```yaml
version: "3.8"
services:
  postgres:
    image: postgres:15
    ports:
      - "5432:5432"
    environment:
      - POSTGRES_PASSWORD=${db_password}
      - POSTGRES_USER=${db_user}
      - POSTGRES_DB=${db_name}
    volumes:
      - pgdata:/var/lib/postgresql/data
    networks:
      - backend
    restart: always

volumes:
  pgdata:

networks:
  backend:
```

**New (service.toml):**
```toml
[service]
name = "postgres-db"
image = "postgres:15"
memory = "1g"
cpus = "2.0"

[[port]]
host = 5432
container = 5432

[[env]]
name = "POSTGRES_PASSWORD"
value = "${db_password}"

[[env]]
name = "POSTGRES_USER"
value = "${db_user}"

[[env]]
name = "POSTGRES_DB"
value = "${db_name}"

[[volume]]
source = "pgdata"
target = "/var/lib/postgresql/data"

networks = ["backend"]
restart = "always"

[dispenser]
watch = true
initialize = "immediately"
```

### Example 5: Custom Command and Entrypoint

**Old (docker-compose.yaml):**
```yaml
version: "3.8"
services:
  worker:
    image: ${docker_io}/python:3.11
    command: ["python", "worker.py", "--verbose"]
    working_dir: /app
    volumes:
      - ./src:/app
    restart: on-failure
```

**New (service.toml):**
```toml
[service]
name = "worker"
image = "${docker_io}/python:3.11"
command = ["python", "worker.py", "--verbose"]
working_dir = "/app"
memory = "256m"
cpus = "0.5"

[[volume]]
source = "./src"
target = "/app"

restart = "on-failure"

[dispenser]
watch = true
initialize = "immediately"
```

### Example 6: One-Shot Task with Cron

**Old (docker-compose.yaml):**
```yaml
version: "3.8"
services:
  backup:
    image: backup-tool:latest
    volumes:
      - ./backups:/backups
      - ./data:/data:ro
    restart: no
```

**Old (dispenser.toml entry):**
```toml
[[instance]]
path = "backup"
cron = "0 0 2 * * *"  # Daily at 2 AM
initialize = "on-trigger"
images = [{ registry = "docker.io", name = "backup-tool", tag = "latest" }]
```

**New (service.toml):**
```toml
[service]
name = "backup-job"
image = "backup-tool:latest"
memory = "128m"
cpus = "0.5"

[[volume]]
source = "./backups"
target = "/backups"

[[volume]]
source = "./data"
target = "/data"
readonly = true

restart = "no"

[dispenser]
watch = true
initialize = "on-trigger"
cron = "0 0 2 * * *"  # Daily at 2 AM
```

## Migration Checklist

For each service in your project:

- [ ] Create a new `service.toml` file in the service directory
- [ ] Copy the `[service]` section fields from `docker-compose.yaml`:
  - [ ] `image` (with interpolation if used)
  - [ ] `ports` → `[[port]]` sections
  - [ ] `volumes` → `[[volume]]` sections
  - [ ] `environment` → `[[env]]` sections
  - [ ] `restart` policy
  - [ ] Other fields (`command`, `entrypoint`, `working_dir`, etc.)
- [ ] Add `[dispenser]` section with:
  - [ ] `watch = true/false` (was `images` list present?)
  - [ ] `initialize` (was it in `dispenser.toml`?)
  - [ ] `cron` (if present in `dispenser.toml`)
- [ ] Optional: Add `memory` and `cpus` limits
- [ ] Update `dispenser.toml`:
  - [ ] Change `[[instance]]` to `[[service]]`
  - [ ] Remove all fields except `path`
- [ ] Delete the old `docker-compose.yaml` file
- [ ] Test the service configuration

## Important Notes

1. **Variable interpolation is identical**: Both formats use `${variable_name}` syntax
2. **Cron syntax unchanged**: The cron expression format remains the same
3. **Initialize values**: Use `"immediately"` or `"on-trigger"` (case-insensitive, can use underscores or hyphens)
4. **Watch behavior**: 
   - Old: Presence of `images` array meant watching for updates
   - New: Explicit `watch = true/false` field
5. **Port syntax**: 
   - Old: `"8080:80"` in YAML
   - New: `host = 8080` and `container = 80` in separate fields
6. **Volume readonly**:
   - Old: `./config:/app/config:ro`
   - New: `readonly = true` field in volume section
7. **Resource limits**: New format supports `memory` and `cpus` fields that weren't available in the old format

## Troubleshooting

### Common Issues

**Issue**: Service not starting after migration
- **Check**: Verify all required fields are present in `[service]` section
- **Check**: Ensure `[dispenser]` section exists with `initialize` field

**Issue**: Variables not interpolating
- **Check**: Variable names in `dispenser.vars` match those in `service.toml`
- **Check**: Syntax is `${variable_name}` not `$variable_name` or `{variable_name}`

**Issue**: Cron jobs not triggering
- **Check**: `initialize = "on-trigger"` is set in `[dispenser]` section
- **Check**: `cron` field has valid cron expression

**Issue**: Image updates not detected
- **Check**: `watch = true` in `[dispenser]` section
- **Check**: `delay` value in main `dispenser.toml` is reasonable

## Additional Resources

For more examples, compare the provided example directories:
- `example-old/` - Shows the old docker-compose structure
- `example-new/` - Shows the new service.toml structure

Both directories contain functionally equivalent configurations that can serve as reference implementations.