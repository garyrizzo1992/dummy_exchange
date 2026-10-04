# Local monitoring

Prometheus collects metrics every 15 seconds from the API, matching workers,
market simulator and PostgreSQL exporter. Grafana comes with
Prometheus set up as its data source and an Exchange Overview dashboard.

The dashboard shows buy and sell orders, cancellations, user trades and their
value, market prices, API response times and status codes. It also tracks worker
and simulator errors and PostgreSQL availability.

Metric labels (`instrument`, `side`, and `order_type`) have a limited set of
values. Do not add user or order IDs as labels.

Each monitoring component has a Dockerfile that pins its upstream image by
digest. Compose builds these Dockerfiles, keeping the monitoring settings and
dashboard setup in Git alongside the application.

Start the stack with:

```powershell
docker compose up --build
```

| Service | Local address | Purpose |
|---|---|---|
| Prometheus | http://localhost:9090 | Check targets, run queries and view alert rules. |
| Grafana | http://localhost:3001 | View dashboards; the local login is `admin` / `admin`. |
| API metrics | http://localhost:3000/metrics | Read application metrics. |

Worker and simulator metrics are available inside the Compose network on ports
3001 and 3002. The PostgreSQL exporter is also internal.

Development also provides a **Kafka and autoscaling** dashboard for consumer
lag, worker replicas, random trader targets, live trader replicas and command
throughput. See [the scaling configuration](../docs/keda-scaling.md).

You can view alert rules in Prometheus and Grafana, but this local setup does
not send notifications. To send them, connect Alertmanager or Grafana contact
points and supply their credentials through your deployment's secret management.

## Kubernetes development troubleshooting

The `exchange-monitoring` Helm chart provisions three dashboards in Grafana's
**Exchange** folder:

- **Exchange Overview**: application throughput, latency, errors and database health.
- **Kubernetes Overview**: node CPU, memory, disk and network; pod resource usage,
  CPU throttling, restarts and termination reasons; deployment/StatefulSet replicas,
  storage, scrape health and active Prometheus alerts. Namespace and node filters
  select the scope. Empty series can mean a collector is unsupported (for example,
  some local-path volumes do not expose kubelet volume statistics).
- **Kubernetes Logs and Events**: searchable container logs and Kubernetes events
  across namespaces, with namespace, pod and container filters. Grafana Explore
  also offers the **Loki** data source.

Prometheus scrapes kube-state-metrics, a node-exporter on each Linux node, and
kubelet/cAdvisor via the private API proxy with service-account authentication
and certificate verification. Alloy tails pod logs and watches events through
read-only Kubernetes API permissions; it forwards them to a private, single-node
Loki instance. No public Kubernetes API or local kubeconfig is needed.

Logs are retained for 72 hours on a 5 GiB local-path volume in development.
Collection starts when Alloy is deployed; previously deleted containers and expired
events cannot be recovered. Local-path does not enforce a disk quota, so monitor
node filesystem usage. Loki retention is time-based, not a hard 5 GiB size cap.
Grafana's SQLite database has its own 2 GiB volume, WAL enabled, and one writer.
These volumes are local to their node and are not highly available.

Grafana previously exceeded its 256 MiB limit and repeatedly restarted with
`OOMKilled`, while requests timed out and SQLite reported lock contention. Its
limits are now 1 GiB memory and two CPU cores, with requests of 512 MiB and 250m.
A startup probe allows initialization before readiness checks. Provisioned dashboards
remain in Git; notification destinations still require explicit configuration.
