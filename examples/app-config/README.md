# App config

Your application needs a region, a feature flag, and a retry limit. Keep them in [settings.json](settings.json) so changes go through the same review process as code.

From the raad repository root, run:

```sh
raad read settings.json --root examples/app-config
```

You'll get:

```json
{
  "region": "us-east-1",
  "features": {
    "refunds": true
  },
  "retry_limit": 3
}
```

raad also prints the source commit to stderr, so you can tell which version your app read.

Try changing `retry_limit` to `5`. Save and commit the file, then run the command again. raad reads the committed value, so an unsaved or uncommitted edit won't change the result.

JSON syntax is checked by default. Add a [schema](../../docs/validation.md) if you also want to require particular fields or allowed values.

For the same read in Rust, see [local.rs](../local.rs).
