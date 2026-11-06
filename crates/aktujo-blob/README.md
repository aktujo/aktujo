# aktujo-blob

Originals stored by their SHA-256 hash, on any storage: in memory, in a folder, and – with the feature `object` or
`s3` – in an object store such as S3, MinIO, Garage or Ceph. Bytes are checked against their hash on every read;
damaged bytes are reported, never returned. Originals can be removed, sized and read in byte ranges – to stream a
long recording.

| Feature | What |
|---|---|
| `object` | `ObjectBlobs` over any store of the crate `object_store` |
| `s3` | `ObjectBlobs::s3` for S3 and S3-compatible storage |

Part of the core of [aktujo](https://github.com/aktujo/aktujo) – libraries others build in. The design is in
[RFC 0001](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0001-architecture.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-blob) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on the core is AGPL-3.0-or-later.
