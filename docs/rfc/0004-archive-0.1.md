---
title: "0004 Archive 0.1"
parent: Design
nav_order: 4
---

# 0004 Archive 0.1: a person's archive, on their devices

State: draft. Scope decided by the maintainer on 2026-11-05.

## Summary

Version 0.1 of the program aktujo is the archive of **one person, on their devices**: capture, improve, read, label,
version, keep and find documents – step 3 of the roadmap in the whitepaper – and, brought forward from step 4,
**sync** between devices through a relay and **sharing** a space with other people (maintainer, 2026-11-05). It is
built on what is there: the core (0001), the document crates `aktujo-doc` and `aktujo-extract` (0002) and
`aktujo-rules` (0003). What the program has today – `aktujo-app`, the API, the web app and the terminal interface – is
the skeleton from the starter pack (a list of items in memory); 0.1 replaces it with the archive.

The audit log as a report, the mobile app, servers for organisations, workflows and signatures are later steps of the
roadmap and not part of 0.1 – but nothing in 0.1 may stand in their way. Import from and export to other programs is
**not** part of 0.1 (maintainer, 2026-11-05).

## What 0.1 does

| Step | In 0.1 | Not yet |
|---|---|---|
| **Capture** | Upload one or more files (PDF, PNG, JPEG, TIFF – OPEN: which exactly); a folder at once; **watched folders**: new files in a folder (from a scanner, for example) are taken in on their own. The same bytes twice are found by their hash and not added again. | Scanner and mail directly, the phone camera. |
| **Improve** | For images and scanned pages: rotate, crop, deskew, black and white or greyscale; the result is a **new version**, the original stays. Several images into one document. | Finding the page's edges, perspective (keystone). |
| **Read** | The text layer, OCR for pages without one (`aktujo-extract`, backend `pure`), in the background after adding. | E-invoices (XRechnung, ZUGFeRD), optional AI. |
| **Label** | Kinds defined per space by its people, or taken over from a template ([0005](0005-pure-core.md)); the kind and the fields suggested from their rules (`aktujo-rules`) and confirmed by the person; title suggested; labels added and removed by hand. | Correspondents of their own, rules that learn from what was labelled before. |
| **Version** | Every version listed, opened and downloaded; a new version by upload or by improving. | Comparing versions, restoring an old one as the newest – OPEN. |
| **Keep** | **Several spaces** – a private archive beside a business's, for example; originals in a folder by hash (`aktujo-blob`), events in SQLite (`aktujo-store`), through `aktujo-sync` without a relay; **encrypted at rest**. The bin: put in, take out. | Retention, deleting for good. |
| **Find** | Full-text search over title, labels, fields and the text that was read; filters by space, kind, label, date and the bin. | Search over encrypted content on other devices; saved searches. |
| **Sync** | A space synchronised through a relay (`aktujo-sync`), end-to-end encrypted: started on one device, which shows the recovery code once; further devices join by an invitation. | Direct sync between devices (P2P). |
| **Share** | Inviting other people's devices with rights – read, write, manage; the members of a space listed; a member removed (a new key, which it does not get). | Groups, people with several devices as one entry, links for people without aktujo. |
| **Export** | Everything at any time: the originals and a JSON file of every document's state, also the events as they are – aktujo's own open format. | Import from and export to other programs. |

## Where it runs

- `aktujo serve` – the API and the web app on `127.0.0.1` (port 8071), opened in a browser on the same computer.
- `aktujo tui` – the same archive in the terminal.
- The command line also gets the steps that are useful in scripts: `add`, `export`, `search` (OPEN: which, and the
  names).
- Both interfaces go through `aktujo-app` (0001); the web app only through the API.
- The interface speaks **English and German**; its texts are data, so that more languages can follow.
- Where the data lives: one folder per user – `aktujo` in the folder for application data (`$XDG_DATA_HOME` or
  `~/.local/share` on Linux, `~/Library/Application Support` on macOS, `%APPDATA%` on Windows), or the one given with
  `--data`; `--memory` keeps nothing, to try it out (built so, 2026-11-05).
