// SPDX-License-Identifier: MIT
// Taken over from notenwart (crates/notenwart-app/src/recognize.rs, AGPL-3.0-or-later) and made general; published
// here under MIT by its copyright holder.
//! The backend `external`: pdftotext and pdftoppm (poppler-utils) and Tesseract, found when it is made. A PDF's
//! pages are read from its text layer; a page with too few words is rendered and read by OCR; an image is read by
//! OCR. Scratch files live in a folder of their own, removed afterwards.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{Abilities, ExtractError, Extraction, Extractor, IMAGE_TYPES, Options, Page, parse};

/// The backend that runs pdftotext, pdftoppm and Tesseract.
#[derive(Debug, Clone)]
pub struct External {
    text_layer: bool,
    render: bool,
    languages: Vec<String>,
}

fn runs(program: &str, arg: &str) -> bool {
    Command::new(program).arg(arg).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// A scratch folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, ExtractError> {
        let suffix: String = aktujo_base::random::<8>().iter().map(|b| format!("{b:02x}")).collect();
        let path = std::env::temp_dir().join(format!("aktujo-extract-{suffix}"));
        std::fs::create_dir_all(&path)?;
        Ok(Scratch(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Only our own scratch files.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(program: &'static str, command: &mut Command) -> Result<Vec<u8>, ExtractError> {
    let out =
        command.stdin(Stdio::null()).output().map_err(|e| ExtractError::Program { program, message: e.to_string() })?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(ExtractError::Program { program, message })
    }
}

impl External {
    /// Looks for the programs and Tesseract's installed languages.
    pub fn detect() -> Self {
        let languages = Command::new("tesseract")
            .arg("--list-langs")
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .skip(1)
                    .map(|l| l.trim().to_owned())
                    .filter(|l| !l.is_empty() && l != "osd")
                    .collect()
            })
            .unwrap_or_default();
        External { text_layer: runs("pdftotext", "-v"), render: runs("pdftoppm", "-v"), languages }
    }

    /// Tesseract's languages installed here (without `osd`).
    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    /// The wished languages that are installed, joined for Tesseract; none if none is.
    fn wanted(&self, options: &Options) -> Option<String> {
        let wanted: Vec<&str> =
            options.languages.split('+').filter(|l| self.languages.iter().any(|i| i == l)).collect();
        (!wanted.is_empty()).then(|| wanted.join("+"))
    }

    fn ocr(&self, image: &Path, options: &Options) -> Result<Page, ExtractError> {
        let languages = self.wanted(options).ok_or(ExtractError::Unavailable("OCR in the wished languages"))?;
        let tsv = run(
            "tesseract",
            Command::new("tesseract").arg(image).arg("-").args([
                "-l",
                &languages,
                "--psm",
                &options.layout.to_string(),
                "tsv",
            ]),
        )?;
        Ok(parse::tesseract_tsv(&String::from_utf8_lossy(&tsv), options.min_confidence))
    }

    fn pdf(&self, scratch: &Path, bytes: &[u8], options: &Options) -> Result<Extraction, ExtractError> {
        if !self.text_layer {
            return Err(ExtractError::Unavailable("pdftotext"));
        }
        let file = scratch.join("in.pdf");
        std::fs::write(&file, bytes)?;
        let xhtml = run(
            "pdftotext",
            Command::new("pdftotext").args(["-l", &options.max_pages.to_string(), "-bbox-layout"]).arg(&file).arg("-"),
        )?;
        let mut pages = parse::pdftotext_bbox(&String::from_utf8_lossy(&xhtml));
        let ocr = self.abilities().ocr && self.wanted(options).is_some();
        for (index, page) in pages.iter_mut().enumerate() {
            if page.words() >= options.min_words {
                continue;
            }
            if ocr {
                let number = (index + 1).to_string();
                let base = scratch.join(format!("page{number}"));
                run(
                    "pdftoppm",
                    Command::new("pdftoppm")
                        .args(["-f", &number, "-l", &number, "-singlefile", "-gray", "-png"])
                        .args(["-r", &options.dpi.to_string()])
                        .arg(&file)
                        .arg(&base),
                )?;
                let read = self.ocr(&base.with_extension("png"), options)?;
                if read.words() > page.words() {
                    page.lines = read.lines;
                    page.method = crate::Method::Ocr;
                }
            }
            if page.words() == 0 {
                page.method = crate::Method::None;
            }
        }
        Ok(Extraction { pages })
    }
}

impl Extractor for External {
    fn abilities(&self) -> Abilities {
        Abilities { text_layer: self.text_layer, ocr: self.render && !self.languages.is_empty() }
    }

    fn extract(&self, bytes: &[u8], media_type: &str, options: &Options) -> Result<Extraction, ExtractError> {
        let scratch = Scratch::new()?;
        if media_type == "application/pdf" {
            return self.pdf(&scratch.0, bytes, options);
        }
        if !IMAGE_TYPES.contains(&media_type) {
            return Err(ExtractError::Unsupported(media_type.to_owned()));
        }
        let image = scratch.0.join("in.image");
        std::fs::write(&image, bytes)?;
        let mut page = self.ocr(&image, options)?;
        if page.words() == 0 {
            page.method = crate::Method::None;
        }
        Ok(Extraction { pages: vec![page] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Method;

    /// A one-page PDF with a text layer: "Invoice 42" and "Total 119.00 EUR" in Helvetica.
    fn pdf() -> Vec<u8> {
        let content = "BT /F1 24 Tf 72 720 Td (Invoice 42) Tj 0 -40 Td (Total 119.00 EUR) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![];
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        out.extend(
            format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes(),
        );
        out
    }

    #[test]
    fn reads_the_text_layer_of_a_pdf() {
        let external = External::detect();
        if !external.abilities().text_layer {
            eprintln!("pdftotext is not installed: skipped");
            return;
        }
        let read = external.extract(&pdf(), "application/pdf", &Options::default()).unwrap();
        assert_eq!(read.pages.len(), 1);
        assert_eq!(read.pages[0].method, Method::TextLayer);
        assert_eq!(read.text(), "Invoice 42\nTotal 119.00 EUR");
        let first = &read.pages[0].lines[0];
        assert!(first.x0 > 0.1 && first.x0 < 0.13 && first.y0 > 0.05 && first.y0 < 0.1, "{first:?}");
    }

    #[test]
    fn reads_a_rendered_page_by_ocr() {
        let external = External::detect();
        let options = Options { languages: "eng".into(), ..Options::default() };
        if !external.abilities().ocr || external.wanted(&options).is_none() {
            eprintln!("pdftoppm or Tesseract (eng) is not installed: skipped");
            return;
        }
        let scratch = Scratch::new().unwrap();
        let file = scratch.0.join("in.pdf");
        std::fs::write(&file, pdf()).unwrap();
        let base = scratch.0.join("page");
        run(
            "pdftoppm",
            Command::new("pdftoppm").args(["-singlefile", "-gray", "-png", "-r", "300"]).arg(&file).arg(&base),
        )
        .unwrap();
        let png = std::fs::read(base.with_extension("png")).unwrap();
        let read = external.extract(&png, "image/png", &options).unwrap();
        assert_eq!(read.pages[0].method, Method::Ocr);
        assert_eq!(read.text(), "Invoice 42\nTotal 119.00 EUR");
    }

    #[test]
    fn other_media_types_are_refused() {
        assert!(matches!(
            External::detect().extract(b"x", "text/plain", &Options::default()),
            Err(ExtractError::Unsupported(t)) if t == "text/plain"
        ));
    }
}
