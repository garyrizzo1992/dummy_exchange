"""Install pinned, checksum-verified Linux security scanners into a chosen directory."""
import argparse
import hashlib
import io
from pathlib import Path
import platform
import tarfile
import urllib.request

TOOLS = {
    "trivy": ("aquasecurity/trivy", "0.75.0", {
        "x86_64": ("trivy_0.75.0_Linux-64bit.tar.gz", "c6e65abddb348e25f10549df887045629cf28cc72453cd1c63acb717316b3f3f"),
        "aarch64": ("trivy_0.75.0_Linux-ARM64.tar.gz", "a1ee9f6ffb7d112b64ff726a2a0717c21175c1114361391f4a132956751a13b3"),
    }),
    "gitleaks": ("gitleaks/gitleaks", "8.30.1", {
        "x86_64": ("gitleaks_8.30.1_linux_x64.tar.gz", "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb"),
        "aarch64": ("gitleaks_8.30.1_linux_arm64.tar.gz", "e4a487ee7ccd7d3a7f7ec08657610aa3606637dab924210b3aee62570fb4b080"),
    }),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("Only Linux runners are supported")
    args.directory.mkdir(parents=True, exist_ok=True)
    for tool, (repo, version, assets) in TOOLS.items():
        asset, checksum = assets[platform.machine()]
        url = f"https://github.com/{repo}/releases/download/v{version}/{asset}"
        with urllib.request.urlopen(url, timeout=120) as response:
            archive = response.read()
        if hashlib.sha256(archive).hexdigest() != checksum:
            raise RuntimeError(f"Checksum mismatch for {asset}")
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
            member = tar.getmember(tool)
            if not member.isfile():
                raise RuntimeError(f"Expected a regular binary for {tool}")
            destination = args.directory / tool
            destination.write_bytes(tar.extractfile(member).read())
            destination.chmod(0o755)
        print(f"Installed {tool} {version} with verified SHA-256")


if __name__ == "__main__":
    main()
