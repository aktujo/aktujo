// SPDX-License-Identifier: MIT
//! Measures the backend `pure` on documents with a reference text: every PDF or image in a folder that has a `.txt`
//! of the same name beside it. Prints the character error rate (CER) and the share of the reference's words found
//! (word recall), for the text layer and for OCR of the same pages (the models of aktujo-extract-det and -latin, or others from a folder). The method is the one of the OCR
//! spike (docs/spikes/ocr.md); the synthetic samples come from `scripts/make-ocr-samples.sh`, real ones go in the
//! same way.
//!   cargo run --release --example measure -- <folder> [<folder with other det.onnx, rec.onnx, dict.txt>]

use std::path::Path;
use std::time::Instant;

use aktujo_extract::{Extractor, Models, Options, Pure};

/// Text as compared: all runs of white space one space; characters of the private use area and control characters
/// dropped (music fonts without a Unicode mapping produce them).
fn normalise(text: &str) -> Vec<char> {
    let text: String = text
        .chars()
        .filter(|c| !('\u{e000}'..='\u{f8ff}').contains(c) && (!c.is_control() || c.is_whitespace()))
        .collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect()
}

/// Levenshtein distance of two character sequences.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// Edit distance over the reference's length; above 1 when much extra text comes out.
fn cer(reference: &str, hypothesis: &str) -> f64 {
    let (r, h) = (normalise(reference), normalise(hypothesis));
    distance(&r, &h) as f64 / r.len().max(1) as f64
}

/// Share of the reference's words found in the hypothesis (as a multiset), whatever the reading order.
fn word_recall(reference: &str, hypothesis: &str) -> f64 {
    let (reference, hypothesis): (String, String) =
        (normalise(reference).into_iter().collect(), normalise(hypothesis).into_iter().collect());
    let mut pool: Vec<&str> = hypothesis.split_whitespace().collect();
    let words: Vec<&str> = reference.split_whitespace().collect();
    let found =
        words.iter().filter(|w| pool.iter().position(|p| p == *w).map(|i| pool.swap_remove(i)).is_some()).count();
    found as f64 / words.len().max(1) as f64
}

fn media_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_lowercase().as_str() {
        "pdf" => Some("application/pdf"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "tif" | "tiff" => Some("image/tiff"),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let folder = args.next().ok_or("usage: measure <folder> [<models>]")?;
    let pure = match args.next() {
        Some(dir) => {
            let dir = Path::new(&dir);
            let (detection, recognition) = (std::fs::read(dir.join("det.onnx"))?, std::fs::read(dir.join("rec.onnx"))?);
            let dictionary = std::fs::read_to_string(dir.join("dict.txt"))?;
            Pure::with_models(Models { detection: &detection, recognition: &recognition, dictionary: &dictionary })?
        }
        // The models of aktujo-extract-det and aktujo-extract-latin; a folder tries others.
        None => Pure::with_models(Models {
            detection: aktujo_extract_det::MODEL,
            recognition: aktujo_extract_latin::MODEL,
            dictionary: aktujo_extract_latin::DICT,
        })?,
    };
    let mut files: Vec<_> = std::fs::read_dir(&folder)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| media_type(p).is_some() && p.with_extension("txt").exists())
        .collect();
    files.sort();
    println!("| Document | Read by | CER | Word recall | Time |");
    println!("|---|---|---|---|---|");
    for file in files {
        let (bytes, reference) = (std::fs::read(&file)?, std::fs::read_to_string(file.with_extension("txt"))?);
        let media_type = media_type(&file).expect("filtered above");
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let mut runs = vec![("as it comes", Options::default())];
        // A PDF once more by OCR of its pages, as if it were a scan.
        if media_type == "application/pdf" && pure.abilities().ocr {
            runs.push(("OCR", Options { min_words: usize::MAX, ..Options::default() }));
        }
        for (how, options) in runs {
            let start = Instant::now();
            let read = pure.extract(&bytes, media_type, &options)?;
            let ms = start.elapsed().as_millis();
            let methods: Vec<String> = read.pages.iter().map(|p| p.method.to_string()).collect();
            let text = read.text();
            println!(
                "| {name} | {how} ({}) | {:.1} % | {:.0} % | {ms} ms |",
                methods.join(", "),
                cer(&reference, &text) * 100.0,
                word_recall(&reference, &text) * 100.0
            );
        }
    }
    Ok(())
}
