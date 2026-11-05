# aktujo-extract

Text with its position from PDFs and images: lines of text per page, in fractions of the page, with where the text
came from – the PDF's own text layer or OCR – behind the trait `Extractor`.

| Feature | What |
|---|---|
| `external` | `External`: pdftotext and pdftoppm (poppler-utils) and Tesseract, found at run time |

The parsers of their output (`parse`) are there without the feature. A backend in pure Rust is OPEN.

Part of [aktujo](https://github.com/aktujo/aktujo): a library others build in, on the aktujo core. The design is in
[RFC 0002](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0002-documents.md); the documentation is on
[docs.rs](https://docs.rs/aktujo-extract) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on it is AGPL-3.0-or-later.
