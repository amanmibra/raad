# Verification record

Implementation verified on macOS arm64 with Rust 1.94.0, Cargo 1.94.0, and Git 2.45.2. This is the first raad v0.1 implementation from the supplied design v0.2, not an upgrade of existing software.

## Automated acceptance

Run formatting, Clippy with warnings denied, and all tests using the commands in the README. Tests use disposable local Git repositories and authenticated loopback GitHub API mocks. Coverage includes local discovery, mounted filtered fetch, immutable/retained reads, candidate validation, stale/error recovery, corruption, force pushes, OS sync locks, path and symlink boundaries, validation environments/timeouts, schema retrieval rejection, API optimistic concurrency, PR composition, rate limits, token-provider refresh/redaction, direct commits, CLI output, and real stdio MCP negotiation.

The released `sop` v0.1.1 macOS arm64 artifact was checksum-verified and used to validate the shipped SOP fixture. `RAAD_REQUIRE_SOP=1 cargo test --test examples` enforces that dependency; ordinary tests print a skip notice when it is absent. The runnable host pattern reuses a token provider without modifying sop or sopops.

## Live public read

A real filtered fetch/list of `sopops-dev/sop`, mounted at `spec`, returned four schema files at commit `7658e7bd6691641d06f567facb12ced8dd9ff9f6`. All subsequent reads came from the materialized snapshot. This exercises the production GitHub fetch adapter, including default-branch discovery.

## Synthetic monorepo

`python3 scripts/benchmark.py` creates a Git repository containing 64 MiB of incompressible unrelated blobs and two small mounted files. A local Git daemon is accessed through a counting TCP proxy. On the implementation machine:

| Measurement | Observed |
|---|---:|
| Unrelated working-tree blobs | 67,108,864 bytes |
| Server-to-client transfer | 2,887 bytes |
| Client-to-server transfer | 1,260 bytes |
| Cache files | 32,816 bytes |
| Cold mounted sync | 341 ms |
| Mean local read (100 iterations) | 181 µs |

These are synthetic local-network measurements, not a GitHub latency promise. The harness asserts transfer and cache remain below one quarter of unrelated content size. A separate test inspects Git packs and confirms an unrelated blob was not downloaded. Repository-wide metadata may still be cached; this does not establish a privacy boundary.

## Unverified or deliberately limited

- Live private repository access and live GitHub PR/direct commits were not performed. Authenticated mocks cover the adapters; token scope, installation grants, branch protections, and real provider behavior still need a deployment-specific acceptance run on an authorized disposable repository.
- Fine-grained PAT/App token strings share the same production adapter. Actual App token mint/refresh is delegated to the host; tests verify callback refresh and failure redaction.
- MCP stdio protocol and dispatcher were tested. Codex, Claude Code, and Cursor GUI registration were not automated here.
- Linux CI is configured, while Windows process-tree cleanup and filesystem permission behavior were not verified. Unix process-group cleanup is tested.
- History is retained validated snapshots. PRs always use new proposal branches; updating existing PRs is deferred. No LFS, symlink, or submodule support.
- Validation commands are trusted host executables, not sandboxed plugins. Streaming Git fetch can temporarily exceed cache budgets; use OS quotas for hard limits.
- Registry metadata and release workflows are included, but no crate, tag, release, or registry listing was published.

## Native SDK verification

Python and TypeScript/Node SDKs embed the core through PyO3 and Node-API. Each has ten integration tests covering scoped/pinned reads, binary data, explicit sync, invalid candidate recovery, token refresh/redaction, authenticated multi-file PRs, direct expected-head writes, read-only permissions, typed errors, schema/symlink rejection, event-loop responsiveness, and coexistence with host subprocess handling. The built Python stable-ABI wheel and npm tarball are installed into clean consumers; the Node consumer reads the cache created by Python without syncing. The Python source distribution is also rebuilt into a wheel. The stable-ABI wheel passes on Python 3.10 and 3.12; the Node package also runs on Node 20. Both native SDKs completed live public reads of sopops-dev/sop/spec at the commit recorded above.

SDK package tests and examples run locally on macOS arm64. The Native SDKs workflow builds and tests Linux x86-64 and macOS arm64, including Python 3.10/3.12 and Node 22. Additional architectures, Windows, musl Linux, and browser/edge runtimes are not verified. The SDK packages have not been published to PyPI or npm. Actual private GitHub credentials and live mutations remain subject to the limitations above.
