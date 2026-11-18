# Build raad v0.1

Implement a complete, testable Rust project according to [DESIGN.md](DESIGN.md). Read [README.md](README.md), [AGENTS.md](AGENTS.md), and [docs/agents/README.md](docs/agents/README.md) first.

Produce a reusable Rust core library, a `raad` CLI, and a local stdio MCP server. GitHub is the first remote backend. Reuse Git/GitHub commits, branches, pull requests, approvals, and CI/CD; do not reimplement them. No standalone raad HTTP server, required database, UI, or daemon.

Required working vertical slices:

1. Local Git repository discovery; scoped read/list/search/history at a committed SHA.
2. Directory-mounted GitHub repository; filtered Git object fetch without an unrestricted worktree checkout; cached read-only paths with provenance.
3. Built-in JSON/YAML validation and trusted optional schema/command validators; last-known-good activation and sync status.
4. GitHub auth via existing local credentials and supplied GitHub App/PAT token providers; remote PR proposals with stale-write protection; optional authorized direct commits.
5. CLI commands, JSON output, local stdio MCP with scoped read-only defaults, and explicit PR write authorization.
6. Tests, example `sops/` layout with `sop validate`, release/docs assets, and useful README/agent skill materials.

Mandatory security/property tests: mount path traversal, symlink escapes, no off-mount file exposure, invalid revision rejection, cold/warm offline behavior, concurrent sync and stale writes, force-pushed branch handling, token redaction, and MCP permission boundaries.

Critical documentation rule: **both the human README and agent README must warn that a mounted folder limits API access but Git partial clone/sparse checkout does not guarantee that only mounted content or metadata is cached.** Recommend a dedicated data repo for strict minimization. Never make unsupported isolation claims.

After each slice run and fix available formatting, linting, unit, and integration tests. Update documentation to match implemented behavior and make the quickstart genuinely executable. Do not stop at scaffolding.

Reference implementations for integration, not projects to edit:
- https://github.com/sopops-dev/sop
- https://github.com/sopops-dev/sopops
- https://github.com/sopops-dev/whatsop
