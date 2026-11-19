---
title: "0007 Erasure and locks"
parent: Design
nav_order: 7
---

# 0007 Erasure and locks: deleting for good, revisions that hold, people who leave

State: draft. Changes [0001](0001-architecture.md) (events, members), [0002](0002-documents.md) (nothing is deleted)
and [0004](0004-archive-0.1.md) (deleting for good was left out). The direction was decided by the maintainer on
2026-11-19; the mechanism is a proposal. [0009](0009-streams.md) says in which stream "the ordered list of the space"
is kept, and replaces the rights of section 5 by permissions and roles.

## Summary

The log of a space is immutable: an event is identified by the hash of its canonical encoding and chained per device.
That is what makes merging safe – and it means that, today, nothing written can ever be taken back. A document put
into the bin can come back; a document added by mistake stays for ever, and so does the name of every device and
member. That is wrong for an archive that people use for anything, and it is wrong in front of the law on data
protection.

This document lets aktujo forget, and lets a space say what must not be forgotten:

- **Deleting for good.** A document can be purged: its content becomes unreadable everywhere it was synchronised to,
  its originals are removed; what stays is the fact that a document was purged, by whom and when.
- **Locks.** A lock fixes one **revision** of a document – its originals and its metadata – so that it cannot be
  purged, and, if the lock says so, not changed either. A lock can be lifted again, with a right of its own.
- **People who leave.** Whom a document belongs to depends on the use: in a music school the space, in a family's
  shared archive each person. A document has an **owner** – the space or a person – chosen document by document, and
  a space can fix that everything belongs to it. When a person leaves or asks to be erased, what the space owns
  stays, what they own goes (or goes with them), and their name goes. Events carry pseudonyms, names live where they
  can be erased.
- **Private documents.** A document a person owns can be private: only their devices can read it. A member is a
  device; a **person** groups its devices.

The means is the same in all three: what may have to be forgotten is **sealed with a key of its own**, and forgetting
is throwing that key away. The log stays immutable; its hashes and chains stay valid.

## Decided

By the maintainer, 2026-11-19:

1. Whom a document belongs to differs from use to use, so it is decided document by document: whoever adds a
   document chooses whether it is theirs or the space's. A space can fix that everything added belongs to it – in
   notenwart the sheet music belongs to the music school, and a person leaving deletes none of it.
2. A document can be deleted for good – a wrong upload must not stay for ever.
3. A lock covers **the whole document with its metadata**: one exact revision. A change after the lock is a new
   revision; the locked revision stays. (A tax return refers to one exact revision of a receipt – title, amount and so
   on.)
4. Whether a locked document may get new revisions is a setting.
5. aktujo can **unlock**: it serves many uses. Locking and unlocking have rights of their own. A program that must
   never unlock – spezado for receipts used in a tax return – gives that right to no one; cancelling a receipt is the
   program's own business, not aktujo's.
6. **Labels are not part of a revision** – they become folders ([0008](0008-visibility.md)) – nor is **derived data**
   (read text, suggestions, results of recognition).
7. A document a person owns can be **private** – shared and private documents in one space.

Settled on the same day, from the questions this document had left open:

8. A person can make a **recovery code of its own** for what is private; with a server, the server can restore too.
9. Documents **about** a person are found through fields of the type `person`; a person decides what happens to each.
10. A new space: owners chosen, "mine" offered first; private documents allowed; new documents shown to the space.
11. The relay removes a purged document's stream **at once**.
12. Due locks follow the server's clock if there is one, else the devices'; a written lock stays.
13. Nothing is kept compatible with 0.4.
14. Anchors for what comes later: a **signature** always signs a revision – its hash; a **workflow** (A approves, B
    signs) can set a lock. Everything else about them gets a document of its own when it is planned.

## Terms

- A **version** stays what [0002](0002-documents.md) calls it: one original of a document – a scan, a corrected file
  (`document_version`).
- A **revision** is new: the state of a document that a lock can fix. It is made of
  - its versions, in order (each an `Original`: hash, name, media type, size),
  - its title,
  - its kind, if confirmed,
  - its fields with a confirmed value.

  Not part of it: the folders it is in (labels until now), the bin, suggested kinds and values, read text. Changing
  those never makes a new revision.
- A **snapshot** of a revision is its canonical JSON object, `{"type": "revision", "document": …, "versions": […],
  "title": …, "kind": …, "fields": {…}}`, stored as an original by hash like read text. The hash of the snapshot is
  the **revision hash**.

## 1. Sealed content

### Documents

