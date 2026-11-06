// SPDX-License-Identifier: AGPL-3.0-or-later
//! Improving a version (`docs/rfc/0004-archive-0.1.md`, decision 5): its pages turned, straightened, cropped, in grey
//! or black and white, chosen and ordered – written as the document's newest version; the earlier one stays. Several
//! documents into one: their pages as a PDF, the newest version of the first, the others into the bin.
//!
//! The pictures are made without the workspace ([`version_file`](Workspace::version_file) reads the bytes, the free
//! functions here work on them), so that a slow page holds nothing. Only in the full build: `aktujo-minimal` has no
//! `aktujo-scan`.

use aktujo_scan::{DPI, Written};
use image::{DynamicImage, ImageFormat};

use crate::{AppError, DocumentId, NewFile, SpaceId, Workspace};

pub use aktujo_scan::{Colour, Crop, Improve, ScanError};

impl From<ScanError> for AppError {
    fn from(error: ScanError) -> Self {
        AppError::Scan(error.to_string())
    }
}

/// The file of a version: its name, media type and bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionFile {
    /// The name it came with.
    pub name: String,
    /// Its media type.
    pub media_type: String,
    /// Its bytes.
    pub bytes: Vec<u8>,
}

impl Workspace {
    /// The file of the version `number` of a document (the first 1; none: the newest).
    pub fn version_file(
        &self,
        space: &SpaceId,
        id: &DocumentId,
        number: Option<usize>,
    ) -> Result<VersionFile, AppError> {
        let document = self.document(space, id)?;
        let version = match number {
            Some(number) => number.checked_sub(1).and_then(|i| document.versions.get(i)),
            None => document.versions.last(),
        }
        .ok_or_else(|| AppError::NotFound("no such version".into()))?;
        let original = &version.original;
        Ok(VersionFile {
            name: original.name.clone(),
            media_type: original.media_type.clone(),
            bytes: self.original(&original.hash)?,
        })
    }

    /// Writes an improved file as the newest version of a document, named after the file it came from.
    pub fn add_improved(
        &mut self,
        space: &SpaceId,
        id: &DocumentId,
        from: &str,
        written: &Written,
    ) -> Result<(), AppError> {
        let name = improved_name(from, written.extension);
        self.add_version(space, id, NewFile { name: &name, media_type: written.media_type, bytes: &written.bytes })
    }

    /// Writes the pages of several documents as the newest version of the first, and puts the others into the bin
    /// – nothing is lost: their versions stay, and the first one's earlier versions too.
    pub fn add_combined(
        &mut self,
        space: &SpaceId,
        ids: &[DocumentId],
        from: &str,
        written: &Written,
    ) -> Result<(), AppError> {
        let (first, others) = ids.split_first().ok_or_else(|| AppError::NotFound("no documents".into()))?;
        for id in ids {
            self.document(space, id)?;
        }
        self.add_improved(space, first, from, written)?;
        for id in others {
            self.trash(space, id)?;
        }
        Ok(())
    }
}

/// The name of an improved file: the stem of the one it came from with the new extension.
fn improved_name(from: &str, extension: &str) -> String {
    let stem = match from.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => from,
    };
    format!("{stem}.{extension}")
}

/// How many pages a file has.
pub fn page_count(file: &VersionFile) -> Result<usize, AppError> {
    Ok(aktujo_scan::page_count(&file.bytes, &file.media_type)?)
}

