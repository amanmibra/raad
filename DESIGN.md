> Original supplied implementation specification, preserved for provenance. raad v0.1 is implemented; current behavior and deliberate scope choices are in [README.md](README.md), [architecture](docs/architecture.md), and [verification](docs/verification.md). Use [configuration](docs/configuration.md) for the executable configuration format.

# raad: Technical design

**Design version:** 0.2  
**Target release:** raad v0.1  
**Status:** Implementation specification, not yet an implemented product  
**Date:** November 18, 2026

## 1. Summary

**raad (repo as a db)** is a Rust-based data access layer that lets applications and AI agents use Git repositories as the source of truth for read-heavy, infrequently edited data.

Use cases include agent instructions, SOPs, policies, prompts, and application configuration. The data changes at approximately the frequency of code changes, not events or transactions.

The value is not merely reading files from GitHub. **Git provides commits, branching, version history, and rollback. GitHub adds pull requests, approvals, branch protections, CI/CD, permissions, and webhooks. raad reuses those capabilities instead of rebuilding them inside each application.**

raad provides mount-scoped reads, caching, validation, history, and safe change proposals through a **reusable Rust core, CLI, and local MCP server**. GitHub is the first remote provider. No standalone raad HTTP server, database, background daemon, or hosted Git service is required.

### Goals

- Work against an existing application repository, a dedicated data repository, or a local Git repository.
- Treat a selected directory, such as `sops/`, as an application-visible data root.
- Provide low-latency, commit-addressed reads that do not call GitHub on every request.
- Preserve a last-known-good, validated revision when synchronization fails.
- Create pull requests by default, with explicitly authorized direct commits.
- Support JSON, YAML, Markdown, and arbitrary files, with built-in syntax checks and optional custom validation.
- Make the same functionality available through Rust, CLI, and MCP.
- Offer zero-config local reads and optional configuration for production or multiple mounts.
- Document both the value proposition and precise privacy boundaries for humans and agents.

### Non-goals for v0.1

- Relational queries, ACID transactions, event ingestion, or high-frequency updates.
- Custom branching, approval, review, user-management, or CI/CD engines.
- A raad REST service, hosted MCP endpoint, GitHub App onboarding UI, or Git hosting.
- GitLab or other remote provider implementations.
- A general-purpose indexed or semantic search engine.
- A guarantee that a Git object cache contains zero metadata from outside a mounted directory.

## 2. Architecture and responsibilities

```text
GitHub repository (or local Git repository)
  ├── Git: commits, branches, history
  └── GitHub: PRs, reviews, CI/CD, permissions
             |
        raad Git adapter
             |
       raad Rust core
       ├── scoped file access
       ├── cached Git objects + active validated SHA
       ├── built-in / custom validation
       └── revision-aware reads and proposals
             |
        +----+--------+
        |             |
      CLI         local MCP (stdio)
        |             |
      humans       AI agents

Embedded applications, including sopops, call the Rust core directly.
```

GitHub is the remote authoritative system. raad is an **adapter/facade and local materialized read view**, not another source of truth. For a managed application, the application decides when to refresh and which commit to serve.

Preferred initial workspace: one reusable core library and one binary crate exposing `raad` CLI and `raad mcp`. Split crates only where it measurably simplifies dependencies or testing. Do not build a large plugin framework.

Suggested libraries: `gix` or the Git CLI for Git object operations, `octocrab` or narrowly scoped REST requests for GitHub operations, `serde`, `tokio`, `clap`, and a maintained Rust MCP SDK. Select implementation libraries after verifying current APIs. No framework choice may change the public behavior described here.

## 3. Data convention and mounts

**Recommended convention: put raad-managed data under a dedicated directory in an existing repository.** The directory name is application-defined, not necessarily `raad/`.

Example, existing voice-agent project:

```text
livekit-agent/
├── src/
├── tests/
├── pyproject.toml
└── sops/
    ├── sop.yaml
    ├── agents/
    ├── instructions/
    └── procedures/
```

