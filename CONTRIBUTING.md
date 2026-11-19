# Contributing

Read [AGENTS.md](AGENTS.md) and [architecture](docs/architecture.md). Keep the human and agent READMEs, skill, and MCP tool names aligned. Add behavior-level tests for substantive changes. Never commit tokens or cache contents.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --all
```

Install sop and run `RAAD_REQUIRE_SOP=1 cargo test --test examples` for integration changes. Run `python3 scripts/benchmark.py` when changing fetch or cache behavior. Tests must not mutate customer repositories; use temporary fixtures and local API mocks.

For native bindings, also run the [SDK build and test commands](sdks/README.md#build-and-verify). Test the wheel and npm tarball in clean consumers before changing installation instructions.

## Publish a release

Early releases stay on `0.1.x`, with patch increases by default. A minor increase requires an explicit maintainer decision once the project has users. Keep the major version at `0`; the automated release check currently rejects tags outside `v0.1.x`.

Keep versions aligned in the root and SDK Cargo manifests, Python `pyproject.toml`, npm `package.json` and lockfile, and MCP metadata. Update the changelog, release notes, and version-pinned installation URLs. Run `python3 scripts/prepare_release.py --tag vX.Y.Z` with Python 3.11+ to check package versions, using the version being released.

Push an annotated `vX.Y.Z` tag on the reviewed commit. The GitHub Release workflow runs core and native SDK checks, packages CLI/MCP downloads, combines the matching Node builds, generates `SHA256SUMS`, and publishes the assets. PyPI uses the `pypi` environment and the trusted publisher for `release.yml`. npm uses the `npm` environment and its configured trusted publisher when the repository variable `NPM_TRUSTED_PUBLISHING` is `true`. Initial npm publication requires an authenticated maintainer before configuring that publisher. No registry credentials belong in the repo.

Published release assets are immutable through this workflow. A failed upload can resume while the release is still a draft; a published release is not overwritten. Rerun only failed jobs after a partial failure, and inspect registry state before retrying a publication. Release installation jobs verify public downloads on both supported platforms.

The registry-installation jobs also install the exact version from PyPI and npm into clean apps on each supported platform. They wait up to ten minutes for the requested version to become visible because npm can process an accepted upload asynchronously. Checks use npm's installation metadata; a temporary holding version is not considered available. If processing exceeds that window, inspect the registry and rerun installation verification instead of republishing. Tests check native reads, TypeScript declarations, and Python-to-Node cache compatibility. Package registry setup is a separate authorization from browser sign-in; adding another destination requires an explicit maintainer request.
