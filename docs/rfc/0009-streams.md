---
title: "0009 Streams"
parent: Design
nav_order: 9
---

# 0009 Streams: a log per thing, a tree of keys, a server as one more node

State: draft. Changes [0001](0001-architecture.md) (one log per space), [0007](0007-erasure-and-locks.md) (the
ordered list, rights) and [0008](0008-visibility.md) (what others see, enforcing). The direction was decided by the
maintainer on 2026-11-19; the mechanism is a proposal.

## Summary

Today a space is one log, and every device receives all of it – every event, every original, sealed or not. With
visibility ([0008](0008-visibility.md)) that no longer fits: a pupil's phone would download the sealed library of the
whole orchestra to open three parts, every member would see that documents exist that they may not read, and removing
a device would mean new keys for everything it could reach, sealed again for every other device.

This document splits a space into **streams** – one log per thing: the space, a person, a group, a folder, a document,
a branch – and joins them by a **tree of keys**:

- A device finds what it may read by following keys down from its person, and **receives only those streams**.
- The relay stores streams by random identities and serves them to whoever proves to hold their key; it **knows no
  audiences**.
- A device is removed **at once** at the relay, and new keys are made **when next needed**, per person rather than per
  device.
- A **server** is one more node: a member with every key, visible as such, always online.
- **Rights** are single permissions; **roles** are data of a space, with three that come with aktujo.

## Decided

By the maintainer, 2026-11-19:

1. A server and devices that work offline go together in one space: a server is a node of the space like a device,
   with every right.
2. A device fetches only what it can see.
3. aktujo offers every permission singly, so that programs make real roles of them; aktujo comes with the roles
   **Admin**, **User** and **Guest**.
4. A person known only to a server gets a device by signing in and confirming it; the server seals the person key.
5. A tree of keys within a group (TreeKEM) only when measurements call for it; access patterns at the relay are
   accepted.
6. Nothing is kept compatible with 0.4.

## 1. Streams

A stream is a log as [0001](0001-architecture.md) describes one – events with a hybrid logical clock, chained per
device, merged by union – with an identity of its own (`s-` and 26 characters of `a-z0-9`, random) and a **stream key**
its events are sealed with. A space has:

| Stream | Holds | Who reads it |
|---|---|---|
| **space** | members, persons, roles, kinds, rules, lock rules, settings; links to the documents and folders shown to the space | every member |
| **person** | the person's name, its devices' names, links to what is shown to the person: groups, folders, documents, branches | the person's devices |
| **group** | its name, its persons, links to what is shown to the group | the group's persons |
| **folder** | its name, its settings, links to its documents and to the folders in it | the folder's audience, and the folders above it |
| **document** | the document's events: revisions, locks, purge, guest links; its originals | the document's audience |
| **branch** | the revisions of a branch ([0008](0008-visibility.md), section 9) | the branch's audience |

The clock stays one per device across all streams, so events of different streams still have one order where one is
needed (a lock and an edit, [0007](0007-erasure-and-locks.md)). Chains are per device **and** stream.

### Order within a stream

What [0007](0007-erasure-and-locks.md) and [0008](0008-visibility.md) put into "the ordered list of the space" is
ordered **in the stream it is about**: locks, unlocks, purges and branches in the document's stream, filings and
settings in the folder's, persons joining in the group's, members, roles and rights in the space stream. The relay
gives each stream's records a position, as it does for the member list today.

A record carries **`at`**: the position in the space stream its author had seen. Rights are checked there – against
the members, roles and rights at that position – the same on every device. A record whose author was removed later
stays valid, like an event written offline before the removal.

Folders placed into each other across streams cannot be ordered against each other. A placing that closes a circle
is broken the same way everywhere: of the placings in the circle, the one with the latest timestamp (then the larger
hash) does not hold.

## 2. A tree of keys

```
device keys
  └─ person key           (sealed to each device of the person)
       ├─ space key       (sealed to every person key)
       ├─ group keys      (sealed to the person keys of the group)
       └─ folder keys     (sealed to the keys of the folder's audience and of the folders above it)
            └─ document keys
                 └─ branch keys
```

A **link** is a stream's key sealed with the key of a stream above it, written into that stream:
`key_link {stream, generation, sealed}`. Filing a document into a folder writes a link into the folder's stream;
showing a document to a person writes one into the person's stream. A device starts at its person stream, opens the
links it finds there, follows them down, and so knows every stream it may read – and no other.

The **person key** is new. Sealing to a person instead of each of its devices is what keeps adding and removing devices
cheap: a person with three devices is one key to seal for, not three.

Keys have **generations**. A new generation is sealed with the next one up and seals the previous one, so that whoever
gets the newest reads back to the beginning – as space keys chain today.

## 3. The relay

The relay stores streams by identity and knows nothing else of them:

- **Reading** a stream needs a token derived from the stream key (`HKDF(stream key, "aktujo/stream read")` per
  generation); the relay keeps only a hash of it. **Writing** needs the write token and the signature of a member with
  at least `write`. Both also need the device to be a member now – the relay checks the member list, which it reads
  as today.
- **Following**: a device sends the identities of the streams it follows with its positions and gets what is new.
  Containers say when something in them changed (`changed {stream, position}`), so a device need not ask every
  document of a large library each time.
- **Originals** belong to a document stream and are fetched with its token – **on demand**, or all of a folder ahead of
  time ("available offline", as notenwart has it per ensemble).
- **Purging** a document removes its stream at the relay, with its originals and branches.

