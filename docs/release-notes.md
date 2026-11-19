# 🤙 raad v0.1.0

Read application data from Git at validated, commit-pinned revisions. Use the Rust library, Python SDK, Node/TypeScript SDK, CLI, or local stdio MCP server. Propose changes through GitHub PRs or explicitly authorized direct commits.

## Install

```sh
python -m pip install raad==0.1.0
npm install @amanmibra/raad@0.1.0
```

SDK installs need Git 2.45+ but no Rust compiler. Python requires 3.10+; Node requires 20+. Prebuilt packages support Apple Silicon macOS 11+ and Linux x86-64 (Python: glibc 2.34+; Node/CLI: glibc 2.35+).

For direct GitHub downloads and CLI installation, follow the [installation guide](https://github.com/amanmibra/raad/blob/v0.1.0/docs/installation.md). The combined `amanmibra-raad-0.1.0.tgz` works on both supported platforms. Platform-specific tarballs are also available. `SHA256SUMS` covers all downloadable packages.

Public and private repositories use the same adapters, with local credentials, PATs, or host-supplied GitHub App installation tokens. Invalid updates preserve the last validated snapshot. A mount limits API access, not all Git downloads or cached metadata.

Authenticated mocks cover private reads and writes; live private GitHub mutations have not been verified. Windows, Intel macOS, Linux arm64, and musl aren't covered by these downloads. See the [verification record](https://github.com/amanmibra/raad/blob/v0.1.0/docs/verification.md) for limits.
