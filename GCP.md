# Using Google Secret Manager

Dispenser can retrieve secrets from Google Cloud Secret Manager and use them in your configuration files.

## Prerequisites

The environment where Dispenser runs must be authenticated with Google Cloud and have permission to access secrets:

1. **Service Account**: The VM must use a Service Account with the `roles/secretmanager.secretAccessor` role
2. **Authentication**: If running outside GCP, set `GOOGLE_APPLICATION_CREDENTIALS` to a service account key file

## Configuration

Define secrets in your `dispenser.vars` (or `*.dispenser.vars`) file:

```toml
# dispenser.vars

# Regular variables
registry = "gcr.io"
project = "my-project"

# Secrets from Google Secret Manager
db_password = { source = "google", name = "projects/123456789012/secrets/DB_PASSWORD" }
api_key = { source = "google", name = "projects/123456789012/secrets/API_KEY" }
oauth_client = { source = "google", name = "projects/123456789012/secrets/OAUTH_CLIENT_ID", version = "2" }
```

### Syntax

```toml
variable_name = { source = "google", name = "projects/PROJECT_ID/secrets/SECRET_NAME" }
```

- `source`: Must be `"google"`
- `name`: Full resource name of the secret
- `version`: (Optional) Secret version, defaults to `"latest"`

## Usage

Use secrets like any other variable in your configuration files:

```toml
# my-app/service.toml

[service]
name = "my-app"
image = "${registry}/${project}/my-app:latest"

[env]
DATABASE_URL = "postgres://user:${db_password}@postgres:5432/mydb"
API_KEY = "${api_key}"
OAUTH_CLIENT_ID = "${oauth_client}"

[dispenser]
watch = true
```

## Setting Up Secrets

### Enable API

```sh
gcloud services enable secretmanager.googleapis.com --project=PROJECT_ID
```

### Create Secret

```sh
# Create secret
gcloud secrets create DB_PASSWORD --project=PROJECT_ID

# Add value
echo -n "my-secure-password" | gcloud secrets versions add DB_PASSWORD --data-file=- --project=PROJECT_ID
```

### Grant Access

```sh
gcloud secrets add-iam-policy-binding DB_PASSWORD \
  --member="serviceAccount:SERVICE_ACCOUNT_EMAIL" \
  --role="roles/secretmanager.secretAccessor" \
  --project=PROJECT_ID
```

## Validation

Test your configuration:

```sh
dispenser --test
```

This verifies:
- Connectivity to Secret Manager
- All secrets exist
- Proper permissions

## Troubleshooting

### Permission Denied

Check service account has the correct role:

```sh
gcloud projects add-iam-policy-binding PROJECT_ID \
  --member="serviceAccount:SERVICE_ACCOUNT_EMAIL" \
  --role="roles/secretmanager.secretAccessor"
```

### Secret Not Found

Verify the secret exists:

```sh
gcloud secrets list --project=PROJECT_ID
gcloud secrets describe SECRET_NAME --project=PROJECT_ID
```

### Test Access

```sh
gcloud secrets versions access latest --secret="SECRET_NAME" --project="PROJECT_ID"
```
