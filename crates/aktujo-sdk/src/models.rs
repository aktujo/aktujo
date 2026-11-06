// SPDX-License-Identifier: MIT
//! The OCR models, built in (`docs/rfc/0004-archive-0.1.md`, decision 6): from the crates of `aktujo/models`.

use aktujo_extract::Models;

/// PP-OCRv3 detection and PP-OCRv5 Latin recognition with its dictionary.
pub(crate) const MODELS: Models<'static> = Models {
    detection: aktujo_extract_det::MODEL,
    recognition: aktujo_extract_latin::MODEL,
    dictionary: aktujo_extract_latin::DICT,
};
