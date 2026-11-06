// SPDX-License-Identifier: MIT
//! Pages as grey images for OCR: rendering a PDF page, decoding an image and straightening a page scanned at an
//! angle – the tools of `aktujo-scan`, in grey.

use hayro::hayro_syntax::page::Page as PdfPage;
use image::GrayImage;

use crate::ExtractError;

/// A PDF page rendered in grey at `dpi`, on white.
pub(crate) fn render(page: &PdfPage<'_>, dpi: u32) -> GrayImage {
    aktujo_scan::grey(&aktujo_scan::render(page, dpi as f32).image)
}

/// An image of one of [`crate::IMAGE_TYPES`], in grey; a transparent one on white, as a viewer shows it – as it is,
/// its transparent parts would be black, and the text in them lost.
pub(crate) fn decode(bytes: &[u8], media_type: &str) -> Result<GrayImage, ExtractError> {
    match aktujo_scan::decode(bytes, media_type) {
        Ok(image) => Ok(aktujo_scan::grey(&image)),
        Err(aktujo_scan::ScanError::Unsupported(other)) => Err(ExtractError::Unsupported(other)),
        Err(error) => Err(ExtractError::Unreadable(error.to_string())),
    }
}

#[cfg(test)]
pub(crate) use aktujo_scan::rotate;
pub(crate) use aktujo_scan::straighten;

#[cfg(test)]
mod tests {
    use image::ImageFormat;

    use super::*;

    #[test]
    fn transparent_images_are_read_on_white() {
        // Black text on a transparent ground: transparent black pixels, and opaque black ones.
        let mut rgba = image::RgbaImage::from_pixel(4, 1, image::Rgba([0, 0, 0, 0]));
        rgba.put_pixel(1, 0, image::Rgba([0, 0, 0, 255]));
        rgba.put_pixel(2, 0, image::Rgba([0, 0, 0, 128]));
        let mut png = Vec::new();
        rgba.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let grey = decode(&png, "image/png").unwrap();
        assert_eq!(grey.as_raw(), &[255, 0, 127, 255]);
    }

    #[test]
    fn refuses_other_types() {
        assert!(matches!(decode(b"x", "image/gif"), Err(ExtractError::Unsupported(_))));
        assert!(matches!(decode(b"x", "image/png"), Err(ExtractError::Unreadable(_))));
    }
}
