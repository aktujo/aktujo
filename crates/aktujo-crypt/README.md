# aktujo-crypt

End-to-end encryption of a space: members and their keys from a seed, the chain of space keys, the strictly ordered
member list, envelopes for events and originals, signed requests to a relay, invitations and recovery codes.

Ed25519, HPKE (RFC 9180) with X25519, HKDF-SHA256 and ChaCha20Poly1305, XChaCha20-Poly1305, HMAC-SHA256. The labels of
key derivation are a parameter (`Domain`): keys of one domain never open anything of another.

Part of the core of [aktujo](https://github.com/aktujo/aktujo) – libraries others build in. The design is in
[RFC 0001](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0001-architecture.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-crypt) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on the core is AGPL-3.0-or-later.
