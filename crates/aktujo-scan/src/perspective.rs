// SPDX-License-Identifier: MIT
//! A sheet of paper in a photo: finding its four corners, and cutting it out with its perspective corrected – as if
//! it had been photographed from straight above.
//!
//! The corners are found on a small grey copy of the photo, in two ways: the paper as the large light area against a
//! darker ground (a threshold by Otsu's method), and the paper as the area that edges close in (Sobel, then what is not
//! reached from the border). For each, the largest four-cornered shape in its convex hull is taken, and each side is
//! fitted to the outline near it. The better of the two is kept – if it is four-cornered enough, large enough and not
//! the whole photo. OPEN: paper on a ground of the same brightness without a visible edge is not found; the person
//! moves the corners then.

use image::{GrayImage, Rgb, RgbImage, imageops};
use serde::{Deserialize, Serialize};

/// The four corners of a sheet in a picture, as shares of its width and height (0 to 1, y from the top): top left, top
/// right, bottom right, bottom left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quad {
    /// The corners, each `[x, y]`.
    pub points: [[f32; 2]; 4],
}

impl Quad {
    /// The corners of the whole picture.
    pub const WHOLE: Quad = Quad { points: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] };

    /// Whether the corners are a shape to cut out: finite, within the picture, convex and in order.
    pub fn is_valid(&self) -> bool {
        let p = self.points;
        if p.iter().flatten().any(|v| !v.is_finite() || !(-0.001..=1.001).contains(v)) {
            return false;
        }
        // Clockwise on screen (y down): every turn has the same sign.
        (0..4).all(|i| cross(p[i], p[(i + 1) % 4], p[(i + 2) % 4]) > 0.0) && area(&p) > 0.01
    }
}

/// The side of the small copy the corners are looked for on, in pixels.
const SIDE: f32 = 400.0;
/// A sheet takes at least this share of the photo.
const MIN_AREA: f32 = 0.15;
/// The largest four-cornered shape fills at least this share of the outline's hull.
const MIN_RECTANGULAR: f32 = 0.9;

/// The corners of the sheet of paper in a photo, or none if there is none to tell apart – the page fills the picture
/// already, as from a scanner, or no four-cornered light shape stands out.
pub fn find_corners(image: &RgbImage) -> Option<Quad> {
    let (w, h) = image.dimensions();
    if w < 16 || h < 16 {
        return None;
    }
    let scale = (SIDE / w.max(h) as f32).min(1.0);
    let (sw, sh) = (((w as f32 * scale).round() as u32).max(16), ((h as f32 * scale).round() as u32).max(16));
    let grey = imageops::grayscale(&imageops::resize(image, sw, sh, imageops::FilterType::Triangle));
    let grey = imageops::blur(&grey, 1.2);

    let candidates = [light_area(&grey), closed_by_edges(&grey)];
    let best = candidates
        .into_iter()
        .flatten()
        .filter_map(|mask| quad_of(&mask, sw, sh))
        .max_by(|a, b| a.1.total_cmp(&b.1))?
        .0;
    // The whole picture: nothing to cut out.
    let near = |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).abs() < 0.02 && (p[1] - q[1]).abs() < 0.02;
    if (0..4).all(|i| near(best.points[i], Quad::WHOLE.points[i])) {
        return None;
    }
    Some(best)
}

