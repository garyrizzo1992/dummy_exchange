# Attach this NSG to the dedicated CI runner's VNIC during runner setup.
# The runner is separate from Kubernetes and only trusted main-branch jobs use it.
resource "oci_core_network_security_group" "ci_runner" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id
  display_name   = "${var.application}-${var.environment}-ci-runner"
  freeform_tags  = local.freeform_tags
}

resource "oci_core_network_security_group_security_rule" "ci_runner_ssh" {
  network_security_group_id = oci_core_network_security_group.kubernetes_nodes.id
  direction                 = "INGRESS"
  protocol                  = "6"
  source                    = oci_core_network_security_group.ci_runner.id
  source_type               = "NETWORK_SECURITY_GROUP"
  description               = "Allow the dedicated CI runner to provision nodes over private SSH"

  tcp_options {
    destination_port_range {
      min = 22
      max = 22
    }
  }
}

output "ci_runner_nsg_id" {
  description = "Attach this NSG to the private CI runner VNIC."
  value       = oci_core_network_security_group.ci_runner.id
}

resource "oci_core_instance" "ci_runner" {
  availability_domain = data.oci_identity_availability_domains.available.availability_domains[0].name
  compartment_id      = oci_identity_compartment.dummy_exchange.id
  display_name        = "${var.application}-${var.environment}-ci-runner"
  shape               = var.instance_shape

  shape_config {
    ocpus         = 1
    memory_in_gbs = 4
  }

  create_vnic_details {
    subnet_id                 = oci_core_subnet.dummy_exchange_priv.id
    assign_public_ip          = false
    assign_private_dns_record = true
    hostname_label            = "ci-runner"
    nsg_ids                   = [oci_core_network_security_group.ci_runner.id]
  }

  agent_config {
    plugins_config {
      name          = "Bastion"
      desired_state = "ENABLED"
    }
  }

  metadata = {
    ssh_authorized_keys = var.ssh_authorized_keys
    user_data           = base64encode(file("${path.module}/runner-cloud-init.yml"))
  }

  source_details {
    source_type             = "image"
    source_id               = var.image_ocid
    boot_volume_size_in_gbs = 50
  }

  freeform_tags = merge(local.freeform_tags, { Component = "ci-runner" })
  depends_on    = [oci_core_nat_gateway.nat_gateway, oci_core_route_table.private_instances]
}

resource "oci_identity_dynamic_group" "ci_runner" {
  compartment_id = var.tenancy_ocid
  name           = "${var.application}-${var.environment}-ci-runner"
  description    = "Dedicated Terraform runner instance principal"
  matching_rule  = "ALL {instance.id = '${oci_core_instance.ci_runner.id}'}"
}

resource "oci_identity_policy" "ci_runner" {
  compartment_id = var.tenancy_ocid
  name           = "${var.application}-${var.environment}-ci-runner"
  description    = "CI manages project resources and the development state bucket"
  statements = [
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner.name} to manage all-resources in compartment id ${oci_identity_compartment.dummy_exchange.id}",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner.name} to inspect compartments in tenancy",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner.name} to read dynamic-groups in tenancy",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner.name} to read policies in tenancy",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner.name} to read buckets in tenancy where target.bucket.name = 'terraform-state'",
    "Allow dynamic-group ${oci_identity_dynamic_group.ci_runner.name} to manage objects in tenancy where target.bucket.name = 'terraform-state'"
  ]
}

output "ci_runner_id" {
  value = oci_core_instance.ci_runner.id
}
