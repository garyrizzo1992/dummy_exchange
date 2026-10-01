terraform {
  # The native OCI state backend requires Terraform 1.12 or newer.
  required_version = ">= 1.12.0"

  required_providers {
    ansible = {
      source  = "ansible/ansible"
      version = "~> 1.4"
    }

    oci = {
      source  = "oracle/oci"
      version = "9.3.0"
    }
    http = {
      source  = "hashicorp/http"
      version = "3.6.2"
    }
  }
}