/// The sheet cut out along `quad` and its perspective corrected: a picture as wide as its top and bottom sides are long
/// on average, and as high as its left and right; outside the photo white.
pub fn unwarp(image: &RgbImage, quad: &Quad) -> RgbImage {
    let (w, h) = (image.width() as f32, image.height() as f32);
    let q = quad.points.map(|[x, y]| [x * w, y * h]);
    let length = |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let max = crate::MAX_SIDE;
    let out_w = ((length(q[0], q[1]) + length(q[3], q[2])) / 2.0).round().clamp(1.0, max) as u32;
    let out_h = ((length(q[0], q[3]) + length(q[1], q[2])) / 2.0).round().clamp(1.0, max) as u32;
    let map = SquareToQuad::new(q);
    let sample = |x: i64, y: i64| -> [f32; 3] {
        if x < 0 || y < 0 || x >= image.width() as i64 || y >= image.height() as i64 {
            [255.0; 3]
        } else {
            image.get_pixel(x as u32, y as u32).0.map(f32::from)
        }
    };
    RgbImage::from_fn(out_w, out_h, |x, y| {
        let (u, v) = ((x as f32 + 0.5) / out_w as f32, (y as f32 + 0.5) / out_h as f32);
        let [sx, sy] = map.apply(u, v);
        let (sx, sy) = (sx - 0.5, sy - 0.5);
        let (x0, y0) = (sx.floor(), sy.floor());
        let (fx, fy) = (sx - x0, sy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (sample(x0, y0), sample(x0 + 1, y0), sample(x0, y0 + 1), sample(x0 + 1, y0 + 1));
        Rgb([0, 1, 2].map(|i| {
            let top = a[i] * (1.0 - fx) + b[i] * fx;
            let bottom = c[i] * (1.0 - fx) + d[i] * fx;
            (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
        }))
    })
}

/// The projective map from the unit square – (0, 0), (1, 0), (1, 1), (0, 1) – onto four points in that order
/// (Heckbert, "Fundamentals of Texture Mapping and Image Warping", 1989, section 2.2.3).
struct SquareToQuad([f32; 8]);

impl SquareToQuad {
    fn new(q: [[f32; 2]; 4]) -> Self {
        let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = q;
        let (dx3, dy3) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
        if dx3.abs() < 1e-6 && dy3.abs() < 1e-6 {
            // A parallelogram: affine.
            return SquareToQuad([x1 - x0, x2 - x1, x0, y1 - y0, y2 - y1, y0, 0.0, 0.0]);
        }
        let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
        let den = dx1 * dy2 - dx2 * dy1;
        let g = (dx3 * dy2 - dx2 * dy3) / den;
        let h = (dx1 * dy3 - dx3 * dy1) / den;
        SquareToQuad([x1 - x0 + g * x1, x3 - x0 + h * x3, x0, y1 - y0 + g * y1, y3 - y0 + h * y3, y0, g, h])
    }

    fn apply(&self, u: f32, v: f32) -> [f32; 2] {
        let [a, b, c, d, e, f, g, h] = self.0;
        let z = g * u + h * v + 1.0;
        [(a * u + b * v + c) / z, (d * u + e * v + f) / z]
    }
}

/// A mask of the small copy: `true` where the paper may be.
type Mask = (Vec<bool>, u32, u32);

/// The light part of the picture, by Otsu's threshold – if light and dark differ enough to tell.
fn light_area(grey: &GrayImage) -> Option<Mask> {
    let mut histogram = [0u64; 256];
    for p in grey.pixels() {
        histogram[p.0[0] as usize] += 1;
    }
    let threshold = otsu(&histogram)?;
    Some((grey.pixels().map(|p| p.0[0] > threshold).collect(), grey.width(), grey.height()))
}

/// What the strong edges of the picture close in: everything not reached from the border without crossing one.
fn closed_by_edges(grey: &GrayImage) -> Option<Mask> {
    let (w, h) = (grey.width() as usize, grey.height() as usize);
    let at = |x: usize, y: usize| grey.get_pixel(x as u32, y as u32).0[0] as f32;
    let mut magnitude = vec![0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x - 1, y)
                - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x, y - 1)
                - at(x + 1, y - 1);
            magnitude[y * w + x] = (gx * gx + gy * gy).sqrt();
        }
    }
    let strongest = magnitude.iter().copied().fold(0.0, f32::max);
    if strongest < 40.0 {
        return None;
    }
    // Strong: above a fifth of the strongest, at least 40 (of up to about 1440).
    let limit = (strongest / 5.0).max(40.0);
    let edge: Vec<bool> = magnitude.iter().map(|m| *m >= limit).collect();
    // Thickened by a pixel, so that a line with gaps still closes.
    let mut wall = edge.clone();
    for y in 0..h {
        for x in 0..w {
            if edge[y * w + x] {
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                        wall[ny as usize * w + nx as usize] = true;
                    }
                }
            }
        }
    }
    Some((filled(&wall, w, h), w as u32, h as u32))
}

