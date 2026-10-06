# Architecture review

Reviewed on 2026-10-05. This is a working review to support the portfolio write-up. The [operations notes](operations.md) record later incidents, recovery checks and remaining gaps.

## Design worth explaining

Terraform owns OCI networking, compute, IAM, Vault and Cloudflare routing. Ansible bootstraps Kubernetes and its operators; Argo CD owns application releases. Explain that boundary and the sequence from an empty environment to a running exchange.

Application CI validates changes, builds coordinated images for both CPU architectures, publishes immutable digests and records those digests in Git. Argo CD applies the desired release. Infrastructure CI uses a private runner, and the drift workflow checks the Terraform state separately. A successful promotion commit does not prove the Kubernetes rollout has completed.

Separate Argo applications protect PostgreSQL, monitoring and Kafka from ordinary application pruning. External Secrets supplies runtime credentials. KEDA owns selected replica counts, and Argo ignores those fields. PostgreSQL transactions and command identifiers provide the consistency boundary behind Kafka delivery; explain retries and idempotency rather than claiming exactly-once delivery.

## Changes made during this review

- Restricted application Docker contexts to Rust sources, manifests and migrations, excluding local credentials and Terraform state.
- Bound local Compose ports to localhost and aligned the example database URL with Compose.
- Added shared SIGTERM/Ctrl-C handling and graceful API shutdown; corrected request-ID middleware ordering.
- Added a release promotion guard against changed build inputs on main, bounded CI jobs and PostgreSQL startup, and made integration cleanup run after failures.
- Made local Rust build/test commands use the lockfile and migrations use the application's embedded migration command.
- Updated the chart validator's default Kubernetes schema version to match the configured cluster version, with an explicit override.

## Highest priority improvements

| Area | Finding | Recommended next step |
| --- | --- | --- |
| Vault lifecycle | Development charts reference a different Vault from the one currently managed by Terraform. Recovery of the previous Vault was required during the earlier deployment. | Choose one authoritative Vault lifecycle. Preserve or migrate the existing secrets deliberately, then align chart references with Terraform outputs and document recovery. |
| Infrastructure runner | The runner belongs to the stack it applies. Replacing that stack can disrupt the running job; IAM changes also need permissions beyond the runner's current read access to tenancy resources. | Separate privileged bootstrap/IAM and runner provisioning from routine application infrastructure. Test replacement and recovery from an independent execution environment. |
| Data durability | The small kubeadm cluster and local storage couple persistent data to individual nodes. Three Kafka brokers across two hosts do not provide three independent failure domains. | Document the availability limit. Add off-node PostgreSQL backups and a restore rehearsal before describing the deployment as resilient. Expand failure domains only if the budget and availability goal justify it. |
| Access boundaries | Most application traffic lacks NetworkPolicies; workloads share broad database credentials and node-level cloud identity. | Add tested application network policies, separate migration and runtime database roles, and narrow cloud permissions by workload where supported. |
| Rollout evidence | CI records desired image digests but does not verify the resulting rollout. | Add read-only rollout verification tied to the promoted revision and digests, including migration success and a smoke test. |

## Further engineering improvements

- Scrape API pod endpoints individually. Scraping a Service that balances between replicas can mix counters from different processes and distort request-rate/error-rate graphs.
- Make worker and simulator readiness reflect processing progress and dependencies. A healthy metrics listener alone does not establish that matching or price updates are working.
- Define retention for cancelled simulator orders, audit data and unused outbox records. The current workload continuously creates database history; measure growth and add a scheduled retention policy rather than editing already applied migrations.
- Review maker/taker attribution in the database matcher. Matching is driven from buy orders, while execution price uses order sequence; settlement may be correct while the maker/taker labels misrepresent which order arrived first.
- Explain that adding workers scales command processing but per-market database locks still serialize matching. Measure throughput and contention before claiming linear scaling.
- Remove the temporary Argo database-environment compatibility overrides after compatible images have been promoted. Prefer the separate credential configuration already supported by the application.
- Pin GitHub Actions to commit SHAs with automated updates, verify downloaded tooling, and add an image vulnerability gate. Existing digest pins, SBOMs and provenance are useful foundations.
- Measure real memory, connection counts and storage growth before tuning requests, limits and maximum replicas. Include observed values and the load scenario in the portfolio.

## Publication notes

Keep local credentials, state, plans, generated binaries and environment-specific private files out of Git. Ignore rules do not remove secrets from existing history; review history before making the repository public. Do not commit Terraform state to illustrate the infrastructure.

Use the README to explain decisions and evidence: architecture diagram, pipeline triggers and trust boundaries, immutable promotion, migration ordering, rollback, recovery, observability, costs, and explicit availability limits. Link to the detailed operational documentation rather than duplicating it.

## Validation evidence

The workspace's 18 Rust tests passed, as did formatting and Clippy with warnings denied. GitHub Actions linting, chart profile assertions, Kubernetes 1.37 schema validation, Compose configuration validation, Terraform validation and all five Terraform tests passed. The promotion guard accepted unchanged and documentation-only main revisions and rejected changed release inputs.

Gitleaks found no leaks in 73 commits or in an export of the current tracked and non-ignored files. This is automated evidence, not a guarantee that every environment-specific identifier or sensitive value has been identified.

Docker Desktop was stopped during the review, so container builds and PostgreSQL/Kafka integration tests were not rerun. No live infrastructure apply or application rollout was performed as part of this review.