What the relay learns: identities, sizes and times of streams, and which device follows which – who reads together,
not what. Hiding access patterns from the relay is not attempted; whoever needs that runs their own relay or server.

Without a relay – a space on one device, or devices that synchronise directly – the same rules hold locally.

## 4. Removing a device or a person

**A device** is removed in the space stream, as today. Then:

1. **At once:** the relay serves it nothing more – no stream, sealed or not.
2. **Its person** gets a new person key, sealed to the person's other devices: a handful of seals.
3. **Due:** every stream the person's key opened – space, groups, folders, documents, branches – is marked *due* for a
   new key generation.
4. **When next written:** the first device to write into a due stream makes its new generation first and writes the
   links for its current audience. Nothing new is ever written under a key the removed device knows. A stream nobody
   writes to again keeps its old key: what is in it, the device had already.

**A person** is removed the same way, without step 2, after what [0007](0007-erasure-and-locks.md) says happens to its
documents.

The cost of a removal is so paid only where something changes again, and grows with the persons of an audience, not
their devices. Very large groups – thousands of persons – may later need a tree of keys within the group as well, as
TreeKEM does in MLS ([RFC 9420](https://www.rfc-editor.org/rfc/rfc9420)); that waits until measurements show it is
needed.

## 5. A server

A server – `aktujo serve` with PostgreSQL, notenwart – is a **node of the space** like a device:

- It is a member with the role **`server`**: every permission, and every link is sealed for it as well. The member
  list shows it as such, to every member: in a space with a server, the server's operator can read everything. A
  space without one stays end to end.
- It is **always online**: it writes links and new key generations when they are due, evaluates the rules of a
  program (notenwart: who plays which part) and writes their audiences ([0008](0008-visibility.md), section 4).
- It answers **people without devices** – a login in a browser – and filters by audience; such a person is a person of
  the space ([0007](0007-erasure-and-locks.md)) with no device. When the person installs an app, it signs in to the
  server there and confirms the new device from a session it already has (a code shown in one, typed in the other);
  the server then adds the device and seals the person key for it – it holds the key anyway.
- It serves **guest links** ([0008](0008-visibility.md), section 8) and can be the **relay** of the space.
- Devices of the same space still work offline and synchronise with it like with any relay.

## 6. Permissions and roles

**Permissions** are single, and aktujo offers all it has:

| Permission | Allows |
|---|---|
| `read` | read what is shown to one |
| `write` | add documents, make revisions, folders and branches of one's own |
| `members` | invite and remove devices and persons |
| `roles` | define roles and give them |
| `groups` | make, name, fill and remove groups; see hidden groups |
| `share` | change the audience of what belongs to the space; revoke any guest link |
| `link` | make guest links |
| `lock`, `unlock` | lock and unlock ([0007](0007-erasure-and-locks.md)) |
| `purge` | purge what belongs to the space |

What one owns needs no permission beyond `write`: one's own documents, folders and branches
([0007](0007-erasure-and-locks.md)).

**Roles** are data of the space stream: `role_defined {role, name, permissions}`, `role_removed {role}`. A role is
given to a **person**; a device can be limited below its person's role (a phone that only reads), never above it.
aktujo comes with three roles, which a space can change like any other:

| Role | Permissions |
|---|---|
| **Admin** | all |
| **User** | `read`, `write`, `link` |
| **Guest** | `read` |

A program makes its own – notenwart: an administrator, a library, a leader, a member – from the same permissions.
The person who starts a space is an Admin. This replaces the levels and grants of [0007](0007-erasure-and-locks.md),
section 5.

## 7. What others see

A member sees the space stream and the streams it can open. Of everything else it learns **nothing** – not that a
document exists, not how many there are, not who is in a hidden group. This answers what [0008](0008-visibility.md)
(sections 2, 3, 6) left open about counts, filings and hidden groups, as far as members go; the relay still sees
streams and who follows them (section 3).

## 8. The format

Streams change sync, the relay and the store at once. They come with [0007](0007-erasure-and-locks.md) and
[0008](0008-visibility.md) in one format change, 0.5.0. Nothing is kept compatible with 0.4 – nobody uses aktujo yet
(maintainer, 2026-11-19).

## Prior art

- **Cryptree** (Grolimund, Meisser, Schmid, Wattenhofer, SRDS 2006,
  [summary](https://eprints.cs.univie.ac.at/5637)): a tree of "cryptographic links" for access control on storage
  that is not trusted; access to a folder and all below it in constant time, inherited dynamically. Peergos builds on
  it ([book.peergos.org](https://book.peergos.org/security/cryptree.html)).
- **Plutus** (Kallahalla et al., FAST 2003, [USENIX](https://www.usenix.org/conference/fast-03/plutus-scalable-secure-file-sharing-untrusted-storage)):
  groups of files sharing keys, read and write told apart, revocation handled efficiently on storage that is not
  trusted. Making new keys only when next written ("lazy revocation") is studied in
  [Lazy revocation in cryptographic file systems](https://typeset.io/pdf/lazy-revocation-in-cryptographic-file-systems-5927fd9kbj.pdf);
  how exactly Plutus does it: OPEN.
- **MLS** ([RFC 9420](https://www.rfc-editor.org/rfc/rfc9420)): keys for groups whose members need not be online
  together; TreeKEM keeps the cost of changes logarithmic in the size of the group.

## Open

- **Following many streams**: the cost for a device following tens of thousands of documents, and whether containers'
  `changed` hints are enough: to be measured.
- **Group key chains and new generations** on large spaces: to be measured, before deciding on a tree within groups.
