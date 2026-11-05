---
title: "0003 Recognition"
parent: Design
nav_order: 3
---

# 0003 Recognition: the kind and the fields of a document, from its text

State: draft.

## Summary

`aktujo-rules` turns the positioned lines that `aktujo-extract` read into **suggestions**: the kind of a document
(invoice, receipt, notice …) and values for the fields of that kind (`aktujo-doc`). It is a library, **MIT** like the
other document crates, without I/O; it depends on `aktujo-doc` and `aktujo-extract` (without their backends), never on
the program. Step 3 of the plan: recognition, taken over from notenwart in general form.

## General, so that others specialise it

aktujo is meant to be built in by many programs, and to be a program of its own. So `aktujo-rules` knows only what
every archive needs: the kinds of `aktujo-doc` and the values every document has – who sent it, when, a reference,
an amount, a deadline. What a field means in one trade is not its business:

- **notenwart** specialises it for sheet music – title, composer, arranger, the part and its instrument.
- **spezado**, on **librotenado**, specialises it for accounting – net and tax, VAT identifiers, IBANs, line items,
  e-invoices (XRechnung, ZUGFeRD, after EN 16931), which librotenado already specifies.

A program specialises in three ways, which it can combine:

1. **Kinds as data:** its own kinds, or the built-in kinds with more fields (`aktujo-doc`).
2. **Rules as data:** rules in JSON, added to the built-in ones (`Rules::extend`): more words for a kind, more
   fields, more ways for a field already there.
3. **Code:** its own recogniser behind the trait `Recogniser`. `Chain` runs several and merges their results: the
   kind the most sure one names, and of each field the value the most sure one found for that kind.

The readers of dates, amounts, labels and names (`aktujo_rules::text`) are public, so that a specialisation builds
on them instead of repeating them.

## Rules are data

- For each kind, words that speak for it with a weight, and for each field a list of ways to find its value, tried
  in order. Rules are read from and written as JSON.
- The rules that come with the library read **German, English, French, Italian, Spanish and Dutch** documents.

## The kind

- Every kind gets a score: the sum of the weights of its words that occur in the text of the first two pages, each
  word once. Words are matched as whole words, case does not matter.
- The kind with the highest score is suggested if its score reaches 3 and is higher than any other. Its confidence
  (0–1) grows with the score and with the distance to the second.
- A program may name the kind instead (a person confirmed or chose it); the fields are then read for that kind.

## The fields

The ways to find a value, each with a confidence that the value is right:

| Way | What it does | Confidence |
|---|---|---|
| `label` | after a label (`Rechnungsnummer`, `Invoice date`, `Date de facture`) on the same line; else on the line to its right or right below | 0.8 / 0.6 |
| `largest` | among all values after the labels, the largest – for totals, when several amounts carry a label | 0.7 |
| `first` | the first value of the field's type on the first page – for a date without a label | 0.35 |
| `sender` | the organisation in the letterhead or the sender line: a line near the top (or in the footer) with a legal form (`GmbH`, `Ltd`, `SARL`, `S.r.l.`, `B.V.`) or a word of an authority (`Finanzamt`, `Mairie`, `Gemeente`); else the topmost line of words | 0.6 / 0.3 |
| `present` | yes, if one of the phrases occurs (`Betrag dankend erhalten`) – for flags | 0.6 |

Values are read by their field's type and written as `aktujo-doc` wants them:

- **Dates:** `05.11.2026`, `5.11.26`, `05-11-2026`, `2026-11-05`, `5. November 2026`, `1er décembre 2026`,
  `5 de noviembre de 2026`, `November 5, 2026`, month names in the six languages. Dates with slashes are read in the
  order the rules name (`date_order`: `day_first` or `month_first`); without one only where they cannot mean two
  days (`25/12/2026`), because `03/04/2026` differs between countries.
- **Amounts:** `1.234,56 €`, `EUR 1,234.56`, `1 234,56 €` (no-break space), `CHF 1'234.50`, with cents. The
  currency is the sign or code next to the amount; without one, the currency named most often in the document.
  Without any, no amount is suggested.
- **Text:** the rest of the line after the label, without separators; for numbers and references the first word
  with a digit that is not a date (`2026-0042`).

The confidence is lower (× 0.9) when the page was read by OCR. Every suggestion names its rule (`invoice.total`),
the page and the line it came from, so that an interface can show where it was found.

## Into events

- The kind is a suggestion like a field: `document_kind` carries an origin (`suggested` with its source, or
  `confirmed`). A document keeps the confirmed kind and, apart from it, the latest suggested one; confirming a kind
  clears the suggestion. Events of `document_kind` written before carry no origin and are no longer read – none were
  written outside tests.
- A recogniser's result becomes events through `Recognition::events`: the kind (source `rule`, name `kind`) unless
  a person confirmed one, and the fields (source `rule`, the rule's name) without a confirmed value whose suggestion
  is not already the same. If a person confirmed another kind, nothing is suggested from a result for a different
  kind; `Recogniser::suggest` reads the document again for the confirmed kind.
- A **title** is suggested from the kind, the sender and the date (`Invoice · Stadtwerke Musterstadt GmbH ·
  2026-11-05`). `document_title` has no origin, so the program offers it while the title is still the file name and
  writes it once a person agrees.
- Nothing is written on its own; a person confirms, or not.

## Origin

The approach – labels before a value, cleaning names, telling the publisher by its marks, the size and position of
lines – is taken over from notenwart (`notenwart-app/src/recognize.rs`, AGPL-3.0-or-later) and published here under
MIT by its copyright holder. The rules for sheet music stay in notenwart until it builds on aktujo.

## Open points

- OPEN: measuring with real samples; the weights and confidences are first estimates.
- OPEN: plugins for the program aktujo – a specialisation (librotenado's invoices, notenwart's sheet music) added to
  an installed aktujo without building it anew: rules and kinds as data are possible today; code as a WebAssembly
  module behind `Recogniser` is a candidate (sandboxed, the same on every platform). Whether app stores allow it is
  not checked.
- OPEN: names of kinds and fields in other languages than English (also open in [0002](0002-documents.md)).