Example, general-purpose data:

```text
my-app/
├── src/
└── raad/
    ├── policies/
    ├── prompts/
    └── config/
```

Each **mount** is `(repository, branch, directory)` with an optional friendly name. All exposed file paths are relative to the mount root. A mount can also target the repository root, particularly for dedicated data repositories. Mounting does not relocate or change customer files.

**No configuration required:** local CLI usage discovers the current Git repository and defaults to the current committed revision. A remote read can explicitly specify the repository and directory. Configuration is optional for stable production mounts, validation, and multiple repositories.

Illustrative trusted configuration:

```yaml
version: 1
mounts:
  - name: sops
    repository: acme/livekit-agent
    branch: main
    path: sops/
    refresh_seconds: 60
validation:
  syntax: true
  commands:
    - name: sop
      command: ["sop", "validate", "--dir", "."]
      timeout_seconds: 30
```

The configuration that grants credentials or executes commands must be trusted deployment configuration. Never run validation commands supplied by untrusted repository content or an agent tool request.

## 4. Local cache: fast reads without a second database

raad caches Git object data and a small amount of state locally. It does **not** need SQLite, Postgres, a materialized copy of the whole repository, or a continuously running server.

For each mount, store:

- Repository identity and requested branch/directory.
- Local Git object cache (shared by mounts of the same repo where appropriate).
- Active, validated commit SHA and the last successful sync/validation times.
- Last validation error and optional in-memory path index.
- A bounded set of retained or pinned revisions needed for offline or historical reads.

**Read path:** resolve a path within the mount, read its blob from the active commit's tree, and return bytes plus provenance `{repo, branch, mount_path, commit_sha, file_path, stale}`. File reads never fetch the remote implicitly. Structured decoding is optional and must not modify source bytes.

**First sync:** fetch the configured branch and load only mounted-file content needed for the current validated snapshot. **Later sync:** fetch new Git objects, stage a candidate, validate, and atomically replace the active SHA only after success. Return a clear "no validated snapshot" error if first sync cannot complete. After a prior successful sync, network failures retain the last valid revision.

**Refresh:** explicit `raad sync` locally; a host application such as sopops schedules `sync()` (for example, every 60 seconds) or triggers it from a GitHub webhook received by that application. No raad daemon or webhook receiver is required. Avoid network fetches on read requests. Do not silently serve stale data without exposing staleness status and timestamps.

**Production reliability:** persist the cache if offline startup and last-known-good availability are required. Ephemeral container filesystems can lose the cache on restart; on a cold restart GitHub must be reachable. Multi-instance deployments each maintain their own active SHA unless the host coordinates them. For reproducible releases, the host pins a particular validated commit and stores the release's compiled artifact separately if applicable.

### 4.1 Directory scope, bandwidth, and data minimization

A mounted directory is the **API access boundary**, not automatically a network or disk isolation boundary.

Default Git-backed strategy for v0.1:

1. Fetch only the selected branch where possible, using partial-clone/object filtering such as `--filter=blob:none`.
2. **Do not perform an unrestricted checkout** of the customer repository. Prefer no worktree: enumerate paths from the commit tree, load only blobs inside the configured mount, and stage only those files for validation.
3. Materialize only the mounted directory into a temporary read-only validation view when a custom validator requires files on disk.
4. Never expose parent directories, other folders, unmounted blobs, or raw `.git` paths through raad APIs, CLI, or MCP.
5. Do not auto-initialize submodules or fetch Git LFS content in v0.1. Reject or clearly report unsupported objects. Reject symlinks that escape the mount, and block traversal paths and absolute paths.
6. Enforce size limits for individual files, total mount content, response payloads, and cache disk usage. Document cache pruning and pin retention behavior.

**Crucial limitation:** Git partial clone and sparse checkout are optimizations, not security boundaries. A partial clone can still transfer commit and tree metadata outside the mount; Git can lazily fetch additional blobs during operations. Sparse checkout can also materialize root-level or ancestor files in some modes. Even if raad only exposes `sops/`, its underlying Git object store is not guaranteed to contain only `sops/` data.

