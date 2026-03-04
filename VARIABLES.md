# Dispenser Variables

Dispenser allows you to define variables in `.vars` files that can be used to template your configuration files (`dispenser.toml` and `service.toml`). This allows you to reuse configuration across different environments (e.g., staging vs. production) or dynamically generate configuration without duplicating code.

## Variable Files

Dispenser looks for variable files in the same directory as the `dispenser.toml` file. Variables are defined using standard TOML key-value pairs.

- `dispenser.vars`: The primary variables file.
- `*.dispenser.vars`: Secondary variable files, which can be used to organize configuration by component or environment (e.g., `prod.dispenser.vars`, `secrets.dispenser.vars`).

```toml
# Example dispenser.vars
env = "prod"
registry_url = "ghcr.io"
```

## Load Order and Templating

The variable files are loaded in a specific order. This load order is crucial because it allows the secondary `*.dispenser.vars` files to be dynamically templated based on the variables defined in the primary `dispenser.vars` file.

1. **`dispenser.vars` is loaded first.**
   All variables defined in this file are evaluated and materialized (this includes resolving external secrets, like fetching from GCP Secret Manager).
2. **`*.dispenser.vars` files are loaded next.**
   When loading these secondary files in parallel, Dispenser processes them as templates. It uses the variables that were already materialized from `dispenser.vars` to render their contents.
3. **All variables are combined.**
   Finally, the materialized variables from all files are combined into a single pool. These combined variables are then used to template your actual `dispenser.toml` and `service.toml` configuration files.

### Templating Syntax

Dispenser uses the [MiniJinja](https://docs.rs/minijinja/latest/minijinja/) template engine.
- **Variables:** Use `${ VARIABLE_NAME }` for variable interpolation.
- **Logic Blocks:** Use standard Jinja `{% ... %}` tags for conditional logic, such as `if/else` statements.

### Example: Environment-Based Configuration

Because `dispenser.vars` is evaluated first, you can establish foundational variables (like `env`) and use them to conditionally configure other variables in secondary files.

**`dispenser.vars`**
```toml
# Define the current environment
env = "prod"
```

**`config.dispenser.vars`**
```toml
{% if env == "prod" %}
db_host = "prod-db.internal"
nginx_port = "80"
api_key = { source = "google", name = "projects/123/secrets/PROD_API_KEY" }
{% else %}
db_host = "stg-db.internal"
nginx_port = "8080"
api_key = { source = "google", name = "projects/123/secrets/STG_API_KEY" }
{% endif %}
```

In this setup, the `config.dispenser.vars` file acts dynamically. It evaluates the `env` variable from `dispenser.vars` to define the correct `db_host` and fetch the correct `api_key` secret.

## Using Variables in Configuration

Once all variables are loaded and combined, they can be substituted into your main configuration files. 

**`service.toml`**
```toml
[service]
name = "my-app"
image = "${registry_url}/my-app:latest"

[[port]]
host = ${nginx_port}
container = 80

[env]
DATABASE_URL = "postgres://user:pass@${db_host}/mydb"
API_KEY = "${api_key}"
```

## Secrets Integration

Dispenser supports retrieving secrets from external providers like Google Secret Manager. These are defined as special variable types within your `.vars` files and behave like any other variable once loaded.

```toml
# Fetch a secret from GCP
my_secret = { source = "google", name = "projects/PROJECT_ID/secrets/SECRET_NAME" }
```

For more details on configuring and troubleshooting GCP secrets, see the [GCP Secrets Documentation](GCP.md).