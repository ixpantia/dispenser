# Installation on Debian / Ubuntu Systems

## Requirements

Dispenser requieres Docker and Docker Compose to be installed in the system
as well as [pass](https://www.passwordstore.org/).

## Install Docker

We recommend to follow the official guide to install Docker on Ubuntu/Debian. This
can change so please refer to the official documentation.

* [Install Docker Engine on Debian](https://docs.docker.com/engine/install/debian/)
* [Install Docker Engine on Ubuntu](https://docs.docker.com/engine/install/ubuntu/)

## Install Dispenser

```sh
wget ...
```


```sh
sudo apt install ./dispenser-0.7-0.x86_64.deb
```

You can validate that it was successfully installed by switching to the
`dispenser` user.

```sh
sudo su dispenser
```
