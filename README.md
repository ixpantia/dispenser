# Dispenser

This tool manages applications defined in Docker Compose by continuously
monitoring your artifact registry for new versions of Docker images. When
updates are detected, dispenser automatically deploys the new versions of your
services with zero downtime, updating the running containers on the host
machine.

dispenser operates as a daemon that runs in the background on the host server
that watches your artifact registry, detecting when new versions of your
container images are published.

## Example configuration

This is an example configuration (in a toml config file) that you can base
yours around. This configuration listens to changes on the `nginx:latest` image
and reloads the docker compose found in the directory `/opt/dispenser/example`.

```toml
delay=60 # Will watch for updates every 60 seconds

[[instance]]
path = "example"
images = [
  { registry = "docker.io", name = "nginx", tag = "latest" }
]
```

## Build

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

