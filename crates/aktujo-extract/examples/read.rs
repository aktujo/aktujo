// SPDX-License-Identifier: MIT
//! Reads a PDF or an image with the backend `pure` and prints its lines with their method and boxes.
//!   cargo run --release --example read -- <file> [<folder with other det.onnx, rec.onnx, dict.txt>]

use aktujo_extract::{Extractor, Models, Options, Pure};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let file = args.next().ok_or("usage: read <file> [<models>]")?;
    let pure = match args.next() {
        Some(dir) => {
            let dir = std::path::PathBuf::from(dir);
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
    let media_type = match file.rsplit('.').next().map(str::to_lowercase).as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("tif" | "tiff") => "image/tiff",
        _ => return Err("unknown file type".into()),
    };
    let start = std::time::Instant::now();
    let read = pure.extract(&std::fs::read(&file)?, media_type, &Options::default())?;
    for (number, page) in read.pages.iter().enumerate() {
        println!("-- page {} ({}, {} lines)", number + 1, page.method, page.lines.len());
        for line in &page.lines {
            println!("{:.3} {:.3} {:.3} {:.3}  {}", line.x0, line.y0, line.x1, line.y1, line.text);
        }
    }
    eprintln!("{} ms", start.elapsed().as_millis());
    Ok(())
}
