// SPDX-License-Identifier: MIT
// The positioned lines, the parsers and the external backend are taken over from notenwart
// (crates/notenwart-app/src/recognize.rs, AGPL-3.0-or-later) and made general; published here under MIT by their
// copyright holder (docs/rfc/0002-documents.md).
//! Text with its position from PDFs and images: lines of text on each page, with where the page's text came from –
//! the PDF's own text layer or optical character recognition (OCR).
//!
//! The trait [`Extractor`] hides how. Two backends, each a feature:
//!
//! - `pure` (default): [`Pure`], in pure Rust – also in WebAssembly and mobile apps. hayro reads the text layer and
//!   renders pages; PaddleOCR's PP-OCR models on tract read pages without text and images, once the program hands
//!   them over ([`Models`]).
//! - `external`: [`External`] runs the programs pdftotext and pdftoppm (poppler-utils) and Tesseract; the parsers of
//!   their output ([`parse`]) are there without it.
//!
//! What is read is a suggestion for a person to check, never more.

use std::fmt;

pub mod parse;

#[cfg(feature = "external")]
mod external;

#[cfg(feature = "external")]
pub use external::External;

#[cfg(feature = "pure")]
mod pure;

#[cfg(feature = "pure")]
pub use pure::{Models, Pure};

/// A line of text on a page; its box is in fractions of the page (0–1, y from the top).
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The words of the line, separated by single spaces.
    pub text: String,
    /// Left edge.
    pub x0: f32,
    /// Top edge.
    pub y0: f32,
    /// Right edge.
    pub x1: f32,
    /// Bottom edge.
    pub y1: f32,
}

impl Line {
    /// The height of the line, as a fraction of the page.
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    /// How many words the line has.
    pub fn words(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

/// Where the text of a page came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// The PDF's own text layer.
    TextLayer,
    /// Optical character recognition of the page as an image.
    Ocr,
    /// Nothing could be read.
    None,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Method::TextLayer => "text_layer",
            Method::Ocr => "ocr",
            Method::None => "none",
        })
    }
}

/// A page and its lines, from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// The width, in points for a PDF, in pixels for an image.
    pub width: f32,
    /// The height, in the same unit.
    pub height: f32,
    /// The lines, from the top, then from the left.
    pub lines: Vec<Line>,
    /// Where they came from.
    pub method: Method,
}

impl Page {
    /// How many words the page has.
    pub fn words(&self) -> usize {
        self.lines.iter().map(Line::words).sum()
    }

    /// The text of the page, a line per line.
    pub fn text(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

/// What was read from a document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extraction {
    /// The pages, the first one first.
    pub pages: Vec<Page>,
}

impl Extraction {
    /// The text of all pages, the pages separated by a form feed.
    pub fn text(&self) -> String {
        self.pages.iter().map(Page::text).collect::<Vec<_>>().join("\n\u{c}\n")
    }

    /// How many words were read.
    pub fn words(&self) -> usize {
        self.pages.iter().map(Page::words).sum()
    }
}

/// What a backend can do on this machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Abilities {
    /// Read the text layer of PDFs.
    pub text_layer: bool,
    /// Read scanned pages and images.
    pub ocr: bool,
}

/// How to read.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// At most this many pages, from the first.
    pub max_pages: u32,
    /// The resolution pages are rendered at for OCR, in dots per inch.
    pub dpi: u32,
    /// OCR languages in Tesseract's names, e.g. `deu+eng`; those not installed are left out. Only for `external`:
    /// with `pure` the recognition model decides.
    pub languages: String,
    /// Words OCR is less sure of (0–100) are dropped.
    pub min_confidence: f32,
    /// Fewer words than this on a page of a PDF: the page is read by OCR instead, if that finds more words.
    /// `usize::MAX` reads every page by OCR and keeps what it finds – for a text layer that is wrong, or to measure.
    pub min_words: usize,
    /// Tesseract's page segmentation mode: 3 finds blocks of text (documents), 11 sparse text in any order (sheet
    /// music). Only for `external`.
    pub layout: u8,
}

impl Default for Options {
    fn default() -> Self {
        Options { max_pages: 50, dpi: 300, languages: "deu+eng".into(), min_confidence: 55.0, min_words: 3, layout: 3 }
    }
}

