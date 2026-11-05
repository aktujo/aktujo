---
title: "0004 Archive 0.1"
parent: Design
nav_order: 4
---

# 0004 Archive 0.1: one person, one computer

State: draft. Scope decided by the maintainer on 2026-11-05.

## Summary

Version 0.1 of the program aktujo is the archive for **one person on one computer**: capture, improve, read, label,
version, keep and find documents – step 3 of the roadmap in the whitepaper. It is built on what is there: the core
(0001), the document crates `aktujo-doc` and `aktujo-extract` (0002) and `aktujo-rules` (0003). What the program has
today – `aktujo-app`, the API, the web app and the terminal interface – is the skeleton from the starter pack (a list
of items in memory); 0.1 replaces it with the archive.

Sharing, sync between devices, the audit log as a report, the mobile app, servers, workflows and signatures are
later steps of the roadmap and not part of 0.1 – but nothing in 0.1 may stand in their way. Unlike the whitepaper's
roadmap, the import from paperless-ngx is **not** part of 0.1 (maintainer, 2026-11-05): neither import from nor
export to other programs.

## What 0.1 does

| Step | In 0.1 | Not yet |
|---|---|---|
| **Capture** | Upload one or more files (PDF, PNG, JPEG, TIFF – OPEN: which exactly); a folder at once; **watched folders**: new files in a folder (from a scanner, for example) are taken in on their own. The same bytes twice are found by their hash and not added again. | Scanner and mail directly, the phone camera. |
| **Improve** | For images and scanned pages: rotate, crop, deskew, black and white or greyscale; the result is a **new version**, the original stays. Several images into one document. | Finding the page's edges, perspective (keystone). |
| **Read** | The text layer, OCR for pages without one (`aktujo-extract`, backend `pure`), in the background after adding. | E-invoices (XRechnung, ZUGFeRD), optional AI. |
| **Label** | The kind and the fields suggested (`aktujo-rules`) and confirmed by the person; title suggested; labels added and removed by hand. | Correspondents of their own, kinds defined by users, rules that learn from what was labelled before. |
| **Version** | Every version listed, opened and downloaded; a new version by upload or by improving. | Comparing versions, restoring an old one as the newest – OPEN. |
| **Keep** | **Several spaces** – a private archive beside a business's, for example; originals in a folder by hash (`aktujo-blob`), events in SQLite (`aktujo-store`), through `aktujo-sync` without a relay; **encrypted at rest**. The bin: put in, take out. | Retention, deleting for good. |
| **Find** | Full-text search over title, labels, fields and the text that was read; filters by space, kind, label, date and the bin. | Search over encrypted content on other devices; saved searches. |
| **Export** | Everything at any time: the originals and a JSON file of every document's state, also the events as they are – aktujo's own open format. | Import from and export to other programs (paperless-ngx and others). |

## Where it runs

- `aktujo serve` – the API and the web app on `127.0.0.1` (port 8071), opened in a browser on the same computer.
- `aktujo tui` – the same archive in the terminal.
- The command line also gets the steps that are useful in scripts: `add`, `export`, `search` (OPEN: which, and the
  names).
- Both interfaces go through `aktujo-app` (0001); the web app only through the API.
- The interface speaks **English and German**; its texts are data, so that more languages can follow.
- Where the data lives: one folder per user (OPEN: the default on Linux, macOS and Windows, and `--data` to choose
  another).
- Linux, macOS and **Windows**; CI checks all three.

## Decisions

Decided by the maintainer on 2026-11-05.

1. **Several spaces.** The first space is founded on the first start; more can be founded, named and chosen in
   every interface. A document belongs to one space. Moving a document between spaces: OPEN.
2. **The state is built from the events.** On start the program reads the logs, builds `Documents` (`aktujo-doc`)
   per space and keeps them in memory; views for searching are kept in SQLite beside the core's schema, as tables of
   the program (0001: no general SQL layer in the core). OPEN: how many documents this must carry (proposal:
   100 000 on an ordinary laptop) and measuring it.
3. **The text that was read is kept as an original by hash**, beside the version it was read from, so that it syncs
   later and other devices – a phone above all – need not read again. Reading again with better OCR writes a new
   one. Full-text search runs in SQLite FTS5 over that text, the title, labels and field values. OPEN: the format of
   the stored text (the positioned lines of `aktujo-extract` as JSON is proposed) and the event that links it to its
   version (an amendment to 0002).
4. **Reading in the background.** Adding returns at once; reading, recognising and indexing run in a queue, one
   document after the other, with their state visible (waiting, reading, done, failed). Suggestions are written as
   events when the reading is done.
