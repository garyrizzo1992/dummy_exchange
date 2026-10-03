# Private Kubernetes nodes and the optional dedicated CI runner.

resource "oci_core_instance" "control_plane" {
  availability_domain = data.oci_identity_availability_domains.available.availability_domains[0].name
  compartment_id      = oci_identity_compartment.dummy_exchange.id
  display_name        = "${var.application}-${var.environment}-control-plane"
  shape               = var.instance_shape

  agent_config {
    plugins_config {
      name          = "Bastion"
      desired_state = "ENABLED"
    }
  }

  create_vnic_details {
    subnet_id                 = oci_core_subnet.dummy_exchange_priv.id
    assign_public_ip          = false
    assign_private_dns_record = true
    display_name              = "${var.application}-${var.environment}-control-plane-vnic"
    hostname_label            = "control-plane"
    nsg_ids                   = [oci_core_network_security_group.kubernetes_nodes.id]
  }

  metadata = {
    ssh_authorized_keys = var.ssh_authorized_keys
  }

  preserve_boot_volume = false

  shape_config {
    ocpus         = var.control_plane_ocpus
    memory_in_gbs = var.control_plane_memory_in_gbs
  }

  source_details {
    source_type             = "image"
    source_id               = var.image_ocid
    boot_volume_size_in_gbs = 50
  }

  freeform_tags = merge(local.freeform_tags, {
    Component = "control-plane"
  })
}

resource "oci_core_instance" "worker" {
  availability_domain = data.oci_identity_availability_domains.available.availability_domains[0].name
  compartment_id      = oci_identity_compartment.dummy_exchange.id
  display_name        = "${var.application}-${var.environment}-worker"
  shape               = var.instance_shape

  agent_config {
    plugins_config {
      name          = "Bastion"
      desired_state = "ENABLED"
    }
  }

  create_vnic_details {
    subnet_id                 = oci_core_subnet.dummy_exchange_priv.id
    assign_public_ip          = false
    assign_private_dns_record = true
    display_name              = "${var.application}-${var.environment}-worker-vnic"
    hostname_label            = "worker"
    nsg_ids                   = [oci_core_network_security_group.kubernetes_nodes.id]
  }

  metadata = {
    ssh_authorized_keys = var.ssh_authorized_keys
  }

  preserve_boot_volume = false

  shape_config {
    ocpus         = var.worker_ocpus
    memory_in_gbs = var.worker_memory_in_gbs
  }

  source_details {
    source_type             = "image"
    source_id               = var.image_ocid
    boot_volume_size_in_gbs = 50
  }

  freeform_tags = merge(local.freeform_tags, {
    Component = "worker"
  })
}

resource "oci_core_instance" "ci_runner" {
  count               = var.enable_ci_runner ? 1 : 0
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
    user_data           = base64encode(file("${path.module}/templates/runner-cloud-init.yml"))
  }

  source_details {
    source_type             = "image"
    source_id               = var.image_ocid
    boot_volume_size_in_gbs = 50
  }

  freeform_tags = merge(local.freeform_tags, { Component = "ci-runner" })
  depends_on    = [oci_core_nat_gateway.nat_gateway, oci_core_route_table.private_instances]
}
