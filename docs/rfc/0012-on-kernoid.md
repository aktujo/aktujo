---
title: "0012 On kernoid"
parent: Design
nav_order: 12
---

# 0012 On kernoid: the core becomes kernoid's foundation, aktujo keeps the documents

State: draft. Replaces [0001](0001-architecture.md) as far as the core goes, and changes [0004](0004-archive-0.1.md)
(the relay), [0006](0006-apps-and-sdk.md) (the SDK and the apps), [0009](0009-streams.md) and
[0011](0011-servers-and-accounts.md) (who builds what). The direction was decided by the maintainer on 2026-11-19
(kernoid RFC 0008, decisions 1, 5 and 6); the steps are a proposal.

## Summary

aktujo built a core of its own – originals by hash, an event log that merges by union, the keys of a space's members,
SQLite on a device, sync, a relay. Since 2026-11-19 kernoid is the core of a suite, and aktujo is an add-on to it. kernoid
took aktujo's core over as it was at aktujo 0.4.1 and released it as its **foundation** with kernoid 0.5.1 on
2026-11-20, together with **streams** – the mechanism of [0009](0009-streams.md) – and a relay that keeps them
([kernoid RFC 0010](https://github.com/kernoid/kernoid/blob/main/rfcs/0010-foundation.md),
[RFC 0012](https://github.com/kernoid/kernoid/blob/main/rfcs/0012-streams.md)).

This document says how aktujo moves onto it:

- **The core goes.** `aktujo-base`, `-blob`, `-log`, `-crypt`, `-store`, `-sync` and `-relay` are replaced by
  kernoid's crates; aktujo keeps none of its own.
- **Documents stay aktujo's.** Documents, kinds, recognition, scanning, the catalog, the SDK, the program and the apps
  are aktujo's – and they move from one log per space onto **streams**, in one format change, with nothing kept from
  0.4.
- **What aktujo designed after 0.4.1** ([0007](0007-erasure-and-locks.md)–[0011](0011-servers-and-accounts.md)) is
  split: the generic mechanism is kernoid's (much of it is there), what is about documents stays aktujo's.
- **Accounts** for the server mode come from kernoid (`kernoid-auth`, or kernoid's identity built in).
- **The apps** are put together from kernoid's Flutter packages, with a feature package of aktujo's own.

## Decided

By the maintainer, 2026-11-19 (kernoid RFC 0008, section 1):

1. kernoid is the core of a suite; aktujo (documents, the archive, signing) is an add-on to it.
2. The files, JMAP and end-to-end parts aktujo built become kernoid's; aktujo keeps what is about documents.
3. Order: kernoid 0.5.1 completes this and works; **then aktujo moves onto it**, then notenwart and spezado.
4. Nothing is kept compatible with aktujo 0.4 ([0009](0009-streams.md), decision 6).

## 1. The core: kernoid's crates

| aktujo (0.4.1) | kernoid (0.5.1) | Licence | Where |
|---|---|---|---|
| `aktujo-base` | `kernoid-base` | MIT | crates.io |
| `aktujo-blob` | `kernoid-blob` (features `object`, `s3`; streamed, ranges, `with_prefix`, async – as at aktujo 0.4.1) | MIT | crates.io |
| `aktujo-log` | `kernoid-log` | MIT | crates.io |
| `aktujo-crypt` | `kernoid-crypt` (with `pq` and `stream`) | MIT | crates.io |
| `aktujo-store` (SQLite), `aktujo-sync` | `kernoid-spaces` (`store`, `streams`; features `sqlcipher`, `native`) | MIT | crates.io |
| `aktujo-relay` | `kernoid-relay` – the service `relay` of a kernoid installation | **AGPL** | kernoid's repository; not on crates.io (checked 2026-11-20) |
| `aktujo-store` (PostgreSQL, `PgStore`) | – (not taken over, kernoid RFC 0010, section 2) | – | – |

The mapping is kernoid RFC 0010, section 2. The crates of kernoid were published as 0.5.1 on 2026-11-20 (crates.io,
checked the same day).

- **Labels.** aktujo passes its own `Domain` – `{ label: "aktujo/crypt", scheme: "aktujo" }`, as today
  (`aktujo-crypt`'s `Domain::AKTUJO`) – so that its keys open nothing of kernoid's or another program's. It lives in
  `aktujo-core`, the only place aktujo names it.
- **Dependencies.** `aktujo-doc`, `-rules`, `-core` and `-sdk` depend on `kernoid-base`, `-blob`, `-log`, `-crypt` and
  `-spaces` from crates.io, as ranges (`0.5`). The SDK stays MIT: none of these is copyleft, and CI's check that nothing
  under the SDK is copyleft stays. kernoid's crates join `aktujo/models` as an exception to the rule that a dependency
  is at least 14 days old (`CONTRIBUTING.md`): they are releases of the same maintainer.
- **The relay.** A space of aktujo is kept by kernoid's relay: an installation with `[services] relay = true` and
  `aktujo` in `relay.domains`, below `/relay/aktujo/v2/spaces/…`. A relay alone is a kernoid with every other service
  off (kernoid RFC 0008, section 5) – it needs PostgreSQL; object storage only beyond 64 KiB per envelope (kernoid RFC
  0010, section 4).
- **`aktujo relay` goes.** [0004](0004-archive-0.1.md) let a person run a relay on their own computer with SQLite.
  kernoid's relay runs on PostgreSQL only, and kernoid keeps SQLite out of its server (kernoid `CLAUDE.md`). The command
  is removed; `docs/install.md` shows a kernoid that is a relay alone, in Compose. **OPEN** (maintainer): whether that
  is enough for a person, or aktujo keeps a small relay of protocol 2 on SQLite of its own – a second implementation
  of kernoid's protocol to keep in step.

## 2. Documents onto streams

What aktujo keeps – `aktujo-doc`, `-extract`, `-rules`, `-scan`, `-core`, `-sdk`, `-api`, `-web`, `-tui`,
`-locales`, `aktujo`, the catalog, the models – stays aktujo's. The events of `aktujo-doc` (versions, fields, kinds,
`document_read` …) are no longer written into one log per space (`aktujo-sync`'s `Replica`) but into **streams**
(`kernoid_spaces::streams::Streams`, sealed by `kernoid_crypt::stream`), as [0009](0009-streams.md) lays them out:

| Stream | aktujo's events in it |
|---|---|
| space (`Streams::space_stream`) | kinds (`kind_defined`, `kind_removed`), catalog sources, settings, roles, persons' pseudonyms, links to what is shown to the space |
| person | the person's name, links to what is shown to it |
| folder | its name and settings, links to its documents and to folders in it (labels become folders, [0008](0008-visibility.md)) |
| document | the document's events: versions, fields, read text, locks, purge; its originals (`Streams::put_original`, in parts of 16 MiB) |

- **One stream per document.** A document is made with `Streams::new_stream` below the stream that shows it (the space,
  a folder or a person), its originals belong to it, and purging it removes the stream at the relay
  (`Streams::purge`).
- **The device's store.** The SDK keeps `kernoid_spaces::store::Store` with the feature `sqlcipher`; aktujo's own
  tables (search with FTS5, what was read, watched folders) stay beside it through `Store::connection`, as today. The
  seed stays in the system's key store (`aktujo-sdk`, `system-keys`) – kernoid RFC 0012 leaves that to each program
  (section 11).
- **Projection.** The SDK builds documents from the streams it can open, as it builds them from the log today; what a
  device cannot open it does not know of ([0009](0009-streams.md), section 7).
- **Nothing kept.** A space of 0.4 is not read by 0.5. Export (a folder or a ZIP) and import stay; an export of 0.4 can
  be read back into 0.5 by `aktujo import` – **OPEN** whether that is worth doing, since nobody uses 0.4 yet.

## 3. What aktujo designed after 0.4.1

| RFC | Already in kernoid 0.5.1 | kernoid still has to add | Stays aktujo's |
|---|---|---|---|
| [0007](0007-erasure-and-locks.md) Erasure and locks | purging a stream with its originals (`Streams::purge`, permission `purge`); removing a person and a device, new generations when due (kernoid RFC 0012, section 8); persons with identities `p-…` | – | owners, private documents, locks and unlocks (rules, by time), the person's sealed name and pseudonyms, what happens when a person leaves, recovery codes of a person for what is private (`Streams::add_person` gives one – how aktujo offers it is aktujo's) |
| [0008](0008-visibility.md) Visibility | links into streams and to persons (`new_stream` below a parent, `share`, `link_below`); a device follows only what it can open | groups as a stream kind of the foundation, if more than one program needs them – **OPEN** (until then aktujo's: a group is a stream with links) | audiences, folders and what they pass on, guest links, branches and merging, search over what a device reads |
| [0009](0009-streams.md) Streams | the whole mechanism: streams, generations, the tree of keys, tokens, protocol 2 of the relay, follow, removing, `server` in the roster, hybrid keys (`pq`) | a review of `stream` and `pq` by someone besides their author (kernoid RFC 0012, section 11) | the permissions beyond `write`, `members`, `purge` (`roles`, `groups`, `share`, `link`, `lock`, `unlock`) and the roles Admin, User, Guest – data of the space stream, checked by the devices at a record's `at` |
| [0010](0010-signatures.md) Signatures | – | – | all of it: `aktujo-sign`, after its spike |
| [0011](0011-servers-and-accounts.md) Servers and accounts | accounts, sign-in, sessions, tenants (`kernoid-auth`, `kernoid_server::embedded`, kernoid RFC 0009) | a **server as a node** that keeps streams on PostgreSQL – `kernoid-spaces` is SQLite only and kernoid keeps SQLite out of its server (kernoid RFC 0010, section 8: "with aktujo's server mode") | organisations' spaces, the link account → person, acting `for` a person, filtering by audience, the web app's administration |

## 4. The server mode

[0011](0011-servers-and-accounts.md) wanted a crate taken out of kernoid; kernoid 0.5.1 has it in two forms (kernoid RFC
0009):

- **Beside a kernoid:** `aktujo-api` signs people in through `kernoid-auth` (crates.io, AGPL) – OpenID Connect against
  a kernoid or any provider, the program's own sessions, the `Principal` with tenant and project roles. An
  organisation is a **tenant** of that kernoid; server and organisation administrators are kernoid's roles; rights in a
  space stay aktujo's.
- **Built in:** `aktujo serve --config server.toml` starts kernoid's identity in its process
  (`kernoid_server::embedded::Embedded`, `kernoid-server` without default features), its tables in the schema
  `kernoid` of aktujo's PostgreSQL database, aktujo's routes behind kernoid's layers. kernoid's paths (`/login`, `/app`,
  `/v1`, `/graphql`, `/oauth/v2`, `/oidc/v1`, `/idp`, `/scim/v2`, `/.well-known/…`) are not aktujo's; aktujo's API moves
  below `/aktujo/` in server mode. **OPEN:** `kernoid-server` is taken by git at a tag (kernoid RFC 0009, section 3) –
  `aktujo` is on crates.io and cannot publish with a git dependency.
- **Licence:** with kernoid's identity built in, the program is distributed under **AGPL-3.0-only** (kernoid RFC 0009,
  section 6); its own files stay AGPL-3.0-or-later. Beside a kernoid nothing changes.
- **Personal mode** ([0011](0011-servers-and-accounts.md), section 1) stays without accounts, on `localhost`.

Which of the two comes first: **OPEN** (maintainer). Proposal: beside, with `kernoid-auth` – it is on crates.io, and
it works with any provider.

## 5. The apps

`aktujo/apps` today is a Flutter app with a bridge of its own on `aktujo-sdk` ([0006](0006-apps-and-sdk.md)). On kernoid
(kernoid RFC 0011):

- The app is put together from `kernoid_shell`, `kernoid_ui` and `kernoid_l10n`, and a feature package of aktujo's own,
  **`aktujo_documents`** (documents, capture and scanning, kinds and catalog, search, export), MIT, in `aktujo/apps`
  under `packages/`. Under aktujo's name, bundle and brand (`KTokens` from `aktujo/brand`: seal red, Space Mono).
- **One native library:** aktujo's bridge crate (`aktujo_bridge`) depends on `aktujo-sdk` and on kernoid's kit, and
  generates its own Dart code (kernoid RFC 0011, section 4). Whether kernoid's feature packages and the shell can call
  aktujo's bridge, or aktujo's bridge generates kernoid's API again, is a **spike** – the first step for the apps.
- **Without a server.** aktujo's app is first a personal archive on one device, with sync through a relay later. The
  shell starts with a login to a kernoid; **kernoid has to add** a start without an account, with features that need
  none.
- Files in the system's file manager stay kernoid's app's (kernoid RFC 0011, section 5).

## 6. crates.io

aktujo's seven core crates are on crates.io up to 0.4.1. Proposal: a last version **0.4.2** of each, with a README that
points to the kernoid crate replacing it, and the crate marked deprecated (`[badges] maintenance = { status =
"deprecated" }`); nothing yanked, since yanking breaks whoever pinned it. The crates aktujo keeps go on as 0.5.0.
**OPEN** (maintainer): deprecate as proposed, yank, or leave them as they are.

## 7. Steps

Each step ends with every check green (`CLAUDE.md`, Checks) and is a commit of its own; pushing as the maintainer says.

1. **This document** – accepted by the maintainer.
2. **The core swapped, behaviour unchanged where possible:** `aktujo-doc`, `-rules`, `-core`, `-sdk` on `kernoid-base`,
   `-blob`, `-log`, `-crypt`; `aktujo-base`, `-blob`, `-log`, `-crypt` moved to `_to_delete/` (outside the
   repository, as the rules say for anything removed).
3. **Spaces on streams:** the SDK on `kernoid_spaces::streams` with aktujo's `Domain` – founding, inviting, joining,
   recovering, writing, following, originals, purging; `aktujo-store` and `-sync` go. Tests: two devices against a
   kernoid with the relay (as `kit/spaces/tests/server.rs` does), in CI on Linux.
4. **The relay:** `aktujo relay` and `aktujo-relay` go (or a relay of aktujo's own, as section 1 leaves open);
   `aktujo-api` and the TUI name a relay's URL instead; docs for a kernoid as relay.
5. **Folders, persons, roles and permissions** of [0007](0007-erasure-and-locks.md)–[0009](0009-streams.md) as
   aktujo's data in its streams – labels become folders.
6. **Version 0.5.0** of aktujo's crates and the program; the core's crates as section 6 decides.
7. **The apps:** the bridge spike, then `aktujo_documents` on kernoid's shell (needs the start without an account).
8. **The server mode** ([0011](0011-servers-and-accounts.md)) once kernoid has a server node (section 3).

## 8. What kernoid must deliver first

For kernoid's session to take up – nothing here is changed in kernoid's repository from aktujo:

1. **`kernoid-relay` on crates.io** (or another way `aktujo`, which is on crates.io, can depend on it) – needed by step
   4 only if `aktujo` keeps serving a relay; not needed if a relay is always a kernoid. **OPEN.**
2. **`kernoid-server` on crates.io** for building identity in (kernoid RFC 0009, section 3, OPEN there) – needed by
   step 8, built-in way.
3. **A server as a node with keys, on PostgreSQL:** the stream store of `kernoid-spaces` behind a trait with a
   PostgreSQL backend (or a server-side crate), so that a server can hold every key of a space, write when due and act
   for people (kernoid RFC 0010, section 8) – needed by step 8.
4. **`kernoid_shell` without an account:** an app that starts on the device, with features that need no server, and a
   login added later – needed by step 7.
5. **Several bridges in one native library** (kernoid RFC 0011, section 4, OPEN there) – the spike of step 7, together.
6. **A review of `kernoid_crypt::stream` and `pq`** (kernoid RFC 0012, section 11) – before aktujo 0.5.0 is released
   for use.
7. **Groups** as a kind of stream of the foundation, if kernoid wants them there (section 3) – otherwise aktujo's.

Steps 2, 3 and 5 need none of these.

## Open

- **OPEN:** `aktujo relay` – gone, or a small relay of protocol 2 of aktujo's own (section 1).
- **OPEN:** reading an export of 0.4 into 0.5 (section 2).
- **OPEN:** server mode beside or built in first (section 4).
- **OPEN:** aktujo's old core crates on crates.io – deprecate, yank or leave (section 6).
- **OPEN:** kernoid's deliveries of section 8.
- **OPEN:** whether aktujo's RFCs 0001 and 0004–0009 get a note that this document replaces parts of them, or stay as
  they are with this one in the index.
