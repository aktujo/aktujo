// SPDX-License-Identifier: AGPL-3.0-or-later
//! The OCR models, built in (`docs/rfc/0004-archive-0.1.md`, decision 6; `build.rs` finds and checks them).

use aktujo_extract::Models;

/// PP-OCRv3 detection and PP-OCRv5 Latin recognition with its dictionary.
pub(crate) const MODELS: Models<'static> = Models {
    detection: include_bytes!(concat!(env!("AKTUJO_OCR_DIR"), "/det.onnx")),
    recognition: include_bytes!(concat!(env!("AKTUJO_OCR_DIR"), "/rec.onnx")),
    dictionary: include_str!(concat!(env!("AKTUJO_OCR_DIR"), "/dict.txt")),
};
