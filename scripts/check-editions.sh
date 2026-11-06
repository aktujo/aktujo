#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Checks that aktujo and aktujo-minimal read and write the same data folder (docs/rfc/0004-archive-0.1.md, decision
# 6): each founds a space and imports an export, and each sees what the other wrote.
#
#   scripts/check-editions.sh <aktujo> <aktujo-minimal>
set -euo pipefail

full=$1
minimal=$2
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
data=(--data "$scratch/data" --key-file "$scratch/key")

# A small export, as aktujo writes one: a kind, a document with two versions, a label.
export_root="$scratch/export/aktujo Shared 2026-11-06"
mkdir -p "$export_root/documents/0001 Lease"
printf '%%PDF-1.4\n%% first\n' >"$export_root/documents/0001 Lease/v1 lease.pdf"
printf '%%PDF-1.4\n%% signed\n' >"$export_root/documents/0001 Lease/v2 lease signed.pdf"
cat >"$export_root/space.json" <<'JSON'
{
  "format": 1,
  "space": {"id": "unused", "name": "Shared"},
  "kinds": [{"id": "contract", "name": "Contract", "fields": []}],
  "documents": [{
    "title": "Lease",
    "kind": "contract",
    "fields": {},
    "labels": ["Flat"],
    "trashed": false,
    "versions": [
      {"name": "lease.pdf", "media_type": "application/pdf", "file": "documents/0001 Lease/v1 lease.pdf"},
      {"name": "lease signed.pdf", "media_type": "application/pdf", "file": "documents/0001 Lease/v2 lease signed.pdf"}
    ]
  }]
}
JSON

expect() { # <what> <output> <text>
  grep -qF -- "$3" <<<"$2" || { echo "::error::$1: expected \"$3\" in:"; echo "$2"; exit 1; }
}

"$full" spaces --found "By aktujo" "${data[@]}" >/dev/null
"$minimal" spaces --found "By aktujo-minimal" "${data[@]}" >/dev/null
"$minimal" import "$scratch/export" --name "Imported by aktujo-minimal" "${data[@]}" >/dev/null

seen=$("$full" spaces "${data[@]}")
expect "aktujo sees what aktujo-minimal wrote" "$seen" "By aktujo-minimal"
expect "aktujo sees what aktujo-minimal imported" "$seen" "Imported by aktujo-minimal  1 documents"

"$full" import "$scratch/export" --name "Imported by aktujo" "${data[@]}" >/dev/null
"$full" export --space "Imported by aktujo" --to "$scratch" --zip "${data[@]}" >/dev/null
zip=$(ls "$scratch"/*.zip)
"$minimal" import "$zip" --name "Back from a ZIP" "${data[@]}" >/dev/null

seen=$("$minimal" spaces "${data[@]}")
expect "aktujo-minimal sees what aktujo wrote" "$seen" "By aktujo  0 documents"
expect "aktujo-minimal sees what aktujo imported" "$seen" "Imported by aktujo  1 documents"
expect "an export of aktujo comes back in aktujo-minimal" "$seen" "Back from a ZIP  1 documents"
echo "aktujo and aktujo-minimal share the data folder"
