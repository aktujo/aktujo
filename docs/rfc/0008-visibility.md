---
title: "0008 Visibility"
parent: Design
nav_order: 8
---

# 0008 Visibility: who sees which document

State: draft. Builds on [0007](0007-erasure-and-locks.md) (document keys, persons, owners) and changes
[0001](0001-architecture.md) (every member reads every event). The direction was decided by the maintainer on
2026-11-19; the mechanism is a proposal. [0009](0009-streams.md) splits a space into streams: it says where records
are ordered, how devices fetch only what they see, and replaces the grants of section 10 by permissions and roles.

## Summary

Today a space is all or nothing: whoever is a member reads every document in it. That is too coarse for most uses
beyond one person's archive – a family where not everyone sees everything, a music school where a pupil sees the
parts they play and nothing else, an office where the payroll is for two people.

Visibility is fundamental, so it is part of the core, not of one program:

- Every document has an **audience**: whom it is shown to. The audience is a set of **grants** – the whole space,
  persons, groups.
- **Groups** are data of the space: named sets of persons (an ensemble, the teachers, the parents).
- aktujo stays pure: it knows audiences, not why someone is in one. A program with rules of its own – notenwart: "plays
  clarinet 2 in an ensemble that has this work" – evaluates them and writes grants.
- One model, two ways to enforce it: a **server** that is trusted filters by audience; **end to end**, the audience is
  whoever holds the key.

## Decided

By the maintainer, 2026-11-19:

1. Visibility is fundamental and goes beyond private and shared: a space can say that a person sees only specific
   documents (as notenwart does today).
2. It gets a document of its own; [0007](0007-erasure-and-locks.md) refers to it for private documents.
3. aktujo knows no relations of a trade between documents – a work and its parts are notenwart's.
4. Annotations are revisions of a document ([0007](0007-erasure-and-locks.md)), not documents of their own.
5. There are **private revisions**, like branches in git: seen by oneself only, or by a group (section 9).
6. Folders carry, besides audience, lock rules and guest links: **retention rules**, a **default kind** and a
   **default owner** (section 3).
7. Carrying over and merging branches: aktujo does fields, a program its own data through a trait of the SDK.
8. Guest links without a server: a relay can serve them if whoever runs it switches that on; off by default.
9. Search covers everything a device can read, every line, and says which.

## What notenwart does today

From notenwart's specification ("Who sees a part"): a member sees a part when one of their ensembles has the part's
work and they play the part's instrument there; leaders see every part of their ensembles' works; a leader can grant
a single member a single part, and the grant ends when the member leaves or the work leaves the ensemble. A server
evaluates the rule (an SQL view) and filters what it answers. Everything below must be able to carry that, without
aktujo knowing about ensembles or instruments.

## 1. Audiences

```json
{"type": "audience", "grants": [{"to": "space"}]}
{"type": "audience", "grants": [{"to": "person", "person": "p-…"}, {"to": "group", "group": "g-…"}]}
```

- **`space`** – every person of the space, now and later. This is what every document is today.
- **`person`** – one person, with all its devices.
- **`group`** – every person in the group, now and later.

The owner ([0007](0007-erasure-and-locks.md)) always sees its document, whatever the audience says; a document of the
space has no such person. A **private** document is one whose audience is empty – only its owner sees it.

A grant can carry **`until`**, a date after which it no longer holds (a part for a concert, a document for an
auditor). End to end, "no longer holds" means no new keys from then on (section 5).

### Changing the audience

`document_audience {document, audience}` sets it anew – a record in the document's stream ([0009](0009-streams.md)),
checked against the rights its author had at its position. Who may:

- the owner of a document of its own;
- for a document of the space, a member with the grant **`share`** ([0007](0007-erasure-and-locks.md), section 5);
- a member with `manage`, for any document of the space, never for a person's.

The audience is not part of a revision and may change on locked documents.

### A space's default

