# Migration Guide: Docker Compose to service.toml

This guide helps you migrate from the older Dispenser repository structure using `docker-compose.yaml` files to the new structure using `service.toml` files.

## Overview

The new structure replaces Docker Compose YAML files with TOML-based service configuration files. The key changes are:

1. **Per-service configuration**: Each service now has its own `service.toml` file instead of `docker-compose.yaml`
2. **Network declarations**: Networks are now declared in `dispenser.toml` instead of in each `docker-compose.yaml`
3. **Simplified dispenser.toml**: The main configuration file is simplified - it only lists services by path and defines shared networks
4. **Service-level settings**: Image tracking, cron schedules, and initialization behavior are now defined in each `service.toml`
5. **Same interpolation syntax**: Variable interpolation using `${variable_name}` works exactly the same way

## File Structure Comparison

### Old Structure
```
project/
├── dispenser.toml          # Contains service paths, images, cron, initialize settings
├── dispenser.vars          # Variable definitions
└── service-name/
    └── docker-compose.yaml # Docker Compose service definition (with networks defined here)
```

### New Structure
```
project/
├── dispenser.toml          # Contains service paths, polling delay, and network declarations
├── dispenser.vars          # Variable definitions (unchanged)
└── service-name/
    └── service.toml        # Complete service configuration (references networks)
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

# Network declarations (optional)
[[network]]
name = "dispenser-net"
driver = "bridge"

[[service]]
path = "nginx"

[[service]]
path = "hello-world"
```

**Key changes:**
- `[[instance]]` → `[[service]]`
- Remove `images`, `cron`, and `initialize` fields (they move to `service.toml`)
- Keep only `path` to indicate service location
- Add `[[network]]` declarations at the top level (moved from docker-compose.yaml)

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

# Network references (optional)
[[network]]
name = "dispenser-net"

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

# Network references (optional)
[[network]]
name = "dispenser-net"

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
| `services.<name>.environment` | `[env]` map | Key-value pairs in `[env]` section |
| `services.<name>.restart` | `restart` | Values: "no", "always", "on-failure", "unless-stopped" |
| `services.<name>.command` | `command` | String or array of strings |
| `services.<name>.entrypoint` | `entrypoint` | String or array of strings |
| `services.<name>.working_dir` | `working_dir` | String path |
| `services.<name>.user` | `user` | String (UID or UID:GID) |
| `services.<name>.hostname` | `hostname` | String |
| `services.<name>.networks` | `[[network]]` sections | One `[[network]]` per network reference |
| N/A | `memory` | New: Resource limits (e.g., "256m", "1g") |
| N/A | `cpus` | New: CPU limits (e.g., "0.5", "1.0") |

### Dispenser-Specific Fields

| Old Location | New Location | Notes |
|--------------|--------------|-------|
| `dispenser.toml: [[instance]].images` | `service.toml: [dispenser].watch` | `images` list → `watch = true/false` |
| `dispenser.toml: [[instance]].cron` | `service.toml: [dispenser].cron` | Same cron syntax |
| `dispenser.toml: [[instance]].initialize` | `service.toml: [dispenser].initialize` | Values: "immediately" or "on-trigger" |

### Network Configuration

| Old Location | New Location | Notes |
|--------------|--------------|-------|
| `docker-compose.yaml: networks` (top-level) | `dispenser.toml: [[network]]` | Networks declared centrally in main config |
| `docker-compose.yaml: services.<name>.networks` | `service.toml: [[network]]` sections | Services reference networks by name |

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

[env]
NODE_ENV = "production"
API_KEY = "${api_key}"

[[volume]]
source = "./data"
target = "/app/data"

[[volume]]
source = "./config"
target = "/app/config"
readonly = true

[[network]]
name = "app-network"

restart = "unless-stopped"

[dispenser]
watch = true
initialize = "immediately"
```

**dispenser.toml entry:**
```toml
# Network declaration (moved from docker-compose.yaml)
[[network]]
name = "backend"
driver = "bridge"

