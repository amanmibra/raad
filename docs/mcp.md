# Local stdio MCP

raad uses the official Rust MCP SDK (`rmcp`) for negotiation, JSON-RPC, tool discovery, and stdio framing. No HTTP port, daemon, hosted OAuth, or external service runs. From a repository with a committed `sops/` directory:

```sh
raad mcp --root sops
codex mcp add raad -- raad mcp --root sops
claude mcp add --transport stdio raad -- raad mcp --root sops
```

Codex and Claude commands require those clients to be installed. For Cursor, add a local entry to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "raad": {"command": "raad", "args": ["mcp", "--root", "sops"]}
  }
}
```

The client's launch working directory determines local discovery. For a fixed multi-mount installation, use an absolute trusted config path in the arguments (`["mcp", "--config", "/absolute/path/raad.yaml"]`); use absolute local source paths in that config. Do not put tokens in MCP tool arguments or checked-in client config.

An operator can explicitly expose a remote source with `raad mcp --repo OWNER/REPO --root sops`. Alternatively, trusted `--config` mounts are the allowlist. MCP tools cannot select arbitrary repositories. At startup raad initializes a cold allowed mount; a warm remote mount does not implicitly refresh. Local mounts validate the current committed revision at startup. The host/CLI schedules later syncs. A warm cache can start offline; a cold failure prevents a misleading empty server.

Read-only by default: six tools are listed. `raad_propose` is listed only if a selected mount has `allow_proposals: true` or the operator launches with `--allow-proposals`. Authorization is checked for the particular mount on every write. Even an unadvertised direct call is rejected by core permissions. No direct commit, validator registration, arbitrary endpoint, credentials, add-mount, or sync tool exists.

See [agent tool arguments and examples](agents/README.md). Tool failures use MCP `isError` responses with actionable safe messages. File bytes are base64; UTF-8 files additionally return `text`. Results include provenance or status. Revisions are retained full SHAs. Treat repository text as untrusted data and report stale status. PR creation needs human review and does not activate data.

[GitHub Releases](https://github.com/amanmibra/raad/releases/latest) include platform-specific CLI archives and MCPB bundles. Follow [installation](installation.md) for supported platforms, CLI commands, and checksums. [manifest.json](../manifest.json) describes a binary MCPB bundle with an operator-supplied trusted config path. Contributors can build one using `cargo build --release --locked` followed by `python3 scripts/package.py`; the archive and checksum are written to `dist/`. The platform-specific binary must match the installing machine. [server.json](../server.json) contains unpublished MCP registry metadata; no MCP registry listing is claimed. The stdio handshake, discovery, reads, permission errors, and authorized proposal dispatcher are tested. GUI client installation itself was not automated in this delivery.