Every document gets a **document key** (32 bytes, random) when it is added. The members of every event about the
document that are not the frame and not the document's identity are encrypted with it:

```json
{"type": "event", "kind": "document_title", "time": …, "seq": …, "prev": …,
 "document": "…", "sealed": "<nonce and ciphertext, base64>"}
```

The same key encrypts the document's originals, its read text and its snapshots before they go to a `Blobs` store.
Originals are then stored by the hash of what was encrypted; the hash of the plain bytes stays inside the sealed
members, so that a document is still identified by its content within the space.

The document key itself travels in an event `document_key {document, key}`, whose key is encrypted with the space key
of the moment – so that a member who joins later can read the space's documents, as today.

- *Locally*, events are stored as they are: sealed. The search index (FTS5) and every other view are derived from the
  opened events and rebuilt without the document when it is purged.
- *At the relay* nothing changes: envelopes are encrypted with the space key as before; inside them, documents are
  sealed once more.
- *Without sync* the same: a space that never syncs gains a way to purge as well.

### People

Today a member is a device. Owning and seeing are about people, and a person has a laptop and a phone. So the member
list gets **persons**:

- Every member record carries a **person**: a pseudonym made at random (`p-` and 12 characters of `a-z0-9`). The
  first device of a space starts a person; an invitation says whether it brings **a new person** or **another device of
  the inviting person** – the second only made by that person, for themselves. A recovery code belongs to the space,
  not to a person (see Private documents).
- Rights stay with members (devices), as today; a person's devices can differ (a phone that only reads).
- Removing a person removes all its devices. Removing one device leaves the person and what it owns.

The member list keeps identities, keys, roles, rights, device names and persons – and no longer a name people call
them by. Names move into an event `person_named {person, sealed}`, sealed with a **person key** that is made with the
person and travels like a document key. Erasing a person's name is throwing that key away; the log shows "a former
member" from then on.

**Device names become pseudonyms.** Today a device name is chosen (`anna-laptop`) and written into the timestamp of
every event it makes – under the hash, for ever. From now on a device name is made at random (`d-` and 12 characters
of `a-z0-9`); what people call a device is a name sealed with its person's key. The rules for device names in
`DeviceId` stay.

A field that refers to a person – in notenwart "lent to", "part played by" – gets the type **`person`** and holds a
person's pseudonym, never a name as text.

### Owners

Every document has an **owner**: the **space**, or a **person**.

- **The person decides, document by document.** Whoever adds a document chooses whether it is theirs or the space's.
- **The space can fix it.** A space has a setting, **owners**, data of the space like its kinds:
  - `chosen` – each document as its adder chooses; the space says which choice is offered first (`space` or
    `adder`);
  - `space` – every document added belongs to the space; nobody is asked. notenwart makes its spaces so.

  A new space: `chosen`, offering `adder` first – a space of one person behaves the same either way. Changing the
  setting to `space` changes no document that exists; the interfaces list the documents persons still own.
- The owner is set when the document is added (`document_added` gets `owner`) and can be changed with
  `document_owner {document, owner}`: an owner may give its document to the space; a document of the space goes to a
  person only by a member with `manage`, and not while the setting is `space`. Changing the owner is not part of a
  revision and is allowed on locked documents.
- A person never loses a document it owns by losing rights; it loses it when it is removed or erased (below).

### Private documents

Owning is not seeing. Who sees a document is its **audience** – the space, persons, groups – and is the subject of
[0008](0008-visibility.md). A **private** document is the narrowest case: an empty audience, only its owner's devices
read it. What follows here is what is particular to it.

- **How.** Its key is sealed to the receiving key of each device of its owner – one `document_key` per device.
  Nothing else changes: its events and originals are sealed with the document key as every document's are.
- **Who chooses.** The owner, through the audience. A space can forbid private documents (setting **private
  documents**: `allowed` or `not`); a new space: `allowed`.
- **A new device** of the person gets the keys of the person's private documents from one of its other devices, at
  the next synchronisation after it was added. The space's recovery code does not open private documents – it would
  let whoever holds it read them. Instead a person can make a **recovery code of its own** (offered once it has
  something private); it opens the person's key, and so everything private of the person. In a space with a server
  ([0009](0009-streams.md)) the server can restore as well – it holds every key anyway. Without either, a person who
  loses every device loses what is private; the interfaces warn of it.
- **Narrowing and widening, and what the others see** of a document they do not see: as for every audience
  ([0008](0008-visibility.md), sections 5 and 6). Given to the space, a private document gets the space's default
  audience.
