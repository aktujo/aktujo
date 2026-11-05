// SPDX-License-Identifier: AGPL-3.0-or-later
// A synthetic official notice of a made-up town. Every line here is one line of notice.txt.
#set page(paper: "a4", margin: (x: 25mm, y: 20mm))
#set text(font: "Libertinus Serif", size: 11pt, lang: "de")
#set par(justify: false, leading: 0.6em)
#grid(columns: (1fr, auto),
  [*Stadt Musterstadt* \ Der Oberbürgermeister \ Amt für Steuern und Abgaben],
  [Aktenzeichen: 32.1-0457/26 \ Datum: 02.11.2026])
#v(10mm)
Frau Erika Beispiel \
Am Gänseanger 3 \
12345 Musterstadt
#v(10mm)
#text(size: 13pt, weight: "bold")[Bescheid über die Grundsteuer für das Jahr 2027]
#v(4mm)
Sehr geehrte Frau Beispiel,

für den unten genannten Grundbesitz wird die Grundsteuer wie folgt festgesetzt.

#table(columns: (auto, 1fr, auto), stroke: 0.4pt,
  [*Objekt*], [*Bezeichnung*], [*Betrag*],
  [Flur 12], [Wohngrundstück, Am Gänseanger 3], [412,80 €],
  [Flur 14], [Garten, Feldstraße], [23,15 €],
  [], [*Jahresbetrag*], [*435,95 €*])

Die Steuer ist zu je einem Viertel am 15.02., 15.05., 15.08. und 15.11. fällig.
Bitte überweisen Sie den Betrag unter Angabe des Kassenzeichens 5100.2026.0457.

*Rechtsbehelfsbelehrung:* Gegen diesen Bescheid kann innerhalb eines Monats nach
Bekanntgabe Widerspruch erhoben werden. Der Widerspruch ist schriftlich oder zur
Niederschrift bei der Stadt Musterstadt einzulegen.

Mit freundlichen Grüßen \
Im Auftrag \
Jörg Größmann
