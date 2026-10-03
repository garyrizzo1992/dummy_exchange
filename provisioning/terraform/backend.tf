terraform {
  backend "oci" {
    bucket    = "terraform-state"
    namespace = "ax6utpsobpgy"
    key       = "dev/terraform.tfstate"
    region    = "eu-milan-1"
  }
}
