// SPDX-License-Identifier: MIT
//! With the feature `ocr`, the OCR models are built into the program (`docs/rfc/0004-archive-0.1.md`, decision 6):
//! they are taken from the folder in `AKTUJO_OCR_MODELS`, else from `target/ocr-models` of the workspace, where
//! `scripts/get-ocr-models.sh` puts them, and checked by their hashes. The build makes no connection.

use std::{env, fs, path::PathBuf};

use sha2::{Digest, Sha256};

/// The files and their SHA-256 hashes – the same as in `scripts/get-ocr-models.sh`.
const MODELS: [(&str, &str); 3] = [
    ("det.onnx", "ee40e80071ba3a320d4efda75f3e22047a7d049e9bf7bcaaf9daea23fc21b935"),
    ("rec.onnx", "614ffc2d6d3902d360fad7f1b0dd455ee45e877069d14c4e51a99dc4ef144409"),
    ("dict.txt", "3c0a8a79b612653c25f765271714f71281e4e955962c153e272b7b8c1d2b13ff"),
];

fn main() {
    println!("cargo::rerun-if-env-changed=AKTUJO_OCR_MODELS");
    if env::var_os("CARGO_FEATURE_OCR").is_none() {
        return;
    }
    let dir = match env::var_os("AKTUJO_OCR_MODELS") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo")).join("../../target/ocr-models"),
    };
    let dir = dir.canonicalize().unwrap_or_else(|_| {
        panic!(
            "the OCR models are not in {} – run scripts/get-ocr-models.sh, set AKTUJO_OCR_MODELS, or build without \
             the feature `ocr` (aktujo-minimal)",
            dir.display()
        )
    });
    for (file, expected) in MODELS {
        let path = dir.join(file);
        println!("cargo::rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let hash: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hash, expected, "{} is not the model aktujo expects", path.display());
    }
    println!("cargo::rustc-env=AKTUJO_OCR_DIR={}", dir.display());
}
