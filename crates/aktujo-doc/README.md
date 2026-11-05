# aktujo-doc

Documents on the aktujo core. A document is not a file: it has an identity, a title, a kind, fields, labels and
versions, each an original stored by its hash. Everything that happens to a document is an event in the log of its
space (`aktujo-log`); `Documents::from_log` folds them into the state every device agrees on.

Nothing in it belongs to one trade: kinds of documents – their fields, the words that speak for them, the ways to
find their fields – are data of a space, events like everything else, defined by the people and programs that use it.
Every
value read from a document – a field, the kind – is a suggestion with its source; only a person confirms one, and a
suggestion never replaces a confirmed value. No I/O.

Part of [aktujo](https://github.com/aktujo/aktujo): a library others build in, on the aktujo core. The design is in
[RFC 0002](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0002-documents.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-doc) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on it is AGPL-3.0-or-later.
