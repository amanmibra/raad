#!/usr/bin/env python3
"""Package an existing raad binary as a platform-specific local MCPB bundle."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import zipfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=Path, default=ROOT / "target/release/raad")
parser.add_argument("--output", type=Path, default=ROOT / "dist/raad.mcpb")
args = parser.parse_args()
manifest = json.loads((ROOT / "manifest.json").read_text())
manifest["compatibility"] = {"platforms": [{"Darwin": "darwin", "Linux": "linux", "Windows": "win32"}[platform.system()]]}
if platform.system() == "Windows":
    manifest["server"]["entry_point"] += ".exe"
    manifest["server"]["mcp_config"]["command"] += ".exe"
args.output.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(args.output, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
    bundle.writestr("manifest.json", json.dumps(manifest, indent=2))
    bundle.write(args.binary, manifest["server"]["entry_point"])
    bundle.write(ROOT / "LICENSE", "LICENSE")
    bundle.write(ROOT / "README.md", "README.md")
checksum = hashlib.sha256(args.output.read_bytes()).hexdigest()
args.output.with_suffix(args.output.suffix + ".sha256").write_text(f"{checksum}  {args.output.name}\n")
print(args.output)
