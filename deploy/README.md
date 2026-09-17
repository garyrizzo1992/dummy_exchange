# Kubernetes and Argo CD deployment

`helm/dummy-exchange` is a self-contained chart for the public API, horizontally scalable matching workers, market simulator, PostgreSQL, optional Redis, Prometheus, and Grafana. `argocd/dummy-exchange-minikube.yaml` is an Argo CD `Application` that deploys it to the same cluster as Argo CD.

## Prerequisites

Push the three application images to Docker Hub first. The chart defaults to `docker.io/garyrizzo1992`; if your Docker Hub username differs, change `imageRegistry` in a committed environment values file or set it in the Argo CD Application.

The chart deliberately does not create credentials. Create one Kubernetes secret before Argo CD syncs the Application:

```powershell
kubectl create namespace dummy-exchange
kubectl -n dummy-exchange create secret generic dummy-exchange-secrets `
  --from-literal=postgres-password='<choose-a-strong-password>' `
  --from-literal=jwt-secret='<at-least-32-random-characters>' `
  --from-literal=grafana-admin-password='<choose-a-strong-password>'
```

For a real environment, manage this secret with a sealed-secret, External Secrets Operator, or your platform secret manager—not with Git.

## Minikube with Argo CD

```powershell
.\scripts\install-argocd.ps1 -StartMinikube
kubectl apply -f deploy/argocd/dummy-exchange-minikube.yaml
```

`install-argocd.ps1` is idempotent: it creates the namespace when absent, applies the pinned default Argo CD manifest, waits for `argocd-server`, and prints the initial `admin` password. Omit `-StartMinikube` when `kubectl` is already configured for another cluster.

Argo CD continuously reconciles the chart from `main`, creates the `dummy-exchange` namespace if necessary, self-heals drift, and prunes resources removed from Git. The chart’s Minikube values expose NodePorts:

| Component | Address |
|---|---|
| API | `http://$(minikube ip):30000` |
| Grafana | `http://$(minikube ip):30001` |
| Prometheus | `http://$(minikube ip):30090` |

Use `minikube service exchange-api -n dummy-exchange --url` when a local NodePort address is not reachable from your host. Grafana uses `admin` and the `grafana-admin-password` secret value.

To reach the Argo CD UI without installing its CLI:

```powershell
kubectl -n argocd port-forward svc/argocd-server 8080:443
```

Open `https://localhost:8080`. Retrieve the initial password with:

```powershell
kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath="{.data.password}" | %{ [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($_)) }
```

## Another cluster

Install Argo CD on the target cluster, register the cluster with that Argo CD instance, then copy the Application manifest and replace `spec.destination.server` with the registered cluster API URL. Use a dedicated values file for each environment—at minimum image digests, `imageRegistry`, storage classes, resource requests, ingress, and secret references. Do not use the Minikube NodePort values for a shared or internet-facing cluster.

Validate rendered resources before handing the change to Argo CD:

```powershell
helm lint deploy/helm/dummy-exchange
helm template exchange deploy/helm/dummy-exchange -f deploy/helm/dummy-exchange/values-minikube.yaml | kubectl apply --dry-run=server -f -
```
