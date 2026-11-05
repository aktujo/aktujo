# aktujo-extract

Text with its position from PDFs and images: lines of text per page, in fractions of the page, with where the text
came from – the PDF's own text layer or OCR – behind the trait `Extractor`.

| Feature | What |
|---|---|
| `pure` (default) | `Pure`, pure Rust, also in WebAssembly: hayro for the text layer and rendering, PaddleOCR's PP-OCR models on tract for OCR, after straightening the page |
| `external` | `External`: pdftotext and pdftoppm (poppler-utils) and Tesseract, found at run time |

The OCR models are data the program hands over (`Models`), never part of this crate; tested with PP-OCRv3 detection
and PP-OCRv5 Latin recognition (Apache-2.0). Without them `Pure` reads text layers only. The parsers of the external
programs' output (`parse`) are there without either feature.

```sh
cargo run --release --example read -- letter.pdf [<folder with det.onnx, rec.onnx, dict.txt>]
```

Part of [aktujo](https://github.com/aktujo/aktujo): a library others build in, on the aktujo core. The design is in
[RFC 0002](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0002-documents.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-extract) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on it is AGPL-3.0-or-later.
