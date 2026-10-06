provider "oci" {
  auth             = var.oci_auth
  tenancy_ocid     = var.tenancy_ocid
  user_ocid        = var.oci_auth == "APIKey" ? var.user_ocid : null
  fingerprint      = var.oci_auth == "APIKey" ? var.fingerprint : null
  private_key_path = var.oci_auth == "APIKey" ? var.private_key_path : null
  region           = var.region
}

provider "cloudflare" {
  # Local commands load the ignored token file; CI uses CLOUDFLARE_API_TOKEN.
  api_token = fileexists("${path.module}/../.cloudflare") ? sensitive(trimspace(file("${path.module}/../.cloudflare"))) : null
}
