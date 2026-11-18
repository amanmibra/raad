# Use raad in your app

This example reads the refund procedure in [sops/](../sopops/sops) from Python or TypeScript. You'll get the file text and the exact Git commit it came from.

The SDKs embed raad's Rust core. Your app needs Git, but it doesn't need the raad CLI or a raad server.

## Run the Python example

From the repository root, with Python 3.10+ and Rust 1.94+:

```sh
python -m pip install ./sdks/python
python examples/sdk/python_app.py
```

[python_app.py](python_app.py) uses the async interface that a Python app such as sopops can call. It syncs the source, reads at a fixed commit, and shows how to keep serving the last valid copy if a later sync fails.

## Run the TypeScript example

From the repository root, with Node.js 22.13+ and Rust 1.94+:

```sh
npm ci --prefix sdks/typescript
npm run build --prefix sdks/typescript
npm ci --prefix examples/sdk
npm start --prefix examples/sdk
```

[typescript_app.ts](typescript_app.ts) does the same thing with promises. Both examples use temporary caches for the demo; use persistent cache storage in production.

## Connect a GitHub folder

For Python:

```python
from raad import AsyncRepository

repo = await AsyncRepository.open(
    "your-team/your-app",
    root="sops",
    cache_dir="/var/cache/your-app/raad",
    token_provider=get_installation_token,
    allow_proposals=True,
)
await repo.sync()
file = await repo.read("procedures/refunds.md")
```

For TypeScript:

```typescript
const repo = await Repository.open({
  repository: 'your-team/your-app',
  root: 'sops',
  cacheDir: '/var/cache/your-app/raad',
  tokenProvider: getInstallationToken,
  allowProposals: true,
});
await repo.sync();
const file = await repo.read('procedures/refunds.md');
```

`get_installation_token` and `getInstallationToken` represent functions supplied by your app. They return its current GitHub App installation token and may be async. sopops can reuse its existing App installation; it doesn't need another one. Public reads can omit the callback.

## Propose an edit

For Python:

```python
from raad import Put

proposal = await repo.propose(
    [Put("procedures/refunds.md", updated_text)],
    expected_base=file.commit_sha,
    title="Update the refund procedure",
)
print(proposal.pull_request_url)
```

For TypeScript:

```typescript
const proposal = await repo.propose(
  [{operation: 'put', path: 'procedures/refunds.md', content: updatedText}],
  {expectedBase: file.commitSha, title: 'Update the refund procedure'},
);
console.log(proposal.pullRequestUrl);
```

These snippets use the file read above and your edited text. Opening a PR doesn't change the active copy. After review and merge, the host calls `sync()` to load the approved files. A stale base is rejected so you can re-read and review the edit.

## Fit this into sopops

Create one repository object for each connected source. Keep its cache on persistent storage. Have the host refresh it on startup, a schedule, or a GitHub webhook. Reads never refresh automatically.

raad supplies the source files and commit. `sop` still validates and compiles them. A published release should record the source commit and the compiled artifact. Sessions, transcripts, and other operational data stay in sopops's existing database.

Mounting a folder limits the SDK's reads, not everything Git caches. Use a dedicated data repository if unrelated repository objects or metadata must stay out of the cache.