A space has a setting, **new documents are shown to**: an audience given to every document added, unless the adder
chooses another (and may). A new space: `space`. notenwart sets none – it writes the audience of every part itself.

## 2. Groups

A group has a pseudonym (`g-` and 12 characters of `a-z0-9`), a name sealed with the group's key, and persons:

- `group_made {group}`, `group_named {group, sealed}`, `group_joined {group, person}`, `group_left {group, person}`,
  `group_removed {group}` – records in the ordered list of the space ([0007](0007-erasure-and-locks.md), section 2), so
  that every device agrees who was in a group when.
- Made, changed and removed by members with the grant **`groups`**, or `manage`.
- A group can be **hidden**: its persons are known only to members with `groups` – for groups of minors, where the
  members of an ensemble should not learn the others (notenwart: "Members of an ensemble only see themselves").
  End to end this holds with streams: who is not in a group learns nothing of it ([0009](0009-streams.md),
  section 7).

Groups do not nest. Nested groups are easy to want and hard to revoke; a program that needs them writes the
persons out.

## 3. Folders

A folder gathers documents. It is not a place: a document can be in any number of folders, a folder in any number of
folders above it, so that the same documents can be ordered in several hierarchies at once, without copies – by year
and by kind, by ensemble and by composer. Decided by the maintainer on 2026-11-19:

- Folders are **like groups**, not like the folders of a file system.
- **Labels become folders.** A label was a folder without settings and without a folder above; one concept instead
  of two.
- **Settings given to a folder are inherited, live** – to save the time of setting each document. What a folder holds
  now is what its settings reach now.

### What a folder is

A folder has a pseudonym (`f-` and 12 characters of `a-z0-9`), a name sealed with the **folder key**, an owner – the
space or a person, like a document ([0007](0007-erasure-and-locks.md)) – the folders it is in, and settings:

- `folder_made {folder, owner}`, `folder_named {folder, sealed}`, `folder_placed {folder, in}`,
  `folder_unplaced {folder, in}`, `folder_audience {folder, audience}`, `folder_removed {folder}` – records in the
  ordered list ([0007](0007-erasure-and-locks.md), section 2). The order decides when two devices place folders into
  each other: a placing that would close a circle is refused, the same on every device.
- `document_filed {document, folder, key}` and `document_unfiled {document, folder}` – records in the same ordered
  list, so that it is clear what rights their author had (below). `key` is the document key sealed with the folder
  key: filing is also handing the key to the folder.
- Removing a folder unfiles its documents and unplaces its folders; it deletes no document.

### What a folder passes on

**The audience.** Who sees a document is the union of

- its owner,
- its own audience (section 1),
- the audience of every folder it is filed in, and of every folder above those, all the way up.

So a part filed in "Orchestra A" and in "Orchestra B" is seen by both. A folder never makes a document's audience
smaller – who must not see a document, the document is not filed for. A folder above another sees what is in it.

End to end this is a chain of keys: the document key sealed with the folder key, the folder key sealed with the keys
of the folder's audience and with the key of each folder above it. Who joins a folder's audience gets one key and
opens everything in it and below it.

**Other settings** a folder can carry, live in the same way:

- **lock rules** ([0007](0007-erasure-and-locks.md), section 3) – "which documents" can name a folder; a lock itself
  is always per document and revision, written when a rule is due;
- **guest links** (section 8) to a folder – what it holds now, or at a moment;
- **retention rules** – the counterpart of lock rules: after how long (from when a document was added, or from a date
  field) a document here goes into the bin, with a reason, and is offered for purging. It never purges by itself, and
  a locked document is left alone. In several folders, the latest date holds – deleting cannot be undone;
- a **default kind** – suggested for a document filed here, as a suggestion, never confirmed by itself. In several
  folders with different defaults, all are offered;
- a **default owner** – the space or the adder, for documents added into the folder; the space's setting `owners`
  ([0007](0007-erasure-and-locks.md)) still holds above it. In several folders that differ, the adder is asked;
