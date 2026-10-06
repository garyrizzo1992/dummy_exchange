# Kubernetes and Argo CD deployment

The business charts are `helm/exchange-api`, `helm/exchange-worker` and
`helm/exchange` (the market simulator). PostgreSQL and monitoring each have
their own chart. The business Argo application renders all three sources in
one sync, with a shared migration hook before their Deployments. See the
[Helm guide](helm/README.md) for configuration and existing-release transfers.

## OCI development cluster

| Service | URL |
| --- | --- |
| Exchange UI | [https://api.garyrizzo.dev/v1/ui/](https://api.garyrizzo.dev/v1/ui/) |
| Exchange API | [https://api.garyrizzo.dev/v1](https://api.garyrizzo.dev/v1) |
| Grafana | [https://grafana.garyrizzo.dev](https://grafana.garyrizzo.dev) |
| Argo CD | [https://argocd.garyrizzo.dev](https://argocd.garyrizzo.dev) |
| Prometheus | [https://prometheus.garyrizzo.dev](https://prometheus.garyrizzo.dev) |

`argocd/dummy-exchange-dev.yaml` keeps the OCI kubeadm cluster in sync with main,
using exact image digests and `values-dev.yaml`. Terraform and Ansible
set up Argo CD and External Secrets. OCI Vault stores the PostgreSQL password,
JWT secret and Grafana password. Git contains only their identifiers. Services
use private ClusterIPs. See the [infrastructure guide](../provisioning/terraform/README.md)
for setup and the [architecture review](../docs/architecture-review.md) for storage limits.

The public API uses `api.garyrizzo.dev` through a free Cloudflare Tunnel and
direct routing to the private API Service. DNS and tunnel routing belong to Terraform. See
[Cloudflare setup](../provisioning/terraform/docs/cloudflare.md) for credentials,
deployment and verification.

The following steps are for the separate local Minikube environment.

## Prerequisites

Push the three application images to Docker Hub first. The chart uses
`docker.io/garyrizzo1992` by default. For a different Docker Hub account, change
`imageRegistry` in a committed environment values file or in the Argo CD Application.

The installer creates the `dummy-exchange` namespace and separate database,
API JWT and monitoring Secrets. Its fixed values (`password`, a local JWT
secret, and `admin`) are only for local testing.

For a real environment, keep secret values out of Git. Use a sealed-secret,
External Secrets Operator or your platform's secret manager.

## Minikube with Argo CD

```powershell
.\scripts\install-argocd.ps1 -StartMinikube
kubectl apply -f deploy/argocd/postgres-minikube.yaml
kubectl -n argocd wait application/dummy-exchange-data-minikube --for=jsonpath='{.status.sync.status}'=Synced --timeout=10m
kubectl -n argocd wait application/dummy-exchange-data-minikube --for=jsonpath='{.status.health.status}'=Healthy --timeout=10m
kubectl apply -f deploy/argocd/monitoring-minikube.yaml
kubectl apply -f deploy/argocd/dummy-exchange-minikube.yaml
```

You can rerun `install-argocd.ps1`. It creates missing Argo CD and application
namespaces, creates or updates the local test secret, and applies the pinned
default Argo CD manifest. It then waits for `argocd-server` and prints the initial
`admin` password. Leave out `-StartMinikube` if `kubectl` already points to another
cluster.

Argo CD keeps the cluster in sync with the chart on `main`. It creates the
`dummy-exchange` namespace if needed, corrects manual changes in the cluster,
and prunes business resources removed from Git. Database and monitoring pruning
is disabled. The Minikube values expose these NodePorts:

| Component | Address |
|---|---|
| API | `http://$(minikube ip):30000` |
| Grafana | `http://$(minikube ip):30001` |
| Prometheus | `http://$(minikube ip):30090` |

Run `minikube service exchange-api -n dummy-exchange --url` if you cannot reach a
NodePort from your computer. Log in to Grafana with `admin` and the
`grafana-admin-password` value in `dummy-exchange-monitoring-secrets`.

To open the Argo CD web interface without installing its CLI:

```powershell
kubectl -n argocd port-forward svc/argocd-server 8080:443
```

Open `https://localhost:8080`. Get the initial password with:

```powershell
kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath="{.data.password}" | %{ [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($_)) }
```

## Another cluster

Install Argo CD on the target cluster and register that cluster with Argo CD.
Copy the Application manifest and set `spec.destination.server` to the registered
cluster's API URL.

Keep a separate values file for each environment. Include image digests,
`imageRegistry`, storage classes, resource requests, ingress and secret references
at a minimum. Do not use the Minikube NodePort values on a shared or internet-facing
cluster.

Check the rendered Kubernetes resources before deploying with Argo CD:

```powershell
helm lint deploy/helm/exchange-api
helm template dummy-exchange deploy/helm/exchange-api -f deploy/helm/exchange-api/values-minikube.yaml | kubectl apply --dry-run=server -f -
```

## Reviewed chart layout

The five charts replace the old bundled and separated compatibility charts.
Existing installations require the staged ownership and credential transfer in
[the Helm guide](helm/README.md). Ansible stops on an existing legacy application
until that transfer is complete. Deployment ordering and migrations require
Argo CD; ordinary `helm upgrade` does not provide this lifecycle.

Validate all supported profiles locally:

```powershell
python -m pip install -r scripts/requirements-helm.txt
python scripts/check-helm.py
```

Install `kubeconform` to include strict Kubernetes schema validation, as CI does.
