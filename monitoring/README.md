# Local monitoring

Prometheus collects metrics every 15 seconds from the API, matching workers,
market simulator, PostgreSQL exporter and Redis exporter. Grafana comes with
Prometheus set up as its data source and an Exchange Overview dashboard.

The dashboard shows buy and sell orders, cancellations, user trades and their
value, market prices, API response times and status codes. It also tracks worker
and simulator errors and whether PostgreSQL and Redis are available.

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
3001 and 3002. The PostgreSQL and Redis exporters are also internal.

You can view alert rules in Prometheus and Grafana, but this local setup does
not send notifications. To send them, connect Alertmanager or Grafana contact
points and supply their credentials through your deployment's secret management.
