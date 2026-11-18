# Shared agent instructions

A support agent needs a consistent voice and a refund procedure. This example keeps those instructions in a `sops/` folder alongside application code.

- [sops/agents/support.yaml](sops/agents/support.yaml) chooses the agent's instructions.
- [sops/instructions/voice.md](sops/instructions/voice.md) sets its speaking style.
- [sops/procedures/refunds.md](sops/procedures/refunds.md) describes how to handle a refund.
- [sops/sop.yaml](sops/sop.yaml) marks the SOP folder.

## Read the instructions

From the raad repository root, run:

```sh
raad list --root examples/sopops/sops
raad read procedures/refunds.md --root examples/sopops/sops
```

The procedure asks the agent to get an order number, check the 30-day window, and send exceptions to a human. It also tells the agent not to promise a refund before checking eligibility.

The `--root` option chooses the folder raad exposes. File paths are relative to that folder, so you read `procedures/refunds.md` rather than its full repository path.

## Check the instructions before using them

Install [sop](https://github.com/sopops-dev/sop#quickstart), then run:

```sh
sop validate --dir examples/sopops/sops
raad sync --config examples/sopops/raad.yaml --mount sops
```

The [raad configuration](raad.yaml) runs `sop validate` before accepting an update. If validation fails, raad keeps the previous valid copy. On the first sync, a failure leaves no copy to read.

## Use this in your app

Copy the `sops/` folder into your own repository and edit it for your agent. Let raad read and refresh the files; let sop validate and compile them. Record the source commit with each published prompt so you can trace what an agent used.

The [Rust host example](../sopops.rs) shows how to read a GitHub folder using credentials supplied by your application.
