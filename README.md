# 🤙 raad

**Your repo is your database. Git is your infrastructure.**

Pronounced **rad**. Short for **repo as a db**.

raad v0.1 is a Rust library, CLI, and local stdio MCP server for read-heavy application data: instructions, SOPs, policies, prompts, and configuration. Git owns commits, branches, history, and rollback. GitHub owns PRs, review, permissions, branch protection, and CI. raad supplies scoped local reads, validated snapshots, and GitHub change proposals.

Use a dedicated directory such as `sops/` inside an existing repository, or a dedicated data repository. raad exposes paths relative to that directory. It does not need a database, daemon, or HTTP service. Event logs, counters, sessions, and transactional workloads belong elsewhere.

## Install and try it

Requires Rust 1.94+ and Git 2.45+. From this repository:

```sh
cargo install --path . --locked
raad --version

# Read committed example data; no raad init needed.
raad list examples/sopops/sops/
raad read examples/sopops/sops/procedures/refunds.md
raad search refunds --root examples/sopops/sops --json
raad history examples/sopops/sops/procedures/refunds.md --json
cargo run --example local

# Public GitHub read. First access syncs a cold cache.
raad list --repo sopops-dev/sop --root spec --json
raad read agent.schema.json --repo sopops-dev/sop --root spec
raad sync --repo sopops-dev/sop --root spec --json
raad status --repo sopops-dev/sop --root spec --json
```

Local reads discover the containing repository and current committed `HEAD`; uncommitted files are ignored. From a subdirectory, paths and `--root` are relative to that directory. Remote reads use a cached snapshot after first sync. Call `sync` explicitly to refresh. The library's `read`, `list`, `search`, and `history` never fetch.

Plain `read` prints exact file bytes to stdout and SHA/staleness to stderr. `--json` returns base64 bytes plus provenance. Other commands return JSON objects (pretty printed without `--json`). Search is bounded literal text and filename search. History lists retained validated snapshots, newest synchronization first; it is not the entire upstream Git log.

## Scoped folders and cache privacy

```text
my-app/
├── src/
└── sops/                 ← mount root
    ├── sop.yaml
    ├── agents/
    ├── instructions/
    └── procedures/
```

**A mount limits what raad exposes, not everything Git downloads or caches.** raad fetches a selected branch with `blob:none`, avoids a working-tree checkout, and requests mounted blobs for validation. Git still transfers repository-wide commit/tree metadata and may retain unrelated objects. Partial clone and sparse checkout are optimizations, **not security boundaries**. Use a **dedicated data repository for strict minimization**.

The cache defaults to `$XDG_CACHE_HOME/raad` or `~/.cache/raad`; override with `RAAD_CACHE_DIR` or `--cache-dir`. Keep it on persistent storage for warm offline startup. Failed syncs retain the active validated SHA and expose `last_error`, timestamps, and `stale`. Cold offline startup fails. Staleness is measured against `refresh_seconds` (60 by default); it is not proof of upstream freshness. Multiple replicas have independent caches.

Snapshots are commit-pinned and checksummed. `--revision` reads a retained full SHA. `sync --revision SHA` explicitly fetches/validates a historical SHA without activating it. `pin SHA`, `pin SHA --remove`, `prune`, and `clear-cache` manage the selected mount's cache. Eight snapshots are retained by default, plus pinned/active snapshots. Git objects persist until `clear-cache`; pruning removes old materialized snapshots. See [cache limits and recovery](docs/architecture.md).

## Credentials and writes

Public reads work anonymously. For private reads and public/private writes, raad resolves credentials in this order: `RAAD_GITHUB_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, Git credential helper, then `gh auth token`. Tokens never go in remote URLs or snapshot state. Existing HTTPS Git credentials and `gh auth` work; SSH-only credentials do not authorize GitHub REST writes. Use a fine-grained PAT or a host-refreshed GitHub App installation token in production. The Rust `TokenProvider` callback is called per remote operation so sopops can reuse its App installation.

Write proposals need Contents write and Pull requests write; private reads need Contents read. GitHub enforces repository access and branch policy. Using your repository and a SHA from `raad status`:

```sh
raad propose procedures/refunds.md --repo OWNER/REPO --root sops \
  --file updated-refunds.md --base FULL_SHA --title 'Update refund procedure'

