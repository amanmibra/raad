# Install raad

The SDKs and CLI are available from [GitHub Releases](https://github.com/amanmibra/raad/releases/tag/v0.1.1). Python and Node packages embed the Rust core; you don't need Rust, a separate raad CLI, or a server to use them.

All packages require **Git 2.45+**. Prebuilt downloads support Apple Silicon macOS 11+ and Linux x86-64. Python wheels require glibc 2.34+ on Linux; Node and CLI binaries require glibc 2.35+. Windows, Intel macOS, Linux arm64, and Alpine/musl need separate builds and aren't supported by these downloads.

## Python

Requires Python 3.10+:

```sh
python -m pip install raad==0.1.1
```

For a project managed by uv:

```sh
uv add raad==0.1.1
```

Try it:

```python
from raad import Repository

repo = Repository("sopops-dev/sop", root="spec")
repo.sync()
print(repo.read("agent.schema.json").json())
```

To install directly from GitHub instead of PyPI, choose your platform:

```sh
# Apple Silicon macOS
python -m pip install https://github.com/amanmibra/raad/releases/download/v0.1.1/raad-0.1.1-cp310-abi3-macosx_11_0_arm64.whl

# Linux x86-64, glibc 2.34+
python -m pip install https://github.com/amanmibra/raad/releases/download/v0.1.1/raad-0.1.1-cp310-abi3-manylinux_2_34_x86_64.whl
```

## TypeScript and Node.js

Requires Node.js 20+; use a supported Node LTS release:

```sh
npm install @amanmibra/raad@0.1.1
```

The package includes both supported native binaries and selects the matching one. It runs in Node servers, not browsers or edge isolates.

```typescript
import { Repository } from '@amanmibra/raad';

const repo = await Repository.open({repository: 'sopops-dev/sop', root: 'spec'});
await repo.sync();
console.log((await repo.read('agent.schema.json')).json());
```

To install directly from GitHub instead of npm:

```sh
npm install https://github.com/amanmibra/raad/releases/download/v0.1.1/amanmibra-raad-0.1.1.tgz
```

## CLI and local MCP server

Download the archive for your platform from the release page. For Apple Silicon macOS:

```sh
curl -fL https://github.com/amanmibra/raad/releases/download/v0.1.1/raad-aarch64-apple-darwin.tar.gz -o raad.tar.gz
tar -xzf raad.tar.gz ./raad
mkdir -p "$HOME/.local/bin"
install -m 755 raad "$HOME/.local/bin/raad"
```

For Linux x86-64, use `raad-x86_64-unknown-linux-gnu.tar.gz` in the download URL. Add `$HOME/.local/bin` to your shell's `PATH` if needed, then run:

```sh
raad --version
```

Each platform also has a `.mcpb` bundle for clients that support local binary bundles. The CLI includes `raad mcp`; see [MCP setup](mcp.md).

## Verify downloads

The release includes `SHA256SUMS` for its wheels, npm tarballs, CLI archives, and MCP bundles. Download it into the same directory as your selected assets, then run:

```sh
# macOS: replace the filename with your downloaded asset
shasum -a 256 raad.tar.gz
```

Compare the digest with the matching original asset filename in `SHA256SUMS`. On Linux, `sha256sum` computes the same digest. The manifest comes from the release and detects a changed download; it is not an independent signature.

Source builds remain available in the [Python](../sdks/python/README.md#build-from-source), [Node](../sdks/typescript/README.md#build-from-source), and [contributor](../CONTRIBUTING.md) guides. Source builds require Rust 1.94+.
