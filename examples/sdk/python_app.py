"""Run from this checkout after installing sdks/python. No raad CLI needed."""
import asyncio
import json
import os
from pathlib import Path
import sys
import tempfile

from raad import AsyncRepository, RaadError


async def main():
    source = Path(__file__).resolve().parents[2]
    # The demo cleans up its cache. Give a production app a persistent cache_dir.
    with tempfile.TemporaryDirectory(prefix="raad-python-app-") as cache:
        remote = os.environ.get("RAAD_EXAMPLE_REPO")
        options = ({"repository": remote, "root": os.environ.get("RAAD_EXAMPLE_ROOT", "sops")}
                   if remote else {"local_path": source, "root": "examples/sopops/sops"})
        # A host such as sopops can supply its existing installation-token callback here.
        provider = (lambda: os.environ["RAAD_GITHUB_TOKEN"]) if "RAAD_GITHUB_TOKEN" in os.environ else None
        repo = await AsyncRepository.open(**options, cache_dir=cache, token_provider=provider)
        try:
            await repo.sync()
        except RaadError as error:
            if (await repo.status()).active_sha is None:
                raise
            print(f"Using the last valid copy: {error}", file=sys.stderr)
        status = await repo.status()
        file = await repo.read("procedures/refunds.md", revision=status.active_sha)
        print(json.dumps({"commit": file.commit_sha, "stale": status.stale, "text": file.text}, indent=2))


if __name__ == "__main__":
    asyncio.run(main())
