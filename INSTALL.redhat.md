# Installation on RedHat Systems

## Requirements

Dispenser requires Docker to be installed in the system
as well as [pass](https://www.redhat.com/en/blog/management-password-store).

`pass` is only available on [EPEL](https://www.redhat.com/en/blog/whats-epel-and-how-do-i-use-it) 
so make sure it is enabled before proceeding.

## Install Docker

We recommend to follow the official guide to install Docker on RedHat. This
can change so please refer to the official documentation.

* [Install Docker Engine on RHEL](https://docs.docker.com/engine/install/rhel/)

## Install Dispenser

```sh
wget ...
```


```sh
sudo dnf install ./dispenser-0.8.0-0.x86_64.rpm
```

You can validate that it was successfully installed by switching to the
`dispenser` user.

```sh
sudo su dispenser
```