For strict data minimization, recommend a **dedicated data repository** today. A future **directory-only GitHub API backend** may enumerate a specific tree and fetch only selected blobs, avoiding a Git clone, though it will have separate API limits and historical-operation tradeoffs. Do not advertise the Git-cache backend as strict repository-content isolation.

Both `README.md` and `docs/agents/README.md` must state this distinction prominently, not only the architecture document.

## 5. Synchronization state machine

```text
UNINITIALIZED
   | fetch
   v
STAGING -> VALIDATING -> ACTIVE(validated SHA)
   | failure       | failure         |
   +--------------> ERROR <---------+
                      |
                serve previous ACTIVE,
                or fail if no ACTIVE exists
```

- Candidate revisions are derived from the current remote branch head, not webhook payload order.
- Avoid duplicate syncs with a per-mount lock. Activation is atomic and crash-safe.
- If a branch is force-pushed, compare the **current branch ref** rather than treating lexicographically or chronologically "newer" SHAs as authoritative.
- A delayed job must not overwrite a successfully activated newer branch state. Recheck the remote head or a monotonic sync generation before activation.
- Validate the complete mounted tree, including cross-file relationships.
- Never activate an invalid candidate. Keep the previous validated SHA, record the error, and expose it via `status`.
- Historical reads work when the requested revision is retained locally; otherwise require an explicit fetch with network access.
- Unchanged revisions should be cheap no-ops.

## 6. Reading and searching

Core operations are `read`, `list`, `search`, `history`, `status`, `sync`, `validate`, and `propose`. Support revision-pinned reads (`at` or `--revision`) and structured return metadata. `list` and `search` operate only inside the mount.

Search v0.1 is bounded path/name and literal text search over mounted text files. No database, semantic search, embeddings, or cross-repo query engine.

Errors should distinguish: missing file, invalid path, permission denied, unavailable remote, cache miss, stale expected SHA, validation failure, and rate limiting.

## 7. Git-native writes

**Default:** propose a change by creating or updating a GitHub pull request. **Optional:** trusted, explicitly authorized direct commits. The active read snapshot does not change when a PR is created; it changes only when the configured authoritative branch advances and passes validation.

A change request includes mount ID, put/delete operations with mount-relative paths, an expected base commit SHA, and a title. Mutations outside the mounted directory are rejected. Multiple file changes must share one coherent Git commit.

- Check the expected base SHA and reject stale writes with a conflict; never silently overwrite concurrent changes.
- Validate the candidate mounted tree before writing where possible; GitHub CI and branch protection remain the final review/deployment controls.
- PRs, approvals, checks, branch protection, review history, and deploy workflows are **GitHub features**. raad invokes and reports them, but does not reimplement them.
- Direct commits must use explicit authorization and respect provider branch protections. MCP direct commits are disabled by default.
- Reuse GitHub's Git/Contents APIs for remote branches, trees, commits, and PR operations instead of requiring a full writable checkout.

## 8. Validation

**Zero-config:** raw files remain readable. For recognized formats, raad can check JSON/YAML syntax and optionally parse Markdown YAML frontmatter. Plain Markdown is not rejected for lacking a schema. Invalid structured content rejects a candidate **when built-in validation is enabled**, which is the default for recognized formats on a production mount.

**Optional:** JSON Schema for structured files. **Custom:** trusted local command such as `sop validate`, run against a temporary mount-root directory with a timeout and output bound. Return actionable errors and preserve the previous active SHA.

Never run repository-provided scripts merely because a fetched revision contains a config file. An external validator can execute code, so it must be configured by a trusted operator and isolated appropriately for the deployment. Nonzero exit code fails validation; avoid forwarding provider credentials or unrelated repository paths to validators.

`raad` knows nothing about SOP structure or how to compile prompts. `sop` owns format, linting, validation, and compilation.

## 9. Authentication and GitHub API

