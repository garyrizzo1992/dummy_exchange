# Instance-principal permissions for CI and the External Secrets worker.

resource "oci_identity_dynamic_group" "ci_runner" {
  count          = var.enable_ci_runner ? 1 : 0
  compartment_id = var.tenancy_ocid
  name           = "${var.application}-${var.environment}-ci-runner"
  description    = "Dedicated Terraform runner instance principal"
  matching_rule  = "ALL {instance.id = '${oci_core_instance.ci_runner[0].id}'}"
}

resource "oci_identity_policy" "ci_runner" {
  count          = var.enable_ci_runner ? 1 : 0
  compartment_id = var.tenancy_ocid
  name           = "${var.application}-${var.environment}-ci-runner"
  description    = "CI manages project resources and the development state bucket"
  statements = [
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner[0].name} to manage all-resources in compartment id ${oci_identity_compartment.dummy_exchange.id}",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner[0].name} to inspect compartments in tenancy",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner[0].name} to read dynamic-groups in tenancy",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner[0].name} to read policies in tenancy",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner[0].name} to read buckets in tenancy where target.bucket.name = 'terraform-state'",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner[0].name} to manage objects in tenancy where target.bucket.name = 'terraform-state'"
  ]
}

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
