---
title: "0006 Apps and the SDK"
parent: Design
nav_order: 6
---

# 0006 Apps and the SDK: one workspace under MIT, the apps in Flutter

State: draft. Changes [0001](0001-architecture.md) (which crates are under which licence); decided by the maintainer
on 2026-11-06.

## Summary

aktujo gets apps for desktops and phones – capture with the camera, file, find, keep in step – in the repository
`aktujo/apps`, under MIT so that they can go into the app stores. They are built like the apps of kernoid, another
falkOSS project: **one Flutter app** for macOS, iOS, Linux, Android and Windows, with a Rust side reached through
`flutter_rust_bridge`.

An app keeps a person's archive on the device, offline, end-to-end encrypted and in step through a relay – what the
program does. So the layer every interface uses, the workspace, becomes a library under MIT:

| Crate | Before | Now |
|---|---|---|
| `aktujo-app` → **`aktujo-sdk`** | the workspace, AGPL | the workspace, **MIT** – the SDK of every interface and every app |
| `aktujo-core` | the domain's types and rules, AGPL | **MIT** |
| `aktujo-api`, `aktujo-tui`, `aktujo` (command line), the web app | AGPL | AGPL, unchanged |

The rule of falkOSS stays: what others build in is MIT, what one runs as a service is AGPL. The workspace is built
in – by the apps, and by anyone who wants an archive in a program of their own. The program that serves the API and
the web app, the terminal interface and the command line stay AGPL. The maintainer holds the rights to all of this
code and puts it under MIT.

## The SDK

`aktujo-sdk` is `aktujo-app` renamed, nothing else changed: a data folder (SQLCipher, sealed originals, the key in the
key store or a key file), spaces, documents, kinds, reading in the background, search, sync and sharing, catalogs,
watched folders, improving, export and import. Its features stay: `ocr` builds the OCR models in, `scan` improves
scans; without them it reads text layers only.

- **Permissive all the way down:** every crate it depends on is under a permissive licence (MIT, Apache-2.0, BSD,
  ISC, Zlib, Unicode-3.0, CDLA-Permissive-2.0 – measured on 2026-11-06). CI checks that it reaches no crate under the
  AGPL, the LGPL or the MPL.
- **The interfaces use the SDK only**, as before; the API stays the boundary for clients that are not built on it
  (the web app).
- **On crates.io** with the other library crates, in the program's version. The OCR models are not in the crate: a
  build with the feature `ocr` takes them from the crates `aktujo-extract-det` and `aktujo-extract-latin` of
  `aktujo/models` (Apache-2.0; changed 2026-11-06 – before, from a folder in `AKTUJO_OCR_MODELS`). Until the SDK is on crates.io, the apps
  take it from the repository at a commit.

## The apps

In `aktujo/apps`, laid out as kernoid's apps are:

- `rust/` – the Rust side (`aktujo_bridge`), a workspace of its own with exact, permissive dependencies
  (`cargo deny`); `src/api/` translates the SDK for Dart and nothing more.
- `lib/` – the Flutter app: Dart, the look of aktujo/brand (colours, Space Mono for headings), strings in English
  and German (`lib/l10n/`).
- `rust_builder/` – cargokit, which builds the Rust side for each platform.
- Platforms as falkOSS builds them (starterpack, `docs/practices.md`): macOS on Apple silicon, iOS and iPadOS,
  Android on arm64, Windows on x64, Linux on x64 and arm64.

What they do, in this order:

1. The archive on the device: spaces, documents (add files), titles, kinds and fields, labels, the bin, search –
   the same data folder as the program.
2. Capture with the camera, improved with `aktujo-scan` (straightened, cropped, black and white) into a PDF.
3. Sync and sharing through a relay, invitations, recovery.
4. Kinds and catalogs, export and import.

The apps keep the key of their data folder in the key store of the system, as the program does (Keychain, Android
Keystore through `keyring`'s support or a key handed over from Dart – OPEN for Android and iOS).

## Open points

- OPEN: the key store on Android and iOS – `keyring` reaches Apple's Keychain; on Android a key kept by Dart
  (`flutter_secure_storage`) and handed to the SDK through `KeyStore`.
- OPEN: a review of the licence boundary by someone who knows licence law, as for kernoid's apps.