| Activity | Auth needed? |
|---|---|
| Read public repository | No (rate limits apply) |
| Read private repository | Yes |
| Push to any remote repository, open PR, or direct commit | Yes, with appropriate permissions |
| Read local Git objects | No GitHub credential |

Local development: Git credential helper and `gh auth` where available. Simple private automation: fine-grained PAT. Production/multi-tenant applications: GitHub App installation tokens refreshed by the host application's token provider.

**For sopops:** reuse sopops's existing GitHub App installation and provide a token callback to raad. Do not make customers install a second GitHub App. For read-only App access, Contents read is sufficient; write proposals require Contents write and Pull requests write (plus GitHub's required metadata access).

Never log, expose through MCP, or persist plaintext access tokens in cache state. Respect GitHub primary and secondary rate limits, use conditional requests when applicable, and back off rather than polling aggressively.

## 10. CLI and library (proposed v0.1 contract)

All examples are **target interfaces for Codex to implement**, not claims that raad already ships.

```bash
# Zero-config local committed reads
cd livekit-agent
raad list sops/
raad read sops/procedures/refunds.md
raad history sops/procedures/refunds.md

# Remote, mounted directory
raad read procedures/refunds.md --repo acme/livekit-agent --root sops/
raad sync --repo acme/livekit-agent --root sops/
raad status --repo acme/livekit-agent --root sops/

# Safe write through PR (must include an expected revision)
raad propose procedures/refunds.md --repo acme/livekit-agent \
  --root sops/ --file ./updated-refunds.md \
  --base <commit-sha> --title "Update refunds procedure"

# MCP
raad mcp
```

Use consistent CLI flags, `--json` output, bounded results, and meaningful exit codes. `raad init` is optional convenience, not a prerequisite for read operations. The Rust public API must express the same mount, revision, validation, and proposal semantics.

## 11. MCP and agent distribution

Ship a **local stdio MCP server** in the same `raad` binary. No dedicated MCP HTTP service for v0.1. MCP tools:

- `raad_list_mounts`: discover only configured/authorized mounts.
- `raad_list`: list scoped paths.
- `raad_read`: read file, revision, and provenance.
- `raad_search`: bounded literal/path search.
- `raad_history`: inspect available file revisions.
- `raad_status`: active SHA, sync time, validation errors, staleness.
- `raad_propose`: make a PR after explicit scoped authorization.

MCP starts read-only. If launched in a local project, it may discover that project as the default allowed mount. Remote mounts and write access require explicit trust/allowlisting. Do not let an agent select arbitrary private repositories merely because its host has broad GitHub credentials. Treat fetched files as untrusted data, never as instructions to execute.

Provide tested installation examples for Codex, Claude Code, and Cursor. Include compatible MCP registry/marketplace metadata where local stdio servers are supported; hosted/remote MCP and OAuth flow are deferred. Ship a consumer agent skill at `skills/raad/SKILL.md` and `docs/agents/README.md`.

## 12. sopops integration

GitHub is authoritative for `sops/` files. raad mounts the directory and supplies a validated commit-addressed view. `sop` validates, formats, and compiles it. sopops serves compiled instructions and owns operational data in Postgres (sessions, transcripts, checks, insights, users, and runtime state).

A customer can connect either an existing LiveKit/Vapi application repo with `sops/` or a dedicated SOP repo. An onboarding agent can propose the initial `sops/` directory with a PR, preserving existing prompts. Future SOP edits from the sopops UI become Git proposals, not a separate authoritative DB state. Publishing is separate from merging: a sopops release pins the approved source SHA plus compiled artifact.

Do **not** modify sopops as part of raad v0.1. Its existing GitHub integration is a reference and a later migration candidate.

## 13. Docs as product surface

README materials must describe **why Git-backed data is useful** as well as **how it works**.