/// A PNG of a picture, at most `width` pixels wide.
fn png(image: &image::RgbImage, width: u32) -> Result<Vec<u8>, AppError> {
    let image = DynamicImage::ImageRgb8(image.clone());
    let image = if width > 0 && image.width() > width {
        image.resize(width, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut bytes = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| AppError::Scan(e.to_string()))?;
    Ok(bytes)
}

/// A page of a file as a PNG at most `width` pixels wide – to look at it, also in the browser.
pub fn page_png(file: &VersionFile, page: usize, width: u32) -> Result<Vec<u8>, AppError> {
    let page = aktujo_scan::page(&file.bytes, &file.media_type, page, DPI)?;
    png(&page.image, width)
}

/// The page `page` (from 0) of what `improve` would make of a file, as a PNG at most `width` pixels wide.
pub fn preview_png(file: &VersionFile, improve: &Improve, page: usize, width: u32) -> Result<Vec<u8>, AppError> {
    let index = match &improve.pages {
        Some(pages) => *pages.get(page).ok_or(ScanError::NoPage(page))?,
        None => page,
    };
    let source = aktujo_scan::page(&file.bytes, &file.media_type, index, DPI)?;
    let turns = improve.turns.get(index).copied().unwrap_or(0);
    png(&aktujo_scan::improve_page(&source, turns, improve).image, width)
}

/// What `improve` makes of a file: a PDF stays a PDF, one image an image.
pub fn improved(file: &VersionFile, improve: &Improve) -> Result<Written, AppError> {
    let pages = aktujo_scan::pages(&file.bytes, &file.media_type, DPI)?;
    let is_pdf = file.media_type == "application/pdf";
    Ok(aktujo_scan::write(&aktujo_scan::improve(&pages, improve)?, is_pdf)?)
}

/// The pages of several files, in their order, as one PDF – improved as `improve` says, its pages counted across
/// all of them.
pub fn combined(files: &[VersionFile], improve: &Improve) -> Result<Written, AppError> {
    let mut pages = Vec::new();
    for file in files {
        pages.extend(aktujo_scan::pages(&file.bytes, &file.media_type, DPI)?);
    }
    Ok(aktujo_scan::write(&aktujo_scan::improve(&pages, improve)?, true)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_fn(width, height, |x, _| image::Rgb([(x % 255) as u8, 120, 200]));
        let mut bytes = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png).unwrap();
        bytes
    }

    #[test]
    fn a_version_is_improved_into_a_new_one_and_documents_combined() {
        let mut workspace = Workspace::in_memory("laptop").unwrap();
        let space = workspace.found_space("Home").unwrap();
        let photo = png_of(60, 80);
        let add = |w: &mut Workspace, name: &str, bytes: &[u8]| {
            w.add_document(&space, NewFile { name, media_type: "image/png", bytes }).unwrap().document
        };
        let first = add(&mut workspace, "page 1.png", &photo);
        let second = add(&mut workspace, "page 2.png", &png_of(80, 60));

        let file = workspace.version_file(&space, &first, None).unwrap();
        assert_eq!((file.name.as_str(), page_count(&file).unwrap()), ("page 1.png", 1));
        let turned = Improve { turns: vec![1], colour: Colour::Grey, ..Improve::default() };
        let preview = image::load_from_memory(&preview_png(&file, &turned, 0, 40).unwrap()).unwrap();
        assert_eq!((preview.width(), preview.height()), (40, 30), "turned, then made at most 40 wide");
        let written = improved(&file, &turned).unwrap();
        assert_eq!(written.media_type, "image/png", "grey is a PNG");
        workspace.add_improved(&space, &first, &file.name, &written).unwrap();
        let document = workspace.document(&space, &first).unwrap();
        assert_eq!(document.versions.len(), 2);
        assert_eq!(document.current().original.name, "page 1.png");
        assert_eq!(workspace.version_file(&space, &first, Some(1)).unwrap().bytes, photo, "the original stays");

        let files = vec![
            workspace.version_file(&space, &first, None).unwrap(),
            workspace.version_file(&space, &second, None).unwrap(),
        ];
        let written = combined(&files, &Improve::default()).unwrap();
        workspace.add_combined(&space, &[first.clone(), second.clone()], &files[0].name, &written).unwrap();
        let file = workspace.version_file(&space, &first, None).unwrap();
        assert_eq!((file.name.as_str(), file.media_type.as_str()), ("page 1.pdf", "application/pdf"));
        assert_eq!(page_count(&file).unwrap(), 2);
        assert!(workspace.document(&space, &second).unwrap().trashed);
        assert!(matches!(page_png(&file, 2, 100), Err(AppError::Scan(_))));
    }
}
