---
title: "OCR in pure Rust"
parent: Spikes
nav_order: 1
---

# Spike: document recognition in pure Rust

Step 2 of the roadmap. Question: does the backend `external` (pdftotext, pdftoppm, Tesseract – as notenwart does it
today) stay the default, or does a pure-Rust path take over? Measured on 2026-11-05.

**Decided (maintainer, 2026-11-05): pure Rust everywhere, servers included.** `aktujo-extract`'s backend `pure` is
the default: hayro for the text layer and rendering, our own PP-OCR pipeline on tract with deskewing
([0002](../rfc/0002-documents.md)). The spike compared engines with code of its own; that code is not kept – what
lasts is this report, the samples in `testdata/ocr/`, `scripts/make-ocr-samples.sh`, `scripts/get-ocr-models.sh` and
the measurement of aktujo's own pipeline (below).

## Answer

| Step | Recommendation | Instead of |
|---|---|---|
| Text layer with positions | **hayro** (`hayro-interpret` with a small `Device` of our own that collects glyphs; grouping into words and lines is ours, ~60 lines) | `pdftotext -bbox-layout` |
| Rendering pages | **hayro** (same crate family) | `pdftoppm` |
| OCR | **PaddleOCR PP-OCRv5 Latin models on tract** (today through `pdf_oxide::ocr`), after our own **deskew** and, for photos, **adaptive binarisation** | Tesseract |

- **Text layer and rendering: pure Rust becomes the default now.** On every sample hayro is as exact as poppler (CER
  0 %), 10–20× faster (no process start), builds and runs for `wasm32-unknown-unknown`, and is MIT/Apache-2.0.
- **OCR: pure Rust is more accurate than Tesseract, but slower.** PP-OCRv5 Latin has the lowest error on every kind
  of sample (0.0–1.1 % CER on clean pages, 0.0–1.1 % on straightened photos, 5.9 % on sheet music against 38 % for
  Tesseract), runs in WASM, and its models are Apache-2.0. It needs 2–5 s per A4 page natively (Tesseract 0.5–0.9 s)
  and 8–13 s in WASM.
- **`external` stays available, but no longer as the default.** Recommendation: default `pure` (hayro + PP-OCRv5 on
  tract); `external` (poppler + Tesseract) as an optional backend for servers that want speed on bulk OCR or
  languages without a PP-OCR model. The decision on OCR speed is the maintainer's: if 2–5 s per page is too slow for
  the server, OCR could stay `external` there while the clients use the pure path (OPEN).
- **ocrs is not recommended for now:** it is fast and robust on photos, but recognises ASCII only – no ä, ö, ü, ß, €
  (its alphabet is fixed in `ocrs/src/lib.rs`, `DEFAULT_ALPHABET`) – and its models are CC BY-SA 4.0.

## Candidates

| Crate | Version measured | Licence | Role | Maturity (2026-11-05) |
|---|---|---|---|---|
| hayro, hayro-interpret, hayro-syntax | 0.8.0 (0.9.0 is younger than 14 days) | Apache-2.0 OR MIT | text layer (own device), rendering | 10 releases since 2025-07; 840 stars, 90 open issues; ~3.4 M recent downloads; used by `typst-render` and `typst-svg` and 60 other crates |
| pdf_oxide | 0.3.78 | MIT OR Apache-2.0 | text layer, rendering (`rendering`), OCR (`ocr-tract`) | 82 releases in 12 months (fast-moving 0.x), 1169 stars, 218 open issues, 31 reverse dependencies; 395 crates in its tree with the features used |
| pdf-extract | 0.12.1 | MIT | text layer (plain text; positions only through its `OutputDev`) | since 2018, 291 reverse dependencies |
| ocrs (+ rten 0.26) | 0.13.1 | MIT OR Apache-2.0 | OCR | 21 releases since 2023-12, "early preview" by its README, 1912 stars, 21 reverse dependencies |
| tract (through pdf_oxide) | 0.22.5 | MIT OR Apache-2.0 | ONNX inference, pure Rust | 3113 stars, 108 reverse dependencies of `tract-onnx` |
| poppler (pdftotext, pdftoppm) | Homebrew | GPL-2.0/3.0 | today's `external` | external program – never linked |
| Tesseract | 5.5.4 (Homebrew), tessdata_fast | Apache-2.0 | today's `external` | 77 k stars, the reference |

Not measured, with the reason:

