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
- **Templates live apart**, in the repository `aktujo/catalog`: general cases – the paperwork of a household, a
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
  the ways tried in order (`label`, `largest`, `first`, `sender`, `present`, `tallest`, `within`; [0003](0003-recognition.md));
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

`aktujo/catalog` is the official catalog of templates, built like the catalogs of other falkOSS programs: one folder
per template, `templates/<author>/<name>/template.json`, a `source.toml`, and a generated `catalog.json` that a
program reads to browse it; anyone proposes a template by pull request, the aktujo team reviews it, and anyone can run
a catalog of their own. An identity of a kind is in one template of a catalog only, so that taking over two templates
never replaces a kind of the one with the other's. It starts with three general templates – **`aktujo/paperwork`**
(letter, contract, official notice, invoice, receipt, insurance policy, payslip; the rules that came with the library
before, in six languages), **`aktujo/library`** (book) and **`aktujo/research`** (research paper). Accounting in
depth and sheet music are not general cases; they come with the programs of those trades. Today a template is imported
as a file; taking one from a catalog within aktujo is planned below.

## Catalog sources (planned)

State: proposal. Decided by the maintainer on 2026-11-06: aktujo takes templates from catalogs itself and knows other
sources; the default source can be switched off; templates taken over can be updated – all of it part of Archive 0.1
([0004](0004-archive-0.1.md), decision 13), before the watched folders.

- **A source** is a place where a `catalog.json` lives, with the templates next to it at the paths it names – a
  repository on GitHub (`https://github.com/<owner>/<repository>`, optionally `…/tree/<branch>`; read from its raw
  files) or any HTTPS address of a folder. `aktujo/catalog` is the default source; others are added by address. Each
  is switched on or off – **the default source too**: switched off, it is not read and stays off until switched on
  again; it cannot be removed, only switched off, so that it can always come back. Sources belong to the
  **device** (a setting of the workspace), not to a space: which catalogs a person trusts is theirs, the kinds they
  take over go into the space and sync as always.
- **Only when asked.** aktujo reads a source when a person opens the catalog view or asks for it on the command line
  – never at start, never in the background, no counting of downloads. What it read is kept for that view only.
- **The program reads, not the browser.** `aktujo-sdk` fetches over HTTPS (the HTTP client the sync already uses);
  the API and the terminal show what it found. So the web app keeps talking to its own server only, and the terminal
  can do the same.
- **Checked before it is shown.** `catalog.json` must have a `format` aktujo knows (a newer one: the source is shown
  as needing a newer aktujo); a `template.json` must have the SHA-256 that `catalog.json` names for it, else it is
  refused; then its kinds are checked like any kind (`Kind::check`).
- **Official** is a template only when its author is `aktujo` *and* it comes from the default source – an author name
  alone is not enough. Templates of other sources are marked as not reviewed by the aktujo team.
- **Seen before it is taken.** Taking a template over first shows what it would do in this space: which kinds are new,
  which would replace a kind of the same identity (with what differs), which are already the same. Only then are the
  kinds defined (`import_kinds`, as today).
- **Where a kind came from.** A kind taken from a catalog remembers its template: `kind_defined` gets a member
  `template` – the source's address, the template's ID (`aktujo.paperwork`), the SHA-256 of its `template.json`, and
  `base`, the kind as the template defines it. A person's change writes `kind_defined` without `template`; the space
  keeps the last `template` of the kind, so it still knows where the kind came from and what the template said.
  Removing the kind forgets it.
- **A template imported as a file** remembers the same, without a source and an ID (maintainer, 2026-11-06): the
  SHA-256 of the file and `base`. When the catalog view is opened later and a source names a template with that
  SHA-256, the kind is linked to it – its source and ID are written with the next `kind_defined` – and updates come
  as for one taken from the catalog; `base` keeps a person's own changes apart in the three-way merge. A file changed by
  hand matches no catalog; its kinds work as any other, without updates. A list of kinds (not a template) and kinds
  exported from a space remember nothing new: an export keeps the `template` its kinds have.

