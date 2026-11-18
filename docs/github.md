# GitHub adapter

GitHub `owner/repo` is the first remote backend, public or private. Git fetches over HTTPS with a `blob:none` filter, no tags, and the selected branch. Public reads can be anonymous; `Credentials::Anonymous` guarantees that no helper or token provider is consulted by raad (Git's own trusted configuration still applies). The CLI uses `Credentials::Auto`.

Auto resolution: `RAAD_GITHUB_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, `git credential fill` for `https://github.com/owner/repo.git`, then `gh auth token --hostname github.com`. Noninteractive credential lookups have a five-second timeout. Existing HTTPS credential helpers and `gh auth` are supported. An SSH key by itself is not a GitHub REST credential. Install a suitable helper or supply a PAT/App token. Never place tokens in source URLs, CLI arguments, or configuration files.

The library accepts `Credentials::token(...)` or `Credentials::Provider(Arc<dyn TokenProvider>)`. Providers are called once per explicit sync or write operation. Hosts manage token refresh, installations, tenant selection, and least privilege; raad does not mint App JWTs or install another GitHub App. Callback failures are redacted. Debug output hides credentials. Git receives authorization through process environment configuration; tokens are not persisted by raad. The cache directory is restricted to its user on Unix.

Fine-grained PATs and GitHub App installation tokens need Contents read for private reads; proposals require Contents write and Pull requests write. Metadata access is also required by GitHub. Direct commits require Contents write and whatever branch rules GitHub enforces. Repository workflow changes may require additional provider permissions. Public repositories still require credentials for writes.

## PR writes

A request supplies the expected full base SHA, title, optional body, and put/delete operations. raad checks the current authoritative branch head, loads that retained validated base, applies mount-relative edits, enforces budgets and file/directory consistency, and validates the entire candidate.

The adapter reads the base commit's tree, creates base64 blobs, overlays mount-prefixed entries on the existing tree, and creates one commit with the expected parent. It rechecks the branch head, creates a unique `raad/<commit-prefix>` branch, and opens a PR targeting the authoritative branch. Untouched repository paths remain in the base tree; no unrestricted writable checkout is needed. PR creation never changes the active snapshot. Merging and future synchronization do.

No distributed transaction spans GitHub API calls. A rejected or interrupted proposal can leave unreachable blobs/commits or a proposal branch. PR creation failures report or deterministically identify the branch for manual recovery. Inspect GitHub before retrying; retries are not guaranteed idempotent and may encounter an existing ref. Existing PR update/reuse, automatic branch deletion, merging, approvals, and CI orchestration are outside v0.1.

## Direct commits and conflicts

Both library `allow_direct` and request `mode: direct` are required; CLI requires `--direct --allow-direct`. Direct writes use GraphQL `createCommitOnBranch` with `expectedHeadOid`, which atomically rejects a stale head. This avoids the race in separate REST compare/update-ref operations. v0.1 direct writes support regular nonexecutable files, including arbitrary binary bytes via base64. MCP rejects this mode unconditionally.

REST 401/403 permission errors, 404 inaccessible/missing resources, 409/422 conflicts, and rate limits are mapped to typed errors. HTTP error bodies are suppressed to avoid token leakage. API redirects are disabled so authorization is not forwarded to another host. Rate-limit errors carry retry/reset timing; the host should back off. Git transport failures report a generic unavailable/credential error because the Git transport does not expose structured REST status.

Authenticated mock tests exercise private/PAT-style headers, refreshed App-provider callbacks, multi-file PRs, stale bases, direct expected-head checks, validation rejection, permissions, rate limits, and redaction. A live public partial fetch was verified. Live private access and GitHub mutations were not performed; a deployment should run its own least-privilege acceptance test on a disposable authorized repository.