- **pdfium-render** (MIT/Apache-2.0) – binds PDFium, a C++ library to ship per platform; not pure Rust.
- **mupdf** (AGPL-3.0) – C library; AGPL would not fit an MIT crate.
- **oar-ocr, paddle-ocr-rs, rusto-rs, rust-paddle-ocr** – run PP-OCR through ONNX Runtime or MNN (C++), not pure Rust.
- **kalosm-ocr** – TrOCR on candle; recognises single lines only (no detection), last release 2025-02.
- **pure_onnx_ocr**, **franken_ocr** – too new to judge; franken_ocr has a licence rider that would need review.
- **lopdf**, **pdf** (pdf-rs) – parsers without text positions or rendering of their own.
- Running the PP-OCR models on **rten** instead of tract (rten loads ONNX) – OPEN, would drop the dependency on
  pdf_oxide.

## Test material

All generated for this spike, nothing personal, nothing downloaded: `testdata/ocr/` holds the sources and the
reference texts, `scripts/make-ocr-samples.sh` builds the PDFs with Typst (only its own fonts) and LilyPond (TeX Gyre fonts).

| Sample | What |
|---|---|
| receipt | till receipt of a made-up bakery, 80 mm wide, monospace, umlauts, €-free amounts, VAT lines |
| notice | official notice of a made-up town: letterhead in two columns, a ruled table with €, legal text |
| letter | English letter with numbers and dates |
| score | one part of sheet music (LilyPond): title, subtitle, part "Trompete 2 in B", composer, arranger, tempo, rehearsal mark, dynamics |

Each page is rendered at 300 dpi (pdftoppm, grey) and turned into "photos" by the spike, deterministically:
**skewed** (4° rotation, slight blur and noise, JPEG 85) and **phone** (−2.5° rotation, 6 % keystone, a shadow down to
45 % brightness, blur, noise, JPEG 70) on a grey background.

**Missing – real samples needed from the maintainer (OPEN):** real phone photos (curved pages, glare, low light),
faded thermal receipts, real notices with logos, stamps and signatures, scanned and old engraved sheet music with
pencil marks, PDF exports of notation programs (MuseScore, Sibelius, Finale, Dorico), multi-page and encrypted PDFs,
handwriting. Synthetic photos flatter every engine; real ones will be worse.

## Method

- **CER**: Levenshtein distance between reference and output over the reference length, after joining all white
  space into single spaces; characters of the Unicode private use area and control characters are dropped (music
  fonts without a Unicode mapping produce them). Can exceed 100 % when an engine adds much text.
- **Word recall**: share of reference words found in the output (as a multiset) – independent of reading order.
- **Reading order** is the same for all engines: positioned lines are grouped into rows by their
  vertical centre, rows top to bottom, left to right. On rotated images that are not straightened this splits rows,
  so CER there also measures layout; word recall does not.
- **Times**: Apple M1 Max, macOS 27.2, release build. Text layer and rendering: median of 5 runs. OCR: one run per
  image; Tesseract's times include starting the process and loading its model (as notenwart calls it), the Rust
  engines load their models once (5–15 ms). Other sessions ran on the machine at the same time: times between runs
  varied by up to 30 %.
- Tesseract runs with `deu+eng` and `--psm 3` (automatic page segmentation). notenwart uses `--psm 11` and drops
  words below confidence 55 – not measured here (OPEN).
- PP-OCR runs with pdf_oxide's defaults (detection input at most 960 px on the long side).

The engine comparison is not kept. aktujo's own pipeline is measured again with:

```sh
scripts/make-ocr-samples.sh && scripts/get-ocr-models.sh
cargo run --release -p aktujo-extract --example measure -- target/ocr-samples target/ocr-models
```

Real samples go in the same way: a PDF or an image with a `.txt` of its text beside it.

## Results

### 1. Text layer (CER / word recall, median time)

| Document | pdftotext (external) | pdf_oxide | hayro + own device | pdf-extract (no positions) |
|---|---|---|---|---|
| receipt | 0.0 % / 100 %, 20 ms | 0.0 % / 100 %, 1 ms | 0.0 % / 100 %, 0 ms | 0.0 % / 100 %, 1 ms |
| notice | 0.0 % / 100 %, 20 ms | 0.0 % / 100 %, 1 ms | 0.0 % / 100 %, 1 ms | 10.1 % / 90 %, 1 ms |
| letter | 0.0 % / 100 %, 20 ms | 0.0 % / 100 %, 1 ms | 0.0 % / 100 %, 0 ms | 0.4 % / 95 %, 1 ms |
| score | 0.0 % / 100 %, 21 ms | 1.7 % / 100 %, 2 ms | 0.0 % / 100 %, 0 ms | 1.7 % / 100 %, 1 ms |