## Updating a template

- **Found when asked.** When a person opens the catalog view, aktujo compares, for every kind of the space that came
  from a template of a source that is on, the SHA-256 it remembers with the one in `catalog.json`; a kind from a file
  without a source is first matched by its SHA-256 among all templates of the sources that are on. A difference is
  an update; the view lists the templates with updates for this space. Nothing is looked up otherwise.
- **Shown before it is taken**, kind by kind:
  - *Not changed here* (the kind is its `base`): the template's new definition replaces it.
  - *Changed here*: a three-way merge of `base`, this space's kind and the template's new kind, item by item – the
    name, each field by its key, each word, the title. What only one side changed is taken from that side; fields
    and words a person added stay; where both changed the same item differently, this space's stays and the item is
    shown as a conflict, with the template's version beside it to take instead.
  - *New in the template*: offered to add. *Gone from the template*: offered to remove – never removed on its own.
- **Choices per kind:** take the update (merged as above), take the template's version as it is, or keep this space's
  as it is. Each choice writes `kind_defined` with the new `template` (SHA-256 and `base` of the new version), so the
  same update is not offered again; keeping this space's kind does not change it.
- Documents are not touched: their values stay, also of fields a new version no longer has (as for any change of a
  kind).

Built so (2026-11-06):

| Where | What |
|---|---|
| `aktujo-doc` | `TemplateRef` in `kind_defined` (`template`: `source`, `id`, `sha256`, `base`), kept by `Kinds::template`; `merge(base, mine, theirs)` with `Conflict`s |
| `aktujo-sdk` | `sources()`, `add_source`, `set_source`, `remove_source` (not the official catalog; setting `catalog.sources`); `fetch_catalog`, `fetch_template` (without the workspace; a repository on GitHub read from `raw.githubusercontent.com`, HTTP only on this device); `standing` (new, taken, update); `plan` (new, same, replace, merge, gone) and `apply` with a `Choice` per kind (take, theirs, keep, remove); `import_file` for a file as it is |
| API | `GET/POST/PUT/DELETE /api/v1/sources`; `GET /api/v1/spaces/{space}/catalog` (every source that is on, read now, with each template's standing); `POST /api/v1/spaces/{space}/catalog/plan` and `…/catalog/take` (`{"source", "template", "choices"}`; the template is read and planned anew when taken); `POST /api/v1/spaces/{space}/kind-import` takes the file's bytes as they are |
| Web app | *Settings → Sources of templates* (a switch each, the official catalog too); *Kinds → From a catalog …*: templates of all sources that are on, by tag, updates first, with the preview – a choice per kind, conflicts with both versions – then *Take over* |
| Terminal | `C`: the templates of the sources that are on, numbered, with their standing; the number takes one over as proposed |
| Command line | `aktujo templates [--source <url>]` lists a catalog; `aktujo templates take <id> --space <space>` takes one over as proposed |

## Interfaces

- **API:** `GET/POST /api/v1/spaces/{space}/kinds`, `PUT/DELETE /api/v1/spaces/{space}/kinds/{kind}`,
  `POST /api/v1/spaces/{space}/kind-import`. A suggested title takes the kind's name in the language of
  `Accept-Language`.
- **Web app:** *Kinds* beside *Sync and sharing*: add, rename, fields with type, the labels a field is found after,
  the fields of the title, the words; everything else as JSON.
- **Terminal:** `c` chooses a kind for a document by name or identity, `K` imports a template from a file. Editing
  kinds in the terminal: OPEN.

## Open points

- Until catalog sources and updates are built, a second import of a template replaces kinds of the same identity,
  also the changes a person made to them.
- OPEN: plugins – code behind `Recogniser` added to an installed aktujo (WebAssembly?), and the rules of app stores
  for it ([0003](0003-recognition.md)).
- OPEN: the date order (`day_first`, `month_first`) per space; today the program reads slashed dates only where they
  cannot mean two days.
