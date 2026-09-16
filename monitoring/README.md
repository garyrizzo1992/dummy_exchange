# Local monitoring stack

Prometheus scrapes the API, matching workers, market simulator, PostgreSQL exporter, and Redis exporter every 15 seconds. Grafana is provisioned with the Prometheus datasource and the **Exchange Overview** dashboard.

The dashboard covers buy/sell order flow, cancellations, user fills, execution notional, market prices, API latency and status codes, worker/simulator errors, and PostgreSQL/Redis availability. Metrics have bounded labels (`instrument`, `side`, and `order_type`); never add user or order IDs as labels.

Each monitoring component has its own Dockerfile with a pinned upstream image digest. Compose builds these local Dockerfiles, so monitoring configuration and dashboard provisioning are versioned with the application.

Start the stack with:

```powershell
docker compose up --build
```

| Service | Local address | Purpose |
|---|---|---|
| Prometheus | http://localhost:9090 | Targets, queries, alert-rule state. |
| Grafana | http://localhost:3001 | Dashboard UI; local credentials are `admin` / `admin`. |
| API metrics | http://localhost:3000/metrics | Application metrics endpoint. |

Worker and simulator metrics are intentionally available only inside the Compose network on ports 3001 and 3002. PostgreSQL and Redis exporters are similarly internal-only.

The alert rules are visible in Prometheus and Grafana. This local stack does not configure external alert delivery; connect Alertmanager or Grafana contact points using deployment-managed credentials when you are ready to notify people.