/// The threshold that best splits a histogram in two (Otsu, 1979); none if one side would be nearly empty.
fn otsu(histogram: &[u64; 256]) -> Option<u8> {
    let total: u64 = histogram.iter().sum();
    let sum: f64 = histogram.iter().enumerate().map(|(i, n)| i as f64 * *n as f64).sum();
    let (mut weight, mut sum_below, mut best, mut threshold) = (0u64, 0f64, 0f64, None);
    for (i, n) in histogram.iter().enumerate() {
        weight += n;
        sum_below += i as f64 * *n as f64;
        let above = total - weight;
        if weight == 0 || above == 0 {
            continue;
        }
        let (mean_below, mean_above) = (sum_below / weight as f64, (sum - sum_below) / above as f64);
        let between = weight as f64 * above as f64 * (mean_below - mean_above).powi(2);
        if between > best {
            best = between;
            threshold = Some(i as u8);
        }
    }
    let threshold = threshold?;
    let below: u64 = histogram[..=threshold as usize].iter().sum();
    let share = below as f64 / total as f64;
    (0.03..=0.97).contains(&share).then_some(threshold)
}

/// The four corners of the largest part of a mask, as shares, with how well they fit (higher is better).
fn quad_of(mask: &Mask, w: u32, h: u32) -> Option<(Quad, f32)> {
    let (mask, mw, mh) = (&mask.0, mask.1 as usize, mask.2 as usize);
    // The largest part with its holes filled: text and pictures on the sheet are part of it.
    let part = filled(&largest_part(mask, mw, mh), mw, mh);
    let size = part.iter().filter(|p| **p).count() as f32;
    let whole = (mw * mh) as f32;
    if size < MIN_AREA * whole {
        return None;
    }
    let inside = |x: i64, y: i64| {
        x >= 0 && y >= 0 && (x as usize) < mw && (y as usize) < mh && part[y as usize * mw + x as usize]
    };
    // The outline: pixels of the part with a neighbour outside it (or at the border), at their centre.
    let mut outline = Vec::new();
    for y in 0..mh as i64 {
        for x in 0..mw as i64 {
            if inside(x, y) && (!inside(x - 1, y) || !inside(x + 1, y) || !inside(x, y - 1) || !inside(x, y + 1)) {
                outline.push([x as f32 + 0.5, y as f32 + 0.5]);
            }
        }
    }
    let hull = simplify(&convex_hull(outline.clone()), 1.0);
    if hull.len() < 4 {
        return None;
    }
    let corners = largest_quad(&hull)?;
    let hull_area = area(&hull);
    let rectangular = area(&corners) / hull_area;
    if rectangular < MIN_RECTANGULAR || area(&corners) < MIN_AREA * whole {
        return None;
    }
    let (corners, fitted) = match fit_sides(corners, &outline) {
        Some(fitted) => (fitted, 1.0),
        None => (corners, 0.8),
    };
    let quad = Quad {
        points: in_order(corners).map(|[x, y]| [(x / w as f32).clamp(0.0, 1.0), (y / h as f32).clamp(0.0, 1.0)]),
    };
    if !quad.is_valid() {
        return None;
    }
    // A part that fills its corners well, and is large, is more likely the sheet.
    let filled = (size / area(&corners)).min(1.0);
    Some((quad, fitted * rectangular * filled * (size / whole).sqrt()))
}

/// The largest 4-connected part of a mask.
fn largest_part(mask: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut label = vec![0u32; w * h];
    let (mut best, mut best_size, mut next) = (0u32, 0usize, 0u32);
    for start in 0..w * h {
        if !mask[start] || label[start] != 0 {
            continue;
        }
        next += 1;
        let mut size = 0;
        let mut stack = vec![start];
        label[start] = next;
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if mask[j] && label[j] == 0 {
                    label[j] = next;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        if size > best_size {
            (best, best_size) = (next, size);
        }
    }
    label.iter().map(|l| *l == best && best != 0).collect()
}

/// A part with every hole filled: all that is not reached from the border without crossing it.
fn filled(part: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut outside = vec![false; w * h];
    let mut stack: Vec<usize> =
        (0..w).flat_map(|x| [x, (h - 1) * w + x]).chain((0..h).flat_map(|y| [y * w, y * w + w - 1])).collect();
    while let Some(i) = stack.pop() {
        if outside[i] || part[i] {
            continue;
        }
        outside[i] = true;
        let (x, y) = (i % w, i / w);
        if x > 0 {
            stack.push(i - 1);
        }
        if x + 1 < w {
            stack.push(i + 1);
        }
        if y > 0 {
            stack.push(i - w);
        }
        if y + 1 < h {
            stack.push(i + w);
        }
    }
    outside.iter().map(|o| !o).collect()
}

/// `(b - a) × (c - b)`: positive for a clockwise turn on screen (y down).
fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
}

