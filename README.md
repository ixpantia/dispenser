# Dispenser

This tool manages applications defined in Docker Compose by continuously
monitoring your artifact registry for new versions of Docker images. When
updates are detected, dispenser automatically deploys the new versions of your
services with zero downtime, updating the running containers on the host
machine.

dispenser operates as a daemon that runs in the background on the host server
that watches your artifact registry, detecting when new versions of your
container images are published.

## Prerequisites

Before installing Dispenser, ensure the following are installed on your server:
- **Docker Engine and Docker Compose**: Dispenser orchestrates Docker Compose deployments.
  - [Install Docker Engine on Debian](https://docs.docker.com/engine/install/debian/)
  - [Install Docker Engine on Ubuntu](https://docs.docker.com/engine/install/ubuntu/)
  - [Install Docker Engine on RHEL/CentOS](https://docs.docker.com/engine/install/rhel/)
- **pass**: The standard Unix password manager, used for securely storing registry credentials. It is often available in base repositories or EPEL on RedHat systems.

## Installation

Download the latest `.deb` or `.rpm` package from the [releases page](https://github.com/ixpantia/dispenser/releases).

### Debian / Ubuntu

```sh
# Download the .deb package
# wget https://github.com/ixpantia/dispenser/releases/download/v0.3.0/dispenser-0.3-0.x86_64.deb

sudo apt install ./dispenser-0.3-0.x86_64.deb
```

### RHEL / CentOS / Fedora

```sh
# Download the .rpm package
# wget ...

sudo dnf install ./dispenser-0.3-0.x86_64.rpm
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

Dispenser deploys applications based on a `docker-compose.yaml` file.

1.  Create a directory for your application inside `/opt/dispenser`. Let's call it `my-app`.

    ```sh
    mkdir my-app
    cd my-app
    ```

2.  Create a `docker-compose.yaml` file that defines your service.

    ```sh
    vim docker-compose.yaml
    ```

    Paste your service definition. Note that the image points to the `:latest` tag, which Dispenser will monitor.

    ```yaml
    services:
      my-app:
        image: ghcr.io/my-org/my-app:latest
        ports:
          - "8080:80"
        env_file: .env
    ```

3.  (Optional) Create an `.env` file for your application's environment variables.

    ```sh
    vim .env
    ```
    ```
    DATABASE_URL=postgres://user:password@host:port/db
    API_KEY=your_secret_api_key
    ```

### Step 4: Configure Dispenser to Watch Your Image

Now, tell Dispenser to monitor your image for updates.

1.  Return to the `dispenser` home directory and edit the configuration file.

    ```sh
    cd ~
    vim dispenser.toml
    ```

2.  Add an `[[instance]]` block to the file. This tells Dispenser where your application is and which image to watch.

    ```toml
    # How often to check for new images, in seconds.
    delay = 60

    [[instance]]
    # Path is relative to /opt/dispenser
    path = "my-app"
    images = [{ registry = "ghcr.io", name = "my-org/my-app", tag = "latest" }]
    ```

    Dispenser also supports scheduled deployments using `cron` expressions. For more details on configuring periodic restarts, see the [cron documentation](CRON.md).

### Step 5: Service Initialization (Optional)

By default, Dispenser starts services as soon as the application launches. However, you can control this behavior using the `initialize` option in your `dispenser.toml` file. This is particularly useful for services that should only run on a specific schedule.

The `initialize` option can be set to one of two values:

- `immediately` (Default): The service is started as soon as Dispenser starts. If you don't specify the `initialize` option, this is the default behavior.
- `on-trigger`: The service will not start on application launch. Instead, it will be initialized only when a trigger occurs. Triggers can be either a cron schedule or a detected update to a watched image.

#### Example: Immediate Initialization

This is the default behavior. The following configuration will start the `my-app` service immediately.

```toml
[[instance]]
path = "my-app"
images = [{ registry = "ghcr.io", name = "my-org/my-app", tag = "latest" }]
# initialize = "immediately" # This line is optional
```

#### Example: Initialization on Trigger

This configuration is useful for scheduled tasks. The `backup-service` will not start immediately. Instead, it will be triggered to run based on the cron schedule.

```toml
[[instance]]
path = "backup-service"
cron = "0 3 * * *" # Run every day at 3 AM
initialize = "on-trigger"
```
In this example, the service defined in the `backup-service` directory will only be started when the cron schedule is met. After its first run, it will continue to be managed by its cron schedule.

### Step 6: Start and Verify the Deployment

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

From now on, whenever you push a new image to your registry with the `latest` tag, Dispenser will automatically detect it, pull the new version, and redeploy your service with zero downtime.

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
