# Contributing

Thanks for helping improve LOBOT Lab.

## Before opening an issue

- Confirm the problem still occurs with the latest `main` branch.
- Include the LOBOT Lab version, operating system, and JA2 v1.13 version.
- Describe the active Data roots in low-to-high priority order.
- Include the character, inventory, action, direction, and relevant engine-trace
  messages needed to reproduce the problem.
- Do not upload proprietary JA2 data, executables, or SLF archives.

## Development setup

Install Node.js LTS, Rust 1.85 or newer, and the
[Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/), then run:

```sh
npm ci
npm run tauri dev
```

## Pull requests

Keep changes focused and include tests for parser, resolver, animation, palette,
or VFS behavior when practical. Before submitting:

```sh
npm run check
npm run build
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Real-data integration tests are optional and controlled by
`LOBOT_TEST_INSTALL` and `LOBOT_TEST_OVERLAY`. Never commit local paths or game
assets.

## Source-derived catalogs

Only regenerate the animation catalogs when the corresponding upstream JA2
source tables change:

```sh
npm run catalog -- /path/to/ja2-v1.13-source
```

Commit both generated JSON files together with the source compatibility change
that requires them.
