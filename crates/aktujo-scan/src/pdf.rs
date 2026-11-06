// SPDX-License-Identifier: MIT
//! A PDF of pictures, one page each, sized by the pictures' resolution: colour pages as JPEG, grey pages as
//! compressed grey, black and white pages as compressed single bits – written by hand, it is a few objects only.

use image::RgbImage;

use crate::{Page, ScanError, grey, is_black_white, is_grey};

/// The quality of JPEG pictures, 1–100.
pub(crate) const JPEG_QUALITY: u8 = 85;

/// A picture as a PDF image object wants it: its filter, colour space, bits per component and data.
struct Picture {
    filter: &'static str,
    colour: &'static str,
    bits: u8,
    data: Vec<u8>,
}

fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

fn picture(image: &RgbImage) -> Result<Picture, ScanError> {
    if is_black_white(image) {
        // One bit a pixel, 1 for white, each row padded to whole bytes.
        let (w, h) = (image.width() as usize, image.height() as usize);
        let row = w.div_ceil(8);
        let mut bits = vec![0u8; row * h];
        for (x, y, pixel) in image.enumerate_pixels() {
            if pixel.0[0] == 255 {
                bits[y as usize * row + x as usize / 8] |= 0x80 >> (x % 8);
            }
        }
        return Ok(Picture { filter: "FlateDecode", colour: "DeviceGray", bits: 1, data: deflate(&bits) });
    }
    if is_grey(image) {
        return Ok(Picture {
            filter: "FlateDecode",
            colour: "DeviceGray",
            bits: 8,
            data: deflate(grey(image).as_raw()),
        });
    }
    let mut data = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, JPEG_QUALITY)
        .encode_image(image)
        .map_err(|e| ScanError::Unreadable(e.to_string()))?;
    Ok(Picture { filter: "DCTDecode", colour: "DeviceRGB", bits: 8, data })
}

/// The PDF of `pages`.
pub(crate) fn write(pages: &[Page]) -> Result<Vec<u8>, ScanError> {
    let mut out: Vec<u8> = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    // Objects: 1 the catalog, 2 the page tree, then for each page: the page, its content, its picture.
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 3 + i * 3).collect();
    let object = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, id: usize, head: &str, stream: Option<&[u8]>| {
        while offsets.len() < id {
            offsets.push(0);
        }
        offsets[id - 1] = out.len();
        out.extend_from_slice(format!("{id} 0 obj\n{head}\n").as_bytes());
        if let Some(stream) = stream {
            out.extend_from_slice(b"stream\n");
            out.extend_from_slice(stream);
            out.extend_from_slice(b"\nendstream\n");
        }
        out.extend_from_slice(b"endobj\n");
    };
    object(&mut out, &mut offsets, 1, "<< /Type /Catalog /Pages 2 0 R >>", None);
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    let tree = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len());
    object(&mut out, &mut offsets, 2, &tree, None);
    for (page, &id) in pages.iter().zip(&page_ids) {
        let (w, h) = (page.image.width(), page.image.height());
        let (pw, ph) = (w as f32 * 72.0 / page.dpi, h as f32 * 72.0 / page.dpi);
        let picture = picture(&page.image)?;
        let content = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /P Do Q");
        let head = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Contents {} 0 R /Resources << /XObject << /P {} 0 R >> >> >>",
            id + 1,
            id + 2
        );
        object(&mut out, &mut offsets, id, &head, None);
        object(&mut out, &mut offsets, id + 1, &format!("<< /Length {} >>", content.len()), Some(content.as_bytes()));
        let image = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /{} /BitsPerComponent {} /Filter /{} /Length {} >>",
            picture.colour,
            picture.bits,
            picture.filter,
            picture.data.len()
        );
        object(&mut out, &mut offsets, id + 2, &image, Some(&picture.data));
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes(),
    );
    Ok(out)
}
