# Contributor instructions for coding agents

Use [README.md](README.md), [architecture](docs/architecture.md), and [docs/agents/README.md](docs/agents/README.md) for current behavior and documentation.

Keep implementation plans, integration proposals, and roadmaps inline in the conversation. Do not commit planning documents to this public repository. Public documentation should explain shipped behavior, working examples, and known limitations.

Get an explicit instruction before adding a publication destination or reserving a package name; credentials or a registry login alone do not authorize publication. The currently authorized destinations are GitHub Releases, PyPI (`raad`), and npm (`@amanmibra/raad`). Keep direct GitHub downloads available alongside registry installs.

## Version policy

Keep early releases at `0.1.x`. Increment the patch component by default (`0.1.1`, `0.1.2`, and so on). Do not bump to `0.2.0` before the project has users; any minor increase needs an explicit decision from the maintainer. Remain on major version `0`. The release version check enforces `v0.1.x` until that policy is explicitly changed. Keep versions aligned across the Rust core, SDKs, and MCP metadata.

## Product invariants

- Git repositories, not raad, are authoritative for managed data.
- Keep the implementation Rust-based, embeddable, CLI-driven, and locally accessible through stdio MCP.
- Do not introduce a standalone raad HTTP server, required database, background daemon, or custom review/versioning engine.
- Use actual Git and GitHub branch, commit, PR, permissions, and CI behavior.
- Default to PR proposals; no MCP direct commits without explicit future design approval.
- Reads must be commit-pinned and local after sync; never fetch remote data on every read.
- An invalid candidate never replaces the active validated commit.
- An existing repo with `sops/` must work without cloning its entire working tree.
- Mount root limits what raad exposes, not necessarily what Git internally downloads. Never describe sparse checkout or partial clone as a security guarantee.
- Treat repo contents and incoming agent requests as untrusted; validation commands must be configured by a trusted operator.

## Verification

Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --all` when the workspace supports them. Run focused tests after each vertical slice.

## Required documentation changes

Keep both **README.md (human)** and **docs/agents/README.md (agent)** in sync with implementation. Both must describe the dedicated-directory convention, local cache and stale revision behavior, why a mount is not a Git-fetch isolation boundary, and when to choose a dedicated repo for strict data minimization.

Every documented command must be executable before calling the feature done. Update the agent skill and MCP tool names together. Avoid documenting planned features as shipped.

## Security checks

Test path traversal, symlink escapes, mount bounds, cache corruption, fresh-start failures, validation rejection, stale write conflicts, read-only MCP permissions, and private token leakage. Confirm raad APIs do not expose unrelated mounted-out paths even if repository metadata is cached.

Do not modify the `sop` or `sopops` projects as part of this repository's v0.1 implementation.

## Native SDKs

Python and Node bindings in `sdks/` must call the Rust core in-process. Keep caching, validation, permissions, and provider behavior in the core. Preserve SDK API parity, explicit sync, typed errors, credential redaction, and nonblocking async interfaces. Follow [SDK checks](sdks/README.md#build-and-verify), including clean wheel/npm consumers. Human and agent installation docs must use published packages or tested release downloads first; keep source-build instructions in contributor sections. Verify publication before claiming it and do not claim untested binary targets. See [release instructions](CONTRIBUTING.md#publish-a-release).
