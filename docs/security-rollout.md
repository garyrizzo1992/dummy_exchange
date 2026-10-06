# Security rollout and recovery

Runtime access is split from schema migration access. `exchange_app` inherits
`exchange_runtime`: application-table DML and sequence use, with no superuser,
role/database creation, schema creation, replication, bypass-RLS or migration
history access. The PostgreSQL chart provisions its independent Vault credential
before business migrations. The API migration Job retains administrator
credentials through `migrations.database`; runtime pods receive only the runtime
Secret. Rotation requires updating Vault, waiting for External Secrets, rerunning
the data application's credential Job, then restarting runtime pods. Existing
connections may continue during rotation; validate new connections before
revoking the old credential.

API/PostgreSQL policies admit named application and monitoring components,
Cloudflare connectors (API only), and the restricted `exchange-ops` namespace.
Cluster administrators can use Bastion and `kubectl exec`; general namespaces
cannot connect directly. API Cloudflare-header trust requires this network
boundary. Any principal able to create/label allowed pods can still enter that
boundary. Keep that capability restricted to the deployment/operator identities.

The node exporter needs host namespaces and a read-only host-root mount. It uses
a dedicated service account with no API token. The admission policy permits that
exception while rejecting privileged containers, explicit root users and other
host access. `exchange-ops` enforces Kubernetes restricted pod admission. Apply
the exporter service-account change before enabling the admission binding.

Public Grafana and Argo CD now require Cloudflare Access email login as well as
their own authentication. Public API and frontend remain public. GitOps and CI
use Kubernetes/private SSH rather than public Argo login. For an operator CLI,
use a private `kubectl port-forward` through the existing Bastion tooling; an
Access redirect is not an Argo API credential. Email-code login needs an operator
check; unauthenticated redirects alone do not prove successful login.

## Backup and rollback

The private, versioned OCI bucket stores PostgreSQL custom-format dumps and
SHA-256 sidecars outside the nodes. Daily backup automation downloads the object,
compares its checksum, restores into a new disposable database and removes the
fixture. Dumps contain sensitive data; keep their credentials and contents out of
logs and GitHub artifacts. Bucket lifecycle expires current and previous versions
after 30 days. Verify the Object Storage service policy when changing retention.

For recovery, download a chosen dump and checksum to a directory accessible only
to the operator. Restore to a new database with `pg_restore --exit-on-error
--no-owner --no-acl`, check migration history, representative account/order/fill
queries and API integration, then choose a maintenance window for switching the
database destination. Preserve the current database until the recovered instance
passes checks. Do not use the disposable restore verifier to overwrite it.

For an application rollback, revert the desired image digests to the previous
verified digests in the three business chart values files. Keep the restricted
runtime credentials and compatible schema. Verify `/readyz`, registration,
authenticated orders, matching, generator progress and monitoring after Argo
becomes Healthy. A schema rollback requires a separate compatibility review or
restore into a new database. Measure rollout time from sync start to those checks;
the backup restore measurement does not substitute for an application rollback.

## PostgreSQL and Kafka encryption plan

1. Issue an internal CA and service certificates. Include PostgreSQL's Service
   DNS names in its SANs. Strimzi already manages broker certificate identities;
   use its cluster CA for Kafka. Store private keys/CA material in Vault-backed
   Secrets and mount only the material each workload needs.
2. Enable PostgreSQL SSL with a server key owned by UID 999 and mode 0600. Start
   with encrypted and existing clients supported; verify SCRAM over TLS in a
   disposable database. Mount the CA into all database clients, configure
   `PGSSLROOTCERT` and `PGSSLMODE=verify-full`, and test registration, migrations,
   reservations, matching, generator progress and exporter queries.
3. Add a Strimzi internal TLS listener alongside 9092. Build librdkafka with SSL
   support, configure `security.protocol=SSL` and `ssl.ca.location` for traders
   and workers, and configure KEDA's TLS authentication/CA. Confirm hostname
   verification, production, consumption, offsets, retries and autoscaling.
4. Test certificate renewal with overlapping trust, then roll clients one group
   at a time. Check reconnects and credentials from both architectures. Once all
   clients are verified, require PostgreSQL `hostssl`, reject `hostnossl`, and
   remove the plaintext Kafka listener/network-policy port.
5. Keep the old trust material during the tested transition window. Roll back
   client settings/listeners if connectivity fails; restore encryption without
   weakening authentication. Record expiry alerts and a renewal rehearsal.

Traffic encryption remains a future rollout. Network isolation reduces exposure
but does not encrypt packets. Existing infrastructure-image advisories also need
component-specific upgrades and replacement-digest scans; the application image
gate does not cover all third-party cluster images.
