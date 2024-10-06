# Compose Watcher

This tool manages Docker Compose instances by constantly looking for new
versions of images to deploying seemlessly.

Compose Watcher works as a daemon that runs in the background of the host server.

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


This should create a file called roughly `../compose-watcher-$VERSION.x86_64.rpm`.

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

This should create a file called roughly `./compose-watcher.deb`.

