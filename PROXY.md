# Reverse Proxy Configuration

Dispenser includes a built-in, high-performance reverse proxy powered by [Pingora](https://github.com/cloudflare/pingora). It automatically handles TLS termination, ACME certificate management (Let's Encrypt), and intelligent routing to your containerized services.

## Overview

The Dispenser proxy listens on ports 80 (HTTP) and 443 (HTTPS). It automatically:
1.  Optionally redirects HTTP traffic to HTTPS (configurable).
2.  Routes incoming requests to the correct container based on the `Host` header.
3.  Manages SSL/TLS certificates via Let's Encrypt or self-signed "simulation" mode.
4.  Handles Zero-Downtime reloads when configuration changes or certificates are updated.

## Global Configuration

The reverse proxy is enabled by default and defaults to enforcing HTTPS. You can configure the behavior in your main `dispenser.toml` file.

### Proxy Strategy

The `strategy` field determines how Dispenser handles HTTP (80) and HTTPS (443) traffic.

```toml
# dispenser.toml

[proxy]
enabled = true
# Available options: "https-only", "http-only", "both"
strategy = "https-only"
```

| Strategy | Behavior |
| :--- | :--- |
| `https-only` | (Default) Port 80 redirects all traffic to Port 443. SSL is required. |
| `http-only` | Port 80 serves application traffic. Port 443 and SSL management are disabled. |
| `both` | Both ports serve application traffic. No automatic redirects occur. |

### Global Toggle

When the `enabled` flag is set to `false`, both the proxy server and the automatic certificate maintenance tasks are turned off.

```toml
# dispenser.toml

[proxy]
enabled = false
```

> [!IMPORTANT]
> Enabling or disabling the proxy via the `enabled` flag requires a full process restart. Changing this value and reloading with `dispenser -s reload` will result in a warning and the change will not take effect until the next full start.

## Service Configuration

To expose a service through the proxy, add a `[proxy]` section to your `service.toml`.

```toml
# my-app/service.toml

[service]
name = "web-app"
image = "my-registry/web-app:latest"

[proxy]
# The domain name the proxy should listen for
host = "app.example.com"
# Optional: The path prefix for this service (defaults to "/")
path = "/api"
# The port the service is listening on INSIDE the container
service_port = 8080
```

### Proxy Settings Reference

| Field | Type | Description |
| :--- | :--- | :--- |
| `host` | `string` | The FQDN (Fully Qualified Domain Name) for this service. |
| `path` | `string` | (Optional) The URL path prefix. Defaults to `/`. |
| `service_port` | `u16` | The private port inside the container where the app is running. |
| `cert_file` | `string` | (Optional) Path to a custom SSL certificate file. |
| `key_file` | `string` | (Optional) Path to a custom SSL private key file. |

## SSL/TLS Management

Dispenser provides three ways to handle SSL certificates:

### 1. Automatic ACME (Let's Encrypt)
If you provide an email address in the `[certbot]` section of your main `dispenser.toml`, Dispenser will automatically negotiate certificates with Let's Encrypt using the HTTP-01 challenge.

> [!NOTE]
> The `[certbot]` section must be explicitly defined. If it is missing, Dispenser assumes you are providing custom certificates manually via `cert_file` and `key_file` in your `service.toml`, or it will attempt to use simulation mode if running via the `dev` command.

```toml
# dispenser.toml
delay = 60

[certbot]
email = "admin@example.com"

[[service]]
path = "my-app"
```

Dispenser handles the challenge internally. Ensure your server is accessible on port 80 from the internet.

### 2. Manual Certificates
If you already have certificates (e.g., from a corporate CA or Wildcard cert), you can specify them in the `service.toml`.

```toml
[proxy]
host = "internal.example.com"
service_port = 80
cert_file = "/etc/ssl/certs/internal.crt"
key_file = "/etc/ssl/certs/internal.key"
```

### 3. Simulation Mode (Self-Signed)
For local development or environments without public DNS, you can run Dispenser in simulation mode using the `dev` command. It will generate self-signed certificates for all configured hosts on the fly.

```bash
dispenser dev -s my-app
```

## How Routing Works

1.  **Request Arrival**: A request arrives at Dispenser on port 443.
2.  **SNI Matching**: The proxy looks at the Server Name Indication (SNI) to select the correct SSL certificate.
3.  **Host & Path Matching**: Once the TLS handshake is complete, the proxy looks at the `Host` HTTP header and the request path.
4.  **Upstream Resolution**: It finds the container matching the host and the longest matching path prefix, then forwards the request to the container's internal IP address on the specified `service_port`.

### Internal Networking
The proxy communicates with containers over the default `dispenser` network (`172.28.0.0/16`). You do not need to expose ports to the host machine (via `[[port]]`) for the proxy to work; it communicates directly with the container's private IP.

## Zero-Downtime Reloads

When you reload Dispenser (`dispenser -s reload`) or when certificates are renewed:
1.  Dispenser starts a new "generation" of the proxy.
2.  The new proxy starts listening for new connections.
3.  The old proxy stops accepting new connections but finishes processing existing ones.
4.  Once all old connections are drained, the old proxy instance exits.

## Troubleshooting

- **502 Bad Gateway**: This usually means the container is not running or the `service_port` defined in `service.toml` is incorrect.
- **Connection Refused**: Ensure the `dispenser` process is running and has permission to bind to ports 80 and 443 (this usually requires `sudo` or specific capabilities).
- **Certificate Errors**: 
    - Check the logs: `journalctl -u dispenser -f`.
    - If using Let's Encrypt, ensure port 80 is open to the world.
    - Certificates are stored in `.dispenser/certs` relative to the working directory.