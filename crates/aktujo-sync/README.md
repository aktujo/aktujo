# aktujo-sync

Synchronisation of spaces between the devices of their members through a relay, end-to-end encrypted. A `Replica` is
what one device holds: events and member records (`aktujo-store`), originals (`aktujo-blob`) and its clock. Devices
join by an invitation; when every device is lost, the recovery code adds a new one.

Part of the core of [aktujo](https://github.com/aktujo/aktujo) – libraries others build in. The design is in
[RFC 0001](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0001-architecture.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-sync) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on the core is AGPL-3.0-or-later.
