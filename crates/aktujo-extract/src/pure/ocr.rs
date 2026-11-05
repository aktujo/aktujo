// SPDX-License-Identifier: MIT
//! OCR with PaddleOCR's PP-OCR models (ONNX) on tract, in pure Rust: a detection model (DB) finds the lines of text,
//! a recognition model (SVTR, CTC) reads each. Pre- and post-processing follow PaddleOCR's; boxes are axis-parallel,
//! so the page is straightened first.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use image::GrayImage;
use image::imageops::FilterType;
use tract_onnx::prelude::*;

use crate::{ExtractError, Line, Method, Page};

/// The long side of the detection model's input, in pixels.
const DETECTION_SIDE: u32 = 960;
/// A pixel of the probability map counts as text above this.
const PIXEL_THRESHOLD: f32 = 0.3;
/// A region is kept if its mean probability is at least this.
const BOX_THRESHOLD: f32 = 0.6;
/// How far a region is grown back to the whole text (DB predicts a shrunken core).
const UNCLIP_RATIO: f32 = 1.5;
/// The height the recognition model reads lines at.
const LINE_HEIGHT: u32 = 48;
/// At most this many regions per page.
const MAX_REGIONS: usize = 1000;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

type Plan = TypedRunnableModel<TypedModel>;

/// An ONNX model with one image input; a runnable plan is made per input shape and kept.
struct Model {
    model: InferenceModel,
    plans: Mutex<HashMap<[usize; 4], Arc<Plan>>>,
}

fn model_error(e: impl std::fmt::Display) -> ExtractError {
    ExtractError::Model(e.to_string())
}

impl Model {
    fn load(bytes: &[u8]) -> Result<Self, ExtractError> {
        let model = tract_onnx::onnx().model_for_read(&mut Cursor::new(bytes)).map_err(model_error)?;
        Ok(Model { model, plans: Mutex::new(HashMap::new()) })
    }

    /// Runs the model on `[1, 3, h, w]` and gives the first output with its shape.
    fn run(&self, shape: [usize; 4], data: Vec<f32>) -> Result<(Vec<usize>, Vec<f32>), ExtractError> {
        let plan = {
            let mut plans = self.plans.lock().unwrap_or_else(|e| e.into_inner());
            match plans.get(&shape) {
                Some(plan) => plan.clone(),
                None => {
                    // Optimising is not optional: unoptimised, detection on a page takes minutes.
                    let plan = Arc::new(
                        self.model
                            .clone()
                            .with_input_fact(0, f32::fact(shape).into())
                            .and_then(|m| m.into_optimized())
                            .and_then(|m| m.into_runnable())
                            .map_err(model_error)?,
                    );
                    plans.insert(shape, plan.clone());
                    plan
                }
            }
        };
        let input = Tensor::from_shape(&shape, &data).map_err(model_error)?;
        let output = plan.run(tvec!(input.into())).map_err(model_error)?;
        let first = output.into_iter().next().ok_or_else(|| model_error("no output"))?;
        let values = first.as_slice::<f32>().map_err(model_error)?.to_vec();
        Ok((first.shape().to_vec(), values))
    }
}

/// The PP-OCR models, loaded.
pub(crate) struct Ocr {
    detection: Model,
    recognition: Model,
    /// Index 0 is CTC's blank, the last one a space.
    alphabet: Vec<char>,
}

impl Ocr {
    pub(crate) fn load(detection: &[u8], recognition: &[u8], dictionary: &str) -> Result<Self, ExtractError> {
        let mut alphabet = vec!['\0'];
        alphabet.extend(dictionary.lines().filter(|l| !l.is_empty()).filter_map(|l| l.chars().next()));
        if alphabet.len() == 1 {
            return Err(ExtractError::Model("the dictionary is empty".into()));
        }
        if alphabet.last() != Some(&' ') {
            alphabet.push(' ');
        }
        Ok(Ocr { detection: Model::load(detection)?, recognition: Model::load(recognition)?, alphabet })
    }