/// Why reading failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExtractError {
    /// A media type the backend cannot read.
    #[error("cannot read {0}")]
    Unsupported(String),
    /// The file is not what its media type says, is damaged or encrypted.
    #[error("cannot read the file: {0}")]
    Unreadable(String),
    /// The backend cannot do what is needed here, e.g. OCR without Tesseract or without models.
    #[error("not available: {0}")]
    Unavailable(&'static str),
    /// A program of the backend failed.
    #[error("{program} failed: {message}")]
    Program {
        /// The program.
        program: &'static str,
        /// What it said.
        message: String,
    },
    /// The OCR models cannot be loaded or run.
    #[error("OCR model: {0}")]
    Model(String),
    /// The file system, for scratch files.
    #[error("file system: {0}")]
    Io(#[from] std::io::Error),
}

/// Reads text with its position from the bytes of a document.
pub trait Extractor: Send + Sync {
    /// What this backend can do here.
    fn abilities(&self) -> Abilities;

    /// Reads `bytes` of `media_type` (`application/pdf`, `image/png`, `image/jpeg`, `image/tiff`).
    fn extract(&self, bytes: &[u8], media_type: &str, options: &Options) -> Result<Extraction, ExtractError>;
}

/// The media types of images that OCR reads directly.
pub const IMAGE_TYPES: [&str; 3] = ["image/png", "image/jpeg", "image/tiff"];

/// Lines in reading order: a line whose vertical centre falls inside a row's extent joins that row; rows from the top,
/// the lines of a row from the left.
#[cfg_attr(not(feature = "pure"), allow(dead_code))]
pub(crate) fn reading_order(mut lines: Vec<Line>) -> Vec<Line> {
    let centre = |l: &Line| (l.y0 + l.y1) / 2.0;
    lines.sort_by(|a, b| centre(a).total_cmp(&centre(b)));
    let mut rows: Vec<(f32, f32, Vec<Line>)> = vec![];
    for line in lines {
        let c = centre(&line);
        match rows.last_mut() {
            Some((y0, y1, members)) if c >= *y0 && c <= *y1 => {
                *y0 = y0.min(line.y0);
                *y1 = y1.max(line.y1);
                members.push(line);
            }
            _ => rows.push((line.y0, line.y1, vec![line])),
        }
    }
    rows.into_iter()
        .flat_map(|(_, _, mut members)| {
            members.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            members
        })
        .collect()
}

/// PDFs for the tests, built in code.
#[cfg(test)]
pub(crate) mod testing {
    fn build(content: &str, font: bool) -> Vec<u8> {
        let resources = if font { "/Resources << /Font << /F1 5 0 R >> >>" } else { "" };
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R {resources} >>"),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![];
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        out.extend(
            format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes(),
        );
        out
    }

    /// A one-page PDF with a text layer: "Invoice 42" and "Total 119.00 EUR" in Helvetica.
    pub(crate) fn pdf() -> Vec<u8> {
        build("BT /F1 24 Tf 72 720 Td (Invoice 42) Tj 0 -40 Td (Total 119.00 EUR) Tj ET", true)
    }

    /// A one-page PDF without text: a grey rectangle.
    #[cfg_attr(not(feature = "pure"), allow(dead_code))]
    pub(crate) fn blank_pdf() -> Vec<u8> {
        build("0.5 g 100 100 200 200 re f", false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> Line {
        Line { text: text.into(), x0: 0.0, y0: 0.0, x1: 1.0, y1: 0.1 }
    }

    #[test]
    fn text_and_words_of_pages() {
        let page = |lines: &[&str]| Page {
            width: 1.0,
            height: 1.0,
            lines: lines.iter().map(|l| line(l)).collect(),
            method: Method::TextLayer,
        };
        let extraction = Extraction { pages: vec![page(&["Invoice 42", "Total 119.00 EUR"]), page(&["Thanks"])] };
        assert_eq!(extraction.words(), 6);
        assert_eq!(extraction.text(), "Invoice 42\nTotal 119.00 EUR\n\u{c}\nThanks");
        assert_eq!(Method::TextLayer.to_string(), "text_layer");
    }

    #[test]
    fn orders_rows_then_columns() {
        let at = |text: &str, x0: f32, y0: f32, y1: f32| Line { text: text.into(), x0, y0, x1: x0 + 0.2, y1 };
        let lines = vec![at("Date", 0.7, 0.10, 0.12), at("Subject", 0.1, 0.2, 0.22), at("Town", 0.1, 0.10, 0.125)];
        let text: Vec<String> = reading_order(lines).into_iter().map(|l| l.text).collect();
        assert_eq!(text, ["Town", "Date", "Subject"]);
    }
}
