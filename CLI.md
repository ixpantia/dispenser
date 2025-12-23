# CLI Reference

This document describes all available command-line options for Dispenser.

## Usage

```sh
dispenser [OPTIONS]
```

## Options

### `-c, --config <PATH>`

Specify the path to the configuration file.

**Default:** `dispenser.toml`

**Example:**
```sh
dispenser --config /etc/dispenser/my-config.toml
```

### `-t, --test`

Test the configuration file and exit. This validates your configuration files (including variable substitution) to ensure there are no syntax errors or missing variables.

**Example:**
```sh
dispenser --test
```

**Output on success:**
```
Dispenser config is ok.
```

**Output on error:**
```
---------------------------------- <string> -----------------------------------
   2 |
   3 | [service]
   4 | name = "nginx"
   5 > image = "${missing}/nginx:latest"
     i          ^^^^^^^^^^ undefined value
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
No referenced variables
-------------------------------------------------------------------------------
```

### `-p, --pid-file <PATH>`

Specify the path to the PID file. This file is used to track the running Dispenser process and is required for sending signals with the `--signal` flag.

**Default:** `dispenser.pid`

**Example:**
```sh
dispenser --pid-file /var/run/dispenser.pid
```

### `-s, --signal <SIGNAL>`

Send a signal to the running Dispenser instance. This command relies on the PID file, so you should run it from the same directory where Dispenser is running (typically `/opt/dispenser` for the default installation).

**Valid signals:**
- `reload` - Reload the `dispenser.toml` configuration without restarting the process
- `stop` - Gracefully stop the Dispenser daemon

**Examples:**
```sh
# Reload configuration
dispenser --signal reload

# Stop the daemon
dispenser --signal stop
```

### `-h, --help`

Display help information about available options.

**Example:**
```sh
dispenser --help
```

### `-V, --version`

Display the current version of Dispenser.

**Example:**
```sh
dispenser --version
```

## Common Usage Patterns

### Running in Foreground (for testing)

```sh
dispenser --config ./dispenser.toml
```

### Validating Configuration Before Deployment

```sh
dispenser --test && echo "Configuration is valid!"
```

### Reloading Configuration After Changes

```sh
# After editing dispenser.toml
dispenser --signal reload
```

### Using Custom Paths

```sh
dispenser --config /etc/dispenser/production.toml --pid-file /var/run/dispenser-prod.pid
```

## Systemd Integration

When Dispenser is installed via the `.deb` or `.rpm` package, it runs as a systemd service. You can manage it using standard systemd commands:

```sh
# Start the service
sudo systemctl start dispenser

# Stop the service
sudo systemctl stop dispenser

# Restart the service
sudo systemctl restart dispenser

# Check service status
sudo systemctl status dispenser

# View logs
sudo journalctl -u dispenser -f
```

The systemd service automatically uses the configuration at `/opt/dispenser/dispenser.toml` and runs as the `dispenser` user.
