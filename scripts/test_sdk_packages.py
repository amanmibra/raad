#!/usr/bin/env python3
"""Install built artifacts or registry packages in clean apps and verify shared-cache reads."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import venv


def run(args, **kwargs):
    return subprocess.check_output(args, stderr=subprocess.PIPE, text=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", type=Path)
    parser.add_argument("--npm-package", type=Path)
    parser.add_argument("--registry-version", help="install this exact version from PyPI and npm")
    args = parser.parse_args()
    if args.registry_version:
        if args.wheel or args.npm_package or not re.fullmatch(r"0\.1\.\d+", args.registry_version):
            parser.error("use --registry-version 0.1.x without artifact paths")
        python_install = ["--only-binary=:all:", f"raad=={args.registry_version}"]
        node_install = f"@amanmibra/raad@{args.registry_version}"
    else:
        if not args.wheel or not args.npm_package:
            parser.error("provide both --wheel and --npm-package, or --registry-version")
        python_install = ["--no-index", str(args.wheel.resolve())]
        node_install = str(args.npm_package.resolve())
    with tempfile.TemporaryDirectory(prefix="raad-installed-sdks-") as directory:
        base = Path(directory)
        source = base / "source"
        source.mkdir()
        def git(*arguments):
            return run(["git", "-C", str(source), *arguments])
        git("init", "-b", "main")
        git("config", "user.name", "SDK package test")
        git("config", "user.email", "sdk@example.invalid")
        git("config", "commit.gpgsign", "false")
        (source / "data").mkdir()
        (source / "data/config.json").write_text('{"enabled":true}\n')
        (source / "data/binary.bin").write_bytes(bytes([0, 255, 128]))
        git("add", ".")
        git("commit", "-qm", "fixture")
        expected_sha = git("rev-parse", "HEAD").strip()
        environment = dict(os.environ, RAAD_TEST_REPO=str(source), RAAD_TEST_CACHE=str(base / "cache"))
        python_env = base / "python-app"
        venv.EnvBuilder(with_pip=True).create(python_env)
        python = python_env / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        run([str(python), "-m", "pip", "install", *python_install])
        result = json.loads(run([str(python), "-c", '''
import json, os
import raad._native
from raad import Repository
repo = Repository(local_path=os.environ["RAAD_TEST_REPO"], root="data", cache_dir=os.environ["RAAD_TEST_CACHE"])
repo.sync()
file = repo.read("config.json")
assert repo.read("binary.bin").data == bytes([0, 255, 128])
print(json.dumps({"data":file.json(), "sha":file.commit_sha, "native":raad._native.__file__}))
'''], cwd=base, env=environment))
        assert result["data"] == {"enabled": True} and result["sha"] == expected_sha, result
        assert str(python_env) in result["native"], result
        print("PASS clean Python consumer: installed native wheel, read JSON and binary data")
        node_app = base / "node-app"
        node_app.mkdir()
        (node_app / "package.json").write_text('{"private":true,"type":"module"}')
        run(["npm", "install", "--no-audit", "--no-fund", node_install], cwd=node_app)
        (node_app / "app.ts").write_text('''
import {Repository, type Change, ConflictError} from '@amanmibra/raad';
async function check(repo: Repository) {
  const file = await repo.read('config.json');
  const enabled: boolean = file.json<{enabled: boolean}>().enabled;
  const bytes: Uint8Array = file.data;
  const change: Change = {operation:'put', path:'config.json', content:bytes};
  const proposal = await repo.propose([change], {expectedBase:file.commitSha, title:'Update'});
  const url: string | null = proposal.pullRequestUrl;
  return {enabled, url, error:new ConflictError('example').code};
}
''')
        compiler = Path(__file__).resolve().parents[1] / "sdks/typescript/node_modules/typescript/bin/tsc"
        run(["node", str(compiler), "--noEmit", "--strict", "--target", "ES2022",
             "--module", "NodeNext", "--moduleResolution", "NodeNext", "app.ts"], cwd=node_app)
        print("PASS clean TypeScript consumer: installed declarations type-check")
        result = json.loads(run(["node", "--input-type=module", "-e", '''
import {Repository} from '@amanmibra/raad';
const repo = await Repository.open({localPath:process.env.RAAD_TEST_REPO, root:'data', cacheDir:process.env.RAAD_TEST_CACHE});
// No sync: read the snapshot created by the separately installed Python SDK.
const file = await repo.read('config.json');
if (Buffer.from((await repo.read('binary.bin')).data).toString('hex') !== '00ff80') throw new Error('binary mismatch');
console.log(JSON.stringify({data:file.json(), sha:file.commitSha}));
'''], cwd=node_app, env=environment))
        assert result == {"data": {"enabled": True}, "sha": expected_sha}, result
        print("PASS clean Node consumer: installed native tarball, reused Python's cache without sync")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        # These commands have no tokens in their arguments; avoid dumping host environments.
        print(error.stderr, file=sys.stderr)
        raise SystemExit(error.returncode) from None
