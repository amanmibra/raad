# 🤙 raad

**Your repo is your database. Git is your infrastructure.**

Keep your app's prompts, policies, SOPs, and config in Git. raad lets you read them locally and propose changes through pull requests.

Your data gets version history, branches, and rollback from Git, plus reviews and CI from GitHub. Use raad from Rust, Python, or TypeScript, through the CLI, or as a local MCP server for agents.

## Use it in your app

The [Python and TypeScript SDKs](sdks/README.md) embed the Rust core. Your app needs Git, but no separate raad CLI or server. Prebuilt packages need no Rust compiler; see [supported platforms and direct downloads](docs/installation.md).

```sh
pip install raad
npm install @amanmibra/raad
```

Python:

```python
from raad import Repository

repo = Repository("sopops-dev/sop", root="spec")
repo.sync()
print(repo.read("agent.schema.json").json())
```

TypeScript:

```typescript
import { Repository } from '@amanmibra/raad';

const repo = await Repository.open({repository: 'sopops-dev/sop', root: 'spec'});
await repo.sync();
console.log((await repo.read('agent.schema.json')).json());
```

For sopops and other async hosts, follow the [application walkthrough](examples/sdk/README.md). Rust apps can use the [library directly](examples/local.rs). The 0.1.2 source build also provides [bulk snapshots and recoverable PR updates](sdks/README.md#sync-integrations-012-source-api) for sync integrations.

## Try it

Download the CLI from [GitHub Releases](https://github.com/amanmibra/raad/releases/latest) and follow the [installation guide](docs/installation.md#cli-and-local-mcp-server).

To build from source, install Rust 1.94+ and Git 2.45+, then run:

```sh
git clone https://github.com/amanmibra/raad.git
cd raad
cargo install --path . --locked

raad read examples/policies/refunds.md
```

Want to see how you'd use it? Start with the [examples](examples/README.md): app config, a refund policy, and shared agent instructions.

## Read from your repo

Keep data in a folder such as `sops/` alongside your application code. From your repo:

```sh
raad list sops/
raad read sops/procedures/refunds.md
```

No setup file or `raad init` needed. Reads return committed files; save and commit edits before reading them with raad.

You can also read a folder from GitHub:

```sh
raad list --repo sopops-dev/sop --root spec
raad read agent.schema.json --repo sopops-dev/sop --root spec
```

Public repos need no token. Private repos and writes support existing HTTPS Git credentials, `gh auth`, fine-grained PATs, and GitHub App installation tokens. See [authentication](docs/github.md).

## Propose a change

raad checks your change and opens a GitHub PR for review. Pass the commit you started from so raad can detect conflicting edits.

Replace `OWNER/REPO`, `FULL_SHA`, and the input file with your own:

```sh
raad propose procedures/refunds.md --repo OWNER/REPO --root sops \
  --file updated-refunds.md --base FULL_SHA --title 'Update refund procedure'
```

The PR doesn't change the data your app reads. Merge it, then sync. [Write options](docs/github.md) include multiple files and explicitly authorized direct commits.

## Give an agent access

Run from your project:

```sh
raad mcp --root sops
```

Access is read-only by default. You choose the folders an agent can read and whether it can propose PRs. See [setup for Codex, Claude Code, and Cursor](docs/mcp.md).

## Local reads and validation

The first GitHub read downloads a local copy tied to a commit. Later reads use that copy. Run `raad sync` with the same repo and root options to refresh it; `raad status` shows its age and any errors.

raad checks JSON and YAML by default. You can add schemas or a validator such as `sop validate`. If an update fails validation or GitHub is unavailable, raad keeps the last valid copy and marks it stale. A first-time sync still needs a connection. See [validation](docs/validation.md) and [cache behavior](docs/architecture.md).

**A folder limits what raad exposes, not everything Git caches.** Partial clone and sparse checkout can still retain unrelated objects or repository metadata. They're optimizations, not security boundaries. Use a dedicated data repo if unrelated content must stay out of the cache.

## More

- [Examples](examples/README.md) — walkthroughs you can run
- [Rust library examples](examples/local.rs) and [host integration](examples/sopops.rs)
- [Configuration](docs/configuration.md) and [agent guide](docs/agents/README.md)
- [Contributing and tests](CONTRIBUTING.md)
- [Verification and known limitations](docs/verification.md)

raad fits data that you read often and change through review. Use another store for sessions, event logs, counters, or transactions.

[Apache-2.0](LICENSE).
