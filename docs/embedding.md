---
title: Build it in
nav_order: 3
---

# Build aktujo into your program

The libraries of aktujo are MIT and on crates.io. Take what you need – each crate stands on its own above the ones it
depends on:

| Crate | For |
|---|---|
| [`aktujo-blob`](https://docs.rs/aktujo-blob) | originals by their SHA-256 hash, in memory, in a folder or in S3 (features `object`, `s3`) |
| [`aktujo-extract`](https://docs.rs/aktujo-extract) | text with its position from PDFs and images: the text layer, OCR in Rust (`pure`) or by poppler and Tesseract (`external`) |
| [`aktujo-scan`](https://docs.rs/aktujo-scan) | pages of a PDF or an image as pictures: the sheet in a photo found and its perspective corrected, turned, cropped, straightened, grey or black and white, written as an image or a PDF |
| [`aktujo-rules`](https://docs.rs/aktujo-rules) | the kind and the fields of a document from its text, as suggestions – from kinds as data, plus your own code |
| [`aktujo-doc`](https://docs.rs/aktujo-doc) | documents as events: versions, kinds as data, fields suggested and confirmed, labels |
| [`aktujo-core`](https://docs.rs/aktujo-core) | the domain of the archive: its types and rules, no I/O |
| [`aktujo-sdk`](https://docs.rs/aktujo-sdk) | a whole archive on a device – what the program and the apps use: an encrypted data folder, spaces, documents, reading, search, sync, export ([RFC 0006](rfc/0006-apps-and-sdk.md)) |
| [`aktujo-base`](https://docs.rs/aktujo-base), [`-log`](https://docs.rs/aktujo-log), [`-crypt`](https://docs.rs/aktujo-crypt), [`-store`](https://docs.rs/aktujo-store), [`-sync`](https://docs.rs/aktujo-sync), [`-relay`](https://docs.rs/aktujo-relay) | the core: an event log with hybrid logical clocks, members and keys, end-to-end encryption, SQLite, sync through a relay |

The OCR models are crates of [aktujo/models](https://github.com/aktujo/models) (Apache-2.0):
[`aktujo-extract-det`](https://docs.rs/aktujo-extract-det) finds where text stands,
[`aktujo-extract-latin`](https://docs.rs/aktujo-extract-latin) reads Latin script.

The program aktujo – [`aktujo`](https://docs.rs/aktujo), [`aktujo-api`](https://docs.rs/aktujo-api),
[`aktujo-web`](https://docs.rs/aktujo-web), [`aktujo-tui`](https://docs.rs/aktujo-tui) (AGPL) and the texts of its
interfaces, [`aktujo-locales`](https://docs.rs/aktujo-locales) (MIT) – is built on the same crates; nothing of it is
needed to use them.

These are all of aktujo's crates. A crate named `aktujo-…` on crates.io that is not listed here is not aktujo's. aktujo has no kinds of
documents of its own – your program brings its kinds, as data or as code.

## Originals by their hash

A program that keeps files under names of its own (a UUID in a database, say) keeps the hash instead: the same bytes
are kept once, and damaged bytes are noticed on every read.

```rust
use aktujo_blob::{Blobs, ObjectBlobs, S3Config};

let blobs = ObjectBlobs::s3(&S3Config {
    bucket: "music".into(),
    region: "garage".into(),
    endpoint: Some("http://garage:3900".into()),
    access_key_id,
    secret_access_key,
    allow_http: true,
}, "organisation-1")?;

let hash = blobs.put(&pdf)?;                       // store `hash.to_string()` in your table
let bytes = blobs.get(&hash)?.expect("stored");     // checked against the hash
let part = blobs.get_range(&hash, 0..65_536)?;     // a byte range, e.g. for HTTP `Range`
let length = blobs.size(&hash)?;
blobs.remove(&hash)?;                              // when nothing refers to it any more
```

`ObjectBlobs::new(store, prefix)` takes any `Arc<dyn object_store::ObjectStore>` you have already, and
`FolderBlobs::open(folder)` keeps the originals in a folder. The trait `Blobs` is synchronous; `ObjectBlobs` runs
each call on a small runtime of its own, so it works from a plain thread and from inside tokio alike – in an async
handler, call it in `tokio::task::spawn_blocking` as you would any blocking I/O.

What refers to an original is your program's concern: the store keeps no count of references, so remove an original
only when no row refers to its hash any more.

## Text from PDFs and images

```rust
use aktujo_extract::{Extractor, Options, Pure};

let extractor = Pure::new();                       // text layers only
let extraction = extractor.extract(&pdf, "application/pdf", &Options { max_pages: 1, ..Options::default() })?;
for line in &extraction.pages[0].lines {
    // the box in fractions of the page, y from the top
    println!("{:.2} {:.2} {}", line.x0, line.y0, line.text);
}
```

- **OCR in Rust:** `Pure::with_models(Models { detection, recognition, dictionary })` with PaddleOCR's PP-OCR models
  as ONNX (Apache-2.0). The models are data your program brings; the ones aktujo is tested with are the crates
  `aktujo-extract-det` and `aktujo-extract-latin`: `Models { detection: aktujo_extract_det::MODEL, recognition:
  aktujo_extract_latin::MODEL, dictionary: aktujo_extract_latin::DICT }`.
- **OCR by external programs:** `External::detect()` (feature `external`) uses `pdftotext`, `pdftoppm` and
  Tesseract where they are installed; `Options::languages` and `Options::layout` (Tesseract's page segmentation: `11`
  for sparse text such as the headers of sheet music) apply to it.
- `Options::min_words`: a page of a PDF with fewer words than this is read by OCR instead; `0` never does.
- `abilities()` tells what a backend can do on this machine.

To read only a part of a page – the header of a part of sheet music – render it with `aktujo-scan`, cut it out and
read the picture:

```rust
let page = aktujo_scan::page(&pdf, "application/pdf", 3, 200.0)?;      // the fourth page, at 200 dpi
let (width, height) = page.image.dimensions();
let header = image::imageops::crop_imm(&page.image, 0, 0, width, height / 4).to_image();
let png = aktujo_scan::write(&[aktujo_scan::Page { image: header, dpi: page.dpi }], false)?;
let lines = extractor.extract(&png.bytes, png.media_type, &Options::default())?;
```

## Kinds, fields and your own rules

A kind is data: its fields, the words that speak for it and the ways to find each field. Write your kinds as JSON
(the format of `aktujo_doc::Kind`, the templates of [aktujo/catalog](https://github.com/aktujo/catalog) are
examples) and let `Rules` read them:

```rust
use aktujo_rules::{Chain, Recogniser, Rules};

let kinds: Vec<aktujo_doc::Kind> = serde_json::from_str(KINDS_JSON)?;
let recognition = Rules::default().recognise(&kinds, &extraction, None);
```

What words and labels cannot express – an instrument and its voice from "2. Trompete in B", a tax number with its
check digits – your program writes as code behind `Recogniser` and puts after the rules in a `Chain`:

```rust
struct Parts { /* your instruments */ }

impl Recogniser for Parts {
    fn recognise(&self, kinds: &[aktujo_doc::Kind], extraction: &aktujo_extract::Extraction, kind: Option<&str>)
        -> aktujo_rules::Recognition
    {
        // read `extraction.pages[..].lines`, return the kind and fields you found
        aktujo_rules::Recognition::default()
    }
}

let recogniser = Chain(vec![Box::new(Rules::default()), Box::new(Parts { /* … */ })]);
```

Everything recognised is a **suggestion** with a confidence, the rule that found it and the line it came from; a
person confirms it. Your program keeps the result in its own tables – or as `aktujo-doc` events
(`Recognition::events`), if it keeps its documents in an aktujo log.

## Your own tables

aktujo does not reach into your database. A server keeps its tables (PostgreSQL with sqlx, say) and adds a column for
the hash of each original; the event log, sync and end-to-end encryption of the core come in when your program needs
them.

When they do, the core keeps what it needs – events, member records, settings – behind the trait
`aktujo_store::Backend`: in SQLite on a device (`Store`), in PostgreSQL on a server (`PgStore`, feature `postgres`).
In PostgreSQL its tables stand in a schema of their own (`aktujo`, or one you name), so they live in your database
beside yours without meeting them:

```rust
use aktujo_store::{Backend, PgStore};
use aktujo_sync::Replica;

let store = PgStore::connect("postgres://music@db/music")?;          // tables in the schema `aktujo`
let mut server = Replica::open(domain, "server".parse()?, store, blobs)?;
server.join(&invitation, Some("Server"))?;                           // a member of the space, like a device
server.sync(&space)?;
```

The client is synchronous: from an async handler, call it in `tokio::task::spawn_blocking`. Several servers may share
the database – the schema is set up under a lock, and member records are appended under a lock per space. OPEN: TLS
to the database; today the connection is without it, for a database in the same private network.

## Versions

The crates follow semantic versioning; before 1.0 a new minor version (0.2 → 0.3) may change their API. Each release
lists what changed.
