# Private off-node dumps. Object versioning protects accidental overwrites;
# keep versions for a month and validate restores before relying on the bucket.
data "oci_objectstorage_namespace" "backups" {
  compartment_id = var.tenancy_ocid
}

resource "oci_objectstorage_bucket" "postgres_backups" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  namespace      = data.oci_objectstorage_namespace.backups.namespace
  name           = "${var.application}-${var.environment}-postgres-backups"
  access_type    = "NoPublicAccess"
  versioning     = "Enabled"
  storage_tier   = "Standard"
  freeform_tags  = local.freeform_tags
}

resource "oci_objectstorage_object_lifecycle_policy" "postgres_backups" {
  depends_on = [oci_identity_policy.backup_lifecycle]
  namespace  = oci_objectstorage_bucket.postgres_backups.namespace
  bucket     = oci_objectstorage_bucket.postgres_backups.name
  rules {
    name        = "expire-dumps-after-30-days"
    action      = "DELETE"
    is_enabled  = true
    target      = "objects"
    time_amount = 30
    time_unit   = "DAYS"
  }
  rules {
    name        = "expire-previous-versions-after-30-days"
    action      = "DELETE"
    is_enabled  = true
    target      = "previous-object-versions"
    time_amount = 30
    time_unit   = "DAYS"
  }
}

resource "oci_identity_policy" "backup_lifecycle" {
  compartment_id = var.tenancy_ocid
  name           = "${var.application}-${var.environment}-backup-lifecycle"
  description    = "Allow Object Storage to expire only development PostgreSQL backup objects"
  statements = [
    "Allow service objectstorage-${var.region} to manage object-family in compartment id ${oci_identity_compartment.dummy_exchange.id} where all {target.bucket.name = '${oci_objectstorage_bucket.postgres_backups.name}', any {request.permission = 'BUCKET_INSPECT', request.permission = 'BUCKET_READ', request.permission = 'OBJECT_INSPECT', request.permission = 'OBJECT_DELETE', request.permission = 'OBJECT_VERSION_DELETE'}}"
  ]
}

output "postgres_backup_bucket" {
  value = oci_objectstorage_bucket.postgres_backups.name
}

output "postgres_backup_namespace" {
  value = oci_objectstorage_bucket.postgres_backups.namespace
}