    /// The lines of a (straight) page; lines read with a confidence below `min_confidence` (0–1) are dropped.
    pub(crate) fn read(&self, page: &GrayImage, min_confidence: f32) -> Result<Page, ExtractError> {
        let (w, h) = (page.width() as f32, page.height() as f32);
        let mut lines = vec![];
        for [x0, y0, x1, y1] in self.detect(page)? {
            let crop = image::imageops::crop_imm(page, x0, y0, x1 - x0, y1 - y0).to_image();
            let (text, confidence) = self.recognise(&crop)?;
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() || confidence < min_confidence {
                continue;
            }
            lines.push(Line { text, x0: x0 as f32 / w, y0: y0 as f32 / h, x1: x1 as f32 / w, y1: y1 as f32 / h });
        }
        Ok(Page { width: w, height: h, lines: crate::reading_order(lines), method: Method::Ocr })
    }

    /// The boxes of text, in pixels of the page: `[x0, y0, x1, y1]`, x1 and y1 exclusive.
    fn detect(&self, page: &GrayImage) -> Result<Vec<[u32; 4]>, ExtractError> {
        let (pw, ph) = page.dimensions();
        if pw == 0 || ph == 0 {
            return Ok(vec![]);
        }
        let ratio = (DETECTION_SIDE as f32 / pw.max(ph) as f32).min(1.0);
        let (rw, rh) = (((pw as f32 * ratio) as u32).max(1), ((ph as f32 * ratio) as u32).max(1));
        let small = image::imageops::resize(page, rw, rh, FilterType::Lanczos3);
        // Padded to multiples of 32 with black, as PaddleOCR does.
        let (iw, ih) = (rw.div_ceil(32) * 32, rh.div_ceil(32) * 32);
        let mut data = vec![0f32; 3 * (iw * ih) as usize];
        for c in 0..3 {
            let plane = &mut data[c * (iw * ih) as usize..(c + 1) * (iw * ih) as usize];
            for y in 0..ih {
                for x in 0..iw {
                    let v = if x < rw && y < rh { small.get_pixel(x, y).0[0] as f32 / 255.0 } else { 0.0 };
                    plane[(y * iw + x) as usize] = (v - MEAN[c]) / STD[c];
                }
            }
        }
        let (shape, map) = self.detection.run([1, 3, ih as usize, iw as usize], data)?;
        let (mh, mw) = match shape[..] {
            [1, 1, mh, mw] => (mh, mw),
            _ => return Err(ExtractError::Model(format!("detection gave a shape of {shape:?}"))),
        };
        Ok(regions(&map, mw, mh)
            .into_iter()
            .take(MAX_REGIONS)
            .filter_map(|[x0, y0, x1, y1]| {
                // Back to the page, clamped to it.
                let clamp = |v: f32, max: u32| (v / ratio).round().clamp(0.0, max as f32) as u32;
                let b = [clamp(x0, pw), clamp(y0, ph), clamp(x1, pw), clamp(y1, ph)];
                (b[2] > b[0] + 1 && b[3] > b[1] + 1).then_some(b)
            })
            .collect())
    }

    /// The text of one line and the geometric mean of its characters' probabilities.
    fn recognise(&self, line: &GrayImage) -> Result<(String, f32), ExtractError> {
        let width = ((line.width() as f32 * LINE_HEIGHT as f32 / line.height() as f32).round() as u32).clamp(1, 4000);
        let resized = image::imageops::resize(line, width, LINE_HEIGHT, FilterType::Lanczos3);
        // Padded to a multiple of 4 with black, as PaddleOCR does.
        let iw = width.div_ceil(4) * 4;
        let plane = (iw * LINE_HEIGHT) as usize;
        let mut data = vec![-1f32; 3 * plane];
        for y in 0..LINE_HEIGHT {
            for x in 0..width {
                let v = resized.get_pixel(x, y).0[0] as f32 / 127.5 - 1.0;
                for c in 0..3 {
                    data[c * plane + (y * iw + x) as usize] = v;
                }
            }
        }
        let (shape, scores) = self.recognition.run([1, 3, LINE_HEIGHT as usize, iw as usize], data)?;
        let (steps, classes) = match shape[..] {
            [1, t, c] | [t, c] => (t, c),
            _ => return Err(ExtractError::Model(format!("recognition gave a shape of {shape:?}"))),
        };
        Ok(ctc(&scores, steps, classes, &self.alphabet))
    }
}

