Name: dispenser
Version: 0.2
Release: 0
Summary: Continously Deploy services with Docker Compose
License: see /usr/share/doc/dispenser/copyright
Distribution: Debian
Group: Converted/unknown
Requires: docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin, gnupg2, pass

%define _rpmdir ./
%define _rpmfilename %%{NAME}-%%{VERSION}-%%{RELEASE}.%%{ARCH}.rpm
%define _unpackaged_files_terminate_build 0

%post
# Create the 'dispenser' user and home directory if it doesn't exist
if ! id -u dispenser > /dev/null 2>&1; then
    useradd -r -d /opt/dispenser -s /bin/bash dispenser
    usermod -aG docker dispenser
    mkdir -p /opt/dispenser
    chown dispenser:dispenser /opt/dispenser
    chmod -R 700 /opt/dispenser
fi

# Create the dispenser.toml file if it doesn't exist
if [ ! -f /opt/dispenser/dispenser.toml ]; then
    echo "delay=60 # Will watch for updates every 60 seconds" >> /opt/dispenser/dispenser.toml
    chown dispenser:dispenser /opt/dispenser/dispenser.toml
fi

# Restart the service on upgrade
systemctl daemon-reload || true
systemctl enable dispenser.service || true
systemctl start dispenser.service || true


%preun
systemctl stop dispenser.service || true
systemctl disable dispenser.service || true


%postun
if [ $1 = 0 ]
then
    # the package is really being uninstalled, not upgraded
    systemctl daemon-reload || true
    rm -f /lib/systemd/system/dispenser.service

    rm -rf /usr/local/bin/dispenser

    # Remove the dispenser user and its home directory
    userdel -r dispenser || true
    rm -rf /opt/dispenser
fi



%description

%files
%dir "/opt/dispenser"
"/usr/lib/systemd/system/dispenser.service"
"/usr/local/bin/dispenser"
