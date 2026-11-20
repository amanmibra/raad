# Architecture

raad has a reusable synchronous Rust core, one CLI binary, and small native binding crates for Python and Node.js. The native SDKs embed the core and need no raad executable; Git remains an external dependency. CLI/MCP dependencies are optional Cargo features, excluded from the SDK builds. The binary's `mcp` command uses the official Rust MCP SDK over stdio. Git operations use the installed Git CLI. Narrow GitHub REST requests create PRs; GraphQL provides atomic direct commits. There is no HTTP listener, database, daemon, or separate review engine.

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

## Application SDKs

The shared internal binding surface dispatches typed SDK operations to the same `Mount` methods used by Rust and the CLI. Python uses PyO3, stable-ABI wheels, and typed Python result objects; Node uses Node-API, promises, and TypeScript declarations. The small JSON boundary is in-process; there is no RPC service or subprocess invocation of raad. Python releases the GIL; Node executes work on its native worker pool. Async Python also moves native calls off the event loop.

Credentials are supplied per sync/write operation. Host token callbacks run in the language wrapper before native work; a temporary mount clone receives that token without changing cache identity or persisting credentials. Reads do not resolve a token or sync implicitly. Normal core write authorization still applies. Caches are interoperable across Rust, CLI, Python, and Node with the same mount and validation settings. See [SDK installation and runtime behavior](../sdks/README.md).

Subprocess timeouts poll individual child processes instead of registering a global SIGCHLD handler. This preserves the host runtime's child-process handling when the core is embedded in Node/libuv or Python/asyncio. Unix process-group cleanup and timeout/output bounds still apply.

## Host sync operations

The 0.1.2 source API adds candidate inspection without activation and a bulk validated snapshot read under one local lock. Atomic sync-and-pin protects a selected revision before retention pruning. Excluded path prefixes are part of mount cache identity; their blobs aren't fetched or validated, and writes to them are rejected. Tree listings and Git metadata may still include them.

Stable proposal branches use a persisted operation identity, an expected base and an expected previous machine head. Candidate validation finishes before remote writes. GraphQL `updateRefs.beforeOid` publishes with an atomic head check. Recovery verifies the operation fingerprint, parent, scoped contents and changed paths; unknown or human-edited branches fail. No diff causes no remote mutation. The host owns durable operation storage and retry scheduling. The existing CLI and MCP interfaces keep their current permission model.

Native integration tests use authenticated loopback GitHub APIs backed by actual Git objects. Live PAT/App permissions and branch protection need deployment-specific verification; a simulator can't prove those policies.
