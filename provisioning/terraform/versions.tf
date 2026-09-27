terraform {
  # Ephemeral input variables require Terraform 1.10 or newer.
  required_version = ">= 1.10.0"

  required_providers {
    oci = {
      source = "oracle/oci"
      version = "9.3.0"
    }
    http = {
      source = "hashicorp/http"
      version = "3.6.2"
    }
  }
}