/// The area of a polygon.
fn area(points: &[[f32; 2]]) -> f32 {
    let n = points.len();
    (0..n).map(|i| points[i][0] * points[(i + 1) % n][1] - points[(i + 1) % n][0] * points[i][1]).sum::<f32>().abs()
        / 2.0
}

/// The convex hull, clockwise on screen (Andrew's monotone chain).
fn convex_hull(mut points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let mut hull: Vec<[f32; 2]> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &[f32; 2]>> =
            if pass == 0 { Box::new(points.iter()) } else { Box::new(points.iter().rev()) };
        for p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], *p) <= 0.0 {
                hull.pop();
            }
            hull.push(*p);
        }
        hull.pop();
    }
    hull
}

/// A closed polygon with points closer than `tolerance` to the line of their neighbours left out, repeated until
/// none is.
fn simplify(polygon: &[[f32; 2]], tolerance: f32) -> Vec<[f32; 2]> {
    let mut points = polygon.to_vec();
    loop {
        let n = points.len();
        if n <= 4 {
            return points;
        }
        let distance = |i: usize| {
            let (a, p, b) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt().max(1e-6);
            cross(a, p, b).abs() / length
        };
        let (i, d) = (0..n).map(|i| (i, distance(i))).min_by(|a, b| a.1.total_cmp(&b.1)).expect("points");
        if d >= tolerance {
            return points;
        }
        points.remove(i);
    }
}

/// The four points of a convex polygon that span the largest area, in its order.
fn largest_quad(hull: &[[f32; 2]]) -> Option<[[f32; 2]; 4]> {
    let n = hull.len();
    let triangle = |a: usize, b: usize, c: usize| cross(hull[a], hull[b], hull[c]).abs() / 2.0;
    let mut best: Option<(f32, [usize; 4])> = None;
    for i in 0..n {
        for k in i + 2..n {
            // The farthest point from the diagonal on each side of it.
            let Some(j) = (i + 1..k).max_by(|a, b| triangle(i, *a, k).total_cmp(&triangle(i, *b, k))) else {
                continue;
            };
            let Some(l) =
                (k + 1..n + i).map(|l| l % n).max_by(|a, b| triangle(k, *a, i).total_cmp(&triangle(k, *b, i)))
            else {
                continue;
            };
            let size = triangle(i, j, k) + triangle(k, l, i);
            if best.is_none_or(|(s, _)| size > s) {
                best = Some((size, [i, j, k, l]));
            }
        }
    }
    best.map(|(_, idx)| idx.map(|i| hull[i]))
}

/// Each side fitted to the outline points near it, and the corners where the fitted sides meet; none if a side has
/// too few points or two sides do not meet near the corner they had.
fn fit_sides(corners: [[f32; 2]; 4], outline: &[[f32; 2]]) -> Option<[[f32; 2]; 4]> {
    let mut lines = Vec::new();
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = (dx * dx + dy * dy).sqrt();
        let near: Vec<[f32; 2]> = outline
            .iter()
            .copied()
            .filter(|p| {
                let along = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (length * length);
                let off = ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / length;
                // The middle of the side, away from rounded or folded corners.
                (0.1..=0.9).contains(&along) && off <= 2.5
            })
            .collect();
        if near.len() < 8 {
            return None;
        }
        lines.push(fit_line(&near));
    }
    let length = |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let mut fitted = corners;
    for i in 0..4 {
        let point = meet(lines[(i + 3) % 4], lines[i])?;
        // A corner moves a little – back to where a dark mark or a fold cut it off – but not far.
        let sides = length(corners[i], corners[(i + 1) % 4]).min(length(corners[i], corners[(i + 3) % 4]));
        if length(point, corners[i]) > (0.1 * sides).max(6.0) {
            return None;
        }
        fitted[i] = point;
    }
    Some(fitted)
}

