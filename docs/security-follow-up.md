# Security follow-up

Saved after the 6 October 2026 review. The audit is complete; the security fixes below are still open. Start here next session.

## Started: 6 October 2026

First batch is implemented locally; the original checkboxes remain open until rollout and live verification:

- Terraform now manages zone-wide minimum TLS 1.2. All six mocked Cloudflare plans pass, including the minimum-TLS and disabled-Cloudflare checks. HSTS awaits the domain review.
- Ansible installs Local Path Provisioner [0.0.36](https://github.com/rancher/local-path-provisioner/releases/tag/v0.0.36). Existing clusters update only the Deployment image, preserving StorageClass, ConfigMap path settings and PVCs. Comparing the 0.0.32 and 0.0.36 upstream manifests found only provisioner/helper image changes.
- CI and Ansible now use [Helm 3.22.0](https://github.com/helm/helm/releases/tag/v3.22.0). Removed the Ansible extraction guard that kept an existing old binary. The local verification binary passed the upstream SHA-256 check; Ansible syntax validation and `scripts/check-helm.py` passed with Helm 3.22.0. Kubernetes schema validation was skipped because kubeconform is not installed. Terraform formatting and validation also passed.

Before closing these items, review the infrastructure plan, apply the changes, confirm the installed Helm/provisioner versions and existing PVCs, then create and remove a disposable PVC/pod to check provisioning. Repeat TLS 1.0/1.1/1.2 handshake probes against API and frontend with a client capable of sending legacy handshakes; a local client refusal is not evidence of server rejection. Check API and frontend functionality. No live remediation has been performed in this batch.

## Second local batch

- Updated `jsonwebtoken` to locked 10.4.0 using the AWS-LC backend and disabled unused PEM defaults. Updated OpenTelemetry to 0.32 / SDK 0.32.1 and tracing integration to 0.33. Added JWT regression coverage for valid HS256, wrong signatures/algorithms, expired/missing/string expiration, invalid subjects and unsigned tokens.
- Disabled SQLx default features and enabled only the PostgreSQL/runtime/data/migration/macro features used here. `cargo tree --locked -i rsa` and `cargo tree --locked -i sqlx-mysql` are empty, including RSA with `--target all`. Optional MySQL/RSA lockfile entries remain; this is not a claim that a package-only lockfile scanner will be clean.
- Disabled service-account token mounts for API, migrations, worker, generator, traders, PostgreSQL, Grafana, PostgreSQL exporter and Tempo. Prometheus, kube-state-metrics and Alloy retain required Kubernetes API access. Added a rendered-chart check for unnecessary mounts.
- Workspace tests (25 tests, including the JWT regression), strict workspace Clippy, formatting and rendered chart checks passed. Kubernetes schema validation remains skipped because kubeconform is unavailable. Disposable database integration could not run because Docker Desktop's Linux daemon is unavailable. Replacement container builds, image scans and live rollout verification remain pending. Changes are local and uncommitted.

## Third batch: 7 October 2026

Implemented pinned Actions, checksum-verified scanners, dependency/secret gates and per-architecture image-digest scan gates before promotion. Git history at the start of this batch contained 103 commits with no Gitleaks findings. The local dependency scan has zero high/critical findings after the expiring, package/path-scoped exceptions in `.trivyignore.yaml`. Replacement image gates are awaiting the next published images.

Implemented separate runtime and PostgreSQL-exporter credentials in the operational Vault, a data-chart provisioning Job, runtime DML grants, and separate migration credentials. Disposable database tests pass for runtime DML, denied schema creation/migration-history access and restricted exporter statistics access. Live credentials are prepared; workload rollout is pending publication of these chart changes.

API/database ingress policies are live: disposable unrelated pods fail both TCP connections; pods in the restricted operator namespace succeed. Added non-root PostgreSQL settings and admission safeguards with a dedicated node-exporter exception. Helm schema/profile checks now pass with kubeconform 0.8.0 against Kubernetes 1.37 schemas.

Terraform applied minimum TLS 1.2 and opt-in five-minute HSTS after checking all five proxied HTTPS hostnames. Actual legacy-capable OpenSSL probes now receive server protocol-version alerts for TLS 1.0/1.1, while TLS 1.2 succeeds. API and frontend return 200. Grafana and Argo return Cloudflare Access redirects; their operator email login still needs a human check. Their automation remains on the private Kubernetes/SSH path.

Created the private, versioned OCI backup bucket and its scoped lifecycle-service policy. Uploaded a PostgreSQL dump, fetched it from Object Storage, verified SHA-256 and restored ten public tables and ten successful migrations into a new disposable database in 16 seconds. The fixture was removed. Added scheduled backup/restore automation. Narrowed the infrastructure runner's configured IAM resource families; live IAM rollout and a runner plan remain to verify.

The [rollout and recovery runbook](security-rollout.md) includes credential rotation, network/admission ordering, backup recovery, application rollback and the PostgreSQL/Kafka certificate rollout plan. No plaintext database/Kafka listener has been switched off yet, and existing platform-image advisories remain open.

## Fix first

- [ ] Set Cloudflare minimum TLS to 1.2 through Terraform. Check the domains before enabling HSTS. Verify TLS 1.0/1.1 fail and the frontend/API still work.
- [ ] Upgrade Local Path Provisioner from 0.0.32 to a supported release covering both advisories (at least 0.0.36 as identified in the review). Preserve existing volumes and verify provisioning still works.
- [ ] Update Helm 3.17.3 in CI and Ansible to a supported patched release. Argo uses a separate bundled Helm 4.2.1. Run chart checks after the change.
- [ ] Patch Rust dependencies and container packages. Review jsonwebtoken, OpenTelemetry, unused SQLx MySQL/RSA dependencies, and bundled OS/Go/Java findings. Rescan the replacement image digests and record justified exceptions.
- [ ] Replace runtime PostgreSQL superuser access with limited roles. Keep migrations separate. Test registration, orders, matching and the generator.
- [ ] Add API/database network isolation, disable unnecessary service-account token mounts and enforce suitable pod admission rules. Preserve Tunnel, monitoring and operator access. Repeat the unrelated-pod connection checks.

## Then harden and verify

- [ ] Pin GitHub Actions to commit SHAs, add secret/dependency/image scan gates, and require successful checks before release. Keep image promotion working.
- [ ] Protect Grafana and Argo CD with Cloudflare Access. Verify login and automation paths before rollout.
- [ ] Review the infrastructure runner's broad OCI permissions and the API's trust of Cloudflare client-IP headers.
- [ ] Plan encryption for PostgreSQL and Kafka traffic, with a tested certificate and credential rollout.
- [ ] Store PostgreSQL backups outside the nodes and test restoration. Rehearse a rollback and record recovery time.
- [ ] Repeat the security checks and update the report with what was fixed, what passed and what remains.

## Evidence to keep

- [Review and deployed versions](security-review.md)
- [Sanitised test results](security-review-evidence.json)
- [Running image digests and scan findings](security-image-inventory.json)
- [Operational notes and runbooks](operations.md)

Authentication, cross-account access, tested injection inputs, request limits and browser controls passed the bounded checks. Gitleaks found no secrets in 101 commits. Scanner findings need applicability review; they are not all proven exploits. The disposable audit pod and database were removed.

Recheck vendor releases and the live configuration before choosing updates. Keep changes in Terraform, Ansible, Helm or CI as appropriate, test them, then commit and push. Keep portfolio READMEs short, plain and focused on DevOps/SRE evidence, with the AI disclosure retained.
