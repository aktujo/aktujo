// SPDX-License-Identifier: AGPL-3.0-or-later
// A synthetic till receipt of a made-up shop. Every line here is one line of receipt.txt.
#set page(width: 80mm, height: auto, margin: 5mm)
#set text(font: "DejaVu Sans Mono", size: 8.5pt, lang: "de")
#set par(leading: 0.45em)
#let row(a, b) = grid(columns: (1fr, auto), a, b)
#align(center)[
  #text(size: 11pt, weight: "bold")[Bäckerei Sonnenkorn] \
  Lindenweg 17 · 12345 Musterstadt \
  Tel. 01234 567890 \
  USt-IdNr. DE000000000
]
#line(length: 100%, stroke: 0.4pt)
#row[Roggenbrot 750 g][3,49 A]
#row[2 x Laugenbrezel à 0,85][1,70 A]
#row[Croissant Butter][1,35 A]
#row[Kaffee groß][2,90 B]
#row[Mohnschnecke][1,95 A]
#row[Pfand Becher][1,00 B]
#line(length: 100%, stroke: 0.4pt)
#row[*SUMME EUR*][*12,39*]
#row[Gegeben Karte][12,39]
#line(length: 100%, stroke: 0.4pt)
#row[A 7 % MwSt. netto 7,94][MwSt. 0,56]
#row[B 19 % MwSt. netto 3,28][MwSt. 0,62]
#v(2mm)
#align(center)[
  Beleg-Nr. 4711-0815 · Kasse 2 \
  04.11.2026 08:17 Uhr \
  TSE-Signatur: 7Q3F9K2M \
  Vielen Dank für Ihren Einkauf!
]
