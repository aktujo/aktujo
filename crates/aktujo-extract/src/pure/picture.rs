// SPDX-License-Identifier: MIT
//! Pages as grey images: rendering a PDF page with hayro, decoding an image, and straightening a page photographed or
//! scanned at an angle (deskew).

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::page::Page as PdfPage;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};
use image::{GrayImage, ImageFormat, Luma};

use crate::ExtractError;

/// The largest side of a rendering, in pixels (hayro's pixmaps are at most 65535 wide).
const MAX_SIDE: f32 = 10_000.0;

/// A PDF page rendered in grey at `dpi`, on white.
pub(crate) fn render(page: &PdfPage<'_>, dpi: u32) -> GrayImage {
    let (width, height) = page.render_dimensions();
    let scale = (dpi as f32 / 72.0).min(MAX_SIDE / width.max(height).max(1.0));
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
    );
    // Opaque on white, so premultiplied and straight colours are the same.
    let grey =
        pixmap.data().iter().map(|p| ((p.r as u32 * 299 + p.g as u32 * 587 + p.b as u32 * 114) / 1000) as u8).collect();
    GrayImage::from_raw(pixmap.width() as u32, pixmap.height() as u32, grey).expect("one byte per pixel")
}

/// An image of one of [`crate::IMAGE_TYPES`], in grey; a transparent one on white, as a viewer shows it – as it is,
/// its transparent parts would be black, and the text in them lost.
pub(crate) fn decode(bytes: &[u8], media_type: &str) -> Result<GrayImage, ExtractError> {
    let format = match media_type {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/tiff" => ImageFormat::Tiff,
        other => return Err(ExtractError::Unsupported(other.to_owned())),
    };
    image::load_from_memory_with_format(bytes, format)
        .map(on_white)
        .map_err(|e| ExtractError::Unreadable(e.to_string()))
}

fn on_white(image: image::DynamicImage) -> GrayImage {
    if !image.color().has_alpha() {
        return image.to_luma8();
    }
    let with_alpha = image.to_luma_alpha8();
    GrayImage::from_fn(with_alpha.width(), with_alpha.height(), |x, y| {
        let [grey, alpha] = with_alpha.get_pixel(x, y).0.map(u32::from);
        Luma([((grey * alpha + 255 * (255 - alpha)) / 255) as u8])
    })
}

/// Degrees searched in each direction, and the step.
const RANGE: f32 = 10.0;
const STEP: f32 = 0.1;

/// The skew of the text in degrees (positive: the lines fall to the right): the angle whose horizontal projection of
/// the ink is sharpest (the most variance between rows) wins.
pub(crate) fn skew(page: &GrayImage) -> f32 {
    if page.width() < 2 || page.height() < 2 {
        return 0.0;
    }
    let scale = 1000.0 / page.width() as f32;
    let small = image::imageops::resize(
        page,
        1000,
        ((page.height() as f32 * scale) as u32).max(1),
        image::imageops::FilterType::Triangle,
    );
    // Ink: clearly darker than its neighbourhood, so that shadows and the table around the page do not count.
    let points = ink(&small, 10, 20);
    if points.is_empty() {
        return 0.0;
    }
    let rows = (small.width() + small.height()) as usize * 2;
    let mut best = (f64::MIN, 0.0f32);
    let steps = (RANGE / STEP) as i32;
    let mut histogram = vec![0u32; rows];
    for i in -steps..=steps {
        let (sin, cos) = (i as f32 * STEP).to_radians().sin_cos();
        histogram.fill(0);
        for &(x, y) in &points {
            let r = (y * cos - x * sin) as isize + rows as isize / 2;
            if let Some(bin) = histogram.get_mut(r.max(0) as usize) {
                *bin += 1;
            }
        }
        let score: f64 = histogram.iter().map(|&c| c as f64 * c as f64).sum();
        if score > best.0 {
            best = (score, i as f32 * STEP);
        }
    }
    best.1
}

/// The pixels darker by more than `delta` than the mean of the square of `radius` around them.
fn ink(image: &GrayImage, radius: u32, delta: u32) -> Vec<(f32, f32)> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    // Summed-area table, one row and column larger.
    let mut sums = vec![0u64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0u64;
        for x in 0..w {
            row += image.get_pixel(x as u32, y as u32).0[0] as u64;
            sums[(y + 1) * (w + 1) + x + 1] = sums[y * (w + 1) + x + 1] + row;
        }
    }
    let r = radius as usize;
    let mut points = vec![];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(r), (y + r).min(h - 1) + 1);
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r).min(w - 1) + 1);
            let sum =
                sums[y1 * (w + 1) + x1] + sums[y0 * (w + 1) + x0] - sums[y0 * (w + 1) + x1] - sums[y1 * (w + 1) + x0];
            let mean = sum / ((y1 - y0) * (x1 - x0)) as u64;
            if (image.get_pixel(x as u32, y as u32).0[0] as u64) + (delta as u64) < mean {
                points.push((x as f32, y as f32));
            }
        }
    }
    points
}

/// The page turned back by its skew, the same size, the corners filled with white.
pub(crate) fn straighten(page: &GrayImage) -> GrayImage {
    let angle = skew(page);
    if angle.abs() < STEP {
        return page.clone();
    }
    rotate(page, -angle.to_radians())
}

/// Turned by `radians` about the centre (positive: clockwise on screen), bilinear, white outside.
pub(crate) fn rotate(page: &GrayImage, radians: f32) -> GrayImage {
    let (w, h) = (page.width(), page.height());
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let (sin, cos) = radians.sin_cos();
    let sample = |x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            255.0
        } else {
            page.get_pixel(x as u32, y as u32).0[0] as f32
        }
    };
    GrayImage::from_fn(w, h, |x, y| {
        // Where this pixel comes from: the inverse rotation.
        let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
        let sx = cos * dx + sin * dy + cx - 0.5;
        let sy = -sin * dx + cos * dy + cy - 0.5;
        let (x0, y0) = (sx.floor(), sy.floor());
        let (fx, fy) = (sx - x0, sy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let top = sample(x0, y0) * (1.0 - fx) + sample(x0 + 1, y0) * fx;
        let bottom = sample(x0, y0 + 1) * (1.0 - fx) + sample(x0 + 1, y0 + 1) * fx;
        Luma([(top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// White paper with black bars like lines of text.
    fn page() -> GrayImage {
        GrayImage::from_fn(1200, 1600, |x, y| {
            let line = (y / 40) % 2 == 1 && y > 200 && y < 1400 && x > 150 && x < 1050;
            Luma([if line { 0 } else { 255 }])
        })
    }

    #[test]
    fn finds_and_removes_the_skew() {
        let straight = page();
        assert!(skew(&straight).abs() < 0.2);
        let turned = rotate(&straight, 3.0f32.to_radians());
        let angle = skew(&turned);
        assert!((angle - 3.0).abs() < 0.3, "{angle}");
        assert!(skew(&straighten(&turned)).abs() < 0.3);
    }

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
