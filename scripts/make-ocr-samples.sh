#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Builds the test PDFs from testdata/ocr into a folder (default: target/ocr-samples, not tracked) with Typst and
# LilyPond from nixpkgs. Only the fonts that come with Typst are used (--ignore-system-fonts), so the PDFs are the
# same on every machine. Each PDF's text is in the .txt of the same name – for examples/measure.rs of aktujo-extract.
#   scripts/make-ocr-samples.sh [<folder>]
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-"$root/target/ocr-samples"}
mkdir -p "$out"
for f in receipt notice letter; do
  nix run nixpkgs#typst -- compile --ignore-system-fonts "$root/testdata/ocr/$f.typ" "$out/$f.pdf"
  cp "$root/testdata/ocr/$f.txt" "$out/"
done
# LilyPond from nixpkgs finds no text fonts on macOS by itself: give it the TeX Gyre fonts through a fonts.conf.
gyre=$(nix build --no-link --print-out-paths nixpkgs#gyre-fonts)
printf '<?xml version="1.0"?>\n<fontconfig><dir>%s/share/fonts</dir><cachedir>%s/fc-cache</cachedir></fontconfig>\n' \
  "$gyre" "$out" > "$out/fonts.conf"
FONTCONFIG_FILE="$out/fonts.conf" nix run nixpkgs#lilypond -- -s -o "$out/score" "$root/testdata/ocr/score.ly"
cp "$root/testdata/ocr/score.txt" "$out/"
echo "PDFs in $out: $(cd "$out" && ls *.pdf | tr '\n' ' ')"
