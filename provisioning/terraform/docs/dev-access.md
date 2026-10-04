# Development service access

The OCI nodes and every Kubernetes Service remain private. Cloudflare Tunnel
publishes the exchange API and the operator UIs listed below. NAT supplies
outbound Internet access for Tunnel, package repositories, image registries,
GitHub and OCI APIs. No public subnet, Internet Gateway, public load balancer or
public node IP is required. OCI Bastion supplies SSH access to the private nodes.

| Service | UI / API | Dev access | Authentication |
| --- | --- | --- | --- |
| Exchange API | Trading REST API at `/v1`; no UI | `https://api.garyrizzo.dev/v1` | Register/login, then API JWT |
| Grafana | Dashboards and `/api` management API | `https://grafana.garyrizzo.dev` | Grafana login; admin password comes from OCI Vault through External Secrets |
| Argo CD | GitOps UI, REST API at `/api/v1`, CLI | `https://argocd.garyrizzo.dev` | Argo CD login / API token; CLI uses `--grpc-web` through Tunnel |
| Prometheus | Query UI, `/api/v1` query and targets API | `https://prometheus.garyrizzo.dev` or localhost forwarding | Cloudflare Access email code for `1992rizzogary@gmail.com` |
| PostgreSQL | SQL protocol; no bundled UI | Localhost forwarding, port 5432 | Existing database credentials |
| Redis | Redis protocol; no bundled UI | Localhost forwarding, port 6379 | Private operator connection; no public route |
| Worker / simulator | `/metrics`, `/healthz`; no management UI | Prometheus/Grafana, or localhost forwarding on 3001 / 3002 | Private operator connection |
| PostgreSQL / Redis exporters | `/metrics`; no UI | Prometheus/Grafana, or localhost forwarding on 9187 / 9121 | Private operator connection |
| Kubernetes API | Cluster management API | `dev-kubectl.sh` through Bastion | OCI credentials, SSH key and controller kubeconfig |
| External Secrets | Kubernetes CRDs and webhook; no operator UI | `kubectl get externalsecrets,secretstores -A` through `dev-kubectl.sh` | Kubernetes access |
| Argo controllers, repo server, Dex, internal Redis | Internal Argo components | Argo CD UI/API or Kubernetes diagnostics | Argo CD / Kubernetes access |
| CoreDNS, Calico, local-path provisioner, cloudflared | Infrastructure; no operator UI | Kubernetes diagnostics; cloudflared configuration is Terraform-owned | Kubernetes / Cloudflare access |
| OCI Vault and CI runner | Cloud service / host management | OCI console/CLI and GitHub Actions; private runner SSH when needed | OCI IAM / GitHub permissions |

API `/metrics`, `/healthz` and `/readyz` remain private; the public API hostname
only accepts `/v1` paths. Prometheus does not enable its destructive admin API.
Databases, exporter endpoints, internal Argo components and Kubernetes are not
published as unauthenticated HTTP services.

## Browser and CLI access

Grafana and Argo CD have their own login screens. Argo CD's development origin
uses HTTPS with a self-signed certificate, accepted only for its tunnel route.
The browser connection still uses Cloudflare's public TLS certificate.

```bash
curl https://api.garyrizzo.dev/v1/instruments
curl https://grafana.garyrizzo.dev/api/health
curl https://argocd.garyrizzo.dev/api/version
argocd login argocd.garyrizzo.dev --grpc-web
```

The zone's existing Browser Integrity Check rejects the default `Python-urllib`
User-Agent with Cloudflare error `1010`. Use an identifiable application User-Agent
for automation, for example `User-Agent: dummy-exchange-dev-management/1.0`.
Authenticated Grafana and Argo CD management requests were verified with that
header. A hostname-scoped Cloudflare Configuration Rule can disable that browser
check for API clients if desired; the current token cannot manage rulesets.

Prometheus is published behind Cloudflare Access with an email allow policy.
The dev configuration has `cloudflare_access_enabled = true`. For a new account,
Cloudflare Access must be
enabled in the account, and the API token must have **Access: Apps and Policies â€”
Edit** and **Access: Organizations, Identity Providers, and Groups â€” Edit**. Then set
`cloudflare_access_enabled = true` in `envs/dev.tfvars` and apply. Terraform creates
the email-code identity provider and Access application before publishing DNS or
the tunnel route. Until then, `pending_access_hostnames` lists Prometheus and
neither its DNS nor its tunnel route is created.

An authenticated browser can use the Prometheus API. Automated clients need an
Access token/session; local forwarding below also provides private API access.

## Private management access

### Remote kubectl and service forwarding

Run from the repository root in WSL/Linux. These helpers reuse the same OCI
configuration and SSH key as the Ansible bootstrap. No local kubeconfig download
is needed.

```bash
bash provisioning/ansible/dev-kubectl.sh get applications -n argocd
bash provisioning/ansible/dev-kubectl.sh get pods,services -A
bash provisioning/ansible/dev-kubectl.sh get externalsecrets,secretstores -A
```

Keep a forwarding command running in one terminal and use the local endpoint
from another. Both the controller listener and local listener bind to loopback.
Ctrl-C closes the forward. An occupied port fails without stopping another session.

```bash
bash provisioning/ansible/dev-forward.sh prometheus
# Browser: http://127.0.0.1:9090
# Query API: http://127.0.0.1:9090/api/v1/query?query=up

bash provisioning/ansible/dev-forward.sh postgres
# psql -h 127.0.0.1 -p 5432 -U postgres -d dummy_exchange

bash provisioning/ansible/dev-forward.sh redis
# redis-cli -h 127.0.0.1 -p 6379 ping

bash provisioning/ansible/dev-forward.sh api 13000
# http://127.0.0.1:13000/readyz

bash provisioning/ansible/dev-forward.sh grafana 13001
# http://127.0.0.1:13001

bash provisioning/ansible/dev-forward.sh argocd
# https://127.0.0.1:8443 (development origin uses a self-signed certificate)
```

The forward helper also accepts `worker`, `simulator`, `postgres-exporter` and
`redis-exporter`. The headless worker Service forwards to one worker pod; use
Prometheus for metrics covering both replicas.

Grafana uses username `admin` and the `grafana-admin-password` key in
`dummy-exchange-monitoring-secrets` in namespace
`dummy-exchange`. Argo CD's initial admin password is in
`argocd-initial-admin-secret` in namespace `argocd` until it is rotated/deleted.
Retrieve credentials privately with Kubernetes/OCI permissions; do not commit them.
