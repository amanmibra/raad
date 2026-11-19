#!/usr/bin/env python3
"""Validate release versions and collect the tested artifacts with checksums."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-apple-darwin")


def combine_node_packages(packages, output):
    """Combine matching packages so the existing Node loader can select either binary."""
    merged = {}
    shared = None
    for package in packages:
        entries = {}
        with tarfile.open(package, "r:gz") as archive:
            for member in archive:
                parts = Path(member.name).parts
                if not parts or parts[0] != "package" or ".." in parts:
                    raise ValueError("invalid npm archive path")
                if member.isdir():
                    continue
                if not member.isfile() or member.name in entries:
                    raise ValueError("unsupported or duplicate npm archive entry")
                entries[member.name] = (archive.extractfile(member).read(), member.mode)
        common = {name: value for name, value in entries.items() if not name.endswith(".node")}
        if shared is not None and shared != common:
            raise ValueError("platform packages contain different JavaScript, types, or metadata")
        shared = common
        for name, value in entries.items():
            if name in merged and merged[name] != value:
                raise ValueError(f"conflicting npm entry: {name}")
            merged[name] = value
    native = {name for name in merged if name.endswith(".node")}
    if native != {"package/raad.linux-x64-gnu.node", "package/raad.darwin-arm64.node"}:
        raise ValueError("expected exactly the two tested Node binaries")
    with tarfile.open(output, "w:gz") as archive:
        for name, (data, mode) in sorted(merged.items()):
            member = tarfile.TarInfo(name)
            member.size, member.mode = len(data), mode
            archive.addfile(member, io.BytesIO(data))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--assets", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "dist/release")
    args = parser.parse_args()
    if not re.fullmatch(r"v0\.1\.\d+", args.tag):
        parser.error("only v0.1.x releases are allowed; another minor requires an explicit version-policy update")
    version = args.tag[1:]
    for path, section in (("Cargo.toml", "package"), ("sdks/python/Cargo.toml", "package"),
                          ("sdks/typescript/Cargo.toml", "package"), ("sdks/python/pyproject.toml", "project")):
        actual = tomllib.loads((ROOT / path).read_text())[section]["version"]
        if actual != version:
            parser.error(f"{path} has version {actual}, expected {version}")
    for path in ("sdks/typescript/package.json", "sdks/typescript/package-lock.json", "manifest.json", "server.json"):
        if json.loads((ROOT / path).read_text())["version"] != version:
            parser.error(f"{path} version does not match tag")
    if not args.assets:
        print(f"Release versions match {args.tag}")
        return
    expected = {f"raad-{target}.{extension}" for target in TARGETS for extension in ("tar.gz", "mcpb")}
    expected.update(f"raad-node-{version}-{target}.tgz" for target in TARGETS)
    wheels = list(args.assets.rglob(f"raad-{version}-cp310-abi3-*.whl"))
    if len(wheels) != 2 or not any(p.name.endswith("_arm64.whl") for p in wheels) or not any(p.name.endswith("_x86_64.whl") for p in wheels):
        parser.error("expected one macOS arm64 wheel and one Linux x86_64 wheel")
    expected.update(p.name for p in wheels)
    files = {}
    for path in args.assets.rglob("*"):
        if path.is_file():
            if path.name in files:
                parser.error(f"duplicate asset: {path.name}")
            files[path.name] = path
    if set(files) != expected:
        parser.error(f"asset mismatch: missing={sorted(expected - files.keys())}, extra={sorted(files.keys() - expected)}")
    args.output.mkdir(parents=True, exist_ok=True)
    if any(args.output.iterdir()):
        parser.error("output directory must be empty")
    checksums = []
    for name, path in sorted(files.items()):
        shutil.copyfile(path, args.output / name)
    combine_node_packages([files[f"raad-node-{version}-{target}.tgz"] for target in TARGETS],
                          args.output / f"amanmibra-raad-{version}.tgz")
    for path in sorted(args.output.iterdir()):
        checksums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
    (args.output / "SHA256SUMS").write_text("".join(checksums))
    print(f"Prepared {len(files) + 1} assets and SHA256SUMS for {args.tag}")


if __name__ == "__main__":
    main()
