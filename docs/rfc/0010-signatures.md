---
title: "0010 Signatures"
parent: Design
nav_order: 10
---

# 0010 Signatures: checking, signing, and qualified signatures through those allowed to issue them

State: draft. Builds on [0007](0007-erasure-and-locks.md) (revisions, locks), [0008](0008-visibility.md) (audiences,
guest links) and [0009](0009-streams.md) (streams, persons, a server). Workflows – A approves, B signs – get a document
of their own; this one says what a signature is, so that they can use it. The direction comes from the whitepaper
(section 7) and the maintainer, who also settled the questions of the first draft on 2026-11-19.

## Summary

A document archive that people sign in has to do three things, and does them in this order:

1. **Check** the signatures of documents that arrive – who signed, whether the document changed since, at which level.
2. **Sign** – a simple or an advanced electronic signature, by a person of a space or a guest with a link.
3. **Sign qualified** – through those allowed to issue qualified signatures: qualified trust service providers and the
   European Digital Identity Wallet. aktujo never becomes a trust service provider itself.

A signature always signs **one revision** of a document ([0007](0007-erasure-and-locks.md), decision 14). End to end
stays end to end: to sign qualified, only a hash leaves the device, never the document.

## Decided

From the plan (whitepaper, section 7) and the maintainer:

1. Signatures come in three steps – checking, then simple and advanced signatures, then qualified ones.
2. Qualified signatures only through qualified trust service providers and the EU Digital Identity Wallet; aktujo is
   never a trust service provider.
3. A signature signs a revision – its hash; a workflow can set a lock (2026-11-19).

Settled on 2026-11-19:

4. **Open standards** that the law or interoperability require – PAdES, CMS, X.509, time stamps, the CSC API – are an
   accepted exception to "aktujo refers to no other protocol" ([0005](0005-pure-core.md)) – **and they live only in
   `aktujo-sign`** (section 8). No program and no provider is built in; which providers a space uses stays data.
5. **Both kinds of signature**: in the PDF and of the revision.
6. Certificates for advanced signatures from **both**: a server of the space as the organisation's certificate
   authority, and the person's own.
7. **Time stamps**: a space names a time stamp authority if it wants one; there is no default. Without one,
   signatures are B-B.
8. **Guests sign** only in spaces with a server.
9. **Renewing** is a rule like lock rules; in a space with a time stamp authority it is on by default for qualified
   signatures, and can be switched on for others.
10. A **kind can carry the level** of signature it needs, as data.
11. **`aktujo-sign`**: a spike assesses the candidates first.
12. **Qualified signatures**: first through providers (CSC API), the wallet when it signs (proposal, section 4).

## The law, briefly

What follows is a summary for building the software, not legal advice; which level a kind of document needs is for
someone qualified to say (OPEN, as in [0007](0007-erasure-and-locks.md)).

