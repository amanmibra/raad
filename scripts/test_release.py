#!/usr/bin/env python3
"""Download public release assets, verify checksums, and smoke-test the CLI."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import subprocess
import tarfile
import urllib.request


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "raad-release-verification"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--assets", type=Path, help="verify staged assets locally instead of public URLs")
    args = parser.parse_args()
    if not re.fullmatch(r"v\d+\.\d+\.\d+", args.tag):
        parser.error("expected a version tag")
    if platform.system() == "Darwin" and platform.machine() == "arm64":
        target, wheel_suffix = "aarch64-apple-darwin", "-macosx_11_0_arm64.whl"
    elif platform.system() == "Linux" and platform.machine() == "x86_64":
        target, wheel_suffix = "x86_64-unknown-linux-gnu", "-manylinux_2_34_x86_64.whl"
    else:
        parser.error("release verification supports macOS arm64 and Linux x86_64")
    base = f"https://github.com/amanmibra/raad/releases/download/{args.tag}"
    def download(name):
        return (args.assets / name).read_bytes() if args.assets else fetch(f"{base}/{name}")
    checksums = {}
    for line in download("SHA256SUMS").decode().splitlines():
        digest, name = line.split("  ", 1)
        if not re.fullmatch(r"[0-9a-f]{64}", digest) or Path(name).name != name or name in checksums:
            parser.error("invalid checksum manifest")
        checksums[name] = digest
    version = args.tag[1:]
    names = [f"raad-{version}-cp310-abi3{wheel_suffix}", f"amanmibra-raad-{version}.tgz",
             f"raad-{target}.tar.gz", f"raad-{target}.mcpb"]
    args.output.mkdir(parents=True, exist_ok=True)
    for name in names:
        data = download(name)
        if hashlib.sha256(data).hexdigest() != checksums.get(name):
            parser.error(f"checksum mismatch: {name}")
        (args.output / name).write_bytes(data)
    # Extract only the expected executable, not arbitrary archive paths.
    with tarfile.open(args.output / f"raad-{target}.tar.gz") as archive:
        member = archive.getmember("./raad")
        if not member.isfile():
            parser.error("CLI asset is not a regular file")
        executable = args.output / "raad"
        executable.write_bytes(archive.extractfile(member).read())
    executable.chmod(0o755)
    result = subprocess.check_output([str(executable.resolve()), "--version"], text=True).strip()
    if result != f"raad {version}":
        parser.error(f"unexpected CLI version: {result}")
    print(json.dumps({"tag": args.tag, "target": target, "verified_assets": names, "cli": result}))


if __name__ == "__main__":
    main()
