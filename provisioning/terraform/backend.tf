# Native OCI Object Storage state with locking. The bucket has versioning
# enabled so previous state revisions can be recovered.
terraform {
  backend "oci" {
    bucket    = "terraform-state"
    namespace = "ax6utpsobpgy"
    key       = "dev/terraform.tfstate"
    region    = "eu-milan-1"
  }
}
