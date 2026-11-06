# aktujo-sdk

A person's archive on a device – the workspace every interface of aktujo uses, and the apps: a data folder with an
encrypted database (SQLCipher) and sealed originals, the key in the key store of the system or in a file; spaces,
documents with versions, kinds and fields, labels, the bin; reading in the background and search; sync and sharing
through a relay, invitations and recovery; catalogs of templates, watched folders, export and import.

```rust
use aktujo_sdk::{NewFile, SystemKeys, Workspace};

let mut workspace = Workspace::open(&folder, &SystemKeys)?;
let space = workspace.found_space("Home")?;
let added = workspace.add_document(&space, NewFile { name: "lease.pdf", media_type: "application/pdf", bytes: &pdf })?;
workspace.add_label(&space, &added.document, "Flat")?;
```

| Feature | What |
|---|---|
| `system-keys` (default) | `SystemKeys`: the key in the key store of the operating system; leave it out where the program keeps the key itself (the apps on iOS and Android) |
| `scan` | improving scans and photos (`aktujo-scan`): turned, cropped, straightened, grey, black and white |
| `ocr` | OCR in Rust, the models built in: from the folder in `AKTUJO_OCR_MODELS`, or `target/ocr-models` of the workspace (`scripts/get-ocr-models.sh`), checked by their hashes; implies `scan` |

Without features it reads the text layer of PDFs. Every crate it depends on is under a permissive licence.

Part of [aktujo](https://github.com/aktujo/aktujo). The design is in
[RFC 0004](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0004-archive-0.1.md) and
[RFC 0006](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0006-apps-and-sdk.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-sdk) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo – its API, web app, terminal interface and command line – is
AGPL-3.0-or-later.
