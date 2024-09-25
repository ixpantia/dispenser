# Contpose

I am not very good at naming things. This is
the convination of Continuous and Compose.

This tool manages Docker Compose instances
by constantly looking for new versions of
images to deploying seemlessly.

Contpose is meant to be installed and used
as a service with systemd on Linux.

## Installing

TODO

## Creating a service

Once contpose in installed create a Systemd
service to manage. Replace the fields with `${}`
to the values you need for your particular
use case.

```
[Unit]
Description=${DESCRIPTION}
After=docker.service
BindsTo=docker.service
ReloadPropagatedFrom=docker.service

[Service]
Type=simple
Restart=always
RestartSec=1
User=${USER}
ExecStart=/usr/bin/contpose
WorkingDirectory=${/path/to/contpose.toml}

[Install]
WantedBy=multi-user.target
```

You can then create a file with `${SERVICE_NAME}.service`
to `/etc/systemd/system/`.

### Reload the daemon

```
sudo systemctl daemon-reload
```


### Enable and restart service

```
sudo systemctl enable ${SERVICE_NAME}.service
sudo systemctl start ${SERVICE_NAME}.service
```
