// SPDX-License-Identifier: MIT
//! The backend `pure`, in pure Rust, so that it also runs in WebAssembly and in mobile apps: hayro reads a PDF's text
//! layer and renders its pages; PaddleOCR's PP-OCR models on tract read pages without text and images, after the page
//! is straightened. The models are data the program hands over – without them only text layers are read. With the
//! feature `layer` alone, there is no OCR at all: no rendering, no models, no tract.

use hayro::hayro_syntax::{LoadPdfError, Pdf};

#[cfg(feature = "pure")]
use crate::Page;
use crate::{Abilities, ExtractError, Extraction, Extractor, Method, Options};

#[cfg(feature = "pure")]
mod ocr;
#[cfg(feature = "pure")]
mod picture;
mod text;

#[cfg(feature = "pure")]
/// The files of PaddleOCR's PP-OCR models, converted to ONNX: a detection model (DB), a recognition model (SVTR with
/// CTC) and the recognition model's dictionary, a character per line. Measured: PP-OCRv3 detection with PP-OCRv5
/// Latin recognition, both Apache-2.0.
#[derive(Debug, Clone, Copy)]
pub struct Models<'a> {
    /// The detection model (ONNX).
    pub detection: &'a [u8],
    /// The recognition model (ONNX).
    pub recognition: &'a [u8],
    /// Its dictionary.
    pub dictionary: &'a str,
}

/// The backend in pure Rust.
pub struct Pure {
    #[cfg(feature = "pure")]
    ocr: Option<ocr::Ocr>,
}

impl std::fmt::Debug for Pure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pure").field("ocr", &self.has_ocr()).finish()
    }
}

impl Default for Pure {
    fn default() -> Self {
        Pure::new()
    }
}

impl Pure {
    /// Without OCR: reads the text layers of PDFs.
    pub fn new() -> Self {
        Pure {
            #[cfg(feature = "pure")]
            ocr: None,
        }
    }

    fn has_ocr(&self) -> bool {
        #[cfg(feature = "pure")]
        return self.ocr.is_some();
        #[cfg(not(feature = "pure"))]
        false
    }

    /// With OCR by the given models.
    #[cfg(feature = "pure")]
    pub fn with_models(models: Models<'_>) -> Result<Self, ExtractError> {
        Ok(Pure { ocr: Some(ocr::Ocr::load(models.detection, models.recognition, models.dictionary)?) })
    }

    /// Reads a picture already at hand – a page rendered with `aktujo-scan`, a photo – by OCR, only its
    /// `options.region` if one is given; the lines are in fractions of the whole picture.
    #[cfg(feature = "pure")]
    pub fn read_picture(&self, image: &image::RgbImage, options: &Options) -> Result<Page, ExtractError> {
        let mut page = self.read_image(&aktujo_scan::grey(image), options)?;
        if page.words() == 0 {
            page.method = Method::None;
        }
        Ok(page)
    }

    /// OCR of a grey page – of its region only, if there is one – with the lines in fractions of the whole page.
    #[cfg(feature = "pure")]
    fn read_image(&self, image: &image::GrayImage, options: &Options) -> Result<Page, ExtractError> {
        let ocr = self.ocr.as_ref().ok_or(ExtractError::Unavailable("OCR without models"))?;
        let min_confidence = options.min_confidence / 100.0;
        let Some(region) = options.region else { return ocr.read(&picture::straighten(image), min_confidence) };
        let (width, height) = (image.width() as f32, image.height() as f32);
        let Some(region) = region.within_page() else {
            return Ok(Page { width, height, lines: vec![], method: Method::Ocr });
        };
        let (left, top) = ((region.x0 * width).floor() as u32, (region.y0 * height).floor() as u32);
        let (right, bottom) = ((region.x1 * width).ceil() as u32, (region.y1 * height).ceil() as u32);
        let (w, h) = (right.min(image.width()) - left, bottom.min(image.height()) - top);
        let part = image::imageops::crop_imm(image, left, top, w.max(1), h.max(1)).to_image();
        let mut page = ocr.read(&picture::straighten(&part), min_confidence)?;
        let (sx, sy) = (w.max(1) as f32 / width, h.max(1) as f32 / height);
        let (ox, oy) = (left as f32 / width, top as f32 / height);
        for line in &mut page.lines {
            (line.x0, line.x1) = (ox + line.x0 * sx, ox + line.x1 * sx);
            (line.y0, line.y1) = (oy + line.y0 * sy, oy + line.y1 * sy);
        }
        (page.width, page.height) = (width, height);
        Ok(page)
    }

    fn pdf(&self, bytes: &[u8], options: &Options) -> Result<Extraction, ExtractError> {
        let pdf = Pdf::new(bytes.to_vec()).map_err(|e| {
            ExtractError::Unreadable(match e {
                LoadPdfError::Decryption(_) => "an encrypted PDF".into(),
                LoadPdfError::Invalid => "not a PDF that can be read".into(),
            })
        })?;
        let mut pages = vec![];
        for source in pdf.pages().iter().take(options.max_pages as usize) {
            #[cfg_attr(not(feature = "pure"), allow(unused_mut))]
            let mut page = text::page(source).within(options.region.as_ref());
            #[cfg(feature = "pure")]
            if page.words() < options.min_words && self.ocr.is_some() {
                let read = self.read_image(&picture::render(source, options.dpi), options)?;
                if read.words() > page.words() || options.min_words == usize::MAX {
                    // The size stays in points; the lines are fractions of the page either way.
                    page.lines = read.lines;
                    page.method = Method::Ocr;
                }
            }
            if page.words() == 0 {
                page.method = Method::None;
            }
            pages.push(page);
        }
        Ok(Extraction { pages })
    }
}

impl Extractor for Pure {
    fn abilities(&self) -> Abilities {
        Abilities { text_layer: true, ocr: self.has_ocr() }
    }

    fn extract(&self, bytes: &[u8], media_type: &str, options: &Options) -> Result<Extraction, ExtractError> {
        if media_type == "application/pdf" {
            return self.pdf(bytes, options);
        }
        #[cfg(feature = "pure")]
        {
            let image = picture::decode(bytes, media_type)?;
            let mut page = self.read_image(&image, options)?;
            if page.words() == 0 {
                page.method = Method::None;
            }
            Ok(Extraction { pages: vec![page] })
        }
        #[cfg(not(feature = "pure"))]
        if crate::IMAGE_TYPES.contains(&media_type) {
            Err(ExtractError::Unavailable("OCR in this build"))
        } else {
            Err(ExtractError::Unsupported(media_type.to_owned()))
        }
    }
}

#[cfg(all(test, feature = "pure"))]
mod tests;
