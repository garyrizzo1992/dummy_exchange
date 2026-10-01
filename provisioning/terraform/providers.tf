# Account-specific provider values come from ignored local configuration, such
# as terraform.tfvars, or from the OCI CLI configuration/environment.
provider "oci" {
  auth             = var.oci_auth
  tenancy_ocid     = var.tenancy_ocid
  user_ocid        = var.oci_auth == "APIKey" ? var.user_ocid : null
  fingerprint      = var.oci_auth == "APIKey" ? var.fingerprint : null
  private_key_path = var.oci_auth == "APIKey" ? var.private_key_path : null
  region           = var.region
}
