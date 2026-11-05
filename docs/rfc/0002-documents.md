---
title: "0002 Documents"
parent: Design
nav_order: 2
---

# 0002 Documents: what a document is and how text is read from it

State: draft.

## Summary

Two libraries sit between the core and the program: `aktujo-doc` (what a document is) and `aktujo-extract`
(reading text with its position from PDFs and images). Both are **MIT** like the core, because other programs build
them in. They are not part of
the core: the core knows no documents. They depend on the core, never on the program.

## A document is not a file

- A **document** has an identity (a UUID, version 7), a title, a kind, fields, labels and one or more **versions**.
  A version is an original – the bytes, stored by their hash in `aktujo-blob` – with its file name and media type.
  A new scan of the same letter is a new version of the same document, not a new document.
- Everything that happens to a document is an **event** in the log of its space (`aktujo-log`): added, a version
  added, the title, the kind or a field set, a label added or removed, put in the bin or taken out. The state of
  all documents follows from the events in the agreed order; for the same events every device has the same state.
  Events of kinds that are not about documents are left alone, so a space can hold the events of other programs too.
- **Nothing is deleted** by an event: a document goes into the bin and can come back. Removing originals for good
  (retention, a right to erasure) is OPEN.

## Kinds and fields

- A **kind** is data, not code: an identity (`letter`), a name and its fields, each with a key, a name and a type –
  text, date, money, number or yes/no. None come with the library: kinds are data of a space, defined by its people
  or taken over from a template ([0005](0005-pure-core.md)).
- Values of dates, amounts and numbers are written as text (`2026-11-05`, `"119.00"` with a currency), never as
  binary floating point, so that they are the same on every device.
- A deadline is a field of type date like any other. Rules that compute deadlines are not part of this RFC.

## Suggested and confirmed

Everything read from a document – from its text layer, by OCR, by a rule, by an optional AI, by an import – is a
**suggestion** with its source. Only a person **confirms** a value. A field keeps both: the confirmed value and,
apart from it, the latest suggestion. A suggestion never replaces a confirmed value; confirming a value clears the
suggestion. Later events of the same kind win, in the agreed order of the log. The kind of a document is suggested
and confirmed the same way ([0003](0003-recognition.md)).

## Reading text

- `aktujo-extract` gives **lines of text with their position** on the page, as fractions of the page (0–1, y from
  the top), for every page, with where each page's text came from: the PDF's own text layer or OCR.
- The trait `Extractor` hides how. Two backends, each a feature:
  - **`pure`** (the default, decided 2026-11-05 after a [spike](../spikes/ocr.md), step 2 of the plan): pure Rust everywhere –
    on servers, desktops, in WebAssembly and in mobile apps, also on end-to-end encrypted content on the client. The
    text layer and the rendering of pages come from hayro; OCR runs PaddleOCR's PP-OCR models (detection, then
    recognition of each line) on tract, after the page is straightened (deskew). The models are data: loaded from
    bytes the program hands over, never part of the crate. Without models the backend reads text layers only.
  - **`external`**: programs on the system – `pdftotext -bbox-layout` for the text layer, `pdftoppm` and
    Tesseract for pages without one and for images. Optional, for whoever wants it; no longer the default.
  The parsers of the external programs' output are always there, without either feature.
- In the [spike](../spikes/ocr.md) PP-OCR read more exactly than Tesseract (umlauts, ß and € included; sheet music too) but took 2–5 s per
  A4 page against Tesseract's 0.5–0.9 s. The maintainer decided for pure Rust everywhere anyway, servers included
  (2026-11-05).
- With the feature `layer` alone, `Pure` reads text layers only – without tract, rendering for OCR or models; with
  the feature `serde`, what was read can be kept as JSON (amendment, 2026-11-05, for `aktujo-minimal` in
  [0004](0004-archive-0.1.md)).
- **What was read is kept** (amendment, 2026-11-05, [0004](0004-archive-0.1.md) decision 3): the positioned lines
  of a version as JSON – `{"format": 1, "pages": [{"width", "height", "method", "lines": [{"text", "x0", "y0", "x1",
  "y1"}]}]}`, `method` one of `text_layer`, `ocr`, `none` – stored as an original of their own by hash, so that it
  syncs like any original and other devices need not read again. The event **`document_read`** links it to the
  version: the document, `version` (the hash of the version's original), `text` (the hash of what was read, or
  none), `ocr` (whether the reader could use OCR), `pages`, `unread` (pages without text that could be read) and
  `error` (why reading failed, or none). The latest reading of a version holds; a reading of a version the document
  does not have is reported, not applied. Reading is not a change by a person: it leaves the time the document last
  changed alone.
- Rules that turn lines into a kind and field suggestions are `aktujo-rules` ([0003](0003-recognition.md)). Not yet:
  e-invoices (XRechnung, ZUGFeRD), optional AI, full-text search. Each comes with its own RFC or as an amendment to
  this one.

## Open points

- How the program ships the OCR models: built in ([0004](0004-archive-0.1.md), decision 6). OPEN: naming their
  licence in the program.
- OPEN: speed of OCR (threads, a smaller detection input), rotated lines without deskewing, perspective correction.
- OPEN: measuring with real samples (phone photos, faded receipts, stamps, scanned sheet music).
- Kinds defined by users and stored as events: [0005](0005-pure-core.md).
- OPEN: removing originals for good.
- OPEN: names of kinds and fields in other languages than English.
