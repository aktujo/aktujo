// SPDX-License-Identifier: MIT
use super::*;
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
