# Helm deployment

The three business processes share PostgreSQL and ship together. Each has its
own chart; one Argo CD application coordinates their migration and rollout.
PostgreSQL and monitoring have separate applications and lifecycles.

| Chart | Owns |
|---|---|
| `exchange-api` | API Deployment, Service, optional Ingress, migration Job and JWT ExternalSecret |
| `exchange-worker` | Matching worker Deployment and headless metrics Service |
| `exchange` | Exchange market generator Deployment and metrics Service; the binary remains `exchange-simulator` |
| `exchange-postgres` | PostgreSQL StatefulSet, Service, database ExternalSecret and optional legacy Redis |
| `exchange-monitoring` | Prometheus, Grafana, exporters, alerts, dashboards and Grafana ExternalSecret |

Cloudflared routes public requests directly to private Kubernetes Services.
`exchange-tunnel` is an optional platform chart for Cloudflare connectivity.
Neither belongs to the business release.

Charts render their own resources directly. There are no umbrella charts,
compatibility adapters, local chart archives or dependency packaging scripts.
All objects for a service belong in its chart's `templates/` directory.

## Configuration

Each chart has `values.yaml`, `values-dev.yaml` and `values-minikube.yaml`.
Business settings are local: `image`, `replicas`, `resources`, and the API's
`service` and `migrations`. The exchange generator remains one replica
with `Recreate`; the API and workers use rolling updates. PostgreSQL advisory
locks serialize matching per instrument across worker replicas.

Connection settings use `database.host`, `port`, `name`, `user`, `sslMode`, and
`secret.name`/`secret.key`. An empty host resolves to `<resource-prefix>-postgres`.
For an external database, set the explicit host in all three business charts
and monitoring, then disable the PostgreSQL workload with `enabled: false`.
Redis has its own switch, `redis.enabled`, in the PostgreSQL chart.

Keep the release name, namespace and resource prefix consistent across all five
charts. The Argo manifests use `dummy-exchange-dev` in development and
`dummy-exchange` in Minikube, with `nameOverride: dummy-exchange`. Minikube sets
`fullnameOverride: exchange` in every chart. These choices preserve the previous
names, immutable selectors, StatefulSet service names and PVC identities.
Argo must use annotation tracking so its application names do not overwrite the
shared Helm instance labels. Ansible and the local bootstrap configure this.

Redis is unused by the Rust services. It is retained by default to preserve
existing installations, without being injected into application environments.
For a new cluster without Redis, set `exchange-postgres`'s `redis.enabled: false`
and `exchange-monitoring`'s `redisExporter.enabled: false`. Disabling an exporter
also removes its scrape target and availability alert. Existing Redis resources
have pruning disabled; deleting them and their data is an explicit operation.

Prometheus runs with `Recreate` and a single PVC. Retention defaults to 15 days
and 4 GB of blocks on a 5 GiB volume; WAL and other files need additional room.
Configuration checksums restart Prometheus and Grafana after provisioning
changes. Grafana UI changes are ephemeral; Git provisioning is authoritative.
All containers have CPU and memory requests and limits. Development storage
uses the worker's local disk; it is not replicated or a database backup.

## Credentials

Each credential has one owner and one Vault identifier:

| Owner | Kubernetes Secret | Key |
|---|---|---|
| PostgreSQL | `dummy-exchange-secrets` | `postgres-password` |
| API | `dummy-exchange-api-secrets` | `jwt-secret` |
| Monitoring | `dummy-exchange-monitoring-secrets` | `grafana-admin-password` |

Workers, the exchange generator and PostgreSQL exporter reference the database
credential. The API additionally references `jwtSecret`; Grafana references
`grafana.adminSecret`. Each owner configures `externalSecrets.enabled`, `vaultId`,
`region` and `secretName`. Only identifiers belong in Git. Separate namespaced
SecretStores use the worker's OCI instance identity.

For Minikube, `scripts/install-argocd.ps1` creates these three Secrets with local
test credentials. It does not enable External Secrets or deploy applications.

## Release and ordering

`deploy/argocd/dummy-exchange-dev.yaml` renders API, worker and exchange as three
Helm sources in **one** Argo application. The API chart's migration Job uses the
API image, including its promoted digest. It runs as a `Sync` hook in wave `-1`,
after the API JWT SecretStore/ExternalSecret waves `-4`/`-3`, before every business
workload in wave `0`. It runs again on a full business sync.

The workflow promotes the three chart-local development image digests in one
Git commit. Promotion-only commits are excluded from rebuild triggers.
PostgreSQL and monitoring are not part of this business sync.

The database application must already be `Synced` and `Healthy` before the
business application starts. Ansible waits for both states before applying the
business manifest, and also waits for monitoring ownership to settle before
enabling business pruning. Sync-wave annotations do not order independent applications.
Ordinary Helm installs/upgrades do not execute this Argo migration lifecycle.
Avoid selective sync for schema releases because it skips hooks, and keep
migrations compatible with the previous app version during rolling updates.

