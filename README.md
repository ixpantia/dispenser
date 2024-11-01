# Dispenser

This tool manages Docker Compose instances by constantly looking for new
versions of images to deploying seemlessly.

Dispenser works as a daemon that runs in the background of the host server.

## Example config

This is an example config that you can base yours around. This config
listens to changes on the `nginx:latest` image and reloads the
docker compose found in the directory `/opt/dispenser/example`.

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

To build rpm make sure you have the following installed:

- `cargo`: Rust package manager and build tool
- `rustc`: Rust compiler
- `make`: Run make files
- `rpmbuild`: Tool to build RPMs

Once these dependencies are installed run:

```
make build-rpm
```


This should create a file called roughly `../dispenser-$VERSION.x86_64.rpm`.

### Deb (Debian & Ubuntu)

To build deb make sure you have the following installed:

- `cargo`: Rust package manager and build tool
- `rustc`: Rust compiler
- `make`: Run make files
- `dpkg-dev`: Tool to build DEB files

Once these dependencies are installed run:

```
make build-deb
```

This should create a file called roughly `./dispenser.deb`.