/// Greedy CTC decoding: the best class at every step, repeats and blanks dropped.
fn ctc(scores: &[f32], steps: usize, classes: usize, alphabet: &[char]) -> (String, f32) {
    let mut text = String::new();
    let mut log_sum = 0f32;
    let mut count = 0;
    let mut previous = 0;
    for step in scores.chunks(classes).take(steps) {
        let (best, p) =
            step.iter().copied().enumerate().fold((0, f32::MIN), |b, (i, p)| if p > b.1 { (i, p) } else { b });
        if best != 0 && best != previous && best < alphabet.len() {
            text.push(alphabet[best]);
            log_sum += p.max(f32::MIN_POSITIVE).ln();
            count += 1;
        }
        previous = best;
    }
    let confidence = if count == 0 { 0.0 } else { (log_sum / count as f32).exp() };
    (text, confidence)
}

/// Regions of text in a probability map of `w` × `h`: connected pixels above [`PIXEL_THRESHOLD`], each as its box
/// grown by DB's unclip distance, in the map's pixels: `[x0, y0, x1, y1]`.
fn regions(map: &[f32], w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut seen = vec![false; w * h];
    let mut out = vec![];
    let mut stack = vec![];
    for start in 0..w * h {
        if seen[start] || map[start] <= PIXEL_THRESHOLD {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            let mut visit = |j: usize| {
                if !seen[j] && map[j] > PIXEL_THRESHOLD {
                    seen[j] = true;
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
        let (bw, bh) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        // Too small to be text.
        if bw.min(bh) < 3.0 {
            continue;
        }
        let mut sum = 0f32;
        for y in y0..=y1 {
            sum += map[y * w + x0..=y * w + x1].iter().sum::<f32>();
        }
        if sum / (bw * bh) < BOX_THRESHOLD {
            continue;
        }
        let d = bw * bh * UNCLIP_RATIO / (2.0 * (bw + bh));
        out.push([x0 as f32 - d, y0 as f32 - d, x1 as f32 + 1.0 + d, y1 as f32 + 1.0 + d]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_regions_and_grows_them() {
        let (w, h) = (100, 60);
        let mut map = vec![0f32; w * h];
        for y in 20..30 {
            for x in 10..50 {
                map[y * w + x] = 0.9;
            }
        }
        // A speck and a faint region are no text.
        map[5 * w + 90] = 0.9;
        for y in 40..50 {
            for x in 60..90 {
                map[y * w + x] = 0.4;
            }
        }
        let found = regions(&map, w, h);
        assert_eq!(found.len(), 1);
        // 40 × 10: d = 400 · 1.5 / 100 = 6.
        assert_eq!(found[0], [4.0, 14.0, 56.0, 36.0]);
    }

    #[test]
    fn decodes_ctc() {
        let alphabet = ['\0', 'a', 'b', ' '];
        let step = |best: usize| {
            let mut s = [0.0f32; 4];
            s[best] = 0.9;
            s
        };
        let scores: Vec<f32> = [1, 1, 0, 1, 2, 2, 3, 0].into_iter().flat_map(step).collect();
        let (text, confidence) = ctc(&scores, 8, 4, &alphabet);
        assert_eq!(text, "aab ");
        assert!((confidence - 0.9).abs() < 1e-5);
    }
}
