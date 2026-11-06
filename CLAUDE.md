# CLAUDE.md – aktujo

Rules for working in this repository. This file is public: nothing personal goes here.

## What this is

aktujo – your documents, kept safe – end to end. A falkOSS project for the whole life of a
document – capture (upload, scan, later the phone camera), improve (perspective, crop, black and white), read (text
layer, OCR, e-invoices), label, version, keep, find, share, workflows (A approves, B signs) with an audit log, sign – on a **core** of libraries others build in (originals by hash on
any storage, an event log with sync, end-to-end encryption, relay). It runs from one computer to Kubernetes
(PostgreSQL, S3, an operator). The plan is in the whitepaper (`aktujo/whitepaper`).

- **Licences:** the core (`aktujo-base`, `-blob`, `-log`, `-crypt`, `-store`, `-relay`, `-sync`) and the document crates
  (`aktujo-doc`, `-extract`, `-rules`, `-scan`) MIT, and so are the domain (`aktujo-core`) and the workspace
  (`aktujo-sdk`), which the apps in `aktujo/apps` build on (`docs/rfc/0006-apps-and-sdk.md`); the program (API, TUI,
  command line, web) AGPL-3.0-or-later, the texts (`aktujo-locales`) MIT. **Every crate is on crates.io**, the program
  too (job `crates`); the OCR models are crates of `aktujo/models` (Apache-2.0).
- **Pure:** aktujo refers to no other program or protocol and has nothing built in for one trade – no kinds of
  document, no rules, no fields of their own. Kinds and rules are data of a space (`docs/rfc/0005-pure-core.md`);
  templates live in the repository `aktujo/catalog`, specialisations in the programs that build aktujo in.
- The architecture is `docs/rfc/0001-architecture.md`; a decision that changes it gets a new document in `docs/rfc/`.

## Layout

