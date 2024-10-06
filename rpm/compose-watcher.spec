Buildroot: /home/andres/projects/ixpantia/imasd/contpose/compose-watcher-0.1
Name: compose-watcher
Version: 0.1
Release: 0
Summary: Continously Deploy services with Docker Compose
License: see /usr/share/doc/compose-watcher/copyright
Distribution: Debian
Group: Converted/unknown
Requires: docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

%define _rpmdir ../
%define _rpmfilename %%{NAME}-%%{VERSION}-%%{RELEASE}.%%{ARCH}.rpm
%define _unpackaged_files_terminate_build 0

%post
#!/bin/sh
set -e

# Create the 'compose-watcher' user and home directory if it doesn't exist
if ! id -u compose-watcher > /dev/null 2>&1; then
    useradd -r -d /opt/compose-watcher -s /bin/false compose-watcher
    mkdir -p /opt/compose-watcher
    chown compose-watcher:compose-watcher /opt/compose-watcher
fi

# Create the compose-watcher.toml file if it doesn't exist
if [ ! -f /opt/compose-watcher/compose-watcher.toml ]; then
    echo "delay=60 # Will watch for updates every 60 seconds" >> /opt/compose-watcher/compose-watcher.toml
    chown compose-watcher:compose-watcher /opt/compose-watcher/compose-watcher.toml
fi

# Restart the service on upgrade
systemctl daemon-reload || true
systemctl enable compose-watcher.service || true
systemctl start compose-watcher.service || true


%preun
#!/bin/sh
set -e

systemctl stop compose-watcher.service || true
systemctl disable compose-watcher.service || true


%postun
#!/bin/sh
set -e

systemctl daemon-reload || true
rm -f /lib/systemd/system/compose-watcher.service

rm -rf /usr/local/bin/compose-watcher

# Remove the compose-watcher user and its home directory
userdel -r compose-watcher || true
rm -rf /opt/compose-watcher


%description

%files
%dir "/opt/compose-watcher"
"/lib/systemd/system/compose-watcher.service"
"/usr/local/bin/compose-watcher"
