locals {
  freeform_tags = {
    Application = var.application
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}
# Select the first availability domain offered in the configured OCI region.
# In a single-AD region this is the only available choice.
data "oci_identity_availability_domains" "available" {
  compartment_id = var.tenancy_ocid
}

resource "oci_identity_compartment" "dummy_exchange" {
  compartment_id = var.tenancy_ocid
  description    = "all resources relevant to dummy_exchange project"
  name           = "dummy-exchange"
  freeform_tags  = local.freeform_tags
}

resource "oci_core_vcn" "vcn" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  cidr_blocks    = ["10.0.0.0/16"]
  dns_label      = "dummyexchange"
  display_name   = "${var.application}-${var.environment}-vcn"
  freeform_tags  = local.freeform_tags
}

resource "oci_core_internet_gateway" "internet_gateway" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id
  display_name   = "${var.application}-${var.environment}-igw"
  enabled        = true
  freeform_tags  = local.freeform_tags
}

# Used only by the public subnet that hosts the public load balancer.
resource "oci_core_route_table" "public_load_balancer" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id
  display_name   = "${var.application}-${var.environment}-public-lb-rt"

  route_rules {
    destination       = "0.0.0.0/0"
    destination_type  = "CIDR_BLOCK"
    network_entity_id = oci_core_internet_gateway.internet_gateway.id
  }

  freeform_tags = local.freeform_tags
}

# Used only by the private subnet that hosts the application VMs.
resource "oci_core_route_table" "private_instances" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id
  display_name   = "${var.application}-${var.environment}-private-vm-rt"

  route_rules {
    destination       = "0.0.0.0/0"
    destination_type  = "CIDR_BLOCK"
    network_entity_id = oci_core_nat_gateway.nat_gateway.id
  }

  freeform_tags = local.freeform_tags
}

resource "oci_core_network_security_group" "kubernetes_nodes" {
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id
  display_name   = "${var.application}-${var.environment}-k8s-nodes"
}

resource "oci_core_network_security_group_security_rule" "kubernetes_node_to_node" {
  network_security_group_id = oci_core_network_security_group.kubernetes_nodes.id
  direction                 = "INGRESS"
  protocol                  = "all"
  source                    = oci_core_network_security_group.kubernetes_nodes.id
  source_type               = "NETWORK_SECURITY_GROUP"
  description               = "Allow Kubernetes node-to-node traffic"
}

resource "oci_core_subnet" "dummy_exchange_pub" {
  compartment_id            = oci_identity_compartment.dummy_exchange.id
  vcn_id                    = oci_core_vcn.vcn.id
  cidr_block                = "10.0.1.0/24"
  dns_label                 = "public"
  display_name              = "${var.application}-${var.environment}-public-lb-subnet"
  freeform_tags             = local.freeform_tags
  prohibit_internet_ingress = false
  route_table_id            = oci_core_route_table.public_load_balancer.id
}

resource "oci_core_subnet" "dummy_exchange_priv" {
  compartment_id            = oci_identity_compartment.dummy_exchange.id
  vcn_id                    = oci_core_vcn.vcn.id
  cidr_block                = "10.0.2.0/24"
  dns_label                 = "private"
  display_name              = "${var.application}-${var.environment}-private-vm-subnet"
  freeform_tags             = local.freeform_tags
  prohibit_internet_ingress = true
  route_table_id            = oci_core_route_table.private_instances.id
}

resource "oci_core_nat_gateway" "nat_gateway" {
  #Required
  compartment_id = oci_identity_compartment.dummy_exchange.id
  vcn_id         = oci_core_vcn.vcn.id

  block_traffic = false
  display_name  = "${var.application}-${var.environment}-nat"
  freeform_tags = local.freeform_tags
}

## Assuming terraform is running from the user's pc, Get the IP of the host machine and add it to the bastion's whitelisting.

data "http" "terraform_runner_ip" {
  url = "https://api.ipify.org"
}
resource "oci_bastion_bastion" "bastion" {
  #Required
  bastion_type               = "STANDARD"
  compartment_id             = oci_identity_compartment.dummy_exchange.id
  target_subnet_id           = oci_core_subnet.dummy_exchange_priv.id
  max_session_ttl_in_seconds = 10800

  #Optional
  client_cidr_block_allow_list = length(var.bastion_client_cidrs) > 0 ? var.bastion_client_cidrs : ["${trimspace(data.http.terraform_runner_ip.response_body)}/32"]
  freeform_tags                = local.freeform_tags
  name                         = "${var.application}-${var.environment}-bastion"
}

# Paid x86_64 VMs sized for a kubeadm control plane and application workloads.
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

action "ansible_playbook_run" "cluster" {
  config {
    ansible_playbook_binary = var.ansible_runner == "windows" ? abspath("${path.module}/ansible-playbook.cmd") : "${var.ansible_wsl_directory}/ansible-playbook-wrapper.sh"
    playbooks               = ["${var.ansible_wsl_directory}/playbooks/cluster.yml"]
    inventory_files         = ["${var.ansible_wsl_directory}/inventories/dev/terraform.ini"]
  }
}

resource "terraform_data" "ansible" {
  input = {
    control_plane_id = oci_core_instance.control_plane.id
    worker_id        = oci_core_instance.worker.id
    connection_hash = sha256(join("", [
      filesha256("${path.module}/../ansible/ansible-playbook-wrapper.sh"),
      filesha256("${path.module}/../ansible/managed-ssh.sh"),
      filesha256("${path.module}/../ansible/ansible.cfg"),
      filesha256("${path.module}/../ansible/inventories/dev/terraform.ini")
    ]))
    playbook_hash = filesha256("${path.module}/../ansible/playbooks/cluster.yml")
    control_plane_role_hash = filesha256(
      "${path.module}/../ansible/roles/control_plane/tasks/main.yml"
    )
    worker_role_hash = filesha256("${path.module}/../ansible/roles/worker/tasks/main.yml")
    cluster_addons_role_hash = filesha256(
      "${path.module}/../ansible/roles/cluster_addons/tasks/main.yml"
    )
    argocd_application_hash = filesha256(
      "${path.module}/../../deploy/argocd/dummy-exchange-dev.yaml"
    )
  }

  lifecycle {
    action_trigger {
      events     = [after_create, after_update]
      actions    = [action.ansible_playbook_run.cluster]
      on_failure = halt
    }
  }
}

