# Application SDKs

Use raad inside your application without running a separate service. Python and Node.js packages embed the existing Rust core; they don't call the raad CLI. Git 2.45+ is required.

| Your app | Package | Start here |
|---|---|---|
| Python, including FastAPI/sopops | `raad` | [Python guide](python/README.md) |
| Node.js or TypeScript backend | `@amanmibra/raad` | [TypeScript guide](typescript/README.md) |
| Rust | `raad` crate | [Rust example](../examples/local.rs) |

Install the SDK for your application:

```sh
python -m pip install raad
npm install @amanmibra/raad
```

Prebuilt packages support Apple Silicon macOS and Linux x86-64. See [installation](../docs/installation.md) for runtime requirements, version-pinned commands, and direct GitHub Release downloads. Rust is only needed to build from source.

Rust applications can depend on the same core without CLI/MCP dependencies. The Rust crate is not published to crates.io:

```toml
[dependencies]
raad = { git = "https://github.com/amanmibra/raad.git", tag = "v0.1.1", default-features = false }
```

Start with the [application walkthrough](../examples/sdk/README.md). It includes local examples, GitHub App credentials, and PR proposals.

## Shared behavior

- Open a repository and select a folder; call `sync()` before reading it.
- Reads, lists, search, history, status, and validation use the cache without fetching.
- Read results include bytes, commit provenance, and staleness. Pass a retained full SHA to keep multiple reads at the same commit.
- Invalid candidates don't replace the last valid copy. A failed first sync leaves no copy to serve.
- Writes need `allow_proposals`/`allowProposals` or `allow_direct`/`allowDirect`, plus an explicit proposal or commit call. Supply the expected base SHA.
- Token callbacks run only for explicit sync/write calls. They can reuse the host's existing GitHub App token cache. Missing callbacks use existing local credentials. A failed callback marks sync status stale and preserves the prior snapshot.
- Validation settings are trusted host configuration; fetched files can't register commands. [Validation details](../docs/validation.md).

A mounted folder limits what raad exposes, not all Git downloads or metadata. Use a dedicated data repo for strict minimization. [Cache and privacy](../docs/architecture.md).

Python offers sync and async interfaces. Node methods return promises. Native work runs outside the Python GIL/Node event loop. Cancelling an async wait doesn't cancel an in-flight native sync or write; it may still finish. Hosts should track operations before retrying.

## Build and verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --all

python -m pip install ./sdks/python
python -m unittest discover -s sdks/python/tests -v

npm ci --prefix sdks/typescript
npm run build --prefix sdks/typescript
npm test --prefix sdks/typescript
```

Tests use temporary Git repositories and authenticated mock GitHub APIs. They don't mutate customer repositories. CI builds and installs artifacts for Linux x86-64 (Ubuntu 22.04/glibc 2.35 baseline) and macOS arm64; other targets aren't claimed as verified.

## Sync integrations (0.1.2 source API)

These additions are available from this checkout. Published 0.1.1 packages don't include them yet.

| Python / TypeScript | Use |
|---|---|
| `inspect_candidate()` / `inspectCandidate()` | Fetch bounded raw files, SHA, folder existence and validation issues without activating the candidate. Useful for import previews and repairs. |
| `snapshot(sha)` | Load every validated file in one local read. Returns bytes and executable paths. The normal response limit applies. |
| `sync(pin=True)` / `sync(undefined, {pin: true})` | Sync and pin under one cache lock. Historical sync still doesn't activate. |
| `excluded_paths=["build"]` / `excludedPaths: ['build']` | Skip paths before fetching blobs or validation. Exclusions have a separate cache identity and reject writes to those paths. |
| `upsert_proposal(...)` / `upsertProposal(...)` | Create or update a stable PR branch with an expected source SHA, expected proposal head and persistent operation ID. |
| `proposal_status(number)` / `proposalStatus(number)` | Read an open, closed or merged PR in the mounted repository. |

Persist the operation ID and exact request before sending a proposal. Retry that same request after an uncertain result. raad checks its commit marker, single parent, mounted contents and changed paths before recovering it. Use the returned commit as `expected_head` / `expectedHead` for the next edit. An unrecognized branch or a human edit raises a conflict. PR creation carries an operation footer, and updates record the associated PR number in the commit. Keep those markers for recovery. A closed PR stays closed when recovering its operation; a new operation can create another PR.

`StaleBaseError` (a `ConflictError` subtype) means the source moved before this operation published a ref; a host can clear that rejected pending request, sync and try a new operation. Candidate validation rejections retain their usual error types and also set `error.write_rejected` (Python) / `error.writeRejected` (TypeScript). This flag proves no ref was published by that operation. Clear only explicitly rejected pending requests; other failures may need recovery. An operation whose ref was already published can finish opening its PR even if the source has since advanced; it never rewrites that source branch.

A request matching the base returns `unchanged` without remote mutations. It may report an existing PR; it doesn't close it. Other results are `created` or `updated`, with proposal details and PR identity. Branch updates use GitHub's [atomic expected-OID check](https://docs.github.com/en/graphql/reference/git#updaterefs). Branch protection and token permissions still apply.

`propose` and `upsert_proposal` can repair an invalid or missing mounted folder, provided the complete resulting candidate validates. Inspection doesn't make those raw files readable as a validated snapshot. Token callbacks also run for inspection, upserts and PR status, never for local snapshots. `UnsupportedFeatureError` identifies unsupported Git tree layouts and is a subtype of `ValidationError` in both SDKs. MCP tools and write permissions are unchanged.
