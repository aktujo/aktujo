// SPDX-License-Identifier: MIT
//! Reads a PDF or an image with `aktujo-extract`'s backend `pure` and prints the kind and the fields the rules of
//! the kinds in a JSON file suggest – a template of `aktujo/packs` (`{"kinds": […]}`) or a list of kinds.
//!   cargo run --release --example recognise -- <kinds.json> <file> [<folder with det.onnx, rec.onnx, dict.txt>]

use aktujo_extract::{Extractor, Models, Options, Pure};
use aktujo_rules::{Recogniser, Rules};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: recognise <kinds.json> <file> [<models>]";
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(args.next().ok_or(usage)?)?)?;
    let kinds: Vec<aktujo_doc::Kind> = serde_json::from_value(json.get("kinds").cloned().unwrap_or(json))?;
    let file = args.next().ok_or(usage)?;
    let pure = match args.next() {
        Some(dir) => {
            let dir = std::path::PathBuf::from(dir);
            let (detection, recognition) = (std::fs::read(dir.join("det.onnx"))?, std::fs::read(dir.join("rec.onnx"))?);
            let dictionary = std::fs::read_to_string(dir.join("dict.txt"))?;
            Pure::with_models(Models { detection: &detection, recognition: &recognition, dictionary: &dictionary })?
        }
        None => Pure::new(),
    };
    let media_type = match file.rsplit('.').next().map(str::to_lowercase).as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("tif" | "tiff") => "image/tiff",
        _ => return Err("unknown file type".into()),
    };
    let read = pure.extract(&std::fs::read(&file)?, media_type, &Options::default())?;
    let recognition = Rules::default().recognise(&kinds, &read, None);
    for guess in &recognition.kinds {
        println!("kind {:<10} score {:>5.1}  confidence {:.2}", guess.kind, guess.score, guess.confidence);
    }
    match &recognition.kind {
        Some(kind) => println!("-> {}", kind.kind),
        None => println!("-> no kind"),
    }
    for found in &recognition.fields {
        let line = &read.pages[found.page].lines[found.line].text;
        println!(
            "{:<10} {:<45} {:.2}  {:<16} ← {line}",
            found.key,
            format!("{:?}", found.value),
            found.confidence,
            found.rule
        );
    }
    Ok(())
}