- pdftotext's time is mostly starting the process.
- pdf-extract joins lines without spaces ("Stadt MusterstadtDer Oberbürgermeister") and gives no positions without
  more work.
- On the score, pdftotext emits the music glyphs as control characters (`\x01`…) – filtered by the metric; hayro skips
  glyphs without a Unicode value.
- hayro's `Glyph::as_unicode` is documented as "somewhat experimental"; our grouping guesses ascent and descent
  (0.75/−0.25 em). Boxes are close to poppler's but not checked against it in detail (OPEN).

### 2. Rendering page 1 at 300 dpi to PNG (median time; mean grey difference to pdftoppm, 0–255)

| Document | pdftoppm (external) | hayro | pdf_oxide |
|---|---|---|---|
| receipt | 175 ms | 11 ms, Δ 8.1 | 8 ms, Δ 4.3 |
| notice | 499 ms | 35 ms, Δ 2.4 | 25 ms, Δ 1.3 |
| letter | 365 ms | 26 ms, Δ 1.1 | 18 ms, Δ 0.7 |
| score | 356 ms | 25 ms, Δ 1.1 | 17 ms, Δ 0.6 |

Good enough for OCR? Tesseract on each renderer's image (CER / word recall):

| Document | pdftoppm | hayro | pdf_oxide |
|---|---|---|---|
| receipt | 0.9 % / 95 % | 0.7 % / 96 % | 0.9 % / 95 % |
| notice | 0.5 % / 100 % | 0.7 % / 100 % | 0.7 % / 100 % |
| letter | 0.2 % / 99 % | 0.2 % / 99 % | 0.2 % / 99 % |
| score | 38.1 % / 82 % | 22.0 % / 68 % | 18.6 % / 59 % |

The differences come from antialiasing (Δ is highest on the small monospace receipt); OCR does not suffer. On the score
Tesseract is unstable whatever the renderer (staves). Known gaps of hayro by its README: no encrypted PDFs, some
blend and knockout modes.

### 3. Deskewing (ours, in the spike with imageproc)

The projection-profile method found the exact angle on all 8 photos (+4.0° and −2.5°) in 72–135 ms. It does not
correct perspective (keystone); PaddleOCR offers an unwarping model (UVDoc, 31 MB) for that – not tried (OPEN).

### 4. OCR at 300 dpi (CER / word recall, time)

