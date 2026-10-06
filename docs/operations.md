# Operations notes

The exchange is a workload for practising DevOps and SRE. These are real fixes and checks from the development cluster, plus the work still ahead.

## Incidents and capacity

**DNF memory pressure.** Background package metadata refresh used about 2.8 GB on an 8 GB node. It contributed to memory exhaustion, and limiting its memory still left heavy disk activity. Automatic refresh is now disabled, the service is capped at 512 MiB, and Ansible skips package installation when everything needed is already installed. Kubelet also reserves memory for the host. These fixes live in the [Ansible playbook](../provisioning/ansible/playbooks/cluster.yml), so a rebuild keeps them.

**Too much load on one worker.** PostgreSQL, tracing and trading pods competed for the same 8 GB worker. We capped trading at 8 bots and 2 workers and gave Tempo a 512 MiB Go memory target within its 768 MiB container limit. Increasing one container's limit had made node pressure worse. The control plane was upgraded to 16 GB; the worker still needs careful load budgeting.

**Lighter traders.** A kubelet snapshot on 6 October 2026 showed about 4.5 MB per trader and another 14?16 MB for its tracing sidecar. Traders now send traces directly to Tempo, with bounded runtime threads and Kafka queues. Removing 99 sidecars should save about 1.4 GB, based on that snapshot. This is an estimate; we haven't measured sustained load with all 99 bots. Tests passed and Tempo was receiving spans during rollout. See the [change](https://github.com/garyrizzo1992/dummy_exchange/commit/c873fbf).

The main lesson: measure the whole node, including sidecars and host processes, before adding replicas.

## Recovery checks

- **Restart persistence:** [Generator tests](../scripts/verify-market-generator.py) check saved prices, seed and step across processes. A live pod restart also continued from saved state.
- **Command replay:** [Kafka tests](../scripts/verify-kafka.py) cover duplicate commands, replay, invalid messages and cancellation refunds.
- **Safe bot resets:** Tests check that a reset preserves human balances and refuses to remove a human counterparty trade.
- **Release checks:** [CI](../.github/workflows/application.yml) runs integration tests before promoting image digests. [Helm checks](../scripts/check-helm.py) validate migration ordering, storage and scaling ownership.
- **Drift checks:** A [scheduled Terraform plan](../.github/workflows/drift.yml) flags infrastructure changes. It doesn't apply repairs automatically.

These show recovery while the database survives. They don't prove recovery from losing a node or its storage. Backup restoration and a timed rollback exercise remain to be tested.

## Reliability targets

These are starting targets, not results we've achieved:

| Measure | Target | Next step |
| --- | --- | --- |
| Public API availability | 99.5% over 30 days | Add external checks of `/v1/instruments`. |
| API response time | p95 below 500 ms during a 30-minute load test | Record latency alongside request rate and bot count. |
| Balance correctness | No negative available or reserved balances | Keep integration checks and add a periodic live check. |

Prometheus already collects request status and duration. Its internal scrape checks don't cover the full Cloudflare-to-API path. Latency is grouped by HTTP method, so test traffic needs to be isolated when measuring it.

Alerts cover availability, HTTP errors, node readiness, restarts, memory kills and disk usage. Notification delivery still needs configuration and testing. See [monitoring](../monitoring/README.md).

## Runbooks

Use the [private cluster access guide](../provisioning/terraform/docs/dev-access.md) to run Kubernetes checks.

**API unreachable:** Check `/v1/instruments`, then Argo health, API pods, service endpoints and database availability. If the API works privately, check Tunnel connector readiness and hostname/path routing. `/healthz` is intentionally outside the public route.

**Memory pressure or restarts:** Check node conditions, pod events and termination reasons in Kubernetes, then compare them with Grafana memory graphs. Include sidecars and OS processes. Reduce load if needed, commit the fix in Helm or Ansible, and check headroom before scaling back up.

**Failed deployment:** Check the Argo sync result, migration job and running image digest. For an application regression, revert the relevant digest promotion in Git after checking schema compatibility. An image rollback doesn't undo a database migration. Confirm the rollout is healthy, public requests work and Kafka lag recovers.

## Still to do

- Put PostgreSQL backups outside the nodes and test restoration.
- Rehearse a rollback and record recovery time.
- Load-test increasing bot counts and publish throughput, latency and memory results.
- Test alert delivery and link alerts to these runbooks.
- Review database permissions, network policies and Vault ownership in the [architecture review](architecture-review.md).

This is a two-node development cluster with local storage. Three Kafka brokers across two hosts don't provide three independent failure domains.
