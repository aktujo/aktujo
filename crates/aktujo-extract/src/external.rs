// SPDX-License-Identifier: MIT
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
        let mut pages: Vec<Page> = parse::pdftotext_bbox(&String::from_utf8_lossy(&xhtml))
            .into_iter()
            .map(|page| page.within(options.region.as_ref()))
            .collect();
        let ocr = self.abilities().ocr && self.wanted(options).is_some();
        for (index, page) in pages.iter_mut().enumerate() {
            if page.words() >= options.min_words {
                continue;
            }
            if ocr {
                let number = (index + 1).to_string();
                let base = scratch.join(format!("page{number}"));
                let mut command = Command::new("pdftoppm");
                command
                    .args(["-f", &number, "-l", &number, "-singlefile", "-gray", "-png"])
                    .args(["-r", &options.dpi.to_string()]);
                // Only the region, in pixels of the page at this resolution.
                let scale = options.dpi as f32 / 72.0;
                let (width, height) = (page.width * scale, page.height * scale);
                let cut =
                    options.region.and_then(|r| r.within_page()).filter(|_| width > 0.0 && height > 0.0).map(|r| {
                        let (x, y) = ((r.x0 * width).floor(), (r.y0 * height).floor());
                        let (w, h) = (((r.x1 * width).ceil() - x).max(1.0), ((r.y1 * height).ceil() - y).max(1.0));
                        (x, y, w, h)
                    });
                if let Some((x, y, w, h)) = cut {
                    command.args([
                        "-x",
                        &x.to_string(),
                        "-y",
                        &y.to_string(),
                        "-W",
                        &w.to_string(),
                        "-H",
                        &h.to_string(),
                    ]);
                }
                run("pdftoppm", command.arg(&file).arg(&base))?;
                let mut read = self.ocr(&base.with_extension("png"), &Options { region: None, ..options.clone() })?;
                if let Some((x, y, w, h)) = cut {
                    for line in &mut read.lines {
                        (line.x0, line.x1) = ((x + line.x0 * w) / width, (x + line.x1 * w) / width);
                        (line.y0, line.y1) = ((y + line.y0 * h) / height, (y + line.y1 * h) / height);
                    }
                }
                if read.words() > page.words() || options.min_words == usize::MAX {
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
        let mut page = self.ocr(&image, options)?.within(options.region.as_ref());
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
    use crate::testing::pdf;

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
    fn reads_only_a_region() {
        let external = External::detect();
        if !external.abilities().text_layer {
            eprintln!("pdftotext is not installed: skipped");
            return;
        }
        let header = Options { region: Some(crate::Region::top(0.1)), languages: "eng".into(), ..Options::default() };
        assert_eq!(external.extract(&pdf(), "application/pdf", &header).unwrap().text(), "Invoice 42");
        if !external.abilities().ocr || external.wanted(&header).is_none() {
            eprintln!("pdftoppm or Tesseract (eng) is not installed: OCR of a region skipped");
            return;
        }
        // Every page by OCR: only the region is rendered, the lines are on the whole page.
        let ocr = Options { min_words: usize::MAX, ..header };
        let read = external.extract(&pdf(), "application/pdf", &ocr).unwrap();
        assert_eq!((read.pages[0].method, read.text().as_str()), (Method::Ocr, "Invoice 42"));
        let line = &read.pages[0].lines[0];
        assert!(line.x0 > 0.1 && line.x0 < 0.14 && line.y0 > 0.05 && line.y1 < 0.1, "{line:?}");
    }

    #[test]
    fn other_media_types_are_refused() {
        assert!(matches!(
            External::detect().extract(b"x", "text/plain", &Options::default()),
            Err(ExtractError::Unsupported(t)) if t == "text/plain"
        ));
    }
}
