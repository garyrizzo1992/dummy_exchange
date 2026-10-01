# Kubernetes and Argo CD deployment

`helm/dummy-exchange` packages the API, matching workers, market simulator,
PostgreSQL, optional Redis, Prometheus and Grafana. You can scale the matching
workers by adding replicas. `argocd/dummy-exchange-minikube.yaml` is an Argo CD
`Application` that installs the chart in the cluster where Argo CD runs.

## OCI development cluster

`argocd/dummy-exchange-dev.yaml` keeps the OCI kubeadm cluster in sync with main,
using exact image digests and `values-dev.yaml`. Terraform and Ansible
set up Argo CD and External Secrets. OCI Vault stores the PostgreSQL password,
JWT secret and Grafana password. Git contains only their identifiers. Services
use private ClusterIPs. See [continuous delivery](../docs/cicd.md) for setup
instructions and the limits of local-disk storage.

The following steps are for the separate local Minikube environment.

## Prerequisites

Push the three application images to Docker Hub first. The chart uses
`docker.io/garyrizzo1992` by default. For a different Docker Hub account, change
`imageRegistry` in a committed environment values file or in the Argo CD Application.

The installer creates the `dummy-exchange` namespace and its required
`dummy-exchange-secrets` secret. Its fixed values (`password`, a local JWT
secret, and `admin`) are only for local testing.

For a real environment, keep secret values out of Git. Use a sealed-secret,
External Secrets Operator or your platform's secret manager.

## Minikube with Argo CD

```powershell
.\scripts\install-argocd.ps1 -StartMinikube
kubectl apply -f deploy/argocd/dummy-exchange-minikube.yaml
```

You can rerun `install-argocd.ps1`. It creates missing Argo CD and application
namespaces, creates or updates the local test secret, and applies the pinned
default Argo CD manifest. It then waits for `argocd-server` and prints the initial
`admin` password. Leave out `-StartMinikube` if `kubectl` already points to another
cluster.

Argo CD keeps the cluster in sync with the chart on `main`. It creates the
`dummy-exchange` namespace if needed, corrects manual changes in the cluster,
and deletes resources removed from Git. The Minikube values expose these NodePorts:

| Component | Address |
|---|---|
| API | `http://$(minikube ip):30000` |
| Grafana | `http://$(minikube ip):30001` |
| Prometheus | `http://$(minikube ip):30090` |

Run `minikube service exchange-api -n dummy-exchange --url` if you cannot reach a
NodePort from your computer. Log in to Grafana with `admin` and the
`grafana-admin-password` secret value.

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
helm lint deploy/helm/dummy-exchange
helm template exchange deploy/helm/dummy-exchange -f deploy/helm/dummy-exchange/values-minikube.yaml | kubectl apply --dry-run=server -f -
```
