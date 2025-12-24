# Dispenser

This tool manages containerized applications by continuously monitoring your artifact registry for new versions of Docker images. When updates are detected, dispenser automatically deploys the new versions of your services with zero downtime, updating the running containers on the host machine.

dispenser operates as a daemon that runs in the background on the host server that watches your artifact registry, detecting when new versions of your container images are published.

## Documentation

- **[CLI Reference](CLI.md)** - Complete command-line options and usage
- **[Service Configuration](SERVICE_CONFIG.md)** - Detailed `service.toml` reference
- **[Network Configuration](NETWORKS.md)** - Docker network setup guide
- **[Cron Scheduling](CRON.md)** - Scheduled deployments
- **[GCP Secrets](GCP.md)** - Google Secret Manager integration
- **[Migration Guide](MIGRATION_GUIDE.md)** - Migrating from Docker Compose

## Prerequisites

Before installing Dispenser, ensure the following are installed on your server:
- **Docker Engine**: Dispenser orchestrates Docker container deployments.
  - [Install Docker Engine on Debian](https://docs.docker.com/engine/install/debian/)
  - [Install Docker Engine on Ubuntu](https://docs.docker.com/engine/install/ubuntu/)
  - [Install Docker Engine on RHEL/CentOS](https://docs.docker.com/engine/install/rhel/)
- **pass**: The standard Unix password manager, used for securely storing registry credentials. It is often available in base repositories or EPEL on RedHat systems.

## Installation

Download the latest `.deb` or `.rpm` package from the [releases page](https://github.com/ixpantia/dispenser/releases).

### Debian / Ubuntu

```sh
# Download the .deb package
# wget https://github.com/ixpantia/dispenser/releases/download/v0.8.0/dispenser-0.8.0-0.x86_64.deb

sudo apt install ./dispenser-0.8.0-0.x86_64.deb
```

### RHEL / CentOS / Fedora

```sh
# Download the .rpm package
# wget ...

sudo dnf install ./dispenser-0.8.0-0.x86_64.rpm
```

The installation process will:
1.  Create a dedicated system user named `dispenser` with its home directory at `/opt/dispenser`.
2.  Add the `dispenser` user to the `docker` group.
3.  Create a default configuration file at `/opt/dispenser/dispenser.toml`.
4.  Install and enable a systemd service to run Dispenser automatically.

## Configuration and Usage

The following steps guide you through setting up your first continuous deployment.

### Step 1: Switch to the `dispenser` User

For security, all configuration is managed by the `dispenser` user.

```sh
sudo su dispenser
cd ~
```

You are now in the `/opt/dispenser` directory. You will see the `dispenser.toml` configuration file here.

### Step 2: Authenticate with a Private Registry (Optional)

If your Docker images are stored in a private registry (like GHCR, Docker Hub private repos, etc.), the server needs to be authenticated to pull them.

1.  Generate an access token from your registry provider with `read` permissions for packages/images.
2.  Log in using the Docker CLI. Replace `<your_registry>` and `<your_username>` accordingly. Paste your access token when prompted for a password.

```sh
# Example for GitHub Container Registry (ghcr.io)
docker login ghcr.io -u <your_username>
```
Docker will securely store the credentials in the `dispenser` user's home directory.

### Step 3: Prepare Your Application Directory

Dispenser deploys applications based on a `service.toml` file.

1.  Create a directory for your application inside `/opt/dispenser`. Let's call it `my-app`.

    ```sh
    mkdir my-app
    cd my-app
    ```

2.  Create a `service.toml` file that defines your service.

    ```sh
    vim service.toml
    ```

    Paste your service definition. Here's a basic example:

    ```toml
    # Service metadata (required)
    [service]
    name = "my-app"
    image = "ghcr.io/my-org/my-app:latest"
    
    # Port mappings (optional)
    [[port]]
    host = 8080
    container = 80
    
    # Environment variables (optional)
    [env]
    DATABASE_URL = "postgres://user:password@host:port/db"
    API_KEY = "your_secret_api_key"
    
    # Restart policy (optional, defaults to "no")
    restart = "always"
    
    # Dispenser-specific configuration (required)
    [dispenser]
    # Watch for image updates
    watch = true
    
    # Initialize immediately on startup
    initialize = "immediately"
    ```

### Step 4: Configure Dispenser to Monitor Your Service

Now, tell Dispenser about your service so it can monitor it for updates.

1.  Return to the `dispenser` home directory and edit the configuration file.

    ```sh
    cd ~
    vim dispenser.toml
    ```

2.  Add a `[[service]]` block to the file. This tells Dispenser where your application is located.

    ```toml
    # How often to check for new images, in seconds.
    delay = 60

    [[service]]
    # Path is relative to /opt/dispenser
    path = "my-app"
    ```

    Dispenser also supports scheduled deployments using `cron` expressions. For more details on configuring periodic restarts, see the [cron documentation](CRON.md).

### Step 5: Service Initialization (Optional)

By default, Dispenser starts services as soon as the application launches. However, you can control this behavior using the `initialize` option in your service's `service.toml` file. This is particularly useful for services that should only run on a specific schedule.

The `initialize` option can be set to one of two values:

- `immediately` (Default): The service is started as soon as Dispenser starts. If you don't specify the `initialize` option, this is the default behavior.
- `on-trigger`: The service will not start on application launch. Instead, it will be initialized only when a trigger occurs. Triggers can be either a cron schedule or a detected update to a watched image.

#### Example: Immediate Initialization

This is the default behavior. The following configuration will start the service immediately.

```toml
# my-app/service.toml
[service]
name = "my-app"
image = "ghcr.io/my-org/my-app:latest"

[[port]]
host = 8080
container = 80

[dispenser]
watch = true
initialize = "immediately"  # This is the default
```

#### Example: Initialization on Trigger

This configuration is useful for scheduled tasks. The service will not start immediately. Instead, it will be triggered to run based on the cron schedule.

```toml
# backup-service/service.toml
[service]
name = "backup-job"
image = "ghcr.io/my-org/backup:latest"

[[volume]]
source = "./backups"
target = "/backups"

[dispenser]
watch = false
initialize = "on-trigger"
cron = "0 3 * * *"  # Run every day at 3 AM
```

In this example, the service defined in the `backup-service` directory will only be started when the cron schedule is met. After its first run, it will continue to be managed by its cron schedule.

### Step 6: Using Variables (Optional)

Dispenser supports using variables in your configuration files via `dispenser.vars` or any file ending in `.dispenser.vars`. These files allow you to define values that can be reused inside `dispenser.toml` and `service.toml` files using `${VARIABLE}` syntax.

**Note:** While Dispenser uses the `${}` syntax similar to Docker Compose, it does not support all [Docker Compose interpolation features](https://docs.docker.com/compose/how-tos/environment-variables/variable-interpolation/) (such as default values `:-` or error messages `:?`).

Variables defined in these files are substituted directly into your configuration files during loading.

This is useful for reusing the same configuration in multiple deployments.

1.  Create a `dispenser.vars` file (or `*.dispenser.vars`) in `/opt/dispenser`.

    ```sh
    vim dispenser.vars
    ```

2.  Define your variables in TOML format.

    ```toml
    registry_url = "ghcr.io"
    app_version = "latest"
    org_name = "my-org"
    ```

    Dispenser also supports fetching secrets from Google Secret Manager. For more details on configuring secrets, see the [GCP secrets documentation](GCP.md).

3.  Use these variables in your `dispenser.toml`.

    ```toml
    delay = 60
    
    [[service]]
    path = "my-app"
    ```

4.  Use these variables in your `service.toml`.

    ```toml
    [service]
    name = "my-app"
    image = "${registry_url}/${org_name}/my-app:${app_version}"
    
    [[port]]
    host = 8080
    container = 80
    
    [dispenser]
    watch = true
    initialize = "immediately"
    ```

### Step 7: Working with Networks (Optional)

Dispenser supports Docker networks to enable communication between services. Networks are declared in `dispenser.toml` and referenced in individual service configurations.

1.  Declare networks in your `dispenser.toml`.

    ```toml
    delay = 60

    # Network declarations
    [[network]]
    name = "app-network"
    driver = "bridge"
    
    [[service]]
    path = "my-app"
    
    [[service]]
    path = "my-database"
    ```

2.  Reference networks in your service configurations.

    ```toml
    # my-app/service.toml
    [service]
    name = "my-app"
    image = "ghcr.io/my-org/my-app:latest"
    
    [[port]]
    host = 8080
    container = 80
    
    [[network]]
    name = "app-network"
    
    [dispenser]
    watch = true
    initialize = "immediately"
    ```

    ```toml
    # my-database/service.toml
    [service]
    name = "postgres-db"
    image = "postgres:15"
    
    [env]
    POSTGRES_PASSWORD = "secretpassword"
    
    [[network]]
    name = "app-network"
    
    [dispenser]
    watch = false
    initialize = "immediately"
    ```

Now both services can communicate with each other using their service names as hostnames.

For advanced network configuration including external networks, internal networks, labels, and different drivers, see the [Network Configuration Guide](NETWORKS.md).

### Step 8: Advanced Service Configuration

The `service.toml` format supports many advanced features. For a complete reference of all available configuration options, see the [Service Configuration Reference](SERVICE_CONFIG.md).

#### Volume Mounts

```toml
[[volume]]
source = "./data"
target = "/app/data"

[[volume]]
source = "./config"
target = "/app/config"
readonly = true
```

#### Custom Commands and Working Directory

```toml
[service]
name = "worker"
image = "python:3.11"
command = ["python", "worker.py", "--verbose"]
working_dir = "/app"
```

#### Resource Limits

```toml
[service]
name = "my-app"
image = "my-app:latest"
memory = "512m"
cpus = "1.0"
```

#### User and Hostname

```toml
[service]
name = "my-app"
image = "my-app:latest"
user = "1000:1000"
hostname = "myapp-container"
```

### Step 9: Validating Configuration

Before applying changes, you can validate your configuration files (including variable substitution) to ensure there are no syntax errors or missing variables.

Run dispenser with the `--test` (or `-t`) flag:

```sh
dispenser --test
```

If the configuration is valid, it will output:
```
Dispenser config is ok.
```

For more command-line options, see the [CLI Reference](CLI.md).

If there's an error, `dispenser` will show you a detailed error message.

```
---------------------------------- <string> -----------------------------------
   2 |
   3 | [service]
   4 | name = "nginx"
   5 > image = "${missing}/nginx:latest"
     i          ^^^^^^^^^^ undefined value
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
No referenced variables
-------------------------------------------------------------------------------
```

### Step 10: Start and Verify the Deployment

1.  Exit the `dispenser` user session to return to your regular user.
    ```sh
    exit
    ```

2.  Restart the Dispenser service to apply the new configuration.
    ```sh
    sudo systemctl restart dispenser
    ```

3.  Check that the service is running correctly.
    ```sh
    sudo systemctl status dispenser
    ```
    You should see `active (running)`.

4.  Verify that your application container is running.
    ```sh
    sudo docker ps
    ```
    You should see a container running with the `ghcr.io/my-org/my-app:latest` image.

From now on, whenever you push a new image to your registry with the `latest` tag (and `watch = true` is set in the service configuration), Dispenser will automatically detect it, pull the new version, and redeploy your service with zero downtime.

### Managing the Service with CLI Signals

Dispenser includes a built-in mechanism to send signals to the running daemon using the `-s` or `--signal` flag. This allows you to reload the configuration or stop the service without needing to use `kill` manually.

**Note:** This command relies on the `dispenser.pid` file, so you should run it from the same directory where Dispenser is running (typically `/opt/dispenser` for the default installation).

For complete CLI documentation including all available flags, see the [CLI Reference](CLI.md).

**Reload Configuration:**

To reload the `dispenser.toml` configuration without restarting the process:

```sh
dispenser -s reload
```

This is useful for adding new services or changing configuration parameters without interrupting currently monitored services.

**Stop Service:**

To gracefully stop the Dispenser daemon:

```sh
dispenser -s stop
```

## Additional Resources

- **[CLI Reference](CLI.md)** - All command-line flags and options
- **[Service Configuration Reference](SERVICE_CONFIG.md)** - Complete field documentation
- **[Network Configuration Guide](NETWORKS.md)** - Advanced networking setup
- **[Cron Documentation](CRON.md)** - Scheduled deployments
- **[GCP Secrets Integration](GCP.md)** - Using Google Secret Manager
- **[Migration Guide](MIGRATION_GUIDE.md)** - Migrating from Docker Compose format

## Building from Source

### RPM (RHEL)

Before you try to build an rpm package, make sure you have the following
installed:

- `cargo`: Rust package manager and build tool
- `rustc`: Rust compiler
- `make`: Run make files
- `rpmbuild`: Tool to build RPMs

Once these dependencies are installed run:

```
make build-rpm
```

This should create a file called something along the lines of
`../dispenser-$VERSION.x86_64.rpm`. There may be minor variations on the Linux
distribution where you are building the package.

### Deb (Debian & Ubuntu)

Before you try to build a deb package, make sure you have the following
installed:

- `cargo`: Rust package manager and build tool
- `rustc`: Rust compiler
- `make`: Run make files
- `dpkg-dev`: Tool to build DEB files

Once these dependencies are installed run:

```
make build-deb
```

This should create a file called something along the lines of
`./dispenser.deb`. There may be minor variations on the Linux
distribution where you are building the package.