/// The line through points by least squares across it: a point on it and its direction.
fn fit_line(points: &[[f32; 2]]) -> ([f32; 2], [f32; 2]) {
    let n = points.len() as f32;
    let (mx, my) = (points.iter().map(|p| p[0]).sum::<f32>() / n, points.iter().map(|p| p[1]).sum::<f32>() / n);
    let (mut xx, mut xy, mut yy) = (0f32, 0f32, 0f32);
    for p in points {
        let (x, y) = (p[0] - mx, p[1] - my);
        xx += x * x;
        xy += x * y;
        yy += y * y;
    }
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    ([mx, my], [angle.cos(), angle.sin()])
}

/// Where two lines meet; none if they are (nearly) parallel.
fn meet(a: ([f32; 2], [f32; 2]), b: ([f32; 2], [f32; 2])) -> Option<[f32; 2]> {
    let ((p, r), (q, s)) = (a, b);
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-3 {
        return None;
    }
    let t = ((q[0] - p[0]) * s[1] - (q[1] - p[1]) * s[0]) / den;
    Some([p[0] + t * r[0], p[1] + t * r[1]])
}

/// Four corners from the top left, clockwise on screen.
fn in_order(corners: [[f32; 2]; 4]) -> [[f32; 2]; 4] {
    let mut points = corners;
    if cross(points[0], points[1], points[2]) < 0.0 {
        points.reverse();
    }
    let first = (0..4).min_by(|a, b| (points[*a][0] + points[*a][1]).total_cmp(&(points[*b][0] + points[*b][1])));
    points.rotate_left(first.expect("four"));
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether a point lies in a convex quad given clockwise on screen.
    fn within(q: &[[f32; 2]; 4], p: [f32; 2]) -> bool {
        (0..4).all(|i| cross(q[i], q[(i + 1) % 4], p) >= 0.0)
    }

    /// A photo: a sheet with lines of text at `corners` (pixels), on a ground of `ground`, the sheet `paper`.
    fn photo(w: u32, h: u32, corners: [[f32; 2]; 4], ground: [u8; 3], paper: [u8; 3]) -> RgbImage {
        let map = SquareToQuad::new(corners);
        let inverse = |x: f32, y: f32| {
            // Search the sheet's own position of a pixel coarsely: good enough to draw lines of text.
            let mut best = (f32::MAX, 0.0, 0.0);
            for i in 0..=40 {
                for j in 0..=40 {
                    let (u, v) = (i as f32 / 40.0, j as f32 / 40.0);
                    let [px, py] = map.apply(u, v);
                    let d = (px - x).powi(2) + (py - y).powi(2);
                    if d < best.0 {
                        best = (d, u, v);
                    }
                }
            }
            (best.1, best.2)
        };
        RgbImage::from_fn(w, h, |x, y| {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            if !within(&corners, p) {
                return Rgb(ground);
            }
            // Lines of text on every fifth pixel row of the inner part, coarsely.
            if x % 7 == 0 && y % 5 == 0 {
                let (u, v) = inverse(p[0], p[1]);
                if (0.15..0.85).contains(&u) && (0.15..0.85).contains(&v) && ((v * 30.0) as u32) % 2 == 1 {
                    return Rgb([40, 40, 40]);
                }
            }
            Rgb(paper)
        })
    }

    fn close(found: &Quad, corners: [[f32; 2]; 4], w: u32, h: u32, tolerance: f32) {
        for (f, c) in found.points.iter().zip(corners) {
            let (dx, dy) = (f[0] * w as f32 - c[0], f[1] * h as f32 - c[1]);
            assert!(dx.abs() <= tolerance && dy.abs() <= tolerance, "{found:?} against {corners:?}");
        }
    }

    #[test]
    fn finds_a_sheet_on_a_dark_table() {
        let corners = [[150.0, 80.0], [620.0, 120.0], [660.0, 540.0], [110.0, 500.0]];
        let image = photo(800, 600, corners, [70, 55, 40], [240, 238, 230]);
        let found = find_corners(&image).expect("a sheet");
        close(&found, corners, 800, 600, 8.0);
    }

    #[test]
    fn finds_a_sheet_on_a_light_table_by_its_edges() {
        let corners = [[90.0, 60.0], [700.0, 40.0], [740.0, 560.0], [60.0, 530.0]];
        let image = photo(800, 600, corners, [205, 200, 190], [250, 250, 248]);
        let found = find_corners(&image).expect("a sheet");
        close(&found, corners, 800, 600, 8.0);
    }

    #[test]
    fn finds_a_sheet_in_uneven_light_with_noise() {
        let corners = [[130.0, 110.0], [650.0, 70.0], [700.0, 520.0], [160.0, 560.0]];
        let mut image = photo(800, 600, corners, [110, 95, 80], [235, 232, 225]);
        let mut seed = 7u32;
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 24) as f32 / 255.0 * 30.0 - 15.0;
            // Light from the top left, a shadow towards the bottom right.
            let light = 1.05 - 0.35 * (x as f32 / 800.0 + y as f32 / 600.0) / 2.0;
            pixel.0 = pixel.0.map(|c| (c as f32 * light + noise).clamp(0.0, 255.0) as u8);
        }
        let found = find_corners(&image).expect("a sheet");
        close(&found, corners, 800, 600, 10.0);
    }

    #[test]
    fn a_page_that_fills_the_picture_has_no_corners_to_find() {
        let whole = [[0.0, 0.0], [600.0, 0.0], [600.0, 800.0], [0.0, 800.0]];
        assert_eq!(find_corners(&photo(600, 800, whole, [0, 0, 0], [245, 245, 245])), None);
        assert_eq!(find_corners(&RgbImage::from_pixel(300, 300, Rgb([120, 120, 120]))), None);
        assert_eq!(find_corners(&RgbImage::new(4, 4)), None);
    }

    #[test]
    fn unwarping_brings_the_sheet_back_square() {
        // A sheet of 300 × 400 with a red top left and a blue bottom right, put into a photo at an angle.
        let sheet = RgbImage::from_fn(300, 400, |x, y| match (x < 60 && y < 60, x >= 240 && y >= 340) {
            (true, _) => Rgb([220, 20, 20]),
            (_, true) => Rgb([20, 20, 220]),
            _ => Rgb([245, 245, 245]),
        });
        let corners = [[120.0, 50.0], [430.0, 90.0], [460.0, 520.0], [80.0, 470.0]];
        let map = SquareToQuad::new(corners);
        let mut photo = RgbImage::from_pixel(560, 600, Rgb([30, 30, 30]));
        for y in 0..800 {
            for x in 0..600 {
                let (u, v) = (x as f32 / 600.0, y as f32 / 800.0);
                let [px, py] = map.apply(u, v);
                let pixel = *sheet.get_pixel(((u * 300.0) as u32).min(299), ((v * 400.0) as u32).min(399));
                photo.put_pixel(px as u32, py as u32, pixel);
            }
        }
        let quad = Quad { points: corners.map(|[x, y]| [x / 560.0, y / 600.0]) };
        assert!(quad.is_valid());
        let flat = unwarp(&photo, &quad);
        let (w, h) = flat.dimensions();
        assert!((300..=440).contains(&w) && (400..=480).contains(&h), "{w} × {h}");
        assert!(flat.get_pixel(w / 20, h / 20).0[0] > 150, "red top left");
        assert!(flat.get_pixel(w - w / 20, h - h / 20).0[2] > 150, "blue bottom right");
        assert!(flat.get_pixel(w / 2, h / 2).0.iter().all(|c| *c > 200), "white middle");
    }

    #[test]
    fn corners_out_of_order_or_outside_are_not_valid() {
        assert!(Quad::WHOLE.is_valid());
        let crossed = Quad { points: [[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]] };
        assert!(!crossed.is_valid());
        let outside = Quad { points: [[-0.5, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] };
        assert!(!outside.is_valid());
        let nan = Quad { points: [[f32::NAN, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] };
        assert!(!nan.is_valid());
    }
}
