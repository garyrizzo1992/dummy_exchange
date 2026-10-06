# Security follow-up

Saved after the 6 October 2026 review. The audit is complete; the security fixes below are still open. Start here next session.

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
