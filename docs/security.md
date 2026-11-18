# Security boundaries

Mount-relative paths reject absolute paths, parent/current components, empty components, backslashes, drive/colon syntax, control terminators, and `.git` components. Only regular blob entries are accepted. Symlinks (including internal ones), submodules, and Git LFS pointers are rejected. Mount roots must resolve to trees; symlink ancestors cannot be followed into another directory. File/directory write collisions and duplicate operations are rejected. Non-UTF-8 Git paths are unsupported.

Reads use checksummed local snapshots, never unbounded Git revision expressions, worktree paths, or lazy-fetching commands. Requested revisions must be complete retained SHAs. Size, file count, search-result, response, subprocess-output, and cache budgets are bounded. Writes preserve mount bounds and validate complete candidates. An expected base is required; direct writes use GitHub's atomic comparison.

**The mount is an API boundary, not Git fetch isolation.** Partial clone/sparse checkout do not guarantee absence of off-mount objects or metadata. Use a dedicated data repository for strict minimization. Never grant a broad repository credential on the assumption that a mount narrows GitHub permission. The host, token provider, Git executable and local Git configuration, operator config, and validators are trusted.

Remote repositories and incoming agent requests are untrusted. Fetched files cannot register validation commands. Commands receive mounted files, a cleared environment, no token, and bounded execution/output. They are trusted local code, not contained by an OS sandbox. Use external process/container isolation for hostile validator code. Unix process groups are killed on timeout; Windows only guarantees killing the direct process.

Tokens are redacted in debug output, not persisted by raad, never placed in URLs, and not forwarded to validators. GitHub error bodies and process stderr are suppressed. HTTP redirects are disabled for API writes. Unix private cache directories are mode 0700; host filesystem permissions/ACLs are required elsewhere. Anyone with the same OS identity or raw Git credentials can access cached objects; raad cannot isolate those actors.

File locks and atomic durable activation preserve last-known-good state during concurrent syncs and interruptions. The checksum detects accidental snapshot modification, not malicious state+snapshot changes by the host account. Corrupt snapshots fail explicitly; resync repairs snapshots, while malformed state needs clear-cache. Preserve the cache for offline recovery. Pin and coordinate releases in the application, including any compiled artifact.

Resource limits reject oversize candidates but cannot guarantee a hard streaming download/disk quota during Git fetch. Schemas cannot fetch network/host-file references. Cache pruning removes snapshots, while clear-cache removes Git objects too. No special privileges, daemon, or network listener are needed.

Tests cover traversal, symlinks/ancestor mounts, absent off-mount exposure, filtered pack contents, malformed structured data, cache corruption, cold/warm failure, force pushes, sync concurrency, stale writes, token redaction, validator environment/timeouts, MCP allowlists/read-only grants, authorized proposals, and direct-mode rejection.
