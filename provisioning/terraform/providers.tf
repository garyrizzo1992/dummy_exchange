# Account-specific provider values come from ignored local configuration, such
# as terraform.tfvars, or from the OCI CLI configuration/environment.
provider "oci" {
  tenancy_ocid     = var.tenancy_ocid
  user_ocid        = var.user_ocid
  fingerprint      = var.fingerprint
  private_key_path = var.private_key_path
  region           = var.region
}
