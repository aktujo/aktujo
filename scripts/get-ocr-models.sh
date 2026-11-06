#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Downloads the OCR models of aktujo-extract's backend `pure` into a folder (default: target/ocr-models, not tracked)
# and checks their SHA-256 hashes: PP-OCRv3 detection and PP-OCRv5 Latin recognition with its dictionary, PaddleOCR
# models converted to ONNX (monkt/paddleocr-onnx on Hugging Face) – Apache-2.0. The folder is what
# AKTUJO_OCR_MODELS and the examples of aktujo-extract and aktujo-rules take. Measured in docs/spikes/ocr.md.
#   scripts/get-ocr-models.sh [<folder>]
set -eu
dir=${1:-"$(dirname "$0")/../target/ocr-models"}
mkdir -p "$dir"
HF=https://huggingface.co/monkt/paddleocr-onnx/resolve/main
get() { # file url sha256
  [ -f "$dir/$1" ] || curl -sSfL -o "$dir/$1" "$2"
  echo "$3  $dir/$1" | sha256 -c -
}
# shasum on macOS, sha256sum elsewhere (Linux, Git for Windows).
sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
get det.onnx "$HF/detection/v3/det.onnx" ee40e80071ba3a320d4efda75f3e22047a7d049e9bf7bcaaf9daea23fc21b935
get rec.onnx "$HF/languages/latin/rec.onnx" 614ffc2d6d3902d360fad7f1b0dd455ee45e877069d14c4e51a99dc4ef144409
get dict.txt "$HF/languages/latin/dict.txt" 3c0a8a79b612653c25f765271714f71281e4e955962c153e272b7b8c1d2b13ff
