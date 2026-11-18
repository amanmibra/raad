# Contributing

Read [AGENTS.md](AGENTS.md), [the original specification](DESIGN.md), and [architecture](docs/architecture.md). Keep the human and agent READMEs, skill, and MCP tool names aligned. Add behavior-level tests for substantive changes. Never commit tokens or cache contents.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --all
```

Install sop and run `RAAD_REQUIRE_SOP=1 cargo test --test examples` for integration changes. Run `python3 scripts/benchmark.py` when changing fetch or cache behavior. Tests must not mutate customer repositories; use temporary fixtures and local API mocks.

For native bindings, also run the [SDK build and test commands](sdks/README.md#build-and-verify). Test the wheel and npm tarball in clean consumers before changing installation instructions.
