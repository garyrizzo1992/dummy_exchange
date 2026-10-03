# Application vault and encryption key. Secret values are managed outside Terraform.

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