| Image | Tesseract deu+eng (external) | ocrs | PP-OCRv5 Latin (tract) | PP-OCRv5 English (tract) |
|---|---|---|---|---|
| receipt clean | 0.9 % / 95 %, 731 ms | 4.1 % / 80 %, 397 ms | 1.1 % / 95 %, 5452 ms | 0.9 % / 96 %, 3999 ms |
| receipt skewed | 2.0 % / 87 %, 470 ms | 5.7 % / 70 %, 205 ms | 85.5 % / 21 %, 1218 ms | 85.5 % / 21 %, 1157 ms |
| receipt skewed, deskewed | 3.9 % / 84 %, 530 ms | 4.3 % / 78 %, 307 ms | 1.1 % / 91 %, 2913 ms | 0.7 % / 96 %, 3052 ms |
| receipt skewed, deskewed + binarised | 1.4 % / 93 %, 467 ms | 8.2 % / 68 %, 292 ms | 0.5 % / 97 %, 2647 ms | 0.7 % / 96 %, 2611 ms |
| receipt phone | 58.9 % / 39 %, 296 ms | 16.4 % / 76 %, 322 ms | 69.8 % / 39 %, 1470 ms | 70.2 % / 36 %, 1488 ms |
| receipt phone, deskewed | 59.1 % / 38 %, 309 ms | 8.6 % / 66 %, 279 ms | 0.2 % / 99 %, 3263 ms | 0.7 % / 96 %, 3177 ms |
| receipt phone, deskewed + binarised | 3.2 % / 83 %, 495 ms | 8.2 % / 66 %, 258 ms | 1.1 % / 93 %, 2545 ms | 2.0 % / 88 %, 2593 ms |
| notice clean | 0.5 % / 100 %, 869 ms | 5.3 % / 71 %, 562 ms | 0.2 % / 98 %, 3414 ms | 0.6 % / 96 %, 3447 ms |
| notice skewed | 53.6 % / 60 %, 738 ms | 41.5 % / 62 %, 273 ms | 96.1 % / 6 %, 701 ms | 96.1 % / 6 %, 712 ms |
| notice skewed, deskewed | 4.2 % / 94 %, 899 ms | 4.9 % / 77 %, 393 ms | 0.3 % / 98 %, 3448 ms | 0.8 % / 94 %, 3423 ms |
| notice skewed, deskewed + binarised | 14.3 % / 83 %, 749 ms | 5.4 % / 72 %, 377 ms | 0.1 % / 99 %, 3581 ms | 0.5 % / 97 %, 3468 ms |
| notice phone | 69.9 % / 33 %, 561 ms | 15.2 % / 68 %, 291 ms | 80.6 % / 22 %, 1413 ms | 80.7 % / 21 %, 1348 ms |
| notice phone, deskewed | 89.5 % / 10 %, 460 ms | 5.4 % / 77 %, 407 ms | 0.2 % / 98 %, 3270 ms | 0.6 % / 96 %, 3200 ms |
| notice phone, deskewed + binarised | 14.3 % / 83 %, 691 ms | 6.5 % / 69 %, 400 ms | 0.3 % / 99 %, 3225 ms | 0.7 % / 95 %, 3212 ms |
| letter clean | 0.2 % / 99 %, 499 ms | 6.2 % / 72 %, 220 ms | 0.0 % / 100 %, 2053 ms | 0.2 % / 99 %, 2040 ms |
| letter skewed | 45.2 % / 98 %, 640 ms | 49.1 % / 51 %, 165 ms | 100.0 % / 0 %, 632 ms | 100.0 % / 0 %, 630 ms |
| letter skewed, deskewed | 0.2 % / 99 %, 696 ms | 3.1 % / 85 %, 240 ms | 0.0 % / 100 %, 2265 ms | 0.8 % / 95 %, 2283 ms |
| letter skewed, deskewed + binarised | 0.2 % / 99 %, 573 ms | 3.9 % / 87 %, 271 ms | 0.0 % / 100 %, 2244 ms | 0.0 % / 100 %, 2246 ms |
| letter phone | 78.4 % / 22 %, 439 ms | 6.6 % / 72 %, 173 ms | 89.7 % / 8 %, 730 ms | 89.7 % / 8 %, 730 ms |
| letter phone, deskewed | 100.0 % / 0 %, 399 ms | 5.7 % / 80 %, 255 ms | 0.6 % / 96 %, 1901 ms | 0.2 % / 99 %, 1899 ms |
| letter phone, deskewed + binarised | 0.2 % / 99 %, 534 ms | 8.8 % / 67 %, 253 ms | 0.0 % / 100 %, 1986 ms | 0.4 % / 98 %, 1989 ms |
| score clean | 38.1 % / 82 %, 439 ms | 45.8 % / 55 %, 136 ms | 5.9 % / 86 %, 1005 ms | 6.8 % / 82 %, 1005 ms |
| score skewed | 55.9 % / 50 %, 514 ms | 35.6 % / 64 %, 140 ms | 98.3 % / 5 %, 702 ms | 99.2 % / 0 %, 698 ms |
| score skewed, deskewed | 38.1 % / 77 %, 699 ms | 44.1 % / 68 %, 143 ms | 5.1 % / 86 %, 1269 ms | 5.9 % / 82 %, 1257 ms |
| score skewed, deskewed + binarised | 66.9 % / 55 %, 486 ms | 46.6 % / 73 %, 159 ms | 3.4 % / 86 %, 1126 ms | 4.2 % / 86 %, 1111 ms |
| score phone | 100.0 % / 0 %, 452 ms | 89.8 % / 68 %, 145 ms | 33.9 % / 55 %, 976 ms | 36.4 % / 50 %, 967 ms |
| score phone, deskewed | 85.6 % / 9 %, 487 ms | 42.4 % / 68 %, 144 ms | 20.3 % / 64 %, 1032 ms | 19.5 % / 68 %, 1016 ms |
| score phone, deskewed + binarised | 8.5 % / 68 %, 356 ms | 65.3 % / 55 %, 175 ms | 16.9 % / 73 %, 988 ms | 16.9 % / 73 %, 995 ms |

