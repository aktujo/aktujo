# aktujo-relay

The relay of the aktujo core: it holds, per space, the ordered member list and the envelopes of events and originals,
and passes them on to the members. It cannot read them – it checks signatures, the rules of the member list and who
may write, never content. A library (an axum router) and a service.

Part of the core of [aktujo](https://github.com/aktujo/aktujo) – libraries others build in. The design is in
[RFC 0001](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0001-architecture.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-relay) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on the core is AGPL-3.0-or-later.
