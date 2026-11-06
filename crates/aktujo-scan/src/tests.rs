// SPDX-License-Identifier: MIT
use image::Rgb;

use super::*;

/// A page of 300 × 400 pixels: light paper, dark bars like lines, a red mark in the top left.
fn paper() -> Page {
    let image = RgbImage::from_fn(300, 400, |x, y| {
        if x < 20 && y < 20 {
            Rgb([200, 30, 30])
        } else if (y / 10) % 2 == 1 && y > 60 && y < 340 && x > 40 && x < 260 {
            Rgb([30, 30, 30])
        } else {
            Rgb([235, 230, 220])
        }
    });
    Page { image, dpi: 100.0 }
}

fn png(page: &Page) -> Vec<u8> {
    let mut bytes = Vec::new();
    page.image.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png).unwrap();
    bytes
}

#[test]
fn pages_turn_crop_and_change_colour() {
    let page = paper();
    let turned = improve_page(&page, 1, &Improve::default());
    assert_eq!(turned.image.dimensions(), (400, 300));
    assert_eq!(turned.image.get_pixel(399, 0), &Rgb([200, 30, 30]), "the top left is top right after a quarter");

    let crop_half =
        Improve { crop: Some(Crop { left: 0.1, top: 0.25, right: 0.1, bottom: 0.25 }), ..Improve::default() };
    assert_eq!(improve_page(&page, 0, &crop_half).image.dimensions(), (240, 200));
    let too_much =
        Improve { crop: Some(Crop { left: 0.9, top: 0.0, right: 0.9, bottom: f32::NAN }), ..Improve::default() };
    assert_eq!(improve_page(&page, 0, &too_much).image.dimensions(), (30, 400), "at most 0.45 from a side");

    let grey_page = improve_page(&page, 0, &Improve { colour: Colour::Grey, ..Improve::default() });
    assert!(is_grey(&grey_page.image));
    let bw = improve_page(&page, 0, &Improve { colour: Colour::BlackWhite, ..Improve::default() });
    assert!(is_black_white(&bw.image));
    assert_eq!(bw.image.get_pixel(100, 75), &Rgb([0, 0, 0]), "a line is black");
    assert_eq!(bw.image.get_pixel(100, 365), &Rgb([255, 255, 255]), "the paper is white");
}

#[test]
fn a_page_scanned_at_an_angle_is_straightened() {
    let page = paper();
    let tilted = Page { image: skew::rotate_rgb(&page.image, 2.0f32.to_radians()), dpi: page.dpi };
    assert!(skew(&grey(&tilted.image)).abs() > 1.5);
    let straight = improve_page(&tilted, 0, &Improve { deskew: true, ..Improve::default() });
    assert!(skew(&grey(&straight.image)).abs() < 0.4);
}

#[test]
fn pages_are_chosen_and_written_as_an_image_or_a_pdf() {
    let pages = vec![paper(), paper(), paper()];
    let chosen = Improve { pages: Some(vec![2, 0]), turns: vec![0, 0, 2], ..Improve::default() };
    let improved = improve(&pages, &chosen).unwrap();
    assert_eq!(improved.len(), 2);
    assert_eq!(improved[0].image.get_pixel(299, 399), &Rgb([200, 30, 30]), "page 3, turned by half");
    assert_eq!(improve(&pages, &Improve { pages: Some(vec![]), ..Improve::default() }), Err(ScanError::NoPages));
    assert_eq!(improve(&pages, &Improve { pages: Some(vec![5]), ..Improve::default() }), Err(ScanError::NoPage(5)));

    // One page from an image is an image again: JPEG in colour, PNG in grey.
    let one = write(&improved[..1], false).unwrap();
    assert_eq!((one.media_type, one.extension), ("image/jpeg", "jpg"));
    let bw = improve(&pages[..1], &Improve { colour: Colour::BlackWhite, ..Improve::default() }).unwrap();
    assert_eq!(write(&bw, false).unwrap().media_type, "image/png");

    // Several pages are a PDF, which reads again with its pages and their size.
    let pdf = write(&improved, false).unwrap();
    assert_eq!(pdf.media_type, "application/pdf");
    assert_eq!(page_count(&pdf.bytes, "application/pdf").unwrap(), 2);
    let back = page(&pdf.bytes, "application/pdf", 1, 100.0).unwrap();
    assert_eq!(back.image.dimensions(), (300, 400), "300 × 400 pixels at 100 dpi are 216 × 288 points");
    // A black and white page is stored with one bit a pixel.
    let small = write(&bw, true).unwrap();
    assert!(small.bytes.len() < 4_000, "{}", small.bytes.len());
    let back = page(&small.bytes, "application/pdf", 0, 100.0).unwrap();
    assert_eq!(back.image.get_pixel(100, 75), &Rgb([0, 0, 0]));
}

#[test]
fn images_are_read_and_others_refused() {
    let bytes = png(&paper());
    assert_eq!(page_count(&bytes, "image/png").unwrap(), 1);
    assert_eq!(page(&bytes, "image/png", 0, DPI).unwrap().image, paper().image);
    assert_eq!(page(&bytes, "image/png", 1, DPI), Err(ScanError::NoPage(1)));
    assert!(matches!(decode(b"x", "image/gif"), Err(ScanError::Unsupported(_))));
    assert!(matches!(decode(b"x", "image/png"), Err(ScanError::Unreadable(_))));
    assert!(matches!(page_count(b"x", "application/pdf"), Err(ScanError::Unreadable(_))));
}

#[test]
fn improving_is_data() {
    let json = serde_json::json!({"pages": [1, 0], "turns": [0, 3], "deskew": true,
        "crop": {"left": 0.05}, "colour": "black_white"});
    let improve: Improve = serde_json::from_value(json).unwrap();
    assert_eq!(improve.turns, [0, 3]);
    assert_eq!(improve.crop, Some(Crop { left: 0.05, ..Crop::default() }));
    assert_eq!(improve.colour, Colour::BlackWhite);
}
