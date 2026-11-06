# aktujo-scan

Improving scanned pages and photos of paper: turning a page by quarters, cropping, straightening a page scanned at an
angle (deskew), greyscale and black and white – and writing the pages again, one as an image, several as a PDF. Pure
Rust, no I/O: bytes in, bytes out, so that it runs on servers, desktops, in WebAssembly and in mobile apps.

```rust
use aktujo_scan::{Colour, Improve, improve, pages, write, DPI};

let pages = pages(&bytes, "application/pdf", DPI)?;
let steps = Improve { deskew: true, colour: Colour::BlackWhite, turns: vec![0, 2], ..Improve::default() };
let written = write(&improve(&pages, &steps)?, true)?; // a PDF again
```

A PDF's pages are rendered with hayro; PNG, JPEG and TIFF are decoded with `image`. What is written is a picture of
each page – colour as JPEG, grey compressed, black and white with one bit a pixel; a PDF's text layer is not kept.
Finding the page's edges and perspective come later.

Part of [aktujo](https://github.com/aktujo/aktujo): a library others build in. The design is in
[RFC 0004](https://github.com/aktujo/aktujo/blob/main/docs/rfc/0004-archive-0.1.md) (decision 5); the documentation
is on [docs.rs](https://docs.rs/aktujo-scan) and [aktujo.dev](https://aktujo.dev).

Licence: MIT ([LICENSE](LICENSE)). The program aktujo built on it is AGPL-3.0-or-later.
