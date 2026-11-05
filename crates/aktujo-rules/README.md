# aktujo-rules

The kind and the fields of a document, read from its text – as suggestions for a person to confirm. Input are the
positioned lines of `aktujo-extract`; output are a suggested kind and values for its fields as `aktujo-doc` writes
them, each with a confidence, the rule's name and the line it came from.

It knows no kinds of documents, words or labels – it is the mechanism only. What speaks for a kind and where its
fields are is data of the kind (`aktujo_doc::Kind`: words with weights, the ways to find each field), defined by the
people and programs that use it – for letters, sheet music, research papers or a collection of books alike. A
program that needs more brings its own code behind the trait `Recogniser`, combined with `Chain`. The readers of
dates, amounts and labels are public in `aktujo_rules::text`; they read German, English, French, Italian, Spanish and
Dutch. No I/O.

```rust
use aktujo_rules::{Recogniser, Rules};

let kinds: Vec<aktujo_doc::Kind> = serde_json::from_str(&json)?;
let recognition = Rules::default().recognise(&kinds, &extraction, None);
if let Some(kind) = &recognition.kind {
    println!("{} ({:.2})", kind.kind, kind.confidence);
}
for found in &recognition.fields {
    println!("{} = {:?} ({:.2}, {})", found.key, found.value, found.confidence, found.rule);
}
```

To try it on a file (with `aktujo-extract`'s backend `pure`; the folder of OCR models is optional):

```sh
cargo run --release -p aktujo-rules --example recognise -- <kinds.json> <file> [<folder with det.onnx, rec.onnx, dict.txt>]
```

Part of [aktujo](https://github.com/aktujo/aktujo): a library others build in, on the aktujo core. The design is in
[RFC 0003](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0003-recognition.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-rules) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on it is AGPL-3.0-or-later.
