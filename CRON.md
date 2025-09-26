# Using Cron for Scheduled Deployments

Dispenser provides a `cron` feature to schedule deployments or restarts of your services at specific intervals. This is useful for tasks that need to run periodically, such as batch jobs, or for ensuring services are restarted regularly for maintenance.

## How it Works

You can add a `cron` attribute to any `[[instance]]` block in your `dispenser.toml` configuration file. The value of this attribute is a cron expression that defines the schedule for the deployment.

When a `cron` schedule is defined for an instance, Dispenser will trigger a redeployment of the corresponding Docker Compose service according to the schedule. This is equivalent to running `docker-compose up -d --force-recreate` for the service.

The cron scheduler uses a 6-field format that includes seconds:

```
┌───────────── second (0 - 59)
│ ┌───────────── minute (0 - 59)
│ │ ┌───────────── hour (0 - 23)
│ │ │ ┌───────────── day of the month (1 - 31)
│ │ │ │ ┌───────────── month (1 - 12)
│ │ │ │ │ ┌───────────── day of the week (0 - 6) (Sunday to Saturday)
│ │ │ │ │ │
│ │ │ │ │ │
* * * * * *
```

You can use online tools like [crontab.guru](https://crontab.guru/) to help generate the correct cron expression. Note that many online tools generate 5-field expressions, so you may need to add the seconds field (`*` or `0`) at the beginning.

## Use Cases

### Scheduled-Only Deployments

You can use `cron` without an `images` attribute. This is ideal for services that run on a schedule such as ETLs or batch processing tasks, and do not have a corresponding image to monitor for updates.

**Example:**
The following configuration will run the `hello-world` service every 10 seconds. Since there is no image to watch, the deployment is only triggered by the cron schedule.

```toml
# dispenser.toml

[[instance]]
path = "hello-world"
cron = "*/10 * * * * *"
```

The `docker-compose.yaml` for this service might look like this. It is important to set `restart: no` to prevent the container from restarting automatically after its task is complete. It will wait for the next scheduled run from Dispenser.

```yaml
# hello-world/docker-compose.yaml

version: "3.8"
services:
  hello-world:
    image: hello-world
    restart: no
```

### Scheduled Restarts with Image Monitoring

You can use `cron` in combination with image monitoring. In this case, Dispenser will deploy a new version of your service under two conditions:
1.  A new Docker image is detected in the registry.
2.  The `cron` schedule is met.

This is useful for services that should be restarted periodically, even if no new image is available.

**Example:**
The following configuration watches the `nginx:latest` image and also restarts the service every minute.

```toml
# dispenser.toml

[[instance]]
path = "nginx"
# Will restart the service every minute or when the nginx image gets updated
cron = "0 */1 * * * *"
images = [{ registry = "docker.io", name = "nginx", tag = "latest" }]
```

By using the `cron` feature, you can extend Dispenser's capabilities beyond continuous deployment to include scheduled task orchestration. You can find more examples in the `example` directory of the project.
