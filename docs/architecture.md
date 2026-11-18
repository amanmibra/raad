# Architecture

raad is one Rust crate with a reusable synchronous library and one CLI binary. The binary's `mcp` command uses the official Rust MCP SDK over stdio. Git operations use the installed Git CLI. Narrow GitHub REST requests create PRs; GraphQL provides atomic direct commits. There is no HTTP listener, database, daemon, or separate review engine.

## Read and sync model

`MountConfig` identifies a local repository or GitHub `owner/repo`, an optional branch (local `HEAD` or remote default), and a tree-relative mount root. Local discovery maps the current directory back to the repository root. Reads expose only committed bytes, not the working tree.

A remote cache contains a bare Git repository, no checkout. Fetch uses a selected branch, no tags, and `blob:none`. Sync enumerates the mounted Git tree, explicitly obtains its blobs, rejects unsupported objects and limits violations, and validates the complete candidate. Only mounted files are materialized for command validators. No submodules or Git LFS downloads are initialized.

Validated files are stored as a base64 snapshot keyed by full commit SHA. SHA-256 checksums in atomic state detect snapshot corruption. This duplicated materialization makes reads independent of Git lazy fetching, credentials, connectivity, and branch movement. Reads load only local files and include commit provenance. This implementation favors simple bounded snapshots over a database or index.

An OS file lock serializes sync, writes, pruning, pin changes, and clearing for a mount. Shared locks protect snapshot reads from pruning. Temporary files are flushed, atomically renamed, and the directory flushed before activation. Snapshots precede state; pruning commits state before deleting old snapshots. An interrupted sync may leave an unreferenced snapshot/object but cannot intentionally activate a partially written candidate. Unexpected cache corruption is reported, rather than silently accepted.

Remote sync rechecks the current branch ref after validation. Branch force pushes are accepted after validation, regardless of ancestry. A moving branch returns a conflict for a later retry. The per-mount lock prevents an older concurrent job from overwriting a completed newer sync. Already validated, intact SHAs avoid re-running validators; changing validation policy or content limits creates a different cache namespace.

A failed candidate records `last_error` and retains the active SHA. Reads report stale after a failure or the refresh interval. Historical fetches validate and retain without activating. Their success does not clear a failed authoritative-branch sync. The host schedules refreshes, coordinates replicas, and decides whether stale data is acceptable. Persistent storage is required for warm offline startup.

## Retention, limits, and recovery

Default budgets: 2 MiB per file, 32 MiB raw mount bytes, 10,000 files, 4 MiB response, 512 MiB cache, and eight retained snapshots plus active/pinned snapshots. Configure with `limits`. Custom validator output is capped at 64 KiB; timeout defaults to 30 seconds (clamped to 1–300). Git commands have a 120-second timeout. GitHub requests have a 60-second timeout and 1 MiB response cap. MCP transport uses the SDK's bounded line parser; tool responses are additionally limited to 4 MiB. Read responses can reach their budget before the raw file budget because bytes are base64 encoded.

Cache budgets are checked before/after fetch and before writing snapshots. They are rejection thresholds, not hard streaming disk/network quotas: Git may transfer a blob before its size is known, or temporarily exceed the disk budget during fetch. Hosts needing strict resource confinement should supply filesystem/process limits. Partial-clone support depends on the remote; a server may ignore filtering. raad never claims a mount is a Git cache isolation boundary. A dedicated data repo is the solution for strict minimization.

`raad prune` removes unpinned materialized snapshots beyond retention; it does not garbage-collect Git objects. `raad clear-cache` clears this mount's state, snapshots, and bare cache while retaining the lock file. It does not touch the source repository. Orphan snapshots from interrupted runs are removed by clear-cache. There is no automatic Git GC that might trigger unrelated lazy fetches. Pinned snapshots count against the disk budget.

If a snapshot checksum fails, reads fail visibly. Explicit `sync` can reconstruct it from source objects; malformed state requires `clear-cache` and a successful sync. Cache state is private local state, not an adversarially signed artifact. A process running as the same OS user can alter it; raad does not protect against a hostile host account.

## Deliberate v0.1 choices

History means retained validated snapshots, not every historical Git commit. Fetch a full SHA explicitly to make another revision available. There is no automatic background refresh, schema discovery, repository-supplied command execution, PR merging, semantic search, hosted OAuth, or remote MCP transport. Each proposal creates a new branch/PR; updating an existing PR is deferred. Rate limits are returned with retry timing; the host controls retry/backoff.