- further settings are added here when they come, each saying how it combines when a document is in several folders
  (the audience: union; lock rules: every rule holds).

### Who may

Filing and placing widen an audience, unfiling and unplacing narrow it – so they need the right to change the
audience of what is moved:

- **Filing a document**: whoever sees it and may change its audience – the owner for its own documents, `share` for
  the space's – and may change the folder's contents: the folder's owner, or `share` for folders of the space.
- **Placing a folder in another**: the same, for the folder.
- **Making folders**: every member with `write`. A person's folders are its own, and nobody else sees them unless
  their audience says so.
- **A folder's audience**: its owner for its own folders, `share` for the space's.

**A setting reaches only what its author may change.** A folder's settings act on a document only as far as whoever
set them had the right to change that document's audience – decided by the maintainer on 2026-11-19. Checked per
document, at the setting's position in the ordered list:

- A member with `share` widens a folder of the space; the documents of the space in it are reached. A document a
  person filed there of their own is **not** reached: the folder keeps passing on to it what it passed on before –
  the audience the owner agreed to when filing.
- The same for narrowing, and for every other setting: a lock rule on a folder locks only documents its author may
  lock.
- The folder view shows, per document, which settings reach it and which do not, and why.

End to end the folder key follows this: changing a folder's audience makes a **new folder key** for the new
audience; its author seals with it the keys of the documents it may reach. The others stay under the previous folder
key, sealed to the previous audience. A folder can so hold several keys at once, one per audience it has had for some
of its documents.

Narrowing – unfiling, unplacing, a smaller folder audience – works as in section 5 (Taking away): no new keys from
then on, what was read may have been kept.

## 4. Rules stay in the programs

aktujo knows no rule why someone sees something. A program that has one keeps it and writes what follows from it:

- notenwart makes a group per ensemble and per instrument and part within it, or grants persons directly, and
  rewrites the audiences of a work's parts whenever an ensemble, a member's instrument, a work or a single grant
  changes.
- A program can also leave audiences alone and only filter more in its own interface – that is its business, but it
  is not visibility: every device of the audience can still read.

The rule runs where the program runs. With a server (notenwart), the server writes; end to end, a device of a member
with the right does, when it notices – like lock rules ([0007](0007-erasure-and-locks.md), section 3).

## 5. Enforcing it

### On a server that is trusted

A server that answers others through an API – `aktujo serve` on PostgreSQL, notenwart – holds the keys of every
document it serves (it is a member of the space, or the space never leaves it) and **filters** every answer by
audience: lists, search, originals, events, exports. Taking someone out of an audience takes effect at once.

The people a server answers need not be members with devices: a **person** ([0007](0007-erasure-and-locks.md)) can be
known only to the server, by a login. Audiences name persons, so the same grants work for both.

Its operator can read everything. That is the price of a server, and said so.

### End to end

Without a server that is trusted, seeing is holding the key. The document key ([0007](0007-erasure-and-locks.md)) is
sealed for its audience:

| Grant | The document key is sealed with |
|---|---|
| `space` | the space key, as before |
| `person` | each device of the person (its receiving key) |
| `group` | the **group key** – one key per group, sealed to each device of each person in it |

A group key spares sealing every document to every device: a person joining a group gets one key and reads all of
the group's documents. Group keys form a chain like space keys: a new key when someone leaves; the new key seals the
previous one, so that later members read what came before.

**Taking away** – a person leaves a group, a grant is removed, `until` passes:

- From then on the person gets no new keys: the group gets a new key; documents that were shown to it get a new
  document key for what is added from then on, sealed for the audience as it is now.
- What the person could read, they may have kept. That cannot be taken back – the interfaces say so where an audience
  is made smaller.
- Re-encrypting everything that was there before for the new key would not help against a copy and is not done.

**Who writes the keys.** Sealing a document key for someone needs the key: a device that can read the document does it
when it sees an audience change or a new device of a person in the audience. A document nobody online can read waits.

