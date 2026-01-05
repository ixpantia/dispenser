# Dispenser's mission.

> This is a rough explanation of what the purpose of Dispenser is and why some technical decisions are made.

Dispenser is meant to be a simple, declarative and deterministic approach
to deploying containers inside a virtual machine.

## Built for CD from Day One

Dispenser actually started its life as a continuous deployment (CD) solution first. 

One of the biggest gaps in the current ecosystem is that neither Docker Compose nor Kubernetes actually have CD built-in. With Compose, you're stuck writing bash scripts to pull images and restart services. With Kubernetes, you have to set up and manage entirely separate tools like ArgoCD or Flux just to keep your cluster in sync with your registry.

Dispenser was born from the idea that **deployment should be a core feature of the orchestrator, not an afterthought.**

## The Problem with "Standard" Tools

Many existing solutions such as Kubernetes or Docker Compose work very well
and are mature tools. However, they have a problem: they are hard to manage correctly.

With Kubernetes, it's hard to have a versioned, declarative state without a massive amount of boilerplate and external tooling.

With Docker Compose, it's not very obvious where the files are. Things could be in 
`/home/<some user>/service/docker-compose.yaml` or anywhere else. This leads to "server drift" where nobody knows exactly what is running or why.

Dispenser looks to:

1. Minimize the effort to deploy a service.
2. Be declarative and "code first". A whole deployment should be able to be version controlled.
3. Simplify continuous deployment in environments where it's typically challenging.

## Everything in its place: `/opt/dispenser`

One of the biggest headaches with manual deployments is hunting for config files. Dispenser enforces a standard: everything lives in `/opt/dispenser`. 

By centralizing the configuration, we eliminate the "where is that compose file?" game. If it's running on the machine, the source of truth is in that directory. This makes backups, migrations, and debugging significantly easier.

## Minimize the effort to deploy a service.

By including things like a reverse proxy and scheduling, Dispenser seeks to
make deploying a new service as easy as having the containers
and their environment variables ready.

## Declarative & Deterministic

A Dispenser instance runs and owns what is declared in its configuration. If it's
not declared, it will not run. If you delete a service from your config, Dispenser deletes the container. This "reconciler" mindset ensures that the state of your VM matches your Git repo.

## Simplify CD: Why Polling?

Dispenser intentionally uses a **pull-based polling approach** for CD instead of the traditional push-based webhooks.

**The Trade-off:**
*   **The Bad:** It's not "instant." There’s a delay between pushing an image and the service updating (defaulting to 60 seconds). It also consumes a negligible amount of background I/O by checking the registry periodically.
*   **The Good:** It’s incredibly resilient and firewall-friendly.

Most CD tools require you to open a port so that GitHub or GitLab can "poke" your server when a build is done. In many on-premises or highly secure enterprise environments, whitelisting outbound traffic to a registry is standard, but allowing inbound traffic from the public internet is a non-starter.

By polling, Dispenser eliminates the need for:

1.  **Public Endpoints:** Your server doesn't need a public IP or a domain name for CD to work.
2.  **Webhook Secrets:** No need to manage and rotate tokens for callbacks.
3.  **Complex Tunnels:** You don't need things like Ngrok or Cloudflare Tunnels just to get a deployment signal.

It’s "pull-based" GitOps that works behind air-gapped firewalls, NATs, and VPNs without a single change to your networking infrastructure.

## Secrets

Managing secrets in a "code first" world is always a bit of a dance. You want your config in Git, but you definitely don't want your database password there.

Dispenser handles this in two ways:

1. **The Local Way:** You can use variable files like `prod.dispenser.vars`. By adding `*.dispenser.vars` to your `.gitignore`, you can keep your secrets on the machine and your logic in the repo. Dispenser will merge them at runtime.
2. **The Cloud Way:** If you're running on GCP, Dispenser can talk directly to Google Secret Manager. You just reference the secret name in your vars, and Dispenser fetches it. No more manual env file syncing.

## Batteries-Included Networking (The Proxy)

Usually, setting up a container means you also have to set up Nginx or Caddy, handle Certbot for SSL, and hope the config doesn't break when you restart.

Dispenser has a built-in reverse proxy powered by Pingora (the stuff Cloudflare uses). It handles:
*   **Automatic SSL:** Just give it an email, and it talks to Let's Encrypt for you.
*   **Service Discovery:** You don't need to know IP addresses. If your service is named `api`, the proxy knows how to find it.
*   **Zero-Downtime (Roadmap):** While currently Dispenser restarts containers when updated, true zero-downtime handoffs (spinning up the new one and health-checking it before killing the old one) is a primary goal on our roadmap.

## Scheduling: Cron, but for containers.

Sometimes you don't need a service running 24/7. Maybe you just need to run a backup at 2 AM or an ETL job every hour.

Instead of messing with the system's `crontab` and writing messy shell scripts to run `docker run`, you just add a `cron` field to your service config. Dispenser treats scheduled jobs as first-class citizens—it'll pull the image, run the container, and clean up afterwards.