Summary of the three text documents (receipt, notice, letter), mean CER:

| | Tesseract | ocrs | PP-OCRv5 Latin |
|---|---|---|---|
| clean | 0.5 % | 5.2 % | 0.4 % |
| phone, deskewed | 82.9 % | 6.6 % | 0.3 % |
| phone, deskewed + binarised | 5.9 % | 7.8 % | 0.5 % |
| time per page, clean | 0.5–0.9 s | 0.2–0.6 s | 2.1–5.5 s |

What this shows:

- **PP-OCRv5 Latin is the most accurate** on every kind of sample once the page is straight, and reads umlauts, ß and
  €. pdf_oxide cuts detected lines out as axis-parallel rectangles (`ocr/preprocessor.rs`, `crop_text_region`), so
  **rotated text is lost without deskewing** (85–100 % CER). PaddleOCR itself rectifies rotated boxes; pdf_oxide does
  not. Our deskew step fixes this completely.
- **Tesseract** is good on clean and straightened pages but **fails under a shadow** (its global binarisation): with
  our adaptive binarisation it recovers on most pages (0.2–14 %). Its 14.3 % on the notice comes from a mis-ordered
  table, not from misread characters (word recall 83 %).
- **ocrs** is the fastest and the most robust on unprocessed photos (it finds rotated lines itself), but its alphabet
  has no umlauts, ß or € (4–9 % CER on everything German) and it reads the score badly.
- **Sheet music:** only PP-OCR reads the header reliably (86 % of the words, 3–6 % CER on straight images); staves
  confuse Tesseract and ocrs. notenwart's header-strip approach (OCR of the top third only) was not measured (OPEN).
- **Speed:** PP-OCR on tract takes 2–5 s per A4 page natively – 4–8× Tesseract. Whether tract uses more than one
  thread here, and whether a smaller detection input, batching or rten instead of tract helps, is OPEN.

### 5. WebAssembly (`wasm32-unknown-unknown`, wasm-bindgen 0.2.129, run in Node 26)

| Candidate | Builds | Size (.wasm, before wasm-bindgen) | Runs | Time in WASM (receipt / notice) |
|---|---|---|---|---|
| hayro text layer + rendering | yes | 5.3 MiB | yes, same text as natively | text 14 / 3 ms; rendering 47 / 139 ms |
| pdf_oxide text layer | yes | 5.7 MiB | yes | 36 / 7 ms |
| pdf_oxide rendering | yes | 10.1 MiB | yes | 51 / 69 ms |
| pdf-extract | yes | 1.4 MiB | yes | 14 / 4 ms |
| ocrs | yes | 5.1 MiB (+ 12 MB models) | yes; umlauts come out as `?` | 8.2 / 12.7 s; with SIMD128 3.2 / 4.7 s |
| PP-OCRv5 through pdf_oxide (`wasm-ocr`) | yes, with pdf_oxide's feature `wasm-ocr` (getrandom for JS) | 36.3 MiB (+ 10 MB models) | yes | 11.2 / 13.1 s; with SIMD128 7.8 / 8.8 s |

- All in one thread; WASM threads were not tried (OPEN).
- pdf_oxide's `wasm-ocr` feature pulls in its whole WASM API (signatures, barcodes) – that is most of the 36 MiB. An
  own small PP-OCR pipeline (on tract or rten) would be much smaller (OPEN).
- Not tried in a browser, on iOS or on Android (OPEN); Node runs the same `wasm32-unknown-unknown` module.

### 6. Models

| Model | Files, size | Licence | Source |
|---|---|---|---|
| PP-OCRv3 detection (mobile) | `det.onnx`, 2.4 MB | Apache-2.0 | `monkt/paddleocr-onnx` on Hugging Face (PaddleOCR converted with paddle2onnx) |
| PP-OCRv5 Latin recognition (mobile) | `rec.onnx` 7.9 MB + `dict.txt` (502 characters incl. äöüß €) | Apache-2.0 | same; 40+ Latin languages by its `config.json` |
| PP-OCRv5 English recognition | 7.8 MB | Apache-2.0 | same |
| ocrs detection + recognition | 2.5 MB + 9.7 MB (ONNX) | **CC BY-SA 4.0** (trained on HierText; `ocrs-models/LICENSE.md`) | `ocrs-models.s3-accelerate.amazonaws.com`, as `ocrs-cli` 0.13.1 downloads them. The `.rten` checkpoints on Hugging Face (`robertknight/ocrs`) give nonsense with ocrs 0.13 |
| Tesseract `deu`, `eng` (tessdata_fast) | 1.5 MB + 4.1 MB | Apache-2.0 | `tesseract-ocr/tessdata_fast` |