## 6. What others see

End to end, the log of a space reaches every member, sealed or not. A member who does not see a document still sees:

- that it exists, its identity, and the pseudonym of its owner;
- when its events were written, how many, how large; how large its originals are;
- its audience – which persons and groups – unless the group is hidden; then only that a hidden group is in it.

Not its title, kind, fields, text or originals, nor more of its folders than their pseudonyms. On a server, members
see only what the server shows them.

For most uses that is enough. Where even existence and timing say too much – how many documents there are about one
pupil – a space should not be shared at all: separate spaces, or a server. Streams ([0009](0009-streams.md)) close
most of it: a member learns nothing of streams it cannot open.

## 7. Everything else follows the audience

- **Search** – each device indexes everything it can read, the main line and every branch shown to it; a hit says
  in which line it is. A server searches what the asking person sees.
- **Kinds, rules, lock rules, catalog** – data of the space, seen by every member.
- **Folders of one's own** – a person may file a document shown to them into a folder of their own, seen only by
  them; that widens nothing, as the folder's audience is the person only.
- **Locks** ([0007](0007-erasure-and-locks.md)) – only a device that can read a document can lock it.
- **Purging** – as in [0007](0007-erasure-and-locks.md); only what is in the bin is purged, and only who sees a
  document puts it there.
- **Exports** – a person exports what they see; a full export of a space needs every key, so a server or a member that
  sees everything.
- **Sync** – a device receives only the streams it can open ([0009](0009-streams.md)).

## 8. Guest links

A guest link shows a document to someone who is not a person of the space – no account, no device of the space. Decided
by the maintainer on 2026-11-19:

- **What it shows** is chosen when the link is made: **this revision**, exactly – or **always the current one**.
- An **expiry date** and a **password** can be set.
- **View only** is the default; **download** can be allowed. aktujo's own viewers (web, apps) keep to it; a screenshot
  cannot be prevented, and the interfaces do not promise more.
- A document always shows **whether and how it is shared**: every link with what it shows, its expiry, whether it has
  a password and whether it allows download. Any link can be **revoked** there.

How (proposal):

- A link never carries a document key. It points to a **copy**: the revision's snapshot and originals, sealed with a
  **link key** of its own. Revoking is removing the copy; whoever has opened it may have kept it.
- The link key travels in the part of the address after `#`, which a browser never sends to a server – the host of the
  copy cannot read it. With a password, the link key is sealed with a key derived from the password (Argon2id); the
  address then carries only the sealed key.
- **Always the current one**: when the document gets a new revision, a device that can read it renews the copy under
  the same link key. On a server that is at once; end to end, when such a device is online.
- A link is a record in the ordered list of the space (`link_made`, `link_revoked`), so every device shows the same
  links; its key is sealed with the document key, so only who sees the document sees the link.
- **Who may** make a link: whoever sees the document and holds the grant **`link`**; the owner of a document of its
  own always. Revoking: whoever made it, the owner, or a member with `share`.
- On a **server** the server keeps the copy and checks expiry and password itself, too.
- **Without a server**, a relay can serve guest links – **off by default**, switched on by whoever runs the relay
  (`aktujo relay --guest-links`). It then serves sealed copies to anyone with a link and nothing else; a space uses
  it only if its relay offers it. Otherwise a space without a server has no guest links.

A link can also be made to a **folder** (section 3): it shows what the folder holds – now ("always the current one"),
or as it was when the link was made ("this revision": the documents at their revisions then).

## 9. Branches

A document has one line of revisions that its audience sees – the **main line**. Beside it there can be **branches**,
like branches in git: revisions that only a person or a group sees. Decided by the maintainer on 2026-11-19: there
are private revisions – seen by oneself only, or by a group, and so on. A pupil's own marks in a part are a branch of
the pupil's; an ensemble's marks, one of the ensemble's group.

### What a branch is

