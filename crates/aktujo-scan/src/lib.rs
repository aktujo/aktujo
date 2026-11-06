// SPDX-License-Identifier: MIT
//! Improving scanned pages and photos of paper: turning a page by quarters, cropping, straightening a page scanned at
//! an angle (deskew), greyscale and black and white – and writing the pages again, one as an image, several as a PDF.
//! Pure Rust, no I/O: bytes in, bytes out, so that it runs on servers, desktops, in WebAssembly and in mobile apps
//! (`docs/rfc/0004-archive-0.1.md`, decision 5).
//!
//! A PDF's pages are rendered with hayro; PNG, JPEG and TIFF are decoded with `image`. What is written is a picture of
//! each page: a PDF's text layer is not kept – the program reads the new version again.

use image::{DynamicImage, GrayImage, ImageFormat, Luma, RgbImage};
use serde::{Deserialize, Serialize};

mod pdf;
mod skew;

pub use skew::{rotate, skew, straighten};

/// The media types of images this crate reads.
pub const IMAGE_TYPES: [&str; 3] = ["image/png", "image/jpeg", "image/tiff"];
/// The resolution a PDF's pages are rendered at, and that an image is taken to have when it is written into a PDF.
pub const DPI: f32 = 200.0;
/// The largest side of a page, in pixels.
const MAX_SIDE: f32 = 10_000.0;

/// Why a file could not be improved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScanError {
    /// A media type this crate does not read.
    #[error("{0} cannot be improved; PDF, PNG, JPEG and TIFF can")]
    Unsupported(String),
    /// A file that does not read as its type.
    #[error("unreadable: {0}")]
    Unreadable(String),
    /// A page the file does not have.
    #[error("there is no page {0}")]
    NoPage(usize),
    /// Nothing left: every page was left out.
    #[error("no pages left")]
    NoPages,
}

/// A page as a picture, with the resolution it has.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// The picture, in colour (grey and black and white are colours too).
    pub image: RgbImage,
    /// Pixels per inch, for the size of the page in a PDF.
    pub dpi: f32,
}

fn is_pdf(media_type: &str) -> bool {
    media_type == "application/pdf"
}

/// How many pages a file has: a PDF its pages, an image one.
pub fn page_count(bytes: &[u8], media_type: &str) -> Result<usize, ScanError> {
    if is_pdf(media_type) { Ok(pdf_document(bytes)?.pages().len()) } else { decode(bytes, media_type).map(|_| 1) }
}

/// The page `index` (from 0) of a file: a PDF's page rendered at `dpi`, or the image.
pub fn page(bytes: &[u8], media_type: &str, index: usize, dpi: f32) -> Result<Page, ScanError> {
    if is_pdf(media_type) {
        let pdf = pdf_document(bytes)?;
        let page = pdf.pages().get(index).ok_or(ScanError::NoPage(index))?;
        Ok(render(page, dpi))
    } else if index == 0 {
        Ok(Page { image: decode(bytes, media_type)?, dpi: DPI })
    } else {
        Err(ScanError::NoPage(index))
    }
}

/// Every page of a file: a PDF's pages rendered at `dpi`, or the image.
pub fn pages(bytes: &[u8], media_type: &str, dpi: f32) -> Result<Vec<Page>, ScanError> {
    if is_pdf(media_type) {
        let pdf = pdf_document(bytes)?;
        Ok(pdf.pages().iter().map(|page| render(page, dpi)).collect())
    } else {
        Ok(vec![Page { image: decode(bytes, media_type)?, dpi: DPI }])
    }
}

fn pdf_document(bytes: &[u8]) -> Result<hayro::hayro_syntax::Pdf, ScanError> {
    hayro::hayro_syntax::Pdf::new(bytes.to_vec()).map_err(|e| ScanError::Unreadable(format!("{e:?}")))
}

/// A PDF page rendered in colour at `dpi`, on white.
pub fn render(page: &hayro::hayro_syntax::page::Page<'_>, dpi: f32) -> Page {
    use hayro::{
        PixmapSettings, RenderCache, RenderSettings, hayro_interpret::InterpreterSettings,
        vello_cpu::color::palette::css::WHITE,
    };
    let (width, height) = page.render_dimensions();
    let scale = (dpi / 72.0).min(MAX_SIDE / width.max(height).max(1.0));
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
    );
    // Opaque on white, so premultiplied and straight colours are the same.
    let rgb = pixmap.data().iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    let image = RgbImage::from_raw(pixmap.width() as u32, pixmap.height() as u32, rgb).expect("three bytes a pixel");
    Page { image, dpi: scale * 72.0 }
}