- Which exact PaddleOCR release `monkt/paddleocr-onnx` was converted from is not stated there (OPEN). pdf_oxide's own
  default detection model comes from `deepghs/paddleocr`, whose licence field is a "model-distribution-disclaimer" –
  not used here.
- Models are data, downloaded at run time or shipped beside the program – never inside an MIT crate. Their
  licences (Apache-2.0 here) must be named where they are distributed.

### 7. Licences of the dependency trees

`cargo tree` per candidate, licences as declared: no GPL, LGPL only as one option of three (`MIT OR Apache-2.0 OR
LGPL-2.1-or-later`), otherwise MIT, Apache-2.0, BSD-2/3-Clause, Zlib, Unicode-3.0, CC0, 0BSD, BlueOak – all fit MIT and
AGPL. Without an MIT option: tiny-skia (BSD-3-Clause, pdf_oxide rendering), flatbuffers (Apache-2.0, rten), prost
(Apache-2.0, tract), and a few BSD/Zlib crates. poppler stays an external program (GPL) and is never linked. `cargo
deny` was not run on the spike (OPEN).

Dependency age: every crate in `Cargo.lock` is at least 14 days old (`scripts/check-age.py` on the spike), but
for `ctutils` 0.4.4, the security fix the script allows. hayro 0.9.0 is newer and was not measured. pdf_oxide's
`wasm-ocr` feature pulls in its signature code with pre-release crypto crates (`rsa` 0.10.0-rc.19, `pkcs1`
0.8.0-rc.5) – one more reason not to depend on pdf_oxide for OCR in the end.

### 8. aktujo's own pipeline (`aktujo-extract`, backend `pure`), measured after the spike

The same samples through `examples/measure.rs`: each PDF as it comes (text layer) and once more by OCR of its pages
at 300 dpi (`min_words: usize::MAX`), on the same machine.

| Document | Text layer: CER / word recall, time | OCR: CER / word recall, time |
|---|---|---|
| receipt | 0.0 % / 100 %, 0 ms | 0.7 % / 97 %, 3.3 s |
| notice | 0.0 % / 100 %, 0 ms | 0.0 % / 100 %, 3.8 s |
| letter | 0.0 % / 100 %, 2 ms | 0.0 % / 100 %, 2.3 s |
| score | 0.0 % / 100 %, 0 ms | 5.9 % / 86 %, 1.2 s |

As exact as PP-OCR through pdf_oxide in the spike, at the same speed; it does not depend on pdf_oxide.

## What follows for aktujo

1. Done: `aktujo-extract`'s backend `pure` with hayro for the text layer and rendering – the device and the grouping
   of the spike moved over.
2. Done: OCR in `pure` with our own small PP-OCR pipeline (detection post-processing, crops, CTC decoding) on tract,
   after our own deskewing – without pdf_oxide. Adaptive binarisation as part of "improve", rotated crops (lines
   read without deskewing) and rten instead of tract: OPEN.
3. `external` (poppler + Tesseract) stays as an optional backend.
4. **OCR runs mostly on the client, in the background, as a batch** (maintainer, 2026-11-05): several scans or
   uploads go into a queue, and the interface shows progress – how many documents are done out of how many. Waiting
   2–5 s per page (8–9 s in the browser) is fine there. What follows from the measurements:
   - The recognition ran on one core (CPU time about equal to wall time on a 10-core machine). In a batch, several
     documents can be read at the same time, one per core – natively with a thread pool, in the browser with one Web
     Worker per document, each with its own WASM instance. That needs no shared memory and no WASM threads. How many
     at once depends on memory: each worker loads the models (about 10 MB) and a page image (about 9 MB at 300 dpi)
     (OPEN: measure).
   - Digital PDFs need no OCR (text layer in about 1 ms), so they go through the batch almost at once. Only scans and
     photos take seconds.
   - Progress is counted per document. Within a document it could be counted per page – detection and the lines
     still to read are known after the first step – but whether that is needed is OPEN.
   - Everything read stays a suggestion that a person confirms; the batch only fills in the suggestions.
5. Measure again with real samples from the maintainer (list above), and on a phone (OPEN).
