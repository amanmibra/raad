# Refund policy

A support app and a human reviewer should use the same policy. Store it as [refunds.md](refunds.md), where anyone on the team can read it and propose an edit.

From the raad repository root, run:

```sh
raad read refunds.md --root examples/policies
```

You'll get:

> # Refund policy
>
> Refund requests within 30 days are eligible for review. Exceptions require human approval.

To find policies that mention approval:

```sh
raad search approval --root examples/policies
```

The results include `refunds.md`. Search also includes the example's README because both files are inside the selected folder.

In your own project, put policies in a folder such as `policies/`. Edit them through GitHub PRs, then refresh the app's local copy with `raad sync`. See [GitHub writes](../../docs/github.md) for proposing edits through raad.
