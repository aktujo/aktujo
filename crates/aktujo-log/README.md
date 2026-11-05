# aktujo-log

The event log of a space: immutable events of any kind, ordered by a hybrid logical clock, chained per device and
identified by the hash of their canonical encoding. Two logs merge by union, in any order, any number of times, with
the same result; what does not fit a clean chain is reported, never resolved silently.

Part of the core of [aktujo](https://github.com/aktujo/aktujo) – libraries others build in. The design is in
[RFC 0001](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0001-architecture.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-log) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on the core is AGPL-3.0-or-later.
