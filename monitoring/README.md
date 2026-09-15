# Local monitoring stack

Prometheus scrapes the API, matching workers, market simulator, PostgreSQL exporter, and Redis exporter every 15 seconds. Grafana is provisioned with the Prometheus datasource and the **Exchange Overview** dashboard.

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
