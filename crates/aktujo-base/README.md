# aktujo-base

The base of the aktujo core: the canonical encoding of JSON (RFC 8785, restricted to what the core uses), hashes
`sha256:…`, identities of spaces, bytes as base64url and root objects with their type. No I/O.

Hashes, root objects and the canonical encoding follow parts 0002 and 0003 of the open protocol librotenado, from
whose reference library parts are taken over (MIT).

Part of the core of [aktujo](https://github.com/aktujo/aktujo) – libraries others build in. The design is in
[RFC 0001](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0001-architecture.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-base) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on the core is AGPL-3.0-or-later.
