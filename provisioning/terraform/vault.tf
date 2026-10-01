resource "oci_kms_vault" "application" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  display_name   = "${var.application}-${var.environment}-secrets"
  vault_type     = "DEFAULT"
  freeform_tags  = local.freeform_tags
}

resource "oci_kms_key" "application" {
  compartment_id      = oci_identity_compartment.dummy_exchange.id
  display_name        = "${var.application}-${var.environment}-secrets"
  management_endpoint = oci_kms_vault.application.management_endpoint

  key_shape {
    algorithm = "AES"
    length    = 32
  }
}

# ESO uses the worker's instance principal; secret values are never Terraform inputs.
resource "oci_identity_dynamic_group" "vault_reader" {
  compartment_id = var.tenancy_ocid
  name           = "${var.application}-${var.environment}-vault-reader"
  description    = "Kubernetes worker instance principal for External Secrets"
  matching_rule  = "ALL {instance.id = '${oci_core_instance.worker.id}'}"
}

resource "oci_identity_policy" "vault_reader" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  name           = "${var.application}-${var.environment}-vault-reader"
  description    = "Read application secrets in the project compartment"
  statements = [
    "Allow dynamic-group ${oci_identity_dynamic_group.vault_reader.name} to read vaults in compartment id ${oci_identity_compartment.dummy_exchange.id}",
    "Allow dynamic-group ${oci_identity_dynamic_group.vault_reader.name} to read secret-bundles in compartment id ${oci_identity_compartment.dummy_exchange.id}"
  ]
}

output "application_vault_id" {
  value = oci_kms_vault.application.id
}

output "application_secret_key_id" {
  value = oci_kms_key.application.id
}