- **Locks.** Only a device that can read a document can lock it – the lock names the revision. So a private document
  is locked by its owner's devices, with the right `lock`. Unlocking and purging name only the document and work for
  anyone with the right, subject to who may purge what (section 4).

### When a person leaves

The member who removes a person chooses, and the interfaces show what each choice touches:

| | Documents the space owns | Shared documents the person owns | Private documents of the person |
|---|---|---|---|
| **Remove** (leaves, nothing more) | stay | stay, owned by the space from then on – or handed to another person | purged – nobody else can read them – unless the person first gives them to the space or takes them along |
| **Remove and take along** | stay | exported for the person ([0004](0004-archive-0.1.md), step 9), then purged | the same |
| **Erase** (the person asks to be forgotten) | stay | purged | purged |

Taking along needs a device of the person that can read: the export is made there. A person who leaves of their own
accord does it themselves; a person removed by others keeps what their devices hold already.

In every case the person's name is erased (its person key thrown away) if asked; its pseudonym stays in the log.
A locked document is never purged: it stays – a shared one owned by the space, a private one sealed and unreadable to
the space – and the interfaces list it with its locks, so a person with the right can decide.

The owner says whose document it is, not whom it is about. A school's certificate of a pupil belongs to the school
and is about the pupil. Documents **about** a person are found through fields of the type `person` (above): erasing a
person lists every document with a field that refers to them, in any revision and line, and a person with the right
decides document by document – purge, clear the field, or keep it (a duty to keep records). aktujo does not decide
this by itself.

## 2. The ordered list of a space

Events merge by union, in any order. That is not enough for what cannot be undone: if one device locks a receipt
while another, offline, purges it, the key is gone before the lock arrives.

So locks, unlocks and purges are not events. They are **records in the strictly ordered list of the space** – the list
that already holds the member list at the relay ([0004](0004-archive-0.1.md)). Member records and these records share
one order, so it is always clear what rights the author had at that position. Each record is signed by its author and
its content encrypted with the space key; the relay orders what it cannot read, as it does now.

- A space **with a relay**: a lock, unlock or purge holds once the relay has taken it. Until then it is *pending*: the
  device that made it already treats the document as locked (or keeps a purged document in the bin), and says so.
- A space **without a relay**: the device's own order is the order.

## 3. Locks

```json
{"type": "lock", "lock": "…", "document": "…",
 "revision": "<revision hash>", "seen": {"<device>": <seq>, …},
 "reason": "Tax return 2025", "new_revisions": false}
```

- **`lock`** – an identity of its own. A document can have several locks, for different reasons. It is locked while
  at least one lock holds; lifting one lifts only that one.
- **`revision`** – the hash of the snapshot of the revision that is locked. The snapshot is stored with the lock, so
  the locked revision can be shown and proved later, however the document changes after.
- **`seen`** – the frontier: for every device, the last event of it that the locking device had seen. An event about
  the document belongs to the locked revision exactly when its `seq` is within the frontier of its device. Every device
  decides the same, whatever order events arrive in – an edit made offline before the lock but synchronised after it
  is *after* the lock.
- **`reason`** – text for people, sealed with the document key.
- **`new_revisions`** – whether the document may get new revisions while this lock holds. A space has a default; with
  several locks the strictest holds.

While a document is locked:

| | `new_revisions: true` | `new_revisions: false` |
|---|---|---|
| New version, title, confirmed kind or field, field cleared | a new revision; the locked one stays | refused; if it arrives from another device, it is not applied and is reported as a conflict |
| Folders, suggestions, read text | allowed | allowed |
| Into the bin | refused | refused |
| Purge | refused | refused |

A device that receives an event it must not apply leaves it in the log – the log stays a union – and leaves it out of
the document; the conflict is shown to people, never resolved silently.

A lock fixes a revision of one line of a document – the main line, or a branch ([0008](0008-visibility.md),
section 9). Branches made from a locked revision do not change it and are allowed whatever `new_revisions` says;
merging into a locked line obeys the lock.

An **unlock** names the lock it lifts. The events refused while it held stay refused; nothing is applied after the
fact.

### Locks by time

A space can hold **lock rules** as data, like kinds ([0005](0005-pure-core.md)): which documents (a kind, or all),
after how long (from when the document was added, or from a date field), with which reason and `new_revisions`.
A rule writes nothing by itself: a device with the right to lock, when it notices that a document is due, writes the
lock. A lock holds once written – a rule is a promise, not a lock. **Time:** in a space with a server, the server
writes due locks by its clock; without one, devices do by theirs – a convenience, not a proof, and the documentation
says so. A date field counts with its confirmed value on the main line; a lock once written stays when the field is
changed later. A private document is locked by a rule only once a
device of its owner with the right to lock notices it. Two devices locking the same document for the same
rule make one lock (the identity is derived from rule and document).

