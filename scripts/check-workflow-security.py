"""Fail if external Actions use mutable refs or releases bypass security checks."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
for path in (ROOT / ".github/workflows").glob("*.y*ml"):
    source = path.read_text()
    for ref in re.findall(r"uses:\s*([^\s#]+)", source):
        if ref.startswith("./"):
            continue
        assert re.fullmatch(r"[\w.-]+/[\w./-]+@[0-9a-f]{40}", ref), (path.name, ref)
    if path.name == "application.yml":
        jobs = dict(re.findall(r"^  ([\w-]+):\n(.*?)(?=^  [\w-]+:|\Z)", source, re.M | re.S))
        assert "security" in jobs
        for job in ("deploy-frontend", "publish-images", "verify-images"):
            needs = re.search(r"^    needs: (.+)$", jobs[job], re.M).group(1)
            assert "security" in needs and "test" in needs, job
        assert "Gate the exact published architecture digest" in jobs["publish-images"]
        assert "needs: publish-images" in jobs["publish-manifests"]
        assert "needs: publish-manifests" in jobs["promote-dev"]
    if path.name == "infrastructure.yml":
        assert "needs: [validate, security]" in source
print("External Actions are pinned to full commit SHAs.")
