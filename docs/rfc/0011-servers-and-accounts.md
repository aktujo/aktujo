---
title: "0011 Servers and accounts"
parent: Design
nav_order: 11
---

# 0011 Servers and accounts: organisations, sign-in, and a server that acts for people

State: draft. Builds on [0009](0009-streams.md) (a server as one more node, persons, roles) and
[0008](0008-visibility.md) (audiences). Changes [0004](0004-archive-0.1.md) only in that `aktujo serve` gets a second
mode. The direction was decided by the maintainer on 2026-11-19; the mechanism is a proposal. kernoid 0.5.1 has since
given the crate this document asks for, in two forms – `kernoid-auth` beside a kernoid, and kernoid's identity built in
([0012](0012-on-kernoid.md), section 4); what a server as a node still needs from kernoid is there too (section 8).

## Summary

Up to now `aktujo serve` answers one person on their own computer: it listens on `localhost`, has no accounts, and
whoever reaches it may do everything. An organisation – a music school, an office – needs a server that many people
reach from their browsers, that knows who they are, and that serves several organisations at once where a provider
runs it.

- **Accounts come from kernoid.** Signing in – password, passkeys, second factors, single sign-on with an
  organisation's provider – is kernoid's work already. A crate is taken out of kernoid and built into
  aktujo's server; besides, the server trusts any OpenID Connect provider, a kernoid of its own among them.
- **Organisations over spaces.** A server has organisations; an organisation has accounts and spaces. An account is a
  person in each space it belongs to, under a pseudonym of that space.
- **The server acts for people.** In the browser there are no keys; the server holds them (a node with every key,
  [0009](0009-streams.md)), shows each person what their audience allows, and writes what they do in their name.
- **One binary, one container** for a small installation; several instances on PostgreSQL and S3 for a large one.

## Decided

By the maintainer, 2026-11-19:

1. Accounts and sign-in come from **kernoid**: a crate taken out of kernoid first, and besides it any OpenID Connect
   provider, a separate kernoid included.
2. **Every way of signing in** – through kernoid, none of aktujo's own.
3. The crate stays **AGPL** and is used **only by the server** (the program), never by the SDK or the apps.
4. **Organisations over spaces**: a server has organisations; an organisation has accounts and spaces; an account is a
   person in each of its spaces, with a pseudonym of its own there.
5. The **web app of a server trusts the server**: no keys in the browser. End to end stays with the apps and with
   spaces that have no server.

## 1. Two modes of `aktujo serve`

| | **Personal** (as today) | **Server** (new) |
|---|---|---|
| Who | one person, on their computer | many people, through their browsers and apps |
| Reached at | `localhost` only | a host name, over TLS (itself or behind a proxy) |
| Accounts | none – whoever reaches it is the person | kernoid's, per organisation |
| Data | a data folder, SQLite, SQLCipher | PostgreSQL and S3 – or SQLite and a folder for a small one |
| Keys | the person's, in the system's key store | the server's own, as a node with every key |

The personal mode stays as [0004](0004-archive-0.1.md) describes it. The server mode is switched on by configuration
(`aktujo serve --config server.toml`) and refuses to start without TLS or a proxy it trusts.

## 2. Organisations

```
server
  └─ organisation            (a music school; kernoid: a tenant)
       ├─ accounts           (kernoid: accounts of the tenant)
       └─ spaces             (the school's library, the office's files …)
            └─ persons       (an account in this space, under this space's pseudonym)
```

- An **organisation** has a name, its accounts, its spaces, and its own rules of sign-in (which ways, which provider
  for single sign-on, whether accounts are made only by invitation). It maps one to one onto a tenant of kernoid.
- Organisations are kept apart in storage: a **schema per organisation** in PostgreSQL and a **prefix** in the object
  store (`ObjectBlobs::with_prefix`), so that one can be backed up, moved or deleted on its own. Proposal; a column
  per row is the alternative.
- **Server administrators** make and remove organisations; **organisation administrators** manage its accounts and
  make spaces. Both are roles of kernoid (instance and tenant); rights *within* a space stay the roles of
  [0009](0009-streams.md), section 6.

## 3. Accounts and persons

An **account** is kernoid's: an identity, its ways of signing in, its sessions. aktujo adds nothing to it but the link
to persons:

- In each space an account belongs to, it is a **person** ([0007](0007-erasure-and-locks.md)) with a pseudonym of that
  space. The server keeps the link `(organisation, account) → (space, person)` in its own tables, **not** in any space's
  streams: members of one space cannot tell that a person of theirs is the same as one of another space.
- **Adding** an account to a space makes a person there with a role; the server writes it into the space stream as
  any member would. **Invitations** are kernoid's (a link); an organisation can say that accounts
  are made only by invitation – no account is made by signing in with single sign-on alone (notenwart: pupils are
  minors).
