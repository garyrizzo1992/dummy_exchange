# Dummy Exchange

A simulated crypto exchange built to practise backend engineering, cloud infrastructure and running services in production. Bots trade against generated prices, with balances and market history stored in PostgreSQL.

- **Rust / WebAssembly:** Fast services and a browser frontend in one language.
- **PostgreSQL:** Keeps balances, trades and generated prices across restarts.
- **Kafka:** Queues trade commands for workers to process.
- **OCI / Kubernetes:** Hosts services and scales the trading bots.
- **Terraform / Ansible:** Makes infrastructure and server setup repeatable.
- **GitHub Actions / Helm / Argo CD:** Tests, packages and deploys changes automatically.
- **Cloudflare:** Static hosting, private service routing and access control.
- **Prometheus / Grafana / Loki / OpenTelemetry:** Helps track performance and investigate problems.

Explore the [live exchange](https://exchange.garyrizzo.dev), [API](https://api.garyrizzo.dev/v1/instruments), [Grafana](https://grafana.garyrizzo.dev), [Argo CD](https://argocd.garyrizzo.dev) or [Prometheus](https://prometheus.garyrizzo.dev).

## Cloudflare setup

- **Pages:** The frontend ships as a separate static artifact, uploaded by GitHub Actions to `exchange.garyrizzo.dev`.
- **Tunnel:** Two connectors route the API and dashboards into private Kubernetes services, without a public load balancer or public node IPs. API routing only exposes `/v1/*`.
- **Access:** Prometheus requires an email code and an approved email address before requests reach the cluster.
- **Terraform:** Manages the Pages project, custom domain, DNS, tunnel routes, Access policies and HTTPS redirects.

See the [traffic diagrams](deploy/traffic.md) for request routing, trading, monitoring and deployments.

Deployment details are in [deploy](deploy/README.md), [Terraform](provisioning/terraform/README.md) and [Ansible](provisioning/ansible/README.md).
