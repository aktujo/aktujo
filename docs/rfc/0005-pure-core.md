---
title: "0005 A pure core"
parent: Design
nav_order: 5
---

# 0005 A pure core: kinds and rules as data of a space

State: draft. Changes [0002](0002-documents.md) (kinds) and [0003](0003-recognition.md) (rules); decided by the
maintainer on 2026-11-06.

## Summary

aktujo is for any documents – invoices, sheet music, research papers, a private collection of books. So nothing in it
belongs to one trade, and nothing in it exists for another program or protocol:

- **No kinds are built in.** A kind of document – its name, its fields, the words that speak for it, the ways to find
  its fields – is **data of a space**, written as events like everything else. A new space has none.
- **No rules are built in.** `aktujo-rules` is the mechanism only: it reads what the kinds of a space say. The words,
  labels, legal forms and authorities that came with the library before are a template now.
- **Templates live apart**, in the repository `aktujo/packs`: general cases – the paperwork of a household, a
  library, research. Taking one over is one step; it is optional.
- **A trade's kinds come with its programs**, which build aktujo in and bring their kinds, and code where data is not
  enough (`Recogniser`).
- **aktujo refers to no other program or protocol.** The core derives its keys with its own labels; `Domain` stays
  a parameter, so that a program building the core in can keep its keys apart (see [0001](0001-architecture.md)).

## A kind is data of its space

A kind (`aktujo_doc::Kind`) has

- an **identity** – `a-z`, `0-9`, `.`, `-`, `_`, at most 64 characters; made from the name when a person adds a kind
  (`Research paper` → `research-paper`), given by a template otherwise;
- a **name**, and names in other languages by language tag (`{"de": "Buch"}`) – an interface shows the name in its
  language if the kind has one;
- **fields**, in the order they are shown: a key (like an identity), a name and names, a type (`text`, `date`,
  `money`, `number`, `flag`), and how to find it in a text – `code` (only the first word with a digit) and `find`,
  the ways tried in order (`label`, `largest`, `first`, `sender`, `present`; [0003](0003-recognition.md));
- **words** that speak for it, each with a whole weight; without words a kind is never suggested, only chosen;
- a **title**: the fields a suggested title is made of after the kind's name, in groups – the first with a value of
  each group.

Two events of `aktujo-doc` hold the kinds of a space:

| Event | Payload | Meaning |
|---|---|---|
| `kind_defined` | `definition`: the whole kind | defines a kind, or defines it anew – the whole definition replaces the one before |
| `kind_removed` | `id` | removes a kind; documents keep it as their kind, and it can be defined again |

`Kinds::from_log` folds them in the agreed order, like documents. A definition that is not valid – a bad identity or
name, a field twice, a title of a field the kind does not have – is reported (`KindSkipped`), never applied. Because
events hold no floating-point numbers, weights are whole numbers; a kind is suggested from a sum of **6** (before: 3,
with weights of half steps – the same as doubling every weight).

A document's kind is still just its identity (`document_kind`). The workspace accepts only kinds the space has; a
value must fit the type of the field if the kind has that field, else any well-formed value is taken.

## Templates

A template is JSON: `name`, `names`, `description`, `descriptions` and `kinds` in exactly the JSON of a kind. Taking
one over (`Workspace::import_kinds`, the API's `kind-import`, *Import template* in the web app, `K` in the terminal)
checks every kind first and then defines each that is new or different; a kind with the same identity is replaced.
*Export kinds* writes a space's kinds in the same form.

`aktujo/packs` starts with three general templates – **paperwork** (letter, contract, official notice, invoice,
receipt, insurance policy, payslip; the rules that came with the library before, in six languages), **library**
(book) and **research** (research paper). Accounting in depth and sheet music are not general cases; they come with
the programs of those trades.

## Interfaces

- **API:** `GET/POST /api/v1/spaces/{space}/kinds`, `PUT/DELETE /api/v1/spaces/{space}/kinds/{kind}`,
  `POST /api/v1/spaces/{space}/kind-import`. A suggested title takes the kind's name in the language of
  `Accept-Language`.
- **Web app:** *Kinds* beside *Sync and sharing*: add, rename, fields with type, the labels a field is found after,
  the fields of the title, the words; everything else as JSON.
- **Terminal:** `c` chooses a kind for a document by name or identity, `K` imports a template from a file. Editing
  kinds in the terminal: OPEN.

## Open points

- OPEN: updating a template already taken over – today a second import replaces kinds of the same identity, also
  the changes a person made to them.
- OPEN: plugins – code behind `Recogniser` added to an installed aktujo (WebAssembly?), and the rules of app stores
  for it ([0003](0003-recognition.md)).
- OPEN: the date order (`day_first`, `month_first`) per space; today the program reads slashed dates only where they
  cannot mean two days.