/// An image of one of [`IMAGE_TYPES`], in colour; a transparent one on white, as a viewer shows it – as it is, its
/// transparent parts would be black, and the text in them lost.
pub fn decode(bytes: &[u8], media_type: &str) -> Result<RgbImage, ScanError> {
    let format = match media_type {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/tiff" => ImageFormat::Tiff,
        other => return Err(ScanError::Unsupported(other.to_owned())),
    };
    image::load_from_memory_with_format(bytes, format).map(on_white).map_err(|e| ScanError::Unreadable(e.to_string()))
}

fn on_white(image: DynamicImage) -> RgbImage {
    if !image.color().has_alpha() {
        return image.to_rgb8();
    }
    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, alpha] = rgba.get_pixel(x, y).0.map(u32::from);
        let over = |c: u32| ((c * alpha + 255 * (255 - alpha)) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    })
}

/// A picture in grey: the luminance of its colours.
pub fn grey(image: &RgbImage) -> GrayImage {
    GrayImage::from_fn(image.width(), image.height(), |x, y| {
        let [r, g, b] = image.get_pixel(x, y).0.map(u32::from);
        Luma([((r * 299 + g * 587 + b * 114) / 1000) as u8])
    })
}

/// A grey picture in black and white, by the light around each pixel – so that shadows and uneven light of a photo
/// do not turn whole parts black: a pixel is black when it is clearly darker than the mean of its neighbourhood.
pub fn black_white(image: &GrayImage) -> GrayImage {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w == 0 || h == 0 {
        return image.clone();
    }
    let mut sums = vec![0u64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0u64;
        for x in 0..w {
            row += image.get_pixel(x as u32, y as u32).0[0] as u64;
            sums[(y + 1) * (w + 1) + x + 1] = sums[y * (w + 1) + x + 1] + row;
        }
    }
    // A neighbourhood of about a sixteenth of the shorter side, at least 7 pixels.
    let r = (w.min(h) / 32).max(7);
    GrayImage::from_fn(w as u32, h as u32, |x, y| {
        let (x, y) = (x as usize, y as usize);
        let (x0, x1) = (x.saturating_sub(r), (x + r).min(w - 1) + 1);
        let (y0, y1) = (y.saturating_sub(r), (y + r).min(h - 1) + 1);
        let sum = sums[y1 * (w + 1) + x1] + sums[y0 * (w + 1) + x0] - sums[y0 * (w + 1) + x1] - sums[y1 * (w + 1) + x0];
        let mean = sum as f32 / ((y1 - y0) * (x1 - x0)) as f32;
        let value = image.get_pixel(x as u32, y as u32).0[0] as f32;
        // Darker by more than an eighth of the light around it, and not on a plain light ground.
        Luma([if value < mean * 0.88 { 0 } else { 255 }])
    })
}

/// What becomes of the colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Colour {
    /// As they are.
    #[default]
    Keep,
    /// Grey.
    Grey,
    /// Black and white – the smallest file, the clearest text.
    BlackWhite,
}

/// What is cut away from each side, as a share of the page (0 to 0.45 each).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    /// From the left.
    #[serde(default)]
    pub left: f32,
    /// From the top.
    #[serde(default)]
    pub top: f32,
    /// From the right.
    #[serde(default)]
    pub right: f32,
    /// From the bottom.
    #[serde(default)]
    pub bottom: f32,
}

/// How pages are improved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Improve {
    /// The pages to keep, by their number from 0, in the order they are to be; none: all, as they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<usize>>,
    /// Quarter turns clockwise for each page, by its number from 0 (0 to 3); a page not named is not turned.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<u8>,
    /// Straighten each page by the skew of its text.
    #[serde(default)]
    pub deskew: bool,
    /// What is cut away from each page, after turning and straightening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    /// What becomes of the colours.
    #[serde(default)]
    pub colour: Colour,
}

