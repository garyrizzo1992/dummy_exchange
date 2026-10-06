"""Check denied and operator-allowed TCP connections with disposable dev pods."""
import json
from pathlib import Path
import subprocess
import uuid

ROOT = Path(__file__).resolve().parents[1]
IMAGE = "postgres:18.6-bookworm@sha256:1c59e2c3c818eaa0f0628f695b36e7c9e362d6b219b36a54a32df645cbd7e1af"
targets = [("api", 3000), ("postgres", 5432)]


def kubectl(*args, data=None):
    result = subprocess.run(["bash", "provisioning/ansible/dev-kubectl.sh", *args],
                            input=data, text=True, capture_output=True, cwd=ROOT)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout


for namespace, expected in [("default", False), ("exchange-ops", True)]:
    name = "security-network-" + uuid.uuid4().hex[:10]
    command = "set -eu\n"
    for component, port in targets:
        host = f"dummy-exchange-dev-dummy-exchange-{component}.dummy-exchange.svc.cluster.local"
        command += f"if timeout 5 bash -c 'exec 3<>/dev/tcp/{host}/{port}'; then result=allowed; else result=denied; fi\n"
        command += f"echo '{component}:'$result\ntest \"$result\" = {'allowed' if expected else 'denied'}\n"
    pod = {
        "apiVersion": "v1", "kind": "Pod", "metadata": {"name": name, "namespace": namespace},
        "spec": {
            "restartPolicy": "Never", "automountServiceAccountToken": False,
            "securityContext": {"runAsNonRoot": True, "runAsUser": 999,
                                "seccompProfile": {"type": "RuntimeDefault"}},
            "containers": [{"name": "probe", "image": IMAGE, "command": ["bash", "-c", command],
                            "securityContext": {"allowPrivilegeEscalation": False,
                                                "readOnlyRootFilesystem": True,
                                                "capabilities": {"drop": ["ALL"]}},
                            "resources": {"requests": {"cpu": "10m", "memory": "16Mi"},
                                          "limits": {"cpu": "100m", "memory": "64Mi"}}}],
        },
    }
    try:
        kubectl("create", "-f", "-", data=json.dumps(pod))
        try:
            kubectl("wait", "-n", namespace, f"pod/{name}", "--for=jsonpath={.status.phase}=Succeeded", "--timeout=60s")
        finally:
            print(namespace, kubectl("logs", "-n", namespace, name).strip())
    finally:
        kubectl("delete", "pod", "-n", namespace, name, "--ignore-not-found", "--wait=false")
print("Unrelated pods are blocked; restricted operator pods retain API/database access.")
