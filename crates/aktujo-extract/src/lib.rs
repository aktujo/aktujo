// SPDX-License-Identifier: MIT
// The positioned lines, the parsers and the external backend are taken over from notenwart
// (crates/notenwart-app/src/recognize.rs, AGPL-3.0-or-later) and made general; published here under MIT by their
// copyright holder (docs/rfc/0002-documents.md).
//! Text with its position from PDFs and images: lines of text on each page, with where the page's text came from –
//! the PDF's own text layer or optical character recognition (OCR).
//!
//! The trait [`Extractor`] hides how. With the feature `external`, [`External`] runs the programs pdftotext and
//! pdftoppm (poppler-utils) and Tesseract; the parsers of their output ([`parse`]) are there without it. A backend in
//! pure Rust, for WebAssembly and mobile apps, is OPEN (a spike decides).
//!
//! What is read is a suggestion for a person to check, never more.

use std::fmt;

pub mod parse;

#[cfg(feature = "external")]
mod external;

#[cfg(feature = "external")]
pub use external::External;

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
    /// OCR languages in Tesseract's names, e.g. `deu+eng`; those not installed are left out.
    pub languages: String,
    /// Words OCR is less sure of (0–100) are dropped.
    pub min_confidence: f32,
    /// Fewer words than this on a page of a PDF: the page is read by OCR instead.
    pub min_words: usize,
    /// Tesseract's page segmentation mode: 3 finds blocks of text (documents), 11 sparse text in any order (sheet
    /// music).
    pub layout: u8,
}

impl Default for Options {
    fn default() -> Self {
        Options { max_pages: 50, dpi: 300, languages: "deu+eng".into(), min_confidence: 55.0, min_words: 3, layout: 3 }
    }
}

/// Why reading failed.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// A media type the backend cannot read.
    #[error("cannot read {0}")]
    Unsupported(String),
    /// The backend cannot do what is needed here, e.g. OCR without Tesseract.
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
}