| Path | What |
|---|---|
| `crates/aktujo-base` | core, MIT: canonical JSON (RFC 8785), hashes, space identities, root objects |
| `crates/aktujo-blob` | core, MIT: originals by hash – memory, folder, object store / S3 (features `object`, `s3`); also removed, sized and read in byte ranges; OPEN: kernoid |
| `crates/aktujo-log` | core, MIT: events of any kind, hybrid logical clocks, chains, merge by union |
| `crates/aktujo-crypt` | core, MIT: members, keys, member list, envelopes, invitations, recovery codes |
| `crates/aktujo-store` | core, MIT: the trait `Backend` – events, member records, settings; SQLite (`Store`; programs add tables of their own through `Store::connection`; feature `sqlcipher` encrypts the database as a whole) and PostgreSQL (`PgStore`, feature `postgres`, tables in a schema of their own; tests with `AKTUJO_TEST_POSTGRES`, CI on Linux; OPEN: TLS) |
| `crates/aktujo-relay` | core, MIT: the relay over HTTP (axum) |
| `crates/aktujo-sync` | core, MIT: a device's replica; sync through a relay (ureq), invitations, removal, recovery |
| `crates/aktujo-doc` | documents, MIT: versions, fields suggested and confirmed, labels, and kinds as data of a space (`kind_defined`, `kind_removed`) – as events; no I/O |
| `crates/aktujo-extract` | documents, MIT: text with positions from PDFs and images; backends `pure` (hayro, PP-OCR on tract; default) and `external` (pdftotext, Tesseract); `layer` is `pure` without OCR; `serde` keeps what was read as JSON |
| `crates/aktujo-rules` | documents, MIT: the kind and the fields from the text, as suggestions – the mechanism only: words, labels and ways come from the kinds; programs add code behind `Recogniser`; no I/O |
| `crates/aktujo-scan` | documents, MIT: improving scans and photos – pages of a PDF or an image, the sheet in a photo found (`find_corners`, `perspective.rs`) and its perspective corrected (`unwarp`), turned, cropped, straightened, grey or black and white, written as an image or a PDF (written by hand, no PDF crate); no I/O; `aktujo-extract`'s `pure` straightens with it |
| `crates/aktujo-core` | MIT: the domain – types and rules, no I/O |
| `crates/aktujo-sdk` | MIT (was `aktujo-app`): the workspace – every command and query; the **only** layer the interfaces and the apps use, permissive all the way down (CI checks it) – a data folder (SQLCipher database, originals sealed with XChaCha20-Poly1305, the key in the operating system's key store), spaces, documents, sync and sharing, reading and synchronising in the background (`Worker`, `Syncer`; event `document_read`), search (FTS5 beside the core's tables, filters); feature `ocr` builds the OCR models in (the crates `aktujo-extract-det` and `-latin` of `aktujo/models`, Apache-2.0) and turns on `scan` (improving, `improving.rs`); catalogs of templates (`catalog.rs`: sources, fetching, plan and apply with a three-way merge); watched folders (`Watcher`, `watching.rs`, subfolders if asked); export of a space to a folder or a ZIP, ZIP64 beyond 4 GiB (`exporting.rs`), read back in as a new space (`importing.rs`); the key in a file where there is no key store (`FileKeys`) |
| `crates/aktujo-api` | HTTP/JSON with axum, described in OpenAPI by utoipa (`routes`; `serve` takes the app with the web app around it); answers only `localhost` and changes only for its own pages |
| `crates/aktujo-web` | the web app built in (`around(router)`): `pnpm build` puts it into `crates/aktujo-web/web` (ignored by git, published with the crate); a policy that loads nothing from elsewhere |
| `crates/aktujo-tui` | the terminal interface (ratatui) – keyboard and mouse; language from the environment |
| `crates/aktujo-locales` | MIT: the texts of the web app and the terminal interface, `locales/<language>.json` (English, German); a new key goes into every file; the apps keep their own |
| `crates/aktujo` | the command line: `serve`, `tui` (both `--data`, `--memory`, `--key-file`), `relay`, `openapi`, `spaces`, `templates`, `export`, `import`; feature `ocr` on by default – without it, `aktujo-minimal` |
| `web/` | the web app: Vite, Svelte 5, TypeScript; types from `api/openapi.json` (openapi-typescript, openapi-fetch) |
| `api/openapi.json` | generated by `aktujo openapi`; CI checks it matches the code |
| `docs/` | aktujo.dev – Jekyll, Just the Docs, published by GitHub Actions: install, building it in (`embedding.md`), the API, RFCs |
| `scripts/` | `make-ocr-samples.sh`, `check-age.py`, `check-editions.sh` (CI: both builds share a data folder) |

## Principles

- **Everything included:** one edition, every feature free; no paid tiers, licence keys or unlocks.
- **No telemetry.** Only connections that are technically needed or that users start themselves.
- **The core knows no documents:** crates of the core never depend on the document crates or the program; the
  document crates (`docs/rfc/0002-documents.md`) depend on the core, never on the program.
- **One workspace, many interfaces:** TUI, API, CLI and the apps go through `aktujo-sdk`; the web app only through the
  API.

## Licences

- The core, the document crates, `aktujo-core`, `aktujo-sdk` and `aktujo-locales` MIT, everything else **AGPL-3.0-or-later** – an SPDX header in every own source file and `Cargo.toml`;
  REUSE checked in CI.
- `docs/assets/` and `web/public/favicon.svg` are copies from `aktujo/brand` under CC BY-ND 4.0 – update them by copying, never edit them here.
- Code taken over from elsewhere keeps its licence and names its origin.

## Rules

- English in code, docs and commits; commits signed off (`git commit -s`).
- No personal data in tracked files (names, paths like `/Users/…`, hostnames, keys).
- Claims need a source; unverified statements are marked **OPEN**.
- No externally loaded fonts or scripts in anything web-based.
- Dependencies exact in the lock files, as ranges in `Cargo.toml`, at least 14 days old (CONTRIBUTING.md; aktujo's own crates of `aktujo/models` excepted); Rust
  edition 2024, the toolchain in `rust-toolchain.toml`; TypeScript 6 until Svelte supports 7, Node 26, pnpm.
- After changing the API: `cargo run -q -- openapi > api/openapi.json` and `pnpm --dir web api`.

## Checks

```sh
cargo fmt --check && cargo clippy --all-targets --all-features && cargo test --all-features && cargo deny check
(cd web && pnpm check && pnpm build)
```
