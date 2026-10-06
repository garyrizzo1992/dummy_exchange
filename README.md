# Dummy Exchange

A simulated crypto exchange built to practise backend engineering, cloud infrastructure and running services in production. Bots trade against generated prices, with balances and market history stored in PostgreSQL.

- **Backend:** Rust, order matching and Kafka workers.
- **Infrastructure:** Kubernetes on OCI, provisioned with Terraform and Ansible.
- **Delivery:** GitHub Actions, Helm and Argo CD for tested, automated deployments.
- **Operations:** Prometheus, Grafana, Loki and OpenTelemetry for metrics, logs and traces.
- **Frontend:** Rust/WebAssembly hosted on Cloudflare Pages.

Explore the [live exchange](https://exchange.garyrizzo.dev), [API](https://api.garyrizzo.dev/v1/instruments), [Grafana](https://grafana.garyrizzo.dev), [Argo CD](https://argocd.garyrizzo.dev) or [Prometheus](https://prometheus.garyrizzo.dev).

Deployment details are in [deploy](deploy/README.md), [Terraform](provisioning/terraform/README.md) and [Ansible](provisioning/ansible/README.md).
