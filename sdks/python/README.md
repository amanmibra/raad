# raad for Python

Read Git-backed application data directly from Python. This package embeds the Rust core: it doesn't call the raad CLI or run a server. Requires Python 3.10+ and Git 2.45+.

## Install

```sh
python -m pip install raad
```

Prebuilt wheels need no Rust compiler. They support Apple Silicon macOS and Linux x86-64 with glibc 2.34+. See [installation](https://github.com/amanmibra/raad/blob/v0.1.1/docs/installation.md) for version-pinned commands, uv, and direct GitHub downloads.

## Read your config

```python
from raad import Repository

repo = Repository(local_path="/path/to/my-app", root="config")
repo.sync()
file = repo.read("settings.json")
print(file.json())
print(file.commit_sha)
```

Choose a local repository or set `repository="owner/repo"` for GitHub. `root` selects a folder within it. Call `sync()` before the first read; reads never fetch or validate a newer commit implicitly.

For FastAPI/sopops, use `AsyncRepository`. See the [application walkthrough](https://github.com/amanmibra/raad/blob/v0.1.1/examples/sdk/README.md).

## GitHub and credentials

```python
repo = Repository(
    "your-team/your-app", root="sops", cache_dir="/var/cache/your-app/raad",
    token_provider=get_installation_token, allow_proposals=True,
)
```

The callback is your host's function returning its current token. `AsyncRepository.open()` accepts sync or async callbacks. A sync `Repository` needs a sync callback. Callbacks are invoked for sync/write operations; reads don't need one. Without a callback, raad uses its existing credential resolution. Set `anonymous=True` for anonymous reads; don't combine it with a callback.

## Methods and results

| Method | Result |
|---|---|
| `sync(revision=None)` | `Status`; historical sync retains without activating |
| `read(path, revision=None)` | `ReadResult`: `data` bytes, UTF-8 `text`, `json()`, `commit_sha`, `provenance` |
| `list(prefix="", revision=None)` | File paths within the folder |
| `search(query, revision=None, limit=100)` | Matching paths |
| `history(path=None)` | Retained validated snapshots |
| `status()` | Active commit, timestamps, stale flag, last failure |
| `validate(revision=None)` | `True`, or a typed validation error |
| `propose(changes, expected_base=..., title=..., body="")` | `Proposal` with PR URL |
| `commit(changes, expected_base=..., title=..., body="")` | Direct commit; requires `allow_direct=True` |
| `pin(sha)`, `unpin(sha)`, `prune()`, `clear_cache()` | Manage the local cache |

All methods also exist on `AsyncRepository` and are awaited. `Status` has `active_sha`, `last_sync`, `last_error`, `stale`, `retained`, and other timestamps. A file's provenance includes repository, branch, mounted folder, path, SHA, and staleness.

For writes, pass `Put(path, text_or_bytes, executable=False)` and/or `Delete(path)` objects. PRs require `allow_proposals=True`. They validate the full candidate and reject a stale expected base. They don't activate data or merge the PR. Direct writes require `allow_direct=True` and a `commit()` call; only regular nonexecutable files are supported by the direct adapter.

Options include `branch`, `refresh_seconds`, `validation`, and `limits`. Validation and limits use the same snake_case keys as the [trusted configuration format](https://github.com/amanmibra/raad/blob/v0.1.1/docs/configuration.md). `github_api_url` is a trusted host override for API test servers; it doesn't change Git fetch URLs.

Catch `RaadError` for all raad failures, or a specific subclass: `CacheMissError`, `InvalidPathError`, `NotFoundError`, `ValidationError`, `ConflictError`, `PermissionDeniedError`, `RemoteUnavailableError`, `RateLimitError`, `CacheCorruptionError`, `LimitError`, or `ConfigurationError`. Each error has a stable `code`. Binary `text` decoding can raise Python's `UnicodeDecodeError`; JSON decoding can raise `JSONDecodeError`.

## Build from source

From the raad repository root, with Rust 1.94+:

```sh
python -m pip install ./sdks/python
```

To build a wheel:

```sh
python -m pip install maturin
python -m maturin build --release --manifest-path sdks/python/Cargo.toml --out dist/python
```

Run from the repository root. The wheel uses CPython's stable ABI for Python 3.10+. CI installs wheels in clean environments and runs the integration tests. Published wheels and their checksums are also attached to [GitHub Releases](https://github.com/amanmibra/raad/releases/latest).

The cache must persist for offline startup. Native methods release the GIL; async methods use worker threads. Cancelling a wait doesn't stop an already-running native operation. The same [cache privacy and validation boundaries](https://github.com/amanmibra/raad/blob/v0.1.1/sdks/README.md) apply as in Rust.