- The API answers only requests addressed to `localhost`, `127.0.0.1` or `[::1]`, against pages that rebind their
  name to this device, and takes changes only from its own pages (`Origin`, `Sec-Fetch-Site`); files are uploaded as
  they are, up to 256 MiB each (OPEN: the limit), and shown in the browser only as PDF or image, else downloaded.
- `aktujo relay` – the relay (`aktujo-relay`) for whoever runs one: on a small server or in the home network. Its
  address is given when a space starts to sync. No public relay is run for 0.1 (maintainer, 2026-11-05).
- Linux, macOS and **Windows**; CI checks all three.

## Decisions

Decided by the maintainer on 2026-11-05.

1. **Several spaces.** The first space is founded on the first start; more can be founded, named and chosen in
   every interface. A document belongs to one space. Moving a document between spaces: OPEN.
2. **The state is built from the events.** On start the program reads the logs, builds `Documents` (`aktujo-doc`)
   per space and keeps them in memory; views for searching are kept in SQLite beside the core's schema, as tables of
   the program (0001: no general SQL layer in the core). OPEN: how many documents this must carry (proposal:
   100 000 on an ordinary laptop) and measuring it. Built so (2026-11-05): the program reaches the database through
   `Store::connection` of `aktujo-store`, for its own tables only; the search index is brought up to date for a space
   whenever its state is folded, one document at a time by a stamp of what was indexed.
3. **The text that was read is kept as an original by hash**, beside the version it was read from, so that it syncs
   later and other devices – a phone above all – need not read again. Reading again with better OCR writes a new
   one. Full-text search runs in SQLite FTS5 over that text, the title, labels and field values. Built so
   (2026-11-05): the positioned lines of `aktujo-extract` as JSON, linked to the version by the event `document_read`
   – an amendment to [0002](0002-documents.md). Search, built so (2026-11-05): one FTS5 table over the title, the
   labels, the kind with the field values and the text, case and accents not counting (`unicode61`,
   `remove_diacritics 2`); every word is matched at the start of words and all are needed, `"…"` makes a phrase; hits
   are ordered by BM25 with the title weighing most, and come with an excerpt. The filters – space, kind (confirmed,
   or else suggested), labels (all of them), the bin, and a range of days – run on the state in memory; the day of a
   document is its field `date`, or else the day it was added (UTC). OPEN: whether a range over other date fields is
   needed; searching with words in other languages' word forms (stemming).
4. **Reading in the background.** Adding returns at once; reading, recognising and indexing run in a queue, one
   document after the other, with their state visible (waiting, reading, done, failed). Suggestions are written as
   events when the reading is done. Built so (2026-11-05): the queue is the state – a version waits while it has no
   reading, or, for a reader with OCR, while its reading was made without OCR and left pages unread or failed – so
   nothing is lost when the program stops, and what `aktujo-minimal` read is read again by the full build where that
   helps. Only the newest version of documents outside the bin is read, in spaces this device may write in. The
   reading itself runs outside the workspace's lock, on a thread of its own (`Worker` in `aktujo-app`). A better
   title from the kind's name and the fields of its title is offered while the title is still the file's, not
   written.
