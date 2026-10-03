# Shared paths and resource tags.

locals {
  ansible_directory = abspath("${path.module}/../ansible")

  freeform_tags = {
    Application = var.application
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}
