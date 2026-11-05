# aktujo-rules

The kind and the fields of a document, read from its text – as suggestions for a person to confirm. Input are the
positioned lines of `aktujo-extract`; output are a suggested kind and values for its fields as `aktujo-doc` writes
them, each with a confidence, the rule's name and the line it came from.

It is general on purpose: it knows the kinds every archive has and the values every document has – who sent it,
when, a reference, an amount. Programs that know more specialise it: their own kinds and fields, their own rules as
data (JSON, added with `Rules::extend`), their own code behind the trait `Recogniser`, combined with `Chain`. The
readers of dates, amounts and labels are public in `aktujo_rules::text`. The built-in rules read German, English,
French, Italian, Spanish and Dutch documents. No I/O.

```rust
use aktujo_rules::{Recogniser, Rules};

let rules = Rules::builtin();
let recognition = rules.recognise(&aktujo_doc::builtin(), &extraction, None);
if let Some(kind) = &recognition.kind {
    println!("{} ({:.2})", kind.kind, kind.confidence);
}
for found in &recognition.fields {
    println!("{} = {:?} ({:.2}, {})", found.key, found.value, found.confidence, found.rule);
}
```

To try it on a file (with `aktujo-extract`'s backend `pure`; the folder of OCR models is optional):

```sh
cargo run --release -p aktujo-rules --example recognise -- <file> [<folder with det.onnx, rec.onnx, dict.txt>]
```

Part of [aktujo](https://github.com/aktujo/aktujo): a library others build in, on the aktujo core. The design is in
[RFC 0003](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0003-recognition.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-rules) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on it is AGPL-3.0-or-later.
