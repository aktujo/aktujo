---
title: Install
nav_order: 2
---

# Install and start

## The program

Each [release](https://github.com/aktujo/aktujo/releases) has the program in two builds, for these platforms:

| Platform | `aktujo` | `aktujo-minimal` |
|---|---|---|
| Linux x86-64 | yes | yes |
| Linux ARM64 | yes | yes |
| macOS on Apple silicon | yes | yes |
| Windows x64 | yes | yes |

These are the platforms of falkOSS: macOS on Intel is not built; Windows on ARM64 follows once people ask for it
(an issue).

- **`aktujo`** has everything: reading scans by OCR, improving scans, object stores.
- **`aktujo-minimal`** leaves out scanning, OCR and object stores – smaller, for a server or an old computer. It
  still reads the text layer of PDFs.

Both read and write the same data folder: you can start with one and change to the other.

1. Download the archive for your platform and `SHA256SUMS` from the release.
2. Check the download: `sha256sum -c SHA256SUMS --ignore-missing` (on macOS `shasum -a 256 -c SHA256SUMS
   --ignore-missing`).
3. Unpack it and put `aktujo` (or `aktujo-minimal`) somewhere on your `PATH`.

The binaries are not signed yet (OPEN). macOS stops a program downloaded from the internet: allow it under *System
Settings → Privacy & Security*, or remove the mark with `xattr -d com.apple.quarantine aktujo`. Windows SmartScreen
asks once: *More info → Run anyway*.

## Start

```sh
aktujo serve                   # the archive in the browser, on http://127.0.0.1:8071
aktujo tui                     # the same archive in the terminal
aktujo relay --data <folder>   # a relay for syncing devices, on 127.0.0.1:8072 – you run it yourself
aktujo --help                  # every command
```

The data is kept in the folder for application data of your user (`--data <folder>` for another one). The
database and the originals are encrypted; the key is in the key store of the operating system, so a copy of the data
folder alone cannot be read. Keep your data safe from losing that key: sync your spaces to a second device, or make
an export.

On a server or in a container, where there is no key store, keep the key in a file of its own:
`--key-file <file>` or `AKTUJO_KEY_FILE=<file>` – made with a new key at the first start. Keep that file apart from
the data folder and its backups: whoever has both can read everything. OPEN: the file protected by a passphrase.

More:

```sh
aktujo spaces                            # the spaces, with --found <name> to found one
aktujo templates                         # the templates of the catalogs you use
aktujo templates take aktujo/paperwork --space <space>
aktujo export --space <space> --to <folder>       # a space as files, or with --zip as a ZIP
aktujo import --to <space name> <folder or ZIP>   # an export read back in, as a new space
```

An export is **not encrypted**: keep it as safe as the papers in it.

## From the source

You need Rust (the version in `rust-toolchain.toml` is installed by `rustup` by itself), Node 26 and pnpm:

```sh
git clone https://github.com/aktujo/aktujo && cd aktujo
scripts/get-ocr-models.sh            # the OCR models the full build embeds
(cd web && pnpm install && pnpm build)
cargo run --release -- serve
```

`cargo run --release --no-default-features -- serve` builds `aktujo-minimal` – without the OCR models.