## 4. Purging

A document is purged in two steps:

1. **Into the bin**, as today – reversible.
2. **Purge**: a record in the ordered list, naming the document. It is valid if no lock on the document held at that
   position and its author was allowed to purge it there:
   - a document of its own person – always, with no grant;
   - a document of the space – with the grant `purge`;
   - a document of another person – never, except when removing that person (section 1, When a person leaves), with
     `manage`.

   A valid purge makes every device
   - throw the document key away (and the keys of its snapshots, if any),
   - remove the document's encrypted originals, read text and snapshots from its `Blobs` store – each document
     encrypts its own, so two documents with the same file hold two copies and purging one never touches the other
     (the price: no sharing of originals between documents),
   - leave the document out of every view and the search index.

   The relay removes the document's stream – events, originals, branches – **at once** ([0009](0009-streams.md)):
   devices that did not have it need it no more, devices that had it throw their key away when they next synchronise.

What stays: the record of the purge (document identity, who, when) and the sealed events, unreadable. The number and
size of a purged document's events stay visible to whoever holds the log.

An invalid purge – made offline while a lock was on the way – does nothing; the device that made it brings the
document back from the bin and says why.

## 5. Rights

Today rights are one ladder: `read` < `write` < `manage`. Locking, unlocking and purging do not fit on it – a
bookkeeper may lock without being allowed to unlock. Rights become a level and grants:

```json
{"level": "write", "grants": ["lock", "purge"]}
```

| Grant | Allows |
|---|---|
| `lock` | lock a document it can read; write locks for lock rules |
| `unlock` | lift a lock |
| `purge` | purge a document of the space from the bin (its own person's documents need no grant) |

`manage` may give and take grants, and holds none by itself. The member who starts a space holds all three, so that a
person alone keeps every choice.

**Who checks.** The relay checks what it checks today: that a request is signed by a member with the level it needs.
It cannot read records and so cannot check grants. **Every device checks every record** against the member list at
that position and ignores what its author was not allowed to do. A dishonest program can still write anything – it
just has no effect on honest devices.

## 6. What this cannot do

Said plainly, in the documentation and in the interfaces:

- An **export**, a **backup**, a copy on a device that is **offline** or has been **removed** keeps what it had. A
  removed member has the keys it had ("What it has read, it may have kept", [0004](0004-archive-0.1.md)); purging
  reaches the devices of the space, not copies outside it.
- **Free text** that names a person – a title, a field of type `text` – is part of a document. It goes when the
  document is purged, not when the person leaves.
- A lock protects against mistakes and against people without the right. It does not make a document unchangeable
  for the owner of a device; for books kept by law, the procedure around it matters as much – that is the program's
  part (spezado).

## 7. The format

All of this changes the format: sealed members, pseudonymous device names, persons in the member list and names out
of it, owners and private documents, records of a new type, rights with grants. It comes in **0.5.0**, before
notenwart is rebuilt on aktujo. **Nothing is kept compatible**: nobody uses aktujo yet (maintainer, 2026-11-19);
0.5.0 does not read spaces of 0.4.

## 8. Interfaces

The SDK first; API, web, TUI, CLI and the apps follow it.

- `lock(space, document, reason, new_revisions)`, `unlock(space, document, lock)`, `locks(space, document)`,
  `revision(space, document, revision)` – the locked snapshot.
- `purge(space, document)` – only from the bin; says when it is pending and when it was refused.
- `lock_rules` read and written like kinds.
- `add` takes an owner (and an audience, [0008](0008-visibility.md)); `set_owner(space, document, owner)`; the space
  settings `owners` (`chosen` with the choice offered first, or
  `space`) and `private_documents` (`allowed` or `not`).
- `persons(space)`; `invite` for a new person or for another device of one's own; `remove_device(space, member)`;
  `remove_person(space, person, documents)` with `documents` one of `to_space`, `to(person)`, `take_along`, `erase`;
  `erase_person_name(space, person)`.
- A document view shows its locks with reason and revision, and the conflicts of refused events.

## Open

- Visibility beyond private and shared, and what members learn of documents they do not see: [0008](0008-visibility.md).
- **The cost** of a second encryption per event, and of a key event per document, on large spaces: to be measured.
- **Legal reading** – which of this is needed and enough under the GDPR (Art. 17 against duties to keep records) and
  for books kept by law (GoBD): OPEN, for someone qualified to say.
