# Using Google Secret Manager

Dispenser allows you to securely retrieve sensitive values, such as API keys or passwords, directly from Google Cloud Secret Manager. These secrets are accessed at runtime and injected into your configuration variables.

## Prerequisites

To use this feature, the environment where Dispenser is running (e.g., a Google Compute Engine VM) must be authenticated with Google Cloud and have permission to access the secrets.

1.  **Service Account**: Ensure the Virtual Machine (VM) is running with a Service Account that has the **Secret Manager Secret Accessor** role (`roles/secretmanager.secretAccessor`).
2.  **Authentication**: If running outside of GCP, you may need to set the `GOOGLE_APPLICATION_CREDENTIALS` environment variable pointing to a service account key file.

## Configuration

You can define secrets in your `dispenser.vars` file. Instead of a plain string value, use a table to specify the secret source and details.

### Syntax

```toml
variable_name = { source = "google", name = "projects/PROJECT_ID/secrets/SECRET_NAME" }
```

-   `source`: Must be set to `"google"`.
-   `name`: The full resource name of the secret. This typically follows the format `projects/<PROJECT_ID>/secrets/<SECRET_NAME>`.
-   `version` (Optional): The version of the secret to retrieve. Defaults to `"latest"` if not specified.

## Example

Suppose you have a secret stored in Google Secret Manager that contains an OAuth Client ID.

**1. Define the secret in `dispenser.vars`:**

```toml
# dispenser.vars

# Regular variable
docker_registry = "docker.io"

# Secret variable from Google Secret Manager
oauth_client_id = { source = "google", name = "projects/123456789012/secrets/MY_OAUTH_CLIENT_ID" }

# Secret variable with a specific version
db_password = { source = "google", name = "projects/123456789012/secrets/DB_PASSWORD", version = "2" }
```

**2. Use the variable in `dispenser.toml` or `docker-compose.yaml`:**

Once defined, these variables can be used just like any other variable in Dispenser.

In `dispenser.toml`:
```toml
[[instance]]
path = "my-service"
# ...
```

In your service's `docker-compose.yaml`:
```yaml
services:
  app:
    image: my-app:latest
    environment:
      - CLIENT_ID=${oauth_client_id}
      - DB_PASS=${db_password}
```

When Dispenser runs, it will fetch the actual values from Google Secret Manager and make them available to your Docker Compose configuration.