# Deletion or multiple changes in one coherent commit:
raad propose procedures/obsolete.md --repo OWNER/REPO --root sops \
  --delete --base FULL_SHA --title 'Remove obsolete procedure'
raad propose --changes changes.json --repo OWNER/REPO --root sops \
  --base FULL_SHA --title 'Update policies'

# Optional direct commit: both flags are required. Never available through MCP.
raad propose config.json --repo OWNER/REPO --root sops --file updated.json \
  --base FULL_SHA --title 'Update config' --direct --allow-direct
```

These write examples require your repository, file, and actual full base SHA. `changes.json` is an array of `{"operation":"put","path":"file.md","data":{"content":"BASE64","executable":false}}` and/or `{"operation":"delete","path":"old.md"}`. raad validates the complete candidate mount before remote mutations, rejects stale bases, and creates a PR on a new `raad/…` branch. A proposal does not activate data or merge a PR. Direct writes use GitHub's atomic expected-head check and support regular nonexecutable files. [GitHub behavior and failure recovery](docs/github.md).

## Validation and Rust embedding

JSON/YAML syntax validation is enabled by default. Raw arbitrary files remain supported; trusted configuration can disable syntax checks. Optional inline JSON Schemas and trusted command validators add domain rules. Repository content cannot register executable validators. Commands run on mounted files in a temporary directory, with a timeout, bounded output, and a cleared environment. This is **not an OS security sandbox**; only configure trusted executables. [Validation](docs/validation.md).

```rust,no_run
use raad::{Credentials, Mount, MountConfig};
let config = MountConfig::github("acme/livekit-agent", "sops")?;
let mount = Mount::open(config, "/var/cache/my-app/raad", Credentials::Auto)?;
let status = mount.sync(None)?; // Host decides when to refresh.
let sha = status.active_sha.unwrap();
let file = mount.read("procedures/refunds.md", Some(&sha))?;
# Ok::<(), raad::Error>(())
```

The library is synchronous; async hosts should call it on a blocking worker. [Runnable embedding examples](examples/) include a refreshing token-provider pattern. [Trusted YAML configuration](docs/configuration.md) supports multiple mounts.

For the SOP fixture, install `sop` using [its official instructions](https://github.com/sopops-dev/sop#quickstart), then from this repository:

```sh
sop validate --dir examples/sopops/sops
raad sync --config examples/sopops/raad.yaml --mount sops --json
RAAD_REQUIRE_SOP=1 cargo test --test examples
```

`sop` owns SOP syntax, validation, and compilation. A sopops release pins the approved source SHA and a separately compiled artifact. raad does not store operational sessions or replace sopops's operational database. Neither reference project is modified.

## Local MCP

```sh
# Run inside the repository you want to expose (read-only by default):
raad mcp --root sops
# Codex:
codex mcp add raad -- raad mcp --root sops
# Claude Code:
claude mcp add --transport stdio raad -- raad mcp --root sops
```

Configure Cursor with the JSON in [docs/mcp.md](docs/mcp.md). Client installation commands depend on those clients being installed; the stdio workflow itself is integration tested. Remote access needs an operator-selected `--repo/--root` or explicit trusted `--config`. PR writes require an authorized mount (`allow_proposals: true` in trusted configuration or `--allow-proposals` for the selected mounts). MCP cannot select arbitrary repositories, execute validators from tool arguments, sync on read, or commit directly.

Tools: `raad_list_mounts`, `raad_list`, `raad_read`, `raad_search`, `raad_history`, `raad_status`, and authorized `raad_propose`. Read-only servers omit `raad_propose` from discovery. [Agent README](docs/agents/README.md) · [consumer skill](skills/raad/SKILL.md) · [MCP details](docs/mcp.md).

## Verify and contribute

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
python3 scripts/benchmark.py
```

Tests cover local and filtered fetches, validation/recovery, mount safety, force pushes, concurrent syncs, authenticated GitHub API request sequences, PR/direct writes, CLI commands, and a real stdio MCP exchange. `sop` tests run when installed; set `RAAD_REQUIRE_SOP=1` to require them. Live private GitHub reads and live mutations were not exercised during this implementation; they use the same adapters covered by fixtures and authenticated mock tests. See [verification evidence and limitations](docs/verification.md), [security](docs/security.md), [architecture](docs/architecture.md), and the original [design specification](DESIGN.md).

Apache-2.0. No hosted raad service or published crate is required to build from source.