5. **Improving writes a new version.** The tools for images are a crate of their own, `aktujo-scan` – MIT like the
   document crates, pure Rust, no I/O – so that apps and other programs can use them: rotate, crop, deskew, black and
   white, greyscale. Edges and perspective come after 0.1, when the camera of the mobile app needs them.
   Built so (2026-11-06): `aktujo-scan` renders a PDF's pages (hayro) or decodes an image, and per page turns by
   quarters, straightens by the skew of the text (moved here from `aktujo-extract`, which uses it for OCR), cuts a
   share of each side away and makes it grey or black and white (by the light around each pixel, so that shadows of a
   photo stay white); pages are chosen and ordered. One page from an image is written as an image again (PNG in grey,
   JPEG in colour), anything else as a PDF of the pages' pictures – colour as JPEG, grey compressed, black and white
   one bit a pixel – written by hand, without a PDF crate; the text layer is not kept, the new version is read again.
   Several documents become one: their pages as a PDF, the newest version of the first, the others into the bin.
   The pictures are made without the workspace's lock. Pages of every version can be looked at in the web app.
   Web app: *Improve …* with a preview of each page; *Combine …* in the list. Terminal: `I` with letters (`d`
   straighten, `g` grey, `s` black and white, `r` a quarter turn). API: `…/versions/{number}/pages[/{page}]`,
   `…/improve/preview`, `…/improve`, `/spaces/{space}/combine` – only in the full build.
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
     `aktujo-extract`). Built so (2026-11-05): the feature `ocr` of `aktujo` and `aktujo-app` (on by default) builds
     them in; `build.rs` takes them from `AKTUJO_OCR_MODELS` or `target/ocr-models`, where
     `scripts/get-ocr-models.sh` puts them, checks their hashes and stops if they are missing – the build itself
     makes no connection. Without `ocr`, `aktujo-extract` is built with the feature `layer`: text layers only,
     without tract. OPEN: hayro's rendering is still in `aktujo-minimal` (the text layer comes through the same
     crate); naming the models' licence and origin in the program; the sizes of both builds (to be measured).
7. **Encrypted at rest.** The database is encrypted as a whole with **SQLCipher** (maintainer, 2026-11-05) – events,
   member records, settings, the seeds of the spaces and the search index alike; the bytes of originals and of the
   text read from them are encrypted in their folder with a key of the device. The key of the device lives in the
   **key store of the operating system** (Keychain on macOS, Secret Service on Linux, Credential Manager on Windows);
   where there is none – a server without a desktop, a container – in a file protected by a passphrase. This answers
   an open point of 0001 for the program. SQLCipher is C with OpenSSL built in, beside SQLite, which is C already; it
   is in `aktujo-minimal` too. Built so (2026-11-05): SQLCipher is the feature `sqlcipher` of `aktujo-store`, so the
   core stays usable without it; the originals are sealed with XChaCha20-Poly1305 in their folder, bound to their hash
   (`SealedBlobs` in `aktujo-app`); the key stores are reached through `keyring`; one secret of 32 bytes per data
   folder, the keys of the database and of the originals derived from it with HKDF-SHA256. OPEN: the passphrase file
   where there is no key store; whether `SealedBlobs` moves into the core.
8. **No import or export of other programs** in 0.1; aktujo's own export stays. Built so (2026-11-06): a space is
   exported to a folder or as a ZIP of its own – `README.txt`; `space.json` (format 1) with the space, its kinds and
   every document's state (title, kind, fields confirmed and suggested, labels, the bin, versions with their file and
   SHA-256); `documents/0001 <title>/vN <file>` every version as it was added and `vN text.txt` what was read (pages
   split by a form feed); `events.jsonl` the events as they are, in their order. No keys, no member list, nothing
   encrypted – the README says so. The ZIP is written by hand (stored, no compression – documents are compressed
   already), up to 4 GiB; a larger space goes to a folder. The workspace is locked to read the state, then once per
   original. Web app: *Export* beside the space; terminal: `E`; command line: `aktujo export [--space …] [--to …]
   [--zip]` (every space without `--space`); API: `GET /api/v1/spaces/{space}/export`. OPEN: ZIP64 for larger
   spaces; reading an export back into aktujo.
9. **English and German** in the interface. Built so (2026-11-05): the texts are data, `locales/<language>.json`,
   shared by the web app (the browser's language, or the one chosen) and the terminal interface (the environment's,
   `LC_ALL`, `LC_MESSAGES`, `LANG`). Names of kinds and fields are data of the kind, with names in other languages
   ([0005](0005-pure-core.md)); the title offered takes the kind's name in the interface's language.