```json
{"type": "branch", "branch": "b-…", "document": "…", "owner": "p-…",
 "from": "<revision hash>", "seen": {"<device>": <seq>, …}, "audience": {…}}
```

- **`from`** and **`seen`** – where it starts: a revision of the main line or of another branch, fixed the way a lock
  fixes one ([0007](0007-erasure-and-locks.md), section 3), so that every device agrees what the branch was made from.
- **`owner`** – the person who made it. **`audience`** – whom it is shown to besides its owner: nobody (private),
  persons, groups (section 1). A branch is never shown to more than the document is: its audience is cut to the
  document's, live – who stops seeing the document stops seeing its branches.
- `branch_made`, `branch_audience`, `branch_removed` – records in the ordered list. The revisions on a branch are
  ordinary document events with a member `branch`; without it, an event is on the main line.
- A branch has a **branch key**, sealed for its audience like a document key; its events are sealed with it. Who does
  not see the branch sees, as for a document, that it exists, whose it is and how much is in it (section 6).

### Reading a document with branches

Each person sees the main line and the branches shown to them, and chooses which one to look at. An interface may
open a person's own branch first – notenwart: the part with the pupil's marks. "The current revision" is per line:
the head of the main line, or the head of a branch.

### When the main line moves on

A new edition of a part is a new revision on the main line; the pupil's branch still starts from the old one. The
owner of a branch can **carry it over**: what changed on the branch since `from` is applied to the new head of the
main line, field by field, and where both changed the same field, it is a conflict that the person resolves – never
resolved silently. Annotations are data of the program (an original, a field). aktujo carries fields over itself;
for the rest the SDK offers a trait, **`Merger`**, like `Recogniser`: a program registers it for its own data (an
overlay of marks) and is asked when a branch is carried over or merged. What no merger takes is a conflict.

### Merging

A branch can be **merged** into the line it came from: its changes become a new revision there, with the same
conflicts as carrying over. Merging needs the right to make a revision on the target line – for the main line, what
the document's owner and rights allow, and nothing a lock forbids. The ensemble's leader merging a player's marks into
the ensemble's branch is such a merge.

### Branches and the rest

- **Locks** fix a revision of a line. A lock on the main line does not stop branches: they never change the locked
  revision. A branch can be locked itself.
- **Purging a document** purges its branches. A branch alone is purged by its owner (its key thrown away), or with the
  document's owner's rights.
- **A person leaving** ([0007](0007-erasure-and-locks.md), When a person leaves): their branches go with their private
  documents – purged, unless merged or taken along first.
- **Guest links** show a line: the main line unless the link is made from a branch.
- **Folders** gather documents, not branches.

## 10. Rights

[0007](0007-erasure-and-locks.md) section 5 gets three more grants:

| Grant | Allows |
|---|---|
| `share` | change the audience of a document of the space; revoke any guest link |
| `groups` | make, name, fill and remove groups; see hidden groups |
| `link` | make guest links to documents it sees |

## 11. Interfaces

- `audience(space, document)`, `set_audience(space, document, audience)`; `add` takes an audience.
- `groups(space)`, `make_group`, `name_group`, `join_group`, `leave_group`, `remove_group`.
- The space setting for new documents' audience.
- `branches(space, document)`, `make_branch(space, document, from, audience)`, `carry_over(space, branch)`,
  `merge(space, branch, into)`, `remove_branch(space, branch)`; every document call takes a line (default: main).
- `links(space, document)`, `make_link(space, document, revision or current, expiry, password, download)`,
  `revoke_link(space, link)`; a viewer for guests in the web app, view only unless the link allows download.
- A document view shows whom it is shown to and every guest link; making an audience smaller says what cannot be
  taken back.

## 12. The format

Part of the same format change as [0007](0007-erasure-and-locks.md) and [0009](0009-streams.md), **0.5.0**. Nothing is
kept compatible with 0.4 – nobody uses aktujo yet (maintainer, 2026-11-19).

## Open

- **The cost** of group key chains and new document keys on large spaces: to be measured.
