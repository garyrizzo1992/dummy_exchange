# Dummy Exchange

A simulated crypto exchange built to practise backend engineering, cloud infrastructure and running services in production. Bots trade against generated prices, with balances and market history stored in PostgreSQL.

- **Rust / WebAssembly:** Fast services and a browser frontend in one language.
- **PostgreSQL:** Keeps balances, trades and generated prices across restarts.
- **Kafka:** Queues trade commands for workers to process.
- **OCI / Kubernetes:** Hosts services and scales the trading bots.
- **Terraform / Ansible:** Makes infrastructure and server setup repeatable.
- **GitHub Actions / Helm / Argo CD:** Tests, packages and deploys changes automatically.
- **Cloudflare Pages / Tunnel:** Hosts the frontend and exposes services without public node IPs.
- **Prometheus / Grafana / Loki / OpenTelemetry:** Helps track performance and investigate problems.

Explore the [live exchange](https://exchange.garyrizzo.dev), [API](https://api.garyrizzo.dev/v1/instruments), [Grafana](https://grafana.garyrizzo.dev), [Argo CD](https://argocd.garyrizzo.dev) or [Prometheus](https://prometheus.garyrizzo.dev).

See the [traffic diagrams](deploy/traffic.md) for request routing, trading, monitoring and deployments.

Deployment details are in [deploy](deploy/README.md), [Terraform](provisioning/terraform/README.md) and [Ansible](provisioning/ansible/README.md).