10. **Watched folders** are part of 0.1: a folder is given per space; new files are taken in, then left where they
    are or moved to a folder beside it. Built so (2026-11-06): watched folders belong to the device (a setting of the
    workspace), any number per space; `Watcher` in `aktujo-app` looks at them every 5 s while `serve` or `tui` runs –
    no file events of the operating system, the same everywhere and on network shares. A file is taken once it stayed
    the same between two looks (a scanner still writing is waited for); only PDF, PNG, JPEG and TIFF at the top level,
    hidden and unfinished files (`.part`, `.tmp`, `.crdownload`, `~$…`) left alone; what was taken is remembered by
    name, size and time of change, so it is not read again. The default is to **leave** the file where it is
    (nothing is deleted or moved unasked); moving puts it into `added/` inside the watched folder under a free name.
    In the web app under *Settings*, in the terminal with `W`, in the API under `/api/v1/watches`. OPEN: folders
    inside a watched one; looking while aktujo does not run (a service of the operating system).
11. **Installed from binaries** attached to each GitHub release, built by CI for Linux, macOS and Windows – each
    as `aktujo` and as `aktujo-minimal` (decision 6). Other ways
    (cargo install, Nix, Homebrew) later. OPEN: signing and notarising the binaries for macOS and Windows.
12. **Sync and sharing are part of 0.1**: through a relay the person runs (`aktujo relay`); invitations with rights,
    members listed and removed, the recovery code – what `aktujo-sync` does today, offered in every interface.
    Built so (2026-11-05): `serve` and `tui` synchronise every synchronised space in the background (`Syncer` in
    `aktujo-app`) at once, every minute and after every change, each space on its own, and keep how it went; what
    arrives is read where it needs to be. The device has a name people read, set once in the settings and used when
    it starts a sync, joins or recovers. A device that may only read sees no control that changes something. The web
    app offers joining also on the first start. OPEN: a synchronisation holds the workspace's lock while it talks to
    the relay; a request to join from a device whose device name is taken in the space is passed over without a word
    (`aktujo-sync`), so that device waits for ever – it should be told; the recovery code's member carries an English
    name from `aktujo-sync` (the interfaces show it by its role).
13. **Kinds as data, catalog sources and updates are part of 0.1** (maintainer, 2026-11-06;
    [0005](0005-pure-core.md)): no kinds built in; kinds defined per space; templates taken from `aktujo/catalog` and
    other sources within aktujo, the default source can be switched off; templates taken over can be updated. Built
    so (2026-11-06): kinds as data, their editing, the import of a template as a file, catalog sources and updates
    of templates in every interface ([0005](0005-pure-core.md), "Built so").

## Done when

- A person adds 100 documents – the samples in `testdata/ocr/` and their own – by upload and through a watched
  folder, sees the suggestions, confirms some, labels them, finds them again by a word of their text, puts one in
  the bin and takes it out, adds a second version by improving a scan, founds a second space, and exports
  everything – in the web app and in the terminal, in English and German.
- Starting from a space without kinds, a person takes a template from `aktujo/catalog` within aktujo, sees what it
  would add, changes a kind, and later takes an update of the template with their change kept; a second source is
  added, and the default source switched off is not read.
- A second device joins the space by an invitation and shows the same documents; a change on either arrives on
  the other after a sync through `aktujo relay`; a second person invited to read sees the documents and cannot
  change them; a removed member gets nothing written after its removal.
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
4. The API and the web app for all of it, in English and German; then the terminal interface. Built so
   (2026-11-05): the items are gone; the web app in the colours, font and lockup of `aktujo/brand`, light, dark or
   like the system, the language in its settings; the terminal interface in the language of the environment. OPEN:
   showing a version's pages in the app itself (today: opened in a new tab or downloaded).
5. Sync and sharing in every interface; `aktujo relay`.
6. Kinds as data, catalog sources and updates of templates ([0005](0005-pure-core.md); built 2026-11-06).
7. Watched folders (built 2026-11-06).
8. Improving (`aktujo-scan`; built 2026-11-06).
9. Export (built 2026-11-06).
10. Windows in CI, binaries in the release, the release 0.1.0 of the program.

## Open points

- OPEN: every point marked OPEN above.
- OPEN: the version of the program – it shares the workspace version 0.1.0 with the crates today; whether the
  program and the crates get versions of their own.
- OPEN: real samples from the maintainer (also open in 0002 and 0003) – needed for "done when".
