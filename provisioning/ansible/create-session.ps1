$ErrorActionPreference = "Stop"

$config = Get-Content "$env:USERPROFILE\.oci\config" -Raw
$tenancyId = ([regex]::Match(
  $config,
  '(?m)^tenancy\s*=\s*(.+?)\s*$'
)).Groups[1].Value

$compartment = (
  (oci iam compartment list `
    --compartment-id $tenancyId `
    --compartment-id-in-subtree true `
    --all `
    --output json | ConvertFrom-Json).data |
  Where-Object {
    $_.name -eq "dummy-exchange" -and
    $_."lifecycle-state" -eq "ACTIVE"
  } |
  Select-Object -First 1
)

$bastion = (
  (oci bastion bastion list `
    --compartment-id $compartment.id `
    --all `
    --output json | ConvertFrom-Json).data |
  Where-Object {
    $_.name -eq "dummy-exchange-dev-bastion" -and
    $_."lifecycle-state" -eq "ACTIVE"
  } |
  Select-Object -First 1
)

$controlPlane = (
  (oci compute instance list `
    --compartment-id $compartment.id `
    --all `
    --output json | ConvertFrom-Json).data |
  Where-Object {
    $_."display-name" -eq "dummy-exchange-dev-control-plane" -and
    $_."lifecycle-state" -eq "RUNNING"
  } |
  Select-Object -First 1
)

$worker = (
  (oci compute instance list `
    --compartment-id $compartment.id `
    --all `
    --output json | ConvertFrom-Json).data |
  Where-Object {
    $_."display-name" -eq "dummy-exchange-dev-worker" -and
    $_."lifecycle-state" -eq "RUNNING"
  } |
  Select-Object -First 1
)

$keyPath = "$env:USERPROFILE\.ssh\id_ed25519"

if (-not (Test-Path -LiteralPath $keyPath) -or -not (Test-Path -LiteralPath "$keyPath.pub")) {
  throw "Expected SSH key pair not found at $keyPath."
}

$bastionDetails = (oci bastion bastion get --bastion-id $bastion.id --output json | ConvertFrom-Json).data
$sessionTtl = $bastionDetails."max-session-ttl-in-seconds"

function Get-ManagedSshCommand($node, [string]$sessionName) {
  $plugin = (
    oci instance-agent plugin list `
      --compartment-id $compartment.id `
      --instanceagent-id $node.id `
      --name Bastion `
      --output json | ConvertFrom-Json
  ).data

  if ($plugin.status -ne "RUNNING") {
    throw "Cannot create a Managed SSH session for ${sessionName}: Bastion plugin status is $($plugin.status)."
  }

  $session = (
    (oci bastion session list --bastion-id $bastion.id --all --output json | ConvertFrom-Json).data |
    Where-Object {
      $_."display-name" -eq $sessionName -and
      $_."lifecycle-state" -eq "ACTIVE"
    } |
    Select-Object -First 1
  )

  if (-not $session) {
    oci bastion session create-managed-ssh `
      --bastion-id $bastion.id `
      --target-resource-id $node.id `
      --target-os-username opc `
      --target-port 22 `
      --ssh-public-key-file "$keyPath.pub" `
      --session-ttl $sessionTtl `
      --display-name $sessionName `
      --wait-for-state SUCCEEDED | Out-Null

    if ($LASTEXITCODE -ne 0) {
      throw "OCI failed to create the $sessionName session."
    }

    $session = (
      (oci bastion session list --bastion-id $bastion.id --all --output json | ConvertFrom-Json).data |
      Where-Object {
        $_."display-name" -eq $sessionName -and
        $_."lifecycle-state" -eq "ACTIVE"
      } |
      Select-Object -First 1
    )
  }

  if (-not $session) {
    throw "The $sessionName session did not become active."
  }

  $metadata = (oci bastion session get --session-id $session.id --output json | ConvertFrom-Json).data."ssh-metadata"
  return $metadata.command.Replace("<privateKey>", $keyPath.Replace("\\", "/"))
}

Write-Output "Control plane SSH command:"
Write-Output (Get-ManagedSshCommand $controlPlane "control-plane-ssh")
Write-Output ""
Write-Output "Worker SSH command:"
Write-Output (Get-ManagedSshCommand $worker "worker-ssh")
