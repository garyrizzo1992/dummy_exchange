# Ansible bootstrap action and its configuration change triggers.

action "ansible_playbook_run" "cluster" {
  config {
    ansible_playbook_binary = "${local.ansible_directory}/ansible-playbook-wrapper.sh"
    playbooks               = ["${local.ansible_directory}/playbooks/cluster.yml"]
    inventory_files         = ["${local.ansible_directory}/inventories/dev/terraform.ini"]
    extra_vars = {
      cloudflare_enabled    = tostring(var.cloudflare_enabled)
      cloudflare_account_id = var.cloudflare_account_id
      cloudflare_tunnel_id  = try(cloudflare_zero_trust_tunnel_cloudflared.api[0].id, "")
    }
  }
}

resource "terraform_data" "ansible" {
  input = {
    control_plane_id       = oci_core_instance.control_plane.id
    worker_id              = oci_core_instance.worker.id
    cloudflare_tunnel_id   = try(cloudflare_zero_trust_tunnel_cloudflared.api[0].id, null)
    cloudflare_config_hash = var.cloudflare_enabled ? sha256(jsonencode(cloudflare_zero_trust_tunnel_cloudflared_config.api[0].config)) : null
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
    cluster_addons_role_hash = sha256(join("", [
      filesha256("${path.module}/../ansible/roles/cluster_addons/tasks/main.yml"),
      filesha256("${path.module}/../ansible/roles/cluster_addons/tasks/cloudflare.yml")
    ]))
    argocd_application_hash = sha256(join("", [
      filesha256("${path.module}/../../deploy/argocd/dummy-exchange-dev.yaml"),
      filesha256("${path.module}/../../deploy/argocd/postgres-dev.yaml"),
      filesha256("${path.module}/../../deploy/argocd/monitoring-dev.yaml"),
      filesha256("${path.module}/../../deploy/argocd/keda-dev.yaml"),
      filesha256("${path.module}/../../deploy/argocd/kafka-dev.yaml"),
      filesha256("${path.module}/../../deploy/argocd/tunnel-dev.yaml")
    ]))
  }

  depends_on = [oci_bastion_bastion.bastion, cloudflare_zero_trust_tunnel_cloudflared_config.api, cloudflare_dns_record.api, cloudflare_dns_record.services, cloudflare_zone_setting.https]

  lifecycle {
    action_trigger {
      events     = [after_create, after_update]
      actions    = [action.ansible_playbook_run.cluster]
      on_failure = halt
    }
  }
}
