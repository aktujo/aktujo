# Contributing to aktujo

Thank you for helping. The rules for every aktujo repository – sign-off, licences, everything included, no
telemetry – are in the [organisation's CONTRIBUTING.md](https://github.com/aktujo/.github/blob/main/CONTRIBUTING.md).
This file adds how to work on this one.

## Build and test

Rust from `rust-toolchain.toml`, Node 26 and pnpm (the version in `web/package.json`).

```sh
(cd web && pnpm install && pnpm build)
cargo fmt --check && cargo clippy --all-targets --all-features && cargo test --all-features
cargo run -q -- openapi > api/openapi.json && (cd web && pnpm api)   # after changing the API
(cd web && pnpm check)
cargo deny check
scripts/check-age.py                                                  # after changing Cargo.lock
cargo test -p aktujo-sdk && cargo build -p aktujo --no-default-features   # aktujo-minimal, without OCR
```

The feature `ocr` of `aktujo-sdk` (on by default in `aktujo`) builds the OCR models into the program: the crates
`aktujo-extract-det` and `aktujo-extract-latin` of [aktujo/models](https://github.com/aktujo/models), fetched by cargo
like any dependency. `pnpm build` puts the web app into `crates/aktujo-web/web`, which `aktujo-web` embeds.

The texts of the web app and the terminal interface are in `crates/aktujo-locales/locales/`, one JSON file per
language; a new text gets a key in every file (tests of `aktujo-locales` and `aktujo-tui` check it). Colours, fonts and the logo come from `aktujo/brand` only –
copied, never drawn or edited here. To try the program without touching the key store: `cargo run -- serve --memory`.

## Rules

- **One core, many interfaces:** the terminal interface, the API and the command line work through `aktujo-sdk`
  only; the web app and the apps use the API.
- **Dependencies:** exact versions in the lock files, each the newest that is at least 14 days old –
  `scripts/check-age.py` for `Cargo.lock`, `minimumReleaseAge` in `web/pnpm-workspace.yaml` for pnpm; a fix for a
  security advisory may be younger. `Cargo.toml` names the lowest version a crate needs (`"1.0.229"`, not
  `"=1.0.229"`), because the crates of the core are published and others must be able to resolve them; CI builds
  with `--locked`. No `^` in `package.json`.
- **The TUI** can be used with the keyboard alone and with the mouse: a click does what the key does.
