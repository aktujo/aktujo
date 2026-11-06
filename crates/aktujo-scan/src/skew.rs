// SPDX-License-Identifier: MIT
//! Straightening a page scanned or photographed at an angle: the skew of its text, and turning it back.

use image::{GrayImage, Luma, RgbImage};

/// Degrees searched in each direction, and the step.
const RANGE: f32 = 10.0;
pub(crate) const STEP: f32 = 0.1;

/// The skew of the text in degrees (positive: the lines fall to the right): the angle whose horizontal projection of
/// the ink is sharpest (the most variance between rows) wins.
pub fn skew(page: &GrayImage) -> f32 {
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
pub fn straighten(page: &GrayImage) -> GrayImage {
    let angle = skew(page);
    if angle.abs() < STEP {
        return page.clone();
    }
    rotate(page, -angle.to_radians())
}

/// Turned by `radians` about the centre (positive: clockwise on screen), bilinear, white outside.
pub fn rotate(page: &GrayImage, radians: f32) -> GrayImage {
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

/// A colour picture turned by `radians` about the centre (positive: clockwise on screen), bilinear, white outside.
pub(crate) fn rotate_rgb(image: &RgbImage, radians: f32) -> RgbImage {
    let channels: Vec<GrayImage> = (0..3)
        .map(|c| GrayImage::from_fn(image.width(), image.height(), |x, y| Luma([image.get_pixel(x, y).0[c]])))
        .map(|channel| rotate(&channel, radians))
        .collect();
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        image::Rgb([0, 1, 2].map(|c| channels[c].get_pixel(x, y).0[0]))
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
}
