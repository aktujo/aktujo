---
title: "0003 Recognition"
parent: Design
nav_order: 3
---

# 0003 Recognition: the kind and the fields of a document, from its text

State: draft. Changed by [0005](0005-pure-core.md): the rules are data of the kinds of a space; none are built in.

## Summary

`aktujo-rules` turns the positioned lines that `aktujo-extract` read into **suggestions**: the kind of a document
and values for the fields of that kind (`aktujo-doc`). It is a library, **MIT** like the other document crates,
without I/O; it depends on `aktujo-doc` and `aktujo-extract` (without their backends), never on the program.

## The mechanism, not the content

aktujo is for any documents, so `aktujo-rules` knows no kinds, words or labels. What speaks for a kind and where its
fields are is **data of the kind** – defined by the people of a space, taken over from a template of `aktujo/packs`,
or brought by a program that builds aktujo in ([0005](0005-pure-core.md)). Where data is not enough, a program brings
code:

1. **Kinds as data:** a kind's words with their weights, and for each field the ways to find it (`aktujo-doc`).
2. **Code:** a recogniser of its own behind the trait `Recogniser`. `Chain` runs several and merges their results:
   the kind the most sure one names, and of each field the value the most sure one found for that kind.

The readers of dates, amounts, labels and names (`aktujo_rules::text`) are public, so that a program builds on them
instead of repeating them. They read **German, English, French, Italian, Spanish and Dutch** dates and amounts.

## The kind

- Every kind gets a score: the sum of the weights of its words that occur in the text of the first two pages, each
  word once. Words are matched as whole words, case does not matter.
- The kind with the highest score is suggested if its score reaches 6 and is higher than any other. Its confidence
  (0–1) grows with the score and with the distance to the second.
- A program may name the kind instead (a person confirmed or chose it); the fields are then read for that kind.

## The fields

The ways to find a value, each with a confidence that the value is right:

| Way | What it does | Confidence |
|---|---|---|
| `label` | after a label (`ISBN`, `Order no.`, `Aktenzeichen`) on the same line; else on the line to its right or right below | 0.8 / 0.6 |
| `largest` | among all values after the labels, the largest – e.g. a total, when several amounts carry a label | 0.7 |
| `first` | the first value of the field's type on the first page – for a date without a label | 0.35 |
| `sender` | the name in the letterhead or the sender line: a line near the top (or in the footer) with one of the markers the field names (a legal form, a word of an authority …); else the topmost line of words | 0.6 / 0.3 |
| `present` | yes, if one of the phrases occurs – for flags | 0.6 |

Values are read by their field's type and written as `aktujo-doc` wants them:

- **Dates:** `05.11.2026`, `5.11.26`, `05-11-2026`, `2026-11-05`, `5. November 2026`, `1er décembre 2026`,
  `5 de noviembre de 2026`, `November 5, 2026`, month names in the six languages. Dates with slashes are read in the
  order the rules name (`date_order`: `day_first` or `month_first`); without one only where they cannot mean two
  days (`25/12/2026`), because `03/04/2026` differs between countries (OPEN: the order per space).
- **Amounts:** `1.234,56 €`, `EUR 1,234.56`, `1 234,56 €` (no-break space), `CHF 1'234.50`, with cents. The
  currency is the sign or code next to the amount; without one, the currency named most often in the document.
  Without any, no amount is suggested.
- **Text:** the rest of the line after the label, without separators; for numbers and references the first word
  with a digit that is not a date (`2026-0042`).

The confidence is lower (× 0.9) when the page was read by OCR. Every suggestion names its rule (`kind.field`),
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
- A **title** is suggested from the kind's name – in the reader's language if the kind has a name in it – and the
  fields the kind names for its title (`Letter · Example Ltd · 2026-11-05`). `document_title` has no origin, so the
  program offers it while the title is still the file name and writes it once a person agrees.
- Nothing is written on its own; a person confirms, or not.

## Open points

- OPEN: measuring with real samples; the weights and confidences are first estimates.
- OPEN: plugins for the program aktujo – code behind `Recogniser` added to an installed aktujo without building it
  anew: kinds as data are possible today; code as a WebAssembly module is a candidate (sandboxed, the same on every
  platform). Whether app stores allow it is not checked.
