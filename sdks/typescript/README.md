# raad for TypeScript and Node.js

Read Git-backed application data directly from your app. This package embeds the Rust core: it doesn't call the raad CLI or run a server. Requires Node.js 20+ and Git 2.45+. It's a server-side package, not a browser SDK.

## Install

```sh
npm install @amanmibra/raad
```

The package includes native binaries for Apple Silicon macOS and Linux x86-64 (glibc 2.35+). Installation selects the matching binary and needs no Rust compiler. See [installation](https://github.com/amanmibra/raad/blob/v0.1.0/docs/installation.md) for version-pinned commands and a direct GitHub download.

## Read your config

```typescript
import { Repository } from '@amanmibra/raad';

const repo = await Repository.open({localPath: '/path/to/my-app', root: 'config'});
await repo.sync();
const file = await repo.read('settings.json');
console.log(file.json());
console.log(file.commitSha);
```

Choose a local repository or set `repository: 'owner/repo'` for GitHub. `root` selects a folder within it. Call `sync()` before the first read; reads never fetch or validate a newer commit implicitly. All native operations run in worker threads and return promises.

See the [application walkthrough](https://github.com/amanmibra/raad/blob/v0.1.0/examples/sdk/README.md) for GitHub App credentials and PR proposals.

## GitHub and credentials

```typescript
const repo = await Repository.open({
  repository: 'your-team/your-app', root: 'sops', cacheDir: '/var/cache/your-app/raad',
  tokenProvider: getInstallationToken, allowProposals: true,
});
```

The callback is your host's function returning its current token, directly or as a promise. It runs for sync/write operations; reads don't need it. Without a callback, raad uses existing local credentials. Set `anonymous: true` for anonymous reads; don't combine it with a callback.

## Methods and results

| Method | Result |
|---|---|
| `sync(revision?)` | `Status`; historical sync retains without activating |
| `read(path, revision?)` | `ReadResult`: `data` bytes, UTF-8 `text`, `json<T>()`, `commitSha`, `provenance` |
| `list(prefix?, revision?)` | Paths within the folder |
| `search(query, {revision?, limit?}?)` | Matching paths; default limit 100 |
| `history(path?)` | Retained validated snapshots |
| `status()` | Active commit, timestamps, stale flag, last failure |
| `validate(revision?)` | `true`, or a typed validation error |
| `propose(changes, {expectedBase, title, body?})` | `Proposal` with PR URL |
| `commit(changes, {expectedBase, title, body?})` | Direct commit; requires `allowDirect: true` |
| `pin(sha)`, `unpin(sha)`, `prune()`, `clearCache()` | Manage the local cache |

All methods return promises. Status fields include `activeSha`, `lastSync`, `lastError`, `stale`, and `retained`. Read provenance includes repository, branch, mount folder, file path, SHA, and staleness. `json<T>()` supplies a TypeScript type; configure a schema to validate the actual data.

For writes, pass `{operation: 'put', path, content, executable?: boolean}` and/or `{operation: 'delete', path}`. Content accepts a string, Buffer, or Uint8Array. PRs require `allowProposals: true`. Direct writes additionally need `allowDirect: true` and a `commit()` call; the direct adapter supports regular nonexecutable files.

Other options include `branch`, `refreshSeconds`, `validation`, and `limits`. Validator commands use `timeoutSeconds`; limits use `fileBytes`, `mountBytes`, `maxFiles`, `responseBytes`, `cacheBytes`, and `retainedRevisions`. Schema documents themselves are passed unchanged. `githubApiUrl` is a trusted host override for API test servers; it doesn't change Git fetch URLs.

Catch `RaadError`, or a specific subclass: `CacheMissError`, `InvalidPathError`, `NotFoundError`, `ValidationError`, `ConflictError`, `PermissionDeniedError`, `RemoteUnavailableError`, `RateLimitError`, `CacheCorruptionError`, `LimitError`, or `ConfigurationError`. Each error has a stable `code`. Strict UTF-8 decoding and `json()` can also raise standard JavaScript errors.

## Packaging and runtime

The published package and the combined GitHub Release tarball contain both tested native binaries, the JavaScript API, and TypeScript declarations. They have no runtime npm dependencies. Platform-specific release tarballs contain only one binary; choose the matching OS/architecture if using those. Linux binaries depend on glibc 2.35+; musl/Alpine isn't supported by these artifacts. Git is the only required external executable. Native addons need a Node server runtime; they don't run in browsers or edge isolates.

## Build from source

From the raad repository root, with Rust 1.94+ and Node.js 22.13+ (or a newer supported LTS):

```sh
cd sdks/typescript
npm ci
npm run build
npm pack
```

Install the resulting tarball in your application with `npm install /path/to/amanmibra-raad-0.1.0.tgz`. A source-built tarball contains the native binary for the machine that built it. The release workflow combines the tested Linux and macOS builds for distribution.

The cache must persist for offline startup. Native work runs on Node's worker pool. A dropped promise doesn't stop an already-running sync or write. The same [cache privacy and validation boundaries](https://github.com/amanmibra/raad/blob/v0.1.0/sdks/README.md) apply as in Rust.