- `README.md` (human): value proposition, best-fit data, Git/GitHub capabilities inherited, quickstart, CLI and MCP examples, when not to use raad, auth, mount directory convention, explicit cache/privacy caveat, and links to deeper docs.
- `docs/agents/README.md` (agent): when to use raad, tool semantics, prompt examples, provenance, safe PR workflows, mount boundaries, credential handling, untrusted repo contents, and **explicit cache/privacy caveat**.
- `AGENTS.md` (coding agents contributing to raad): build and test commands, architectural invariants, security constraints, documentation requirements, and stepwise implementation rules.
- `skills/raad/SKILL.md`: installable consumer skill, aligned with MCP tools.
- `docs/architecture.md`, `docs/configuration.md`, `docs/validation.md`, `docs/github.md`, `docs/mcp.md`, `docs/security.md`: deeper docs; generate and keep accurate as implementation is completed.
- Working examples for SOP instructions, application config, and policy data.

Use direct, skimmable, sentence-case technical documentation. Avoid implying GitHub PRs or CI are built into raw Git; distinguish Git from hosting provider features. Do not claim zero network/disk leakage outside mount scope.

## 14. Security and reliability

Enforce mount-relative path normalization and reject traversal, symlink escapes, submodules, repository-supplied validator commands, unbounded files, and oversized responses. Credentials stay outside snapshots. MCP writes require explicit enablement, authorized mounts, and user review through PRs. Use safe process execution (argument arrays, no shell interpolation by default), subprocess timeouts, and filesystem isolation. Cache state must be atomic, crash-safe, and protected from other local users where private repository content is cached. Define retention limits and delete/clear-cache commands.

Do not promise that branch protections or GitHub access permissions prevent a host process with raw Git credentials from reading other objects it has already fetched. The mounted-path restriction is an application interface boundary, not a substitute for repository-level authorization.

## 15. Tests and acceptance

Use temporary local Git fixtures and mocked GitHub APIs. Test: local auto-discovery; remote mounts; reads at immutable SHAs; fetch/candidate/validate/activate; stale reads; invalid candidates; cold-start failure; interrupted sync; force pushes; optimistic concurrency; PR creation; rate limits; path traversal and symlink escapes; cache minimization; validation sandbox boundaries; read-only MCP; authorized proposals; and content outside the mount never being **exposed by raad**.

Explicit tests must demonstrate that `sops/` is the only materialized validation directory, while acknowledging that the Git object store may contain repository-wide metadata. Measure network bytes, disk usage, and cold-sync time against a synthetic large monorepo.

v0.1 is complete when users can mount a public or private repo, read locally at a validated SHA, survive GitHub outages after a warm cache, reject invalid updates, propose a PR, use the CLI and local MCP, and run the sop validation example, with human and agent docs that state the cache limitations accurately.

## 16. Delivery plan for Codex

1. Create `DESIGN.md`, `README.md`, `docs/agents/README.md`, `AGENTS.md`, and the consumer skill. Preserve these contracts.
2. Implement read-only local Git core with zero-config paths and tests.
3. Implement scoped GitHub partial fetch without unrestricted worktree checkout, cache state, revision-pinned reads, and validation.
4. Implement CLI, status, auth resolution, and sync orchestration.
5. Implement GitHub PR proposals, optimistic concurrency, and optional direct commits.
6. Implement stdio MCP with read-only defaults and scoped proposals.
7. Add sopops example, integration tests, CI, release artifacts, and verified docs.

Prefer the simplest implementation that preserves correctness, data minimization, and clear provenance. Do not build a standalone server. Do not stop at scaffolding.

## Reference documentation

- [Git partial clone](https://git-scm.com/docs/partial-clone)
- [Git sparse checkout](https://git-scm.com/docs/git-sparse-checkout)
- [GitHub Git Trees API](https://docs.github.com/en/rest/git/trees)
- [GitHub Contents API](https://docs.github.com/en/rest/repos/contents)
- [GitHub REST API limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api)
- [sop format](https://github.com/sopops-dev/sop/blob/main/FORMAT.md)
- [Existing sopops Git integration](https://github.com/sopops-dev/sopops/blob/main/docs/git.md)
