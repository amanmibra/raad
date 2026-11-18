# Validation

The full mounted tree is validated before snapshot activation or any remote write. Recognized `.json`, `.yaml`, and `.yml` files get syntax validation by default, including in local mounts. Arbitrary bytes and plain Markdown are supported. Optional Markdown YAML frontmatter checking uses `frontmatter: true`. Disabling `syntax` allows otherwise invalid structured bytes to be read raw.

A schema rule targets one mount-relative file. Rules are trusted, inline JSON Schema documents; missing targets fail validation. JSON files decode as JSON; other schema targets decode as YAML. JSON Schema `format` behavior follows the selected draft's library defaults. Network and host-file reference retrieval are disabled; internal `$defs` references work.

```yaml
validation:
  syntax: true
  schemas:
    - path: settings.json
      schema:
        type: object
        required: [region]
        properties:
          region: {type: string}
  commands:
    - name: sop
      command: [sop, validate, --dir, .]
      timeout_seconds: 30
```

Commands are argument arrays, run without shell interpolation. A shell runs only if explicitly named by a trusted operator. The working directory contains only mounted files; source `.git`, unmounted files, and repository paths are not staged. Files have read-only permissions and no executable bits. `HOME` and `TMPDIR` point to the temporary directory, `PATH` is preserved for executable discovery, and the rest of the environment is cleared. Provider tokens are not forwarded.

Commands are trusted host code, **not sandboxed untrusted plugins**. They can access the host filesystem and network with the process's OS permissions. Use your own container/sandbox for stronger confinement. Do not configure a repository-controlled interpreter/script as a validator without independently trusting it. Unix child process groups are terminated at timeout/exit, preventing background pipe holders from hanging raad. Windows only guarantees termination of the direct child.

A nonzero exit, timeout, or output overflow rejects the candidate. Process stderr is not returned or logged because it can contain credentials or unrelated host data; errors identify the validator. Reproduce the validator locally on trusted data for detailed domain diagnostics. Built-in errors identify the offending file. Invalid candidates never replace an existing active SHA. Failed initial validation yields no readable snapshot.

The SOP example uses `sop validate --dir .` with the released `sop` CLI. raad does not parse SOP semantics or compile prompts. Compiled artifacts and publication decisions remain the host's responsibility. See [the runnable fixture](../examples/sopops/raad.yaml).
