# Examples

These examples show how to keep application data in Git and read it with raad. Start with the one closest to your project.

Run commands from the **raad repository root**, after [installing raad](../README.md#try-it). raad reads committed files, so commit your edits before trying a change.

## App config

Keep settings in a JSON file that your team can review before a release.

```sh
raad read settings.json --root examples/app-config
```

You'll see the region, feature flags, and retry limit. raad checks that the JSON is valid before using it.

[Walk through the config example](app-config/README.md).

## Refund policy

Keep a policy in Markdown so people can edit it and your application can read the same file.

```sh
raad read refunds.md --root examples/policies
```

You'll see a policy that allows refund requests within 30 days and sends exceptions for human approval.

[Walk through the policy example](policies/README.md).

## Shared agent instructions

Keep an agent's voice and refund procedure in separate files. Share them across agents without copying a whole prompt.

```sh
raad list --root examples/sopops/sops
raad read procedures/refunds.md --root examples/sopops/sops
```

This example uses [sop](https://github.com/sopops-dev/sop) to check the instructions. raad reads the files; sop understands their format.

[Walk through the agent example](sopops/README.md).

## Use raad in Rust

Once you've tried the CLI, [local.rs](local.rs) shows the same config read using the library:

```sh
cargo run --example local
```

It prints the source commit and the config. [sopops.rs](sopops.rs) shows how a host application can supply GitHub credentials. Its default run reads a public repo:

```sh
cargo run --example sopops
```

To connect your own GitHub folder, adapt [remote.yaml](remote.yaml) using the [configuration guide](../docs/configuration.md).

For fetch and cache measurements, run `python3 scripts/benchmark.py`. It creates a temporary test repo; you don't need it to use the other examples.
