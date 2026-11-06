# aktujo-store

Storage of the aktujo core – in SQLite on a device, in PostgreSQL on a server: the events and member records of every space a device holds, which
originals belong to it, its settings, and what it has sent to a relay. One database holds all spaces of a device, so
that spaces that belong together can be written in one transaction. With the feature `sqlcipher` the database is
encrypted as a whole (SQLCipher with OpenSSL built in, `Store::open_encrypted`). A program keeps tables of its own
beside the core's – views to search, for example – through `Store::connection`; the binding is re-exported as
`aktujo_store::rusqlite`.

The trait `Backend` is what the core keeps; `Store` keeps it in SQLite, `PgStore` (feature `postgres`) in PostgreSQL –
the same tables in a schema of their own (`aktujo` unless named), beside a program's tables in the same database.
Several servers may share it. The client is synchronous; from async code call it in `spawn_blocking`. OPEN: TLS to
the database.

Part of the core of [aktujo](https://github.com/aktujo/aktujo) – libraries others build in. The design is in
[RFC 0001](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0001-architecture.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-store) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on the core is AGPL-3.0-or-later.
