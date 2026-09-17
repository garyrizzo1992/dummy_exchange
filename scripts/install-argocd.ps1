[CmdletBinding()]
param(
    [ValidatePattern('^v\d+\.\d+\.\d+$')]
    [string]$Version = 'v3.5.3',

    [string]$Namespace = 'argocd',

    [string]$ApplicationNamespace = 'dummy-exchange',

    [string]$ApplicationSecretName = 'dummy-exchange-secrets',

    [switch]$StartMinikube,

    [ValidateSet('docker', 'hyperv', 'virtualbox')]
    [string]$MinikubeDriver = 'docker',

    [ValidatePattern('^\d+[smh]$')]
    [string]$WaitTimeout = '5m'
)

$ErrorActionPreference = 'Stop'

function Require-Command {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "'$Name' is required but was not found on PATH."
    }
}

Require-Command kubectl

if ($StartMinikube) {
    Require-Command minikube
    Write-Host "Starting Minikube with the $MinikubeDriver driver..."
    & minikube start "--driver=$MinikubeDriver"
    if ($LASTEXITCODE -ne 0) {
        throw 'Minikube did not start successfully.'
    }
}

& kubectl cluster-info 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'kubectl cannot reach a Kubernetes cluster. Start Minikube with -StartMinikube or select a valid context first.'
}

$namespaceExists = (& kubectl get namespace $Namespace --ignore-not-found -o name) -join ''
if ([string]::IsNullOrWhiteSpace($namespaceExists)) {
    Write-Host "Creating namespace '$Namespace'..."
    & kubectl create namespace $Namespace
    if ($LASTEXITCODE -ne 0) {
        throw "Could not create namespace '$Namespace'."
    }
}

$applicationNamespaceExists = (& kubectl get namespace $ApplicationNamespace --ignore-not-found -o name) -join ''
if ([string]::IsNullOrWhiteSpace($applicationNamespaceExists)) {
    Write-Host "Creating application namespace '$ApplicationNamespace'..."
    & kubectl create namespace $ApplicationNamespace
    if ($LASTEXITCODE -ne 0) {
        throw "Could not create application namespace '$ApplicationNamespace'."
    }
}

# These values are deliberately only suitable for the local test environment.
# `kubectl apply` makes bootstrap repeatable and replaces the prior test secret.
Write-Host "Creating test application secret '$ApplicationSecretName' in '$ApplicationNamespace'..."
$secretManifest = & kubectl -n $ApplicationNamespace create secret generic $ApplicationSecretName `
    '--from-literal=postgres-password=password' `
    '--from-literal=jwt-secret=local-development-secret-change-before-any-shared-use' `
    '--from-literal=grafana-admin-password=admin' `
    '--dry-run=client' `
    '-o' 'yaml'
if ($LASTEXITCODE -ne 0) {
    throw "Could not generate application secret '$ApplicationSecretName'."
}

$secretManifest | & kubectl apply -f -
if ($LASTEXITCODE -ne 0) {
    throw "Could not apply application secret '$ApplicationSecretName'."
}

$manifest = "https://raw.githubusercontent.com/argoproj/argo-cd/$Version/manifests/install.yaml"
Write-Host "Installing Argo CD $Version into '$Namespace'..."
& kubectl apply -n $Namespace --server-side --force-conflicts -f $manifest
if ($LASTEXITCODE -ne 0) {
    throw 'Argo CD installation failed.'
}

Write-Host 'Waiting for the Argo CD API server...'
& kubectl -n $Namespace rollout status deployment/argocd-server "--timeout=$WaitTimeout"
if ($LASTEXITCODE -ne 0) {
    throw 'Argo CD API server did not become available before the timeout.'
}

$encodedPassword = (& kubectl -n $Namespace get secret argocd-initial-admin-secret -o jsonpath='{.data.password}') -join ''
if ([string]::IsNullOrWhiteSpace($encodedPassword)) {
    throw 'Could not retrieve the initial Argo CD admin password.'
}

$password = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($encodedPassword))
Write-Host ''
Write-Host 'Argo CD is ready.'
Write-Host 'Username: admin'
Write-Host "Initial password: $password"
Write-Host "Open the UI with: kubectl -n $Namespace port-forward svc/argocd-server 8080:443"