- **eIDAS** ([Regulation (EU) No 910/2014](https://eur-lex.europa.eu/eli/reg/2014/910)) knows three levels: an
  *electronic signature* (any data attached to other data and used to sign), an *advanced* one (Article 26: uniquely
  linked to and capable of identifying the signatory, created with data under the signatory's sole control, linked to
  the data so that any later change is detectable) and a *qualified* one (an advanced signature made with a qualified
  signature creation device and based on a qualified certificate; Article 25(2): the equivalent of a handwritten
  signature).
- **eIDAS 2** ([Regulation (EU) 2024/1183](https://digital-strategy.ec.europa.eu/en/policies/eudi-regulation), in force
  since 20 May 2024) brings the **European Digital Identity Wallet**. Every Member State provides one; it lets natural
  persons sign qualified **free of charge for non-professional purposes** (professional use may cost). When the
  wallets must be available – 24 months after the implementing acts; secondary sources say December 2026 – OPEN, to
  be checked against Article 5a.
- In German law, where the law asks for the written form, a qualified signature can replace it
  ([§ 126a BGB](https://www.gesetze-im-internet.de/bgb/__126a.html)); some forms cannot be replaced electronically at
  all (OPEN: which, per kind of document).

## The formats

Signatures that hold outside aktujo follow the standards that the law and every PDF reader expect – the accepted
exception to "aktujo refers to no other protocol" ([0005](0005-pure-core.md); decision 4):

- **PAdES** (ETSI EN 319 142-1) – a signature inside a PDF, at the levels **B-B** (the signature), **B-T** (with a
  time stamp), **B-LT** (with what is needed to validate it later: certificates, revocation data) and **B-LTA** (with
  a time stamp over all of it, renewable for long-term archiving).
- **CMS** – the signature container inside the PDF; **X.509** certificates; **time stamps** after RFC 3161.
- **CSC API** ([Cloud Signature Consortium, V2.2](https://cloudsignatureconsortium.org/resources/csc-api-v2-2/)) –
  how remote signing at a qualified provider is reached; the wallet is reached with OpenID4VP (section 4).

## 1. Checking

Every PDF that comes in is checked for signatures, like its text is read ([0002](0002-documents.md)):

- For each signature: who (the certificate's subject), when (signing time, time stamp), which level (PAdES B-B …
  B-LTA; electronic, advanced or qualified, as far as the certificate says), whether the bytes it covers are
  unchanged, whether the certificate chain leads to a trusted root and was not revoked at the time – and **what the
  signature does not cover** (pages or changes added after it, by incremental update).
- The result is **derived data**, like read text: stored as an original by hash, not part of a revision, read again
  when asked (trust changes over time).
- **Trust**: qualified signatures are checked against the EU's trusted lists; other roots are trusted per space
  (data of the space, like kinds). Fetching trusted lists and revocation data is a connection the person starts by
  checking, or that a server makes; offline, the last fetched lists are used and the result says so.
- A document view shows its signatures and their state; search can filter by them ("signed qualified", "signature
  broken").

## 2. What is signed

There are two kinds of signature, and a revision can carry both:

**A signature in the PDF** (PAdES). It is what holds outside aktujo – in any PDF reader, at a court, at an office.
Signing writes an incremental update into the PDF: new bytes, so a **new version** and a **new revision**
([0007](0007-erasure-and-locks.md)). The signature covers the PDF's bytes, not aktujo's metadata.

**A signature of the revision.** It signs the revision hash – originals, title, kind, confirmed fields – for any kind
of document, a photo as much as a PDF, and changes nothing: it is a record in the document's stream
(`revision_signed {revision, signature, certificate}`), the signature a CMS signature over the snapshot, detached. It
holds inside aktujo and can be exported with the document (a `.p7s` beside each original and the snapshot); outside
aktujo, only a tool that understands detached signatures checks it.

What a person signs is shown before they sign: the revision, its originals, its fields – and for a PDF, the pages as
they will be signed.

**Several signatures.** Signatures in a PDF are made **one after another**: each signs the PDF including the ones
before. Two people signing the same version at once would make two different PDFs; so starting a signature in a PDF
is a record in the document's stream (`signing_started`), ordered there ([0009](0009-streams.md)) – whoever comes
second signs the PDF with the first signature in it. Signatures of a revision can be made side by side.

**Locks.** A signature of a revision changes nothing and is allowed on a locked revision. A signature in a PDF makes a
new revision and obeys the lock (`new_revisions`). A workflow can lock a revision once it is signed.

## 3. Simple and advanced signatures

**Simple.** A person of the space confirms "I sign this revision"; their device signs the record with its device key,
and the person's pseudonym and name (sealed, [0007](0007-erasure-and-locks.md)) go with it. Optionally a visible stamp
in the PDF (name, date, an image of a handwritten signature). It proves that someone with access to that device
signed – legally an electronic signature, not an advanced one.

**Advanced.** Needs a certificate that identifies the person and a key under their sole control:

- The **signing key** is made on the device and stays there – in the operating system's key store, in hardware where
  there is some (Secure Enclave, a TPM, a security key). It is not the device key and is never synchronised: each
  device of a person has its own.
- The **certificate** comes from
  - a certificate authority **of the organisation**: a server of the space ([0009](0009-streams.md)) can be one; it
    issues short-lived certificates after the person signs in with the organisation's OpenID Connect provider. That
    makes the organisation a certificate authority, not a trust service provider; signatures are advanced, and a PDF
    reader outside shows them as valid only where that authority is trusted;
  - or the person: a certificate they have already (a file, a smart card, a security key).
- **Time stamps** (B-T and above) need a time stamp authority. A space names one if it wants; there is no default
  and no connection nobody asked for. Without one, signatures are B-B. A qualified provider usually adds its own.

## 4. Qualified signatures

aktujo prepares the PDF, computes the hash to be signed, and lets a qualified provider sign it. Only the **hash**
leaves the device – the document stays end to end. The signature comes back, is put into the PDF (a new version) and
recorded; aktujo then adds what B-LT and B-LTA need, where the provider does not.

**First: providers.** A qualified trust service provider with remote signing, reached through the **CSC API** – the
person has an account there, or the provider makes a short-lived certificate after identifying them. This works today
and does not wait for the wallets.

**Then: the wallet.** The EU Digital Identity Wallet signs in one of two ways, according to the German architecture
([BMI, QES](https://bmi.usercontent.opencode.de/eudi-wallet/eidas-2.0-architekturkonzept/content/upcoming-features/qes/))
and the EU's reference framework
([ARF, annex 4](https://eu-digital-identity-wallet.github.io/eudi-doc-architecture-and-reference-framework/1.4.0/annexes/annex-4/annex-4.07-remote-qes-enrolment.pdf)):
*wallet-centric* – the relying party asks the wallet for a signature with **OpenID4VP**, the wallet has a provider sign
through the CSC API – or *provider-centric* – the wallet authorises a provider, which signs. aktujo would be the relying
party in both, and asks the wallet with OpenID4VP; which of the two first: OPEN, when the wallets sign.

**The state of the German wallet** (as of 2026-11-19, secondary sources): it starts on 2 January 2027
([iGrant tracker](https://eudi-wallet-tracker.igrant.io/tracker/germany/)); the official site announces qualified
signatures for later ([eudi-wallet.gov.de](https://eudi-wallet.gov.de/en/app)) – when: OPEN.

**A relying party registers.** Under eIDAS 2 a relying party that asks wallets registers in its Member State. That is
whoever runs a server or offers the app – not aktujo as software. How a self-hosted server registers, and whether an
app on a device can be a relying party without one: OPEN.

Which providers are offered is data, not code: a space lists the ones it uses; aktujo builds none in
([0005](0005-pure-core.md)).

## 5. Guests sign by link

A person without an account signs by link (whitepaper): a guest link ([0008](0008-visibility.md), section 8) with the
permission **sign**. The guest sees the revision, signs – simple (a typed name, confirmed by a code sent to an address
the inviting person entered) or qualified (their own wallet or provider) – and the signature goes back into the
document's stream. A guest is no member and cannot write a stream: a **server** receives the signature and writes it
([0009](0009-streams.md), section 5). Without a server, guests cannot sign (decision 8); a relay takes nothing from
strangers.

## 6. Long-term

Signatures outlive the certificates they were made with. aktujo can **extend** PAdES signatures to B-LTA and renew
the archive time stamp before its algorithms weaken – a new version each time, done by a server or a device with the
right, on a **renewal rule** like lock rules ([0007](0007-erasure-and-locks.md)): renew before the certificate or the
last time stamp expires, or when an algorithm is announced as weakening. In a space with a time stamp authority the
rule is on for qualified signatures by default; for others it can be switched on, per space or folder
([0008](0008-visibility.md), section 3). Extending writes into the PDF and so makes a new revision; where a lock
forbids new revisions, the extension – validation data and an archive time stamp – is kept beside the PDF instead,
as derived data, and the interfaces say so.

## 7. Rights and audit

- **Required level.** A kind ([0005](0005-pure-core.md)) can say which level of signature its documents need:
  `"signature": "simple" | "advanced" | "qualified"`. Signing offers that level and warns before a lower one. aktujo
  sets none itself – catalog templates and programs do; which level the law asks for stays for someone qualified.
- New permissions ([0009](0009-streams.md), section 6): **`sign`** – sign what one can read; **`sign_link`** – make
  guest links that allow signing; **`trust`** – change the trusted roots and providers of a space. User gets `sign`.
- Every step – started, signed, refused, extended, checked – is a record in the document's stream: the audit log the
  whitepaper describes, signed by the device that wrote it and chained.

## 8. The crate

A new crate **`aktujo-sign`** (MIT) beside the document crates, and the **only** place the standards of this
document live: PAdES and detached CMS signatures, made and checked; incremental updates to PDFs; X.509 certificates
and their chains; time stamp requests and answers; the EU's trusted lists; the messages of the CSC API and of
OpenID4VP. It has no I/O of its own – fetching trusted lists, revocation data and time stamps, and talking to a
provider or a wallet, go through traits the program fills (like `Blobs`); the messages are built and read here.

Everything else stays free of them: `aktujo-doc` knows a signature record as aktujo's own (`revision_signed` with the
signature as bytes, its level, who, when) and a kind's `signature` level as a word; the SDK calls `aktujo-sign` and
fills its traits; the core never sees a signature format. One exception outside the crate: the organisation's
certificate authority signs people in with OpenID Connect – that is a login of the server (the program), not part of
a signature.

A **spike** comes first (decision 11) and assesses, by licence, maturity, dependencies and conformance against the
EU's validation: [underskrift](https://docs.rs/crate/underskrift/0.1.3) (PAdES B-B to B-LTA, according to its
documentation) and [pdf_oxide](https://docs.rs/crate/pdf_oxide/0.3.76) (checking; signing since 0.3.50, according to
its documentation), and the RustCrypto crates (`cms`, `x509-cert`) as the base for writing it by hand, as
`aktujo-scan` writes PDFs.

`aktujo-minimal` ([0004](0004-archive-0.1.md)) checks signatures too; whether it signs: OPEN.

## Open

- **The wallet**: wallet-centric or provider-centric first, and when the German wallet signs.
- **Registering as a relying party**: how a self-hosted server does, and whether an app alone can.
- **`aktujo-minimal`**: checks signatures; whether it signs.
- **Legal**: which level each kind of document needs – for someone qualified to say.
