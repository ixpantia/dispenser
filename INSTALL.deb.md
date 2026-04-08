# Installation on Debian / Ubuntu Systems

## Requirements

Dispenser requires Docker to be installed in the system.

## Install Docker

We recommend to follow the official guide to install Docker on Ubuntu/Debian. This
can change so please refer to the official documentation.

* [Install Docker Engine on Debian](https://docs.docker.com/engine/install/debian/)
* [Install Docker Engine on Ubuntu](https://docs.docker.com/engine/install/ubuntu/)

## Install Dispenser

Download the package matching your operating system:

*   **Debian 12**: `dispenser-0.21.0-0-debian-12.x86_64.deb`
*   **Debian 13**: `dispenser-0.21.0-0-debian-13.x86_64.deb`
*   **Ubuntu 24.04**: `dispenser-0.21.0-0-ubuntu-24.x86_64.deb`

```sh
# Example for Ubuntu 24.04
sudo apt install ./dispenser-0.21.0-0-ubuntu-24.x86_64.deb
```

You can validate that it was successfully installed by switching to the
`dispenser` user.

```sh
sudo su dispenser
```