5. **Improving writes a new version.** The tools for images are a crate of their own, `aktujo-scan` – MIT like the
   document crates, pure Rust, no I/O – so that apps and other programs can use them: rotate, crop, deskew, black and
   white, greyscale. Edges and perspective come after 0.1, when the camera of the mobile app needs them.
6. **The OCR models are built in – with a minimal build beside it**: PP-OCRv3 detection (2.4 MB) and PP-OCRv5
   Latin recognition (7.9 MB with its dictionary), both Apache-2.0 ([spike](../spikes/ocr.md)).
   - The **apps** always carry them.
   - The **program** `aktujo` (server, command line, terminal interface, web app) comes in two builds of the same
     program – not two editions: the full one, and **`aktujo-minimal`** (maintainer, 2026-11-05). Features of the
     crate `aktujo`, on by default, decide it; the release carries both builds.
   - `aktujo-minimal` leaves out **everything about scanning and OCR** – the OCR code (tract, PP-OCR, deskew) and its
     models, rendering pages, `aktujo-scan` – and **object stores** (`aktujo-blob` without `object` and `s3`;
     originals in a folder only). It reads text layers only and says so where a page has none; improving is not
     offered.
   - It keeps everything else: several spaces, encryption at rest, recognition (`aktujo-rules` on the text layer),
     full-text search, versions, the bin, watched folders, export – and both interfaces, the terminal interface and
     the web app.
   - Both builds read and write the **same data**: a data folder of `aktujo-minimal` opens in the full build without
     change and the other way round; a version improved or read by OCR in the full build stays readable in the
     minimal one.
   - Neither build makes a connection for the models. They stay data for the crates (0002: never part of
     `aktujo-extract`). OPEN: the names of the features; where the build takes the models from
     (`scripts/get-ocr-models.sh`, checked by hash); naming their licence and origin in the program and in
     `REUSE.toml`; the sizes of both builds (to be measured).
7. **Encrypted at rest.** Originals and the text read from them are stored as envelopes (`aktujo-crypt`), the
   events of a space encrypted with its key. The device's seed lives in the **key store of the operating system**
   (Keychain on macOS, Secret Service on Linux, Credential Manager on Windows); where there is none – a server
   without a desktop, a container – in a file protected by a passphrase. This answers an open point of 0001 for the
   program. OPEN: what in SQLite is encrypted (proposal: the payloads of events and the search views; what the core
   needs to merge – hashes, clocks – stays readable), how the search index is protected, and the crate for the key
   stores.
8. **No import or export of other programs** in 0.1; aktujo's own export stays.
9. **English and German** in the interface.
10. **Watched folders** are part of 0.1: a folder is given per space; new files are taken in, then left where they
    are or moved to a folder beside it (OPEN: which is the default).
11. **Installed from binaries** attached to each GitHub release, built by CI for Linux, macOS and Windows – each
    as `aktujo` and as `aktujo-minimal` (decision 6). Other ways
    (cargo install, Nix, Homebrew) later. OPEN: signing and notarising the binaries for macOS and Windows.

## Done when

- A person adds 100 documents – the samples in `testdata/ocr/` and their own – by upload and through a watched
  folder, sees the suggestions, confirms some, labels them, finds them again by a word of their text, puts one in
  the bin and takes it out, adds a second version by improving a scan, founds a second space, and exports
  everything – in the web app and in the terminal, in English and German.
- The data folder copied to another computer cannot be read without the key from the first one's key store or the
  passphrase.
- The program closed and opened again shows the same state; a log read by another device would show the same state
  (`aktujo-doc`: same events, same state).
- The skeleton (`items`) is gone; `api/openapi.json` describes the archive.
- CI checks it on Linux, macOS and Windows as `aktujo` and as `aktujo-minimal`, checks that both open the same data
  folder, and attaches both builds to the release.
- The repository's README and aktujo.dev describe how to install and start it.

## Order of the work

1. `aktujo-app`: the workspace on `aktujo-sync` with several spaces, encrypted at rest, the key in the key store;
   documents added, labels, kinds, fields, versions, the bin – tested without an interface.
2. Reading in the background with `aktujo-extract` and `aktujo-rules`, the models built in; the text stored by
   hash.
3. Search (FTS5) and filters.
4. The API and the web app for all of it, in English and German; then the terminal interface.
5. Watched folders.
6. Improving (`aktujo-scan`).
7. Export.
8. Windows in CI, binaries in the release, the release 0.1.0 of the program.

## Open points

- OPEN: every point marked OPEN above.
- OPEN: the version of the program – it shares the workspace version 0.1.0 with the crates today; whether the
  program and the crates get versions of their own.
- OPEN: real samples from the maintainer (also open in 0002 and 0003) – needed for "done when".
