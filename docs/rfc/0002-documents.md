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
them in – spezado for its receipts, notenwart for sheet music, kernoid for its search index. They are not part of
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

- A **kind** is data, not code: an identity (`invoice`), a name and its fields, each with a key, a name and a type –
  text, date, money, number or yes/no. A few kinds come with the library (invoice, receipt, contract, notice,
  payslip, policy, letter); a program can define its own (notenwart: a part of a piece of music).
- Values of dates, amounts and numbers are written as text (`2026-11-05`, `"119.00"` with a currency), never as
  binary floating point, so that they are the same on every device.
- A deadline is a field of type date like any other. Rules that compute deadlines are not part of this RFC.

## Suggested and confirmed

Everything read from a document – from its text layer, by OCR, by a rule, by an optional AI, by an import – is a
**suggestion** with its source. Only a person **confirms** a value. A field keeps both: the confirmed value and,
apart from it, the latest suggestion. A suggestion never replaces a confirmed value; confirming a value clears the
suggestion. Later events of the same kind win, in the agreed order of the log.

## Reading text

- `aktujo-extract` gives **lines of text with their position** on the page, as fractions of the page (0–1, y from
  the top), for every page, with where each page's text came from: the PDF's own text layer or OCR.
- The trait `Extractor` hides how. The first backend, `external` (a feature), runs the programs that notenwart uses
  today: `pdftotext -bbox-layout` for the text layer, `pdftoppm` and Tesseract for pages without one and for images.
  The parsers of their output are always there, without the feature.
- A backend in pure Rust – so that recognition runs in WebAssembly and in mobile apps, also on end-to-end
  encrypted content on the client – is the subject of a spike (step 2 of the plan). It decides whether `external`
  stays the default. OPEN.
- Not yet: e-invoices (XRechnung, ZUGFeRD), rules that turn lines into field suggestions per kind (`aktujo-rules`),
  optional AI, full-text search. Each comes with its own RFC or as an amendment to this one.

## Origin

The positioned lines, the parsers of pdftotext and Tesseract and their use are taken over from notenwart
(`notenwart-app/src/recognize.rs`, AGPL-3.0-or-later) and published here under MIT by its copyright holder.

## Open points

- OPEN: the pure-Rust backend and whether it becomes the default (spike).
- OPEN: kinds defined by users and stored as events.
- OPEN: removing originals for good.
- OPEN: names of kinds and fields in other languages than English.
