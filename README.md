# Dummy Exchange

A DevOps/SRE portfolio project using a simulated crypto exchange as the workload. The focus is repeatable setup, automated deployments and fixing real operational problems.

- **Infrastructure:** Terraform provisions OCI and Cloudflare; Ansible bootstraps Kubernetes. Scheduled drift checks flag changes.
- **Delivery:** GitHub Actions tests Rust services and builds images pinned by digest; Helm and Argo CD deploy them with migrations first.
- **Cloudflare:** Pages hosts the frontend; Tunnel reaches private services without public node IPs; Access protects Grafana, Argo CD and Prometheus with email login.
- **Security:** SHA-pinned Actions, secret/dependency scans and image scan gates protect delivery; network policies and pod admission restrict cluster access.
- **Operations:** Prometheus and Grafana track health and capacity; Loki and OpenTelemetry help investigate failures.
- **Workload:** PostgreSQL stores balances and market state; Kafka feeds trading workers, with KEDA scaling bots and consumers.

## What this shows

- [Incidents and capacity](docs/operations.md#incidents-and-capacity): DNF memory pressure, workload limits and lighter traders.
- [Recovery checks and runbooks](docs/security-rollout.md): private off-node PostgreSQL backups, a verified restore in 16 seconds, and rollout/rollback procedures.
- [Reliability targets and remaining gaps](docs/operations.md#reliability-targets): proposed SLOs, backup restoration and alert delivery.
- [Security review](docs/security-review.md): bounded attack checks and findings; [follow-ups](docs/security-follow-up.md) track rollout verification and remaining work.
- [Traffic diagrams](deploy/traffic.md): public routing, private trading, observability and deployment flows.

The demo runs on a two-node development cluster with node-local storage. The security configuration rollout is verified: all seven Argo CD applications are synced and healthy.

Explore the [exchange](https://exchange.garyrizzo.dev), [API](https://api.garyrizzo.dev/v1/instruments), [Grafana](https://grafana.garyrizzo.dev), [Argo CD](https://argocd.garyrizzo.dev) or [Prometheus](https://prometheus.garyrizzo.dev).

Setup: [deployment](deploy/README.md), [Terraform](provisioning/terraform/README.md), [Ansible](provisioning/ansible/README.md).

AI tools helped with code, infrastructure, troubleshooting and documentation. The project records the checks performed and the work still to do.
