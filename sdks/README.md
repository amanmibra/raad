# Application SDKs

Use raad inside your application without running a separate service. Python and Node.js packages embed the existing Rust core; they don't call the raad CLI. Git 2.45+ is required.

| Your app | Package | Start here |
|---|---|---|
| Python, including FastAPI/sopops | `raad` | [Python guide](python/README.md) |
| Node.js or TypeScript backend | `@amanmibra/raad` | [TypeScript guide](typescript/README.md) |
| Rust | `raad` crate | [Rust example](../examples/local.rs) |

The Python and npm packages are version 0.1.0 source packages in this repository. They haven't been published to package registries. Source builds require Rust 1.94+; a matching prebuilt wheel or npm tarball doesn't.

Rust applications can depend on the same core without CLI/MCP dependencies:

```toml
[dependencies]
raad = { git = "https://github.com/amanmibra/raad.git", default-features = false }
```

Start with the [application walkthrough](../examples/sdk/README.md). It includes local examples, GitHub App credentials, and PR proposals.

## Shared behavior

- Open a repository and select a folder; call `sync()` before reading it.
- Reads, lists, search, history, status, and validation use the cache without fetching.
- Read results include bytes, commit provenance, and staleness. Pass a retained full SHA to keep multiple reads at the same commit.
- Invalid candidates don't replace the last valid copy. A failed first sync leaves no copy to serve.
- Writes need `allow_proposals`/`allowProposals` or `allow_direct`/`allowDirect`, plus an explicit proposal or commit call. Supply the expected base SHA.
- Token callbacks run only for explicit sync/write calls. They can reuse the host's existing GitHub App token cache. Missing callbacks use existing local credentials.
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
