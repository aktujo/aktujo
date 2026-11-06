// SPDX-License-Identifier: MIT
use super::*;
use crate::Region;
use crate::testing::{blank_pdf, pdf};

/// The models of `aktujo-extract-det` and `aktujo-extract-latin`.
fn with_models() -> Pure {
    Pure::with_models(Models {
        detection: aktujo_extract_det::MODEL,
        recognition: aktujo_extract_latin::MODEL,
        dictionary: aktujo_extract_latin::DICT,
    })
    .unwrap()
}

#[test]
fn reads_the_text_layer_of_a_pdf() {
    let read = Pure::new().extract(&pdf(), "application/pdf", &Options::default()).unwrap();
    assert_eq!(read.pages.len(), 1);
    assert_eq!((read.pages[0].width, read.pages[0].height), (612.0, 792.0));
    assert_eq!(read.pages[0].method, Method::TextLayer);
    assert_eq!(read.text(), "Invoice 42\nTotal 119.00 EUR");
    let first = &read.pages[0].lines[0];
    // 72 pt from the left, the baseline 72 pt from the top of 792.
    assert!(first.x0 > 0.11 && first.x0 < 0.13 && first.y0 > 0.06 && first.y1 < 0.1, "{first:?}");
}

#[test]
fn without_models_there_is_no_ocr() {
    let pure = Pure::new();
    assert_eq!(pure.abilities(), Abilities { text_layer: true, ocr: false });
    let read = pure.extract(&blank_pdf(), "application/pdf", &Options::default()).unwrap();
    assert_eq!(read.pages[0].method, Method::None);
    assert!(matches!(pure.extract(&[], "image/png", &Options::default()), Err(ExtractError::Unreadable(_))));
    assert!(matches!(pure.extract(b"x", "application/pdf", &Options::default()), Err(ExtractError::Unreadable(_))));
    assert!(matches!(pure.extract(b"x", "text/plain", &Options::default()), Err(ExtractError::Unsupported(_))));
}

#[test]
fn reads_a_rendered_and_a_skewed_page_by_ocr() {
    let pure = with_models();
    let pdf = Pdf::new(pdf()).unwrap();
    let rendered = picture::render(&pdf.pages()[0], 300);
    for (name, image) in [("straight", rendered.clone()), ("skewed", picture::rotate(&rendered, 4f32.to_radians()))] {
        let mut png = vec![];
        image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let read = pure.extract(&png, "image/png", &Options::default()).unwrap();
        assert_eq!(read.pages[0].method, Method::Ocr, "{name}");
        assert_eq!(read.text(), "Invoice 42\nTotal 119.00 EUR", "{name}");
    }
}

#[test]
fn only_a_region_of_the_page_is_read() {
    // The text layer: "Invoice 42" stands in the top tenth, "Total" below it.
    let header = Options { region: Some(Region::top(0.1)), ..Options::default() };
    let read = Pure::new().extract(&pdf(), "application/pdf", &header).unwrap();
    assert_eq!(read.text(), "Invoice 42");

    // OCR of the region only – of a page rendered, and of a picture at hand – with the lines on the whole page.
    let pure = with_models();
    let rendered = aktujo_scan::render(&Pdf::new(pdf()).unwrap().pages()[0], 300.0).image;
    let page = pure.read_picture(&rendered, &header).unwrap();
    assert_eq!((page.method, page.text().as_str()), (Method::Ocr, "Invoice 42"));
    let line = &page.lines[0];
    assert!(line.x0 > 0.1 && line.x0 < 0.14 && line.y0 > 0.05 && line.y1 < 0.1, "{line:?}");
    let lower = Options { region: Some(Region { x0: 0.0, y0: 0.1, x1: 1.0, y1: 0.2 }), ..Options::default() };
    assert_eq!(pure.read_picture(&rendered, &lower).unwrap().text(), "Total 119.00 EUR");
    let nothing = Options { region: Some(Region { x0: 0.5, y0: 0.5, x1: 0.5, y1: 0.9 }), ..Options::default() };
    assert_eq!(pure.read_picture(&rendered, &nothing).unwrap().method, Method::None);

    // A scanned page: the region read by OCR, as the text layer has nothing.
    let scanned = Options { region: Some(Region::top(0.1)), min_words: 1, ..Options::default() };
    let mut png = vec![];
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    assert_eq!(pure.extract(&png, "image/png", &scanned).unwrap().text(), "Invoice 42");
}
