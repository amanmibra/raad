# Trusted configuration

No configuration is needed for local committed reads. Use `--repo OWNER/REPO --root sops` for a remote mount, optionally `--branch feature/policies`. Remote default branches are discovered through Git. `--revision` always means a full retained lowercase SHA; `sync --revision` explicitly fetches/validates it.

Load configuration only through `--config PATH`. raad never auto-loads executable settings from repository content. Keep this file under trusted operator control, particularly validators and write grants. Source paths are relative to the process's working directory. For production, use absolute local repository paths.

```yaml
version: 1
mounts:
  - name: sops
    source:
      kind: github
      repository: acme/livekit-agent
    branch: main
    path: sops
    refresh_seconds: 60
    allow_proposals: false
    allow_direct: false
    limits:
      file_bytes: 2097152
      mount_bytes: 33554432
      max_files: 10000
      response_bytes: 4194304
      cache_bytes: 536870912
      retained_revisions: 8
    validation:
      syntax: true
      frontmatter: false
      schemas: []
      commands: []
  - name: local-policies
    source:
      kind: local
      path: /absolute/path/to/my-app
    path: policies
```

Select a mount for CLI operations with `--mount sops`. MCP exposes all mounts in the explicit configuration unless narrowed with `--mount`. Names must be unique. Unknown configuration keys are rejected. A top-level `validation` block can apply the same trusted policy to all mounts; it replaces per-mount validation settings.

`--config` cannot be combined with `--repo`, `--root`, or `--branch`. `--cache-dir` overrides `RAAD_CACHE_DIR`, then `$XDG_CACHE_HOME/raad` or `~/.cache/raad`. Mount identity, validation policy, and content limits determine its cache namespace. Credentials are not configuration fields and never enter cache keys or snapshot state.

CLI `propose` explicitly authorizes the proposal operation. Direct writes additionally require `--direct --allow-direct`; the configuration alone cannot trigger a CLI direct commit. MCP respects per-mount `allow_proposals`; `--allow-proposals` explicitly grants proposals to all selected mounts. Use `--mount` to narrow that flag. MCP direct commits are always disabled.

`refresh_seconds` governs the stale flag; it does not start a poller. The host must call `sync`. Errors exit 1 generally, 3 for stale conflicts, 4 for validation failure, 5 for permission denial, and 6 for GitHub API rate limits. JSON errors go to stderr when `--json` is set.
