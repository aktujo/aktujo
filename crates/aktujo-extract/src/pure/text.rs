// SPDX-License-Identifier: MIT
//! The text layer of a PDF through hayro's interpreter and a device of our own that only collects glyphs; grouping
//! them into words and lines is ours.

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, interpret_page,
};
use hayro::hayro_syntax::page::Page as PdfPage;
use hayro::kurbo::{Affine, BezPath, Point, Rect};

use crate::{Line, Method, Page};

/// A device that keeps each glyph's text and where it is drawn, in the page's own space (points, y from the top).
#[derive(Default)]
struct Glyphs {
    chars: Vec<(String, Rect)>,
}

impl<'a> Device<'a> for Glyphs {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for glyph in run.glyphs() {
            // Glyphs without a Unicode value (music fonts, for one) are skipped.
            let text = match glyph.as_unicode() {
                Some(BfString::Char(c)) => c.to_string(),
                Some(BfString::String(s)) => s,
                None => continue,
            };
            let advance = match &**glyph {
                Glyph::Outline(g) => g.advance_width().unwrap_or(500.0),
                Glyph::Type3(_) => 500.0,
            } as f64;
            // Glyph space has 1000 units to the em; ascent and descent are guessed (0.75 / −0.25 em).
            let t: Affine = props.transform * glyph.transform();
            let rect = Rect::from_points(t * Point::new(0.0, -250.0), t * Point::new(advance, 750.0));
            self.chars.push((text, rect));
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// The lines of a page's text layer, in reading order; the page's size in points.
pub(crate) fn page(page: &PdfPage<'_>) -> Page {
    let (width, height) = page.render_dimensions();
    let cache = InterpreterCache::new();
    let mut context = Context::new(
        page.initial_transform(true).to_kurbo(),
        Rect::new(0.0, 0.0, width as f64, height as f64),
        &cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut device = Glyphs::default();
    interpret_page(page, &mut context, &mut device);
    let lines = group(device.chars, width as f64, height as f64);
    Page { width, height, lines: crate::reading_order(lines), method: Method::TextLayer }
}

/// Glyphs into lines: a glyph continues the line if it sits on the same baseline and does not jump back; a gap wider
/// than a fifth of the height starts a new word.
fn group(chars: Vec<(String, Rect)>, width: f64, height: f64) -> Vec<Line> {
    let mut lines: Vec<Line> = vec![];
    let mut last: Option<Rect> = None;
    for (text, r) in chars {
        match (last, lines.last_mut()) {
            (Some(l), Some(line)) if (r.y1 - l.y1).abs() < l.height() * 0.4 && r.x0 > l.x0 - 1.0 => {
                if r.x0 - l.x1 > l.height() * 0.2 && !line.text.ends_with(' ') && text != " " {
                    line.text.push(' ');
                }
                line.text.push_str(&text);
                line.x1 = line.x1.max((r.x1 / width) as f32);
                line.y0 = line.y0.min((r.y0 / height) as f32);
                line.y1 = line.y1.max((r.y1 / height) as f32);
            }
            _ => lines.push(Line {
                text,
                x0: (r.x0 / width) as f32,
                y0: (r.y0 / height) as f32,
                x1: (r.x1 / width) as f32,
                y1: (r.y1 / height) as f32,
            }),
        }
        last = Some(r);
    }
    for line in &mut lines {
        line.text = line.text.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    lines.retain(|l| !l.text.is_empty());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_glyphs_into_words_and_lines() {
        let glyph = |c: &str, x: f64, y: f64| (c.to_string(), Rect::new(x, y, x + 6.0, y + 10.0));
        let chars =
            vec![glyph("A", 10.0, 10.0), glyph("b", 16.0, 10.0), glyph("c", 30.0, 10.0), glyph("D", 10.0, 30.0)];
        let lines = group(chars, 100.0, 100.0);
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Ab c", "D"]);
        assert_eq!((lines[0].x0, lines[0].x1, lines[0].y0), (0.1, 0.36, 0.1));
    }
}
