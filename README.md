# aktujo

**Your documents, kept safe – end to end.** A [falkOSS](https://falkoss.org) project.

**aktujo** takes care of a document's whole life: scan or upload it, read and label it, keep every version, find and share it, and route it through approvals and signatures – every step in an audit log. Documents are end-to-end encrypted and stay where you want them – on your own devices, synced directly between them, or in your organisation's cloud. aktujo runs on one computer as well as on Kubernetes with PostgreSQL and S3, on its own or built into other apps. Free software – everything included, no telemetry.

aktujo is at its beginning: there is nothing to install yet.
The idea and the plan are in the [whitepaper](https://github.com/aktujo/whitepaper).
The website is [aktujo.org](https://aktujo.org), the documentation is at [aktujo.dev](https://aktujo.dev).

| Path | What |
|---|---|
| [crates/](crates/) | the core others build in (MIT): `aktujo-base`, `-blob` (originals by hash), `-log` (events), `-crypt` (end-to-end encryption), `-store` (SQLite), `-relay`, `-sync`; documents (MIT): `aktujo-doc`, `-extract` (text from PDFs and images); the program (AGPL): `aktujo-core` (the domain), `-app` (the one layer every interface uses), `-api` (HTTP/JSON), `-tui` (terminal), `aktujo` (the command line) |
| [web/](web/) | the web app – Svelte and TypeScript, embedded into the binary |
| [api/](api/) | the API description in OpenAPI, generated from the code |
| [docs/](docs/) | the documentation site – Jekyll, published by GitHub Actions; the design in [docs/rfc/](docs/rfc/) |

The brand kit lives in its own repository, [aktujo/brand](https://github.com/aktujo/brand).

## Try it

```sh
(cd web && pnpm install && pnpm build)   # the web app the binary embeds
scripts/get-ocr-models.sh                # the OCR models the binary embeds (checked by their hashes)
cargo run -- serve                       # the archive in the browser, on http://127.0.0.1:8071
cargo run -- tui                         # the same archive in the terminal
cargo run -- serve --memory              # to try it out: nothing is kept, no key is stored
cargo run -- relay --data relay          # a relay to synchronise through, on http://127.0.0.1:8072
```

A space can be synchronised between devices and shared with other people through a relay you run yourself
(`aktujo relay`): end-to-end encrypted, the relay sees neither keys nor content. In the web app it is under
*Sync and sharing*; joining with an invitation, naming the device and bringing a space back with its recovery code are
in the settings.

The documents live in a data folder – `aktujo` in the folder for application data of the user (on macOS
`~/Library/Application Support/aktujo`, on Linux `~/.local/share/aktujo`, on Windows `%APPDATA%\aktujo`), or the one
given with `--data`. It is encrypted; its key is kept in the key store of the operating system. The interfaces speak
English and German – their texts are data in `locales/`.

`cargo run --no-default-features -- …` builds `aktujo-minimal`: without scanning and OCR, so without the models.

## The name

```
akto   a document that attests something
 -ujo  a container for it
      → aktujo
```

*aktujo* is Esperanto for a document folder. *akto* is an official document that attests something – a notice, a contract, a certificate – and the suffix *-ujo* makes a container for it. The Reta Vortaro, one of the standard dictionaries, describes an *aktujo* as a container for documents, at its simplest a folded sheet of cardboard. Said ak-TU-jo, stressed on the second-to-last syllable like every Esperanto word.

## Principles

- **Everything included** – one edition, every feature free; no paid tiers, licence keys or unlocks.
- **No telemetry** – only connections that are technically needed or that users start themselves.
- **In the open** – issues, plans, reviews and decisions happen in public.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Every commit is signed off under the
[Developer Certificate of Origin](https://developercertificate.org) (`git commit -s`); there is no CLA.

## Licence

The program – server, terminal and web app – is under [AGPL-3.0-or-later](LICENSES/AGPL-3.0-or-later.txt); the
core that other projects build on – `aktujo-base`, `-blob`, `-log`, `-crypt`, `-store`, `-relay`, `-sync` – is under
[MIT](LICENSES/MIT.txt), marked in each crate's `Cargo.toml`. Later libraries for documents (recognition) are planned
under MIT as well. An SPDX header in every source file ([REUSE](https://reuse.software)). The
copies of the brand's artwork are under [CC BY-ND 4.0](LICENSES/CC-BY-ND-4.0.txt); the brand (name, logo, marks)
itself is not covered by any licence.

© 2026 falkOSS – Falcon Lake Open Source Software gUG (haftungsbeschränkt) i. G. and the aktujo contributors