## New deployment

From the repository root, after configuring Vault identifiers:

```powershell
kubectl apply -f deploy/argocd/postgres-dev.yaml
kubectl -n argocd wait application/dummy-exchange-data-dev --for=jsonpath='{.status.sync.status}'=Synced --timeout=10m
kubectl -n argocd wait application/dummy-exchange-data-dev --for=jsonpath='{.status.health.status}'=Healthy --timeout=10m
kubectl apply -f deploy/argocd/monitoring-dev.yaml
kubectl apply -f deploy/argocd/dummy-exchange-dev.yaml
```

Ansible performs this bootstrap for new OCI clusters. It stops when it detects
an existing legacy business application, requiring the transfer below instead
of changing resource ownership during reprovisioning. The old `bundled` and
`separated` layout selector is removed.

For Minikube, run the bootstrap script, apply `postgres-minikube.yaml`, wait for
`dummy-exchange-data-minikube` to be `Synced` and `Healthy`, then apply
`monitoring-minikube.yaml` and `dummy-exchange-minikube.yaml`.

## Transfer an existing release

Do this before letting provisioning apply the new manifests. The old chart paths
are removed, so pause legacy automatic sync before publishing this change.
Do not uninstall the old release, delete PVCs or cascade-delete applications.

1. Back up PostgreSQL and record resource names, selectors and PVC/PV identities.
   Configure Argo annotation tracking using `deploy/argocd/patches/tracking.yaml`.
   Pause automation, self-heal and pruning on the existing business and any
   existing infrastructure applications. Preserve the existing image digests,
   database settings, JWT value and Grafana password in the new chart profiles.
2. Prepare a temporary copy of `postgres-dev.yaml`, with automation disabled.
   Add the following `spec.source.helm.valuesObject` bridge. It keeps the old
   shared Secret usable while older API/Grafana pods still reference its keys:

   ```yaml
   externalSecrets:
     legacyKeys:
       jwt-secret: dummy-exchange-dev-jwt-secret
       grafana-admin-password: dummy-exchange-dev-grafana-admin-password
   ```

   Use the actual old Vault identifiers if they differ. Apply the staged
   PostgreSQL application, perform a full sync, and wait for `Synced`/`Healthy`.
   Verify PostgreSQL, Redis and the existing database SecretStore/ExternalSecret
   now carry the data application's tracking annotations. No StatefulSet or PVC
   should be deleted or recreated.
3. Stage and sync the monitoring application, still with automation disabled.
   Wait for its new Grafana Secret and pods to be healthy, and verify its
   tracking annotations identify the monitoring application.
4. Replace the business application's old `spec.source` with the three
   `spec.sources` from the new business manifest. Keep automation and pruning
   disabled for the first full sync. Verify the API JWT Secret is ready, the
   migration succeeds and all three business workloads are healthy. Confirm
   API and Grafana now reference their separate Secrets and that no resource
   is claimed by two applications.
5. Apply the canonical PostgreSQL manifest, removing the temporary `valuesObject`
   bridge. It now synchronizes only the database password. Apply the canonical
   monitoring and business manifests to restore automation. Database/monitoring
   pruning stays disabled; verify PVC identities before allowing business pruning.

Already separated releases keep their existing infrastructure application names;
change their chart paths with the same staged procedure. For Minikube, keep the
old shared test Secret until API and Grafana use the new bootstrap-created
Secrets; no Vault bridge is needed when External Secrets is disabled.

## Public access

Development publishes `https://api.garyrizzo.dev/v1` using Cloudflare's free
Tunnel and DNS, managed by Terraform. Cloudflared runs in `ingress` and reaches
the API ClusterIP Service directly on port 3000. No ingress controller, public
OCI load balancer, or paid Cloudflare feature is required.

The tunnel publishes only `/v1`; other API paths return 404. Ansible fetches the
connector token transiently and installs Secret `cloudflare-tunnel`, key `token`.
Argo manages two connector replicas. Tokens do not enter Terraform state.

Additional hostname-to-Service mappings are configured with
`cloudflare_service_routes`. See [Cloudflare deployment](../../provisioning/terraform/docs/cloudflare.md)
for permissions, optional routes, and deployment commands.

## Checks

From the repository root:

```powershell
python -m pip install -r scripts/requirements-helm.txt
python scripts/check-helm.py
helm template dummy-exchange-dev deploy/helm/exchange-worker -f deploy/helm/exchange-worker/values-dev.yaml
```

The checker lints and renders default, development and Minikube profiles; checks
ownership boundaries, selectors, storage, migration ordering, secret ownership,
exporter switches, external database configuration and direct tunnel deployment; and rejects
invalid values. With `kubeconform` installed it checks strict Kubernetes schemas.
External Secrets CRDs still require validation against the installed operator.
These commands do not contact or modify the cluster.