[[service]]
path = "postgres"
```

**dispenser.toml entry:**
```toml
[[network]]
name = "app-network"
driver = "bridge"

[[service]]
path = "webapp"
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
    driver: bridge
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

[env]
POSTGRES_PASSWORD = "${db_password}"
POSTGRES_USER = "${db_user}"
POSTGRES_DB = "${db_name}"

[[volume]]
source = "pgdata"
target = "/var/lib/postgresql/data"

[[network]]
name = "backend"

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

## Network Migration

Networks are handled differently in the new structure. Instead of defining networks in each `docker-compose.yaml` file, they are now declared centrally in `dispenser.toml` and referenced by services.

### Network Declaration Migration

**Old approach** - Networks defined in docker-compose.yaml:
```yaml
version: "3.8"
services:
  web:
    image: nginx
    networks:
      - frontend
      - backend
  
  db:
    image: postgres
    networks:
      - backend

networks:
  frontend:
    driver: bridge
  backend:
    driver: bridge
```

**New approach** - Networks declared in dispenser.toml:

```toml
# dispenser.toml
delay = 60

# Declare all networks used by services
[[network]]
name = "frontend"
driver = "bridge"

[[network]]
name = "backend"
driver = "bridge"

[[service]]
path = "web"

[[service]]
path = "db"
```

Then services reference these networks in their `service.toml`:

```toml
# web/service.toml
[service]
name = "web"
image = "nginx"

[[network]]
name = "frontend"

[[network]]
name = "backend"

[dispenser]
watch = true
initialize = "immediately"
```

```toml
# db/service.toml
[service]
name = "db"
image = "postgres"

[[network]]
name = "backend"

[dispenser]
watch = true
initialize = "immediately"
```

### Key Points

1. **Central declaration**: All networks must be declared in `dispenser.toml` using `[[network]]` sections
2. **Service references**: Services reference networks using `[[network]]` sections (not an array)
3. **Multiple networks**: A service can reference multiple networks by having multiple `[[network]]` sections
4. **Network attributes**: Currently supported attributes in `dispenser.toml`:
   - `name` (required): The network name
   - `driver` (optional): Network driver (e.g., "bridge", "host", "overlay")
5. **Default network**: If no networks are specified, Docker uses a default network

### Network Array Syntax vs Section Syntax

Note the syntax difference for network references in service configuration:

**Old docker-compose.yaml (array syntax):**
```yaml
services:
  app:
    networks:
      - backend
      - frontend
```

**New service.toml (section syntax):**
```toml
[[network]]
name = "backend"

[[network]]
name = "frontend"
```

Each network reference requires its own `[[network]]` section with a `name` field.

## Migration Checklist

For each service in your project:

- [ ] Create a new `service.toml` file in the service directory
- [ ] Copy the `[service]` section fields from `docker-compose.yaml`:
  - [ ] `image` (with interpolation if used)
  - [ ] `ports` → `[[port]]` sections
  - [ ] `volumes` → `[[volume]]` sections
  - [ ] `environment` → `[env]` map
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
  - [ ] Add `[[network]]` declarations for any networks used (moved from docker-compose.yaml)
- [ ] Update service network references:
  - [ ] Change `networks = ["name"]` array to `[[network]]` sections with `name` field
- [ ] Delete the old `docker-compose.yaml` file
6. **Volume readonly**:
   - Old: `./config:/app/config:ro`
   - New: `readonly = true` field in volume section
7. **Environment variables**:
   - Old: `environment:` array in YAML
   - New: `[env]` map with key-value pairs where keys are variable names
8. **Networks**:
   - Old: Networks defined in each `docker-compose.yaml` file
   - New: Networks declared centrally in `dispenser.toml` with `[[network]]`, services reference them with `[[network]]` sections
9. **Network references**:
   - Old: `networks: ["backend"]` array in YAML
   - New: `[[network]]` sections with `name` field in service.toml
10. **Resource limits**: New format supports `memory` and `cpus` fields that weren't available in the old format

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