- The person's **name** in a space is sealed as [0007](0007-erasure-and-locks.md) says; the server fills it from the
  account's name and keeps it current.
- **Deleting an account** erases the person in every space of it, as [0007](0007-erasure-and-locks.md) ("When a person
  leaves") says, with the choices made there, and then the account in kernoid. **Export** of everything about an
  account (GDPR, Articles 15 and 20) gathers its persons' documents, branches and records across spaces.

## 4. Signing in

All of it is kernoid's crate:

- password (argon2id), **passkeys**, **second factors** (TOTP); a one-time **link by email** to sign in, as notenwart
  has it – kernoid sends codes for registering and links for invitations and resetting passwords; a link to sign in:
  OPEN;
- **single sign-on** with the organisation's provider – OpenID Connect or SAML, Microsoft 365, Google Workspace and the
  like – only for accounts that exist, unless the organisation allows otherwise;
- **sessions**: a cookie for the browser (HttpOnly, with protection against cross-site requests), a bearer token for
  apps and the command line.

Besides the crate, a server can **trust an OpenID Connect provider** for an organisation instead – a kernoid running on
its own, or any other. Then aktujo signs no one in itself; it maps the provider's subject to an account.

## 5. The server acts for people

A person in a browser holds no key. The server does everything for them and must say so:

- **Reading**: the server opens what the person's audience allows ([0008](0008-visibility.md)) and nothing else – lists,
  search, originals, exports are filtered per request.
- **Writing**: what a person does in the browser, the server writes into the streams with its own device key and a
  member **`for`** naming the person: `{"type": "event", …, "for": "p-…"}`. Rights are checked for the person, at the
  position the server had seen ([0009](0009-streams.md), section 1). Every member sees that the server wrote it, and
  for whom; the audit log shows both.
- **Apps of the same person** sign in to the server and get their person key as [0009](0009-streams.md) decided; from
  then on they write as themselves, end to end, and work offline.

The trust is plain: the server's operator can read and write everything in the organisation's spaces. A space can
leave the server – take its streams to devices and a relay – but what the server had, it may have kept.

## 6. Running it

- **One binary, one container**: the server, the web app, the relay for the organisation's devices, the background
  work (reading, rules, keys due, renewals, lock and retention rules).
- **Small**: Compose with PostgreSQL and a single-node object store – or SQLite and a folder, with one instance.
- **Large**: several instances on one PostgreSQL and one object store. Work in the background is a queue in PostgreSQL
  (taken with `FOR UPDATE SKIP LOCKED`); instances tell each other of changes with `LISTEN`/`NOTIFY`; the store's locks
  ([0001](0001-architecture.md), `PgStore`) keep writers apart. A Kubernetes operator gets a document of its own.
- **The server's keys** – its device seed, from which it opens everything – are a secret mounted into the container,
  as [0004](0004-archive-0.1.md) does for a key file. Losing it is losing access; stealing it is reading everything.
  Keeping it in a key management service or hardware: OPEN.
- **Backups**: PostgreSQL and the object store together, per organisation (schema and prefix); restoring brings back
  sealed data, which only the server's keys open.
- **Email** for invitations and links: through kernoid's crate, to a mail server the operator names; what else is
  sent by email belongs to notifications (a document of its own).

## 7. What changes for the program

- `aktujo-api` gets the server mode: sign-in routes from kernoid's crate, sessions, organisations, the link from
  accounts to persons, filtering by audience, writing `for` a person. The API stays one API; in personal mode the
  session is the one person.
- The web app gets sign-in and an organisation's administration (accounts, spaces, roles).
- The SDK and the apps do not change for this: they reach a server as a relay and node of the space
  ([0009](0009-streams.md)), with a bearer token.

## Open

- **Taking the crate out of kernoid**: done in kernoid 0.5.1 (kernoid RFC 0009) – `kernoid-auth` on crates.io,
  `kernoid_server::embedded` by git; identity on PostgreSQL only, SQLite OPEN there. See [0012](0012-on-kernoid.md),
  section 4.
- **Organisation and kernoid's levels**: kernoid has installation → instance (per host) → tenant; an organisation is a
  tenant ([0012](0012-on-kernoid.md), section 4); whether one aktujo server is one instance: OPEN.
- **Schema per organisation or a column**: proposal schema; to be measured with many organisations.
- **The server's keys** in a key management service or hardware.
- **Guests** ([0008](0008-visibility.md), section 8) and signing guests ([0010](0010-signatures.md)): no accounts; whether
  a server keeps anything about them beyond the link.
- **The Kubernetes operator** and **notifications**: documents of their own.
