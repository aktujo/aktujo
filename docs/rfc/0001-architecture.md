---
title: "0001 Architecture"
parent: Design
nav_order: 1
---

# 0001 Architecture: the core and the program

State: draft. Applies from version 0.1.0.

## Summary

aktujo has two parts in one repository:

- **The core** – libraries that other programs build in: originals stored by their hash on any storage, an event log
  that merges by union, end-to-end encryption between the members of a space, local storage in SQLite, and a relay.
  The core knows nothing about documents. Licence: **MIT**, so that programs under other licences can use it.
- **The program** – the document archive built on the core: its domain, the workspace every interface goes
  through, the API, the terminal interface, the command line and the web app. Licence: **AGPL-3.0-or-later**.

The core is taken out of two places where it works today: the data layer of spezado (storage of events and
originals, the relay) and the reference library of the open protocol librotenado (canonical encoding, events,
encryption). It is made general: what librotenado calls a *set of books* is a **space** here, and an event carries
any kind a program defines.

## The dependency rule

librotenado is a protocol of its own and its specification never refers to aktujo. aktujo implements its
mechanisms – parts 0002 (events), 0003 (canonical encoding) and 0006 (encryption) – in general form:

- aktujo does not depend on the librotenado crate. Code taken over from it keeps its licence (MIT) and names its
  origin in the file.
- The labels of key derivation and signatures are a parameter (`Domain` in `aktujo-crypt`): aktujo uses its own,
  a program that has to stay compatible with librotenado passes `librotenado/0006`.
- OPEN: whether the core reads and writes the JSON of librotenado 0006 byte for byte (member `books` where aktujo
  writes `space`). Decided when spezado moves onto the core.

Programs that build the core in – spezado, notenwart, kernoid – move onto it only once it covers everything they
need. Until then nothing in them changes.

## The crates

| Crate | Licence | What | I/O |
|---|---|---|---|
| `aktujo-base` | MIT | canonical encoding (RFC 8785, restricted to what the core uses), hashes `sha256:…`, identities of spaces, bytes in base64url, root objects with their type | none |
| `aktujo-blob` | MIT | originals by their SHA-256 hash: the trait `Blobs`, in memory, in a folder and – features `object`, `s3` – in an object store such as S3; checked on every read | files, network |
| `aktujo-log` | MIT | device names, hybrid logical clocks, events of any kind, the log of a space: merge by union, the agreed order, the state, chains per device and their problems | none |
| `aktujo-crypt` | MIT | members and their keys from a seed, the chain of space keys, the ordered member list, envelopes, signed requests, invitations, recovery codes | none |
| `aktujo-store` | MIT | the core's own small schema in SQLite: events, member records, settings, what was sent | SQLite |
| `aktujo-relay` | MIT | the relay as a library and a service: member lists and envelopes per space over HTTP | network |
| `aktujo-sync` | MIT | a device's replica of its spaces: writing events and originals, synchronising through a relay, invitations, removing members, recovery | network |
| `aktujo-doc`, `aktujo-extract` | MIT | documents and reading their text, on the core – [0002](0002-documents.md) | none / hayro, tract |
| `aktujo-core` | AGPL | the domain of the archive | none |
| `aktujo-app` | AGPL | the workspace: every command and query, the one layer the interfaces use | through the core |
| `aktujo-api` | AGPL | HTTP and JSON, described in OpenAPI; serves the web app | network |
| `aktujo-tui` | AGPL | the terminal interface | terminal |
| `aktujo` | AGPL | the command line | – |

The crates of the core depend only on each other and never on a crate of the program. Each is marked with its
licence in `Cargo.toml` and in the SPDX header of every file; the core crates are published on crates.io with the
first release.

What is not in the core: tables of a program (documents, bookings, notes, mail) and a general SQL layer over SQLite
and PostgreSQL – each program builds its own views on the events, as spezado does today, so that queries checked at
compile time (sqlx) stay possible.

## Spaces, events and members

- A **space** is what is shared and encrypted as a whole: a person's archive, a business's books. Its identity is a
  UUID (version 7 when made by the core), given when it is founded; it never changes.
- An **event** is an immutable fact of a space: `{"type": "event", "kind": …, "time": …, "seq": …, "prev": …, …}`,
  identified by the SHA-256 hash of its canonical encoding as received. The kinds and their members belong to the
  program; the core keeps, merges and passes on every event, also those of kinds it does not know.
- **Members** are the devices, backends and recovery codes of a space, in a strictly ordered list beside the
  unordered events (librotenado 0006). Only this list needs an order; a relay keeps it.
- **What a removed member wrote stays.** An event is checked against the device name its author was added with,
  whether the author is still a member or not; only current members may send. Otherwise a device that joins later
  could not read the history written by a member removed since.

## Storage

- **Originals** are stored by their hash, in memory, in a folder or in an object store (S3 and S3-compatible
  storage); later in kernoid. Encryption of originals
  at rest is the program's choice: an envelope (`aktujo-crypt`) is stored like any original.
- **Events and member records** are stored in SQLite, one database per device holding all its spaces, so that
  spaces that belong together can be written in one transaction.
- OPEN: PostgreSQL for servers, with the same schema; the backend for kernoid.

## Version 0.1.0 of the core

Done when: `aktujo-base`, `-blob` (memory, folder, S3), `-log`, `-crypt`, `-store` (SQLite), `-relay` and `-sync`
work, are tested against each other – two devices found a space, write events offline, sync through a relay, add and
remove a member, recover with the code (`crates/aktujo-sync/tests/end_to_end.rs`) – and CI checks them on Linux and
macOS. Then the repository goes public with one
commit, the documentation is built, and the crates are published.

## Open points

- OPEN: the backend kernoid of `aktujo-blob`, PostgreSQL of `aktujo-store`.
- OPEN: originals larger than the relay's limit of a request (64 MiB) – sent in parts.
- OPEN: byte-for-byte compatibility with librotenado 0006 (above).
- OPEN: direct sync between devices (`aktujo-p2p`), transport and reaching devices behind NAT.
- OPEN: where a device keeps its seed – beside the data or in the key store of the operating system.
