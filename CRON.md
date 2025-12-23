# Using Cron for Scheduled Deployments

Dispenser supports cron scheduling to deploy or restart services at specific intervals. This is useful for batch jobs, backups, ETL processes, or periodic maintenance restarts.

## Configuration

Add a `cron` field to the `[dispenser]` section in your `service.toml`:

```toml
[dispenser]
watch = false
initialize = "on-trigger"
cron = "0 0 2 * * *"  # Every day at 2 AM
```

### Cron Expression Format

Dispenser uses a 6-field format (with seconds):

```
┌───────────── second (0 - 59)
│ ┌───────────── minute (0 - 59)
│ │ ┌───────────── hour (0 - 23)
│ │ │ ┌───────────── day of month (1 - 31)
│ │ │ │ ┌───────────── month (1 - 12)
│ │ │ │ │ ┌───────────── day of week (0 - 6, Sunday = 0)
│ │ │ │ │ │
* * * * * *
```

**Common expressions:**
- `0 0 2 * * *` - Daily at 2 AM
- `0 0 */6 * * *` - Every 6 hours
- `0 30 9 * * 1-5` - Weekdays at 9:30 AM
- `0 0 0 1 * *` - First day of each month
- `*/10 * * * * *` - Every 10 seconds

Use [crontab.guru](https://crontab.guru/) for help (add `0` for seconds field).

## Examples

### Scheduled Backup Job

```toml
[service]
name = "backup-job"
image = "my-backup:latest"

[[volume]]
source = "./backups"
target = "/backups"

restart = "no"

[dispenser]
watch = false
initialize = "on-trigger"
cron = "0 0 2 * * *"  # Daily at 2 AM
```

### ETL Job Every Hour

```toml
[service]
name = "etl-processor"
image = "my-etl:latest"
command = ["python", "process.py"]

restart = "no"

[dispenser]
watch = false
initialize = "on-trigger"
cron = "0 0 * * * *"  # Every hour
```

### Periodic Restart with Image Watching

```toml
[service]
name = "worker"
image = "my-worker:latest"

restart = "always"

[dispenser]
watch = true
initialize = "immediately"
cron = "0 0 4 * * *"  # Restart daily at 4 AM
```

This configuration will:
- Deploy when a new image is detected
- Also restart daily at 4 AM (even if no new image)

## Options

### `initialize`

- `immediately` (default) - Start when Dispenser starts
- `on-trigger` - Only start when cron fires or image updates

### `watch`

- `true` - Monitor registry for image updates
- `false` - Only run on cron schedule

### `restart`

Use `restart = "no"` for one-time jobs to prevent automatic restarts between scheduled runs.