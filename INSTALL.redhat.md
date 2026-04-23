# Installation on RedHat Systems

## Requirements

Dispenser requires Docker to be installed in the system.

`pass` is only available on [EPEL](https://www.redhat.com/en/blog/whats-epel-and-how-do-i-use-it) 
so make sure it is enabled before proceeding.

## Install Docker

We recommend to follow the official guide to install Docker on RedHat. This
can change so please refer to the official documentation.

* [Install Docker Engine on RHEL](https://docs.docker.com/engine/install/rhel/)

## Install Dispenser

Download the package matching your operating system:

*   **RHEL 8 / Rocky 8**: `dispenser-0.22.0-0.rhel-8.x86_64.rpm`
*   **RHEL 9 / Rocky 9**: `dispenser-0.22.0-0.rhel-9.x86_64.rpm`

```sh
# Example for RHEL 9
sudo dnf install ./dispenser-0.22.0-0.rhel-9.x86_64.rpm
```

You can validate that it was successfully installed by switching to the
`dispenser` user.

```sh
sudo su dispenser
```
