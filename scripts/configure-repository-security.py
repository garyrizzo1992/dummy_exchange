"""Apply the checked-in main security rules and require SHA-pinned Actions."""
import json
import os
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BASE = "https://api.github.com/repos/garyrizzo1992/dummy_exchange"
TOKEN = os.environ["GITHUB_TOKEN"]


def api(path, method="GET", payload=None):
    request = urllib.request.Request(BASE + path, method=method,
                                     data=None if payload is None else json.dumps(payload).encode(),
                                     headers={"Authorization": "Bearer " + TOKEN,
                                              "Accept": "application/vnd.github+json",
                                              "X-GitHub-Api-Version": "2022-11-28",
                                              "User-Agent": "exchange-security-followups"})
    with urllib.request.urlopen(request, timeout=30) as response:
        body = response.read()
        return json.loads(body) if body else None


payload = json.loads((ROOT / "docs/security-repository-rules.json").read_text())
existing = next((r for r in api("/rulesets") if r["name"] == payload["name"]), None)
result = api("/rulesets" + ("/" + str(existing["id"]) if existing else ""),
             "PUT" if existing else "POST", payload)
print("Main security rules active:", result["id"])
settings = api("/actions/permissions")
settings["sha_pinning_required"] = True
api("/actions/permissions", "PUT", {k: settings[k] for k in ("enabled", "allowed_actions", "sha_pinning_required")})
assert api("/actions/permissions")["sha_pinning_required"] is True
print("SHA pinning is required by repository policy.")