/// Turns a picture by quarters, clockwise.
pub fn turn(image: &RgbImage, quarters: u8) -> RgbImage {
    match quarters % 4 {
        1 => image::imageops::rotate90(image),
        2 => image::imageops::rotate180(image),
        3 => image::imageops::rotate270(image),
        _ => image.clone(),
    }
}

/// Cuts the shares of `crop` away from each side; nothing if they would leave less than a tenth.
pub fn crop(image: &RgbImage, crop: Crop) -> RgbImage {
    let share = |v: f32| if v.is_finite() { v.clamp(0.0, 0.45) } else { 0.0 };
    let (w, h) = (image.width() as f32, image.height() as f32);
    let (left, right) = ((share(crop.left) * w) as u32, (share(crop.right) * w) as u32);
    let (top, bottom) = ((share(crop.top) * h) as u32, (share(crop.bottom) * h) as u32);
    let width = image.width().saturating_sub(left + right).max(1);
    let height = image.height().saturating_sub(top + bottom).max(1);
    image::imageops::crop_imm(image, left, top, width, height).to_image()
}

/// One page improved: turned by `quarters`, straightened, cropped, its colours changed – in that order.
pub fn improve_page(page: &Page, quarters: u8, improve: &Improve) -> Page {
    let mut image = turn(&page.image, quarters);
    if improve.deskew {
        let angle = skew(&grey(&image));
        if angle.abs() >= skew::STEP {
            image = skew::rotate_rgb(&image, -angle.to_radians());
        }
    }
    if let Some(c) = improve.crop {
        image = crop(&image, c);
    }
    image = match improve.colour {
        Colour::Keep => image,
        Colour::Grey => DynamicImage::ImageLuma8(grey(&image)).to_rgb8(),
        Colour::BlackWhite => DynamicImage::ImageLuma8(black_white(&grey(&image))).to_rgb8(),
    };
    Page { image, dpi: page.dpi }
}

/// The pages chosen, in their order, each improved.
pub fn improve(pages: &[Page], improve: &Improve) -> Result<Vec<Page>, ScanError> {
    let chosen: Vec<usize> = improve.pages.clone().unwrap_or_else(|| (0..pages.len()).collect());
    if chosen.is_empty() {
        return Err(ScanError::NoPages);
    }
    chosen
        .into_iter()
        .map(|i| {
            let page = pages.get(i).ok_or(ScanError::NoPage(i))?;
            Ok(improve_page(page, improve.turns.get(i).copied().unwrap_or(0), improve))
        })
        .collect()
}

/// A file written from pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// Its bytes.
    pub bytes: Vec<u8>,
    /// Its media type.
    pub media_type: &'static str,
    /// The extension of its name, without the point.
    pub extension: &'static str,
}

/// Whether a picture has no colour: every pixel grey.
fn is_grey(image: &RgbImage) -> bool {
    image.pixels().all(|p| p.0[0] == p.0[1] && p.0[1] == p.0[2])
}

/// Whether a picture is black and white only.
fn is_black_white(image: &RgbImage) -> bool {
    image.pixels().all(|p| (p.0[0] == 0 || p.0[0] == 255) && p.0[0] == p.0[1] && p.0[1] == p.0[2])
}

/// Writes pages as a file: one page that came from an image as an image again – PNG in black and white or grey,
/// JPEG in colour – and anything else as a PDF of the pages' pictures.
pub fn write(pages: &[Page], as_pdf: bool) -> Result<Written, ScanError> {
    let first = pages.first().ok_or(ScanError::NoPages)?;
    if pages.len() == 1 && !as_pdf {
        let image = &first.image;
        let mut bytes = Vec::new();
        let cursor = &mut std::io::Cursor::new(&mut bytes);
        let (media_type, extension) = if is_grey(image) {
            DynamicImage::ImageLuma8(grey(image))
                .write_to(cursor, ImageFormat::Png)
                .map_err(|e| ScanError::Unreadable(e.to_string()))?;
            ("image/png", "png")
        } else {
            image::codecs::jpeg::JpegEncoder::new_with_quality(cursor, pdf::JPEG_QUALITY)
                .encode_image(image)
                .map_err(|e| ScanError::Unreadable(e.to_string()))?;
            ("image/jpeg", "jpg")
        };
        return Ok(Written { bytes, media_type, extension });
    }
    Ok(Written { bytes: pdf::write(pages)?, media_type: "application/pdf", extension: "pdf" })
}

#[cfg(test)]
mod tests;
