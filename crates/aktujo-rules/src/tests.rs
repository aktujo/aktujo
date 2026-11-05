// SPDX-License-Identifier: MIT
use aktujo_doc::builtin;
use aktujo_extract::Page;

use super::*;

/// A page of lines, each at (x0, y0), one character 0.01 wide and lines 0.015 high.
fn page(lines: &[(&str, f32, f32)], method: Method) -> Page {
    Page {
        width: 595.0,
        height: 842.0,
        lines: lines
            .iter()
            .map(|(text, x0, y0)| Line {
                text: (*text).into(),
                x0: *x0,
                y0: *y0,
                x1: x0 + 0.01 * text.chars().count() as f32,
                y1: y0 + 0.015,
            })
            .collect(),
        method,
    }
}

fn invoice() -> Extraction {
    Extraction {
        pages: vec![page(
            &[
                ("Stadtwerke Musterstadt GmbH · Am Werk 1 · 12345 Musterstadt", 0.1, 0.05),
                ("Rechnung", 0.1, 0.25),
                ("Rechnungs-Nr.: 2026-0042", 0.1, 0.3),
                ("Rechnungsdatum", 0.6, 0.3),
                ("05.11.2026", 0.6, 0.32),
                ("Sehr geehrte Damen und Herren,", 0.1, 0.4),
                ("Strom Oktober 100,00", 0.1, 0.5),
                ("Summe netto 100,00 EUR", 0.1, 0.55),
                ("MwSt 19 % 19,00 EUR", 0.1, 0.57),
                ("Gesamtbetrag 119,00 €", 0.1, 0.6),
                ("Zahlbar bis 19. November 2026", 0.1, 0.65),
                ("USt-IdNr. DE 123 456 789 · IBAN DE89 3704 0044 0532 0130 00", 0.1, 0.95),
            ],
            Method::TextLayer,
        )],
    }
}

fn value<'a>(recognition: &'a Recognition, key: &str) -> Option<&'a Value> {
    recognition.fields.iter().find(|f| f.key == key).map(|f| &f.value)
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn date(s: &str) -> Value {
    Value::Date(s.into())
}

fn money(amount: &str) -> Value {
    Value::Money { amount: amount.into(), currency: "EUR".into() }
}

#[test]
fn recognises_an_invoice() {
    let read = Rules::builtin().recognise(&builtin(), &invoice(), None);
    let kind = read.kind.as_ref().unwrap();
    assert_eq!(kind.kind, "invoice");
    assert!(kind.confidence > 0.5, "{read:?}");
    assert_eq!(value(&read, "issuer"), Some(&text("Stadtwerke Musterstadt GmbH")));
    assert_eq!(value(&read, "number"), Some(&text("2026-0042")));
    // Below its label, which stands alone on its line.
    assert_eq!(value(&read, "date"), Some(&date("2026-11-05")));
    assert_eq!(value(&read, "total"), Some(&money("119.00")));
    assert_eq!(value(&read, "due"), Some(&date("2026-11-19")));
    assert_eq!(value(&read, "paid"), None);
    // What only accounting needs is left to the programs that specialise the rules.
    for key in ["net", "tax", "iban", "vat_id"] {
        assert_eq!(value(&read, key), None, "{key}");
    }
    let total = read.fields.iter().find(|f| f.key == "total").unwrap();
    assert_eq!((total.rule.as_str(), total.page, total.line), ("invoice.total", 0, 9));
    let date = read.fields.iter().find(|f| f.key == "date").unwrap();
    assert_eq!((date.confidence, date.line), (NEIGHBOUR, 4));
}

#[test]
fn recognises_a_receipt_read_by_ocr() {
    let receipt = Extraction {
        pages: vec![page(
            &[
                ("Bäckerei Korn", 0.3, 0.02),
                ("Hauptstraße 5, 12345 Musterstadt", 0.2, 0.05),
                ("2 Brötchen 1,20", 0.1, 0.2),
                ("1 Kaffee 2,80", 0.1, 0.23),
                ("SUMME EUR 4,00", 0.1, 0.3),
                ("Gegeben Bar 10,00", 0.1, 0.33),
                ("Rückgeld 6,00", 0.1, 0.36),
                ("05.11.26 08:12 Kasse 2", 0.1, 0.4),
                ("TSE-Signatur 8f2a", 0.1, 0.45),
            ],
            Method::Ocr,
        )],
    };
    let read = Rules::builtin().recognise(&builtin(), &receipt, None);
    assert_eq!(read.kind.as_ref().map(|k| k.kind.as_str()), Some("receipt"));
    // No legal form: the topmost line of words.
    assert_eq!(value(&read, "issuer"), Some(&text("Bäckerei Korn")));
    assert_eq!(value(&read, "date"), Some(&date("2026-11-05")));
    let total = read.fields.iter().find(|f| f.key == "total").unwrap();
    assert_eq!(total.value, money("4.00"));
    assert!((total.confidence - LARGEST * OCR).abs() < 1e-6);
}

#[test]
fn recognises_a_notice() {
    let notice = Extraction {
        pages: vec![page(
            &[
                ("Finanzamt Musterstadt, Postfach 10 20 30, 12345 Musterstadt", 0.1, 0.05),
                ("Steuernummer 123/456/78901", 0.6, 0.15),
                ("Musterstadt, 3. November 2026", 0.6, 0.18),
                ("Bescheid für 2025 über Einkommensteuer", 0.1, 0.3),
                ("Festgesetzt werden 1.234,56 €", 0.1, 0.4),
                ("Bitte zahlen Sie den Betrag bis zum 07.12.2026.", 0.1, 0.5),
                ("Rechtsbehelfsbelehrung", 0.1, 0.8),
            ],
            Method::TextLayer,
        )],
    };
    let read = Rules::builtin().recognise(&builtin(), &notice, None);
    assert_eq!(read.kind.as_ref().map(|k| k.kind.as_str()), Some("notice"));
    assert_eq!(value(&read, "authority"), Some(&text("Finanzamt Musterstadt")));
    assert_eq!(value(&read, "reference"), Some(&text("123/456/78901")));
    assert_eq!(value(&read, "date"), Some(&date("2026-11-03")));
    assert_eq!(value(&read, "deadline"), Some(&date("2026-12-07")));
    assert_eq!(value(&read, "amount"), Some(&money("1234.56")));
}

#[test]
fn suggests_no_kind_without_enough_words() {
    let note = Extraction { pages: vec![page(&[("Einkaufsliste: Milch, Brot", 0.1, 0.1)], Method::TextLayer)] };
    let read = Rules::builtin().recognise(&builtin(), &note, None);
    assert_eq!(read, Recognition::default());
    assert!(Rules::builtin().recognise(&builtin(), &Extraction::default(), None).kind.is_none());
}

#[test]
fn reads_the_fields_of_a_kind_given() {
    let letter = builtin().into_iter().find(|k| k.id == "letter").unwrap();
    let found = Rules::builtin().fields(&letter, &invoice());
    let keys: Vec<&str> = found.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, ["sender", "date"]);
}

#[test]
fn writes_suggestions_only_where_they_add_something() {
    use aktujo_base::Sha256Hash;
    use aktujo_doc::{Documents, Original};
    use aktujo_log::{Clock, DeviceId, Log};

    let id: DocumentId = "019a5d2e-7c3b-7d4e-8f00-0123456789ab".parse().unwrap();
    let mut log = Log::new();
    let mut clock = Clock::new("laptop".parse::<DeviceId>().unwrap());
    let original = Original {
        hash: Sha256Hash::of(b"pdf"),
        name: "invoice.pdf".into(),
        media_type: "application/pdf".into(),
        size: 3,
    };
    let field = |key: &str, value: Value, origin: Origin| DocumentEvent::DocumentField {
        document: id.clone(),
        key: key.into(),
        value,
        origin,
    };
    for (ms, event) in [
        (1, DocumentEvent::DocumentAdded { document: id.clone(), title: "Strom".into(), original }),
        // A person gave the total; OCR suggested the same number the rules find.
        (2, field("total", money("120.00"), Origin::Confirmed)),
        (3, field("number", text("2026-0042"), Origin::Suggested { source: Source::Ocr })),
    ] {
        log.append(&mut clock, ms, &event).unwrap();
    }
    let (documents, skipped) = Documents::from_log(&log);
    assert!(skipped.is_empty());
    let read = Rules::builtin().recognise(&builtin(), &invoice(), None);
    let events = read.events(documents.get(&id).unwrap());
    let keys: Vec<&str> = events
        .iter()
        .map(|e| match e {
            DocumentEvent::DocumentField { key, origin, .. } => {
                assert_eq!(origin, &Origin::Suggested { source: Source::Rule { name: format!("invoice.{key}") } });
                key.as_str()
            }
            DocumentEvent::DocumentKind { kind, origin, .. } => {
                assert_eq!(kind.as_deref(), Some("invoice"));
                assert_eq!(origin, &Origin::Suggested { source: Source::Rule { name: KIND_RULE.into() } });
                "kind"
            }
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(keys, ["kind", "issuer", "date", "due"]);

    // Once a person confirmed another kind, its own fields are read instead.
    let confirmed =
        DocumentEvent::DocumentKind { document: id.clone(), kind: Some("letter".into()), origin: Origin::Confirmed };
    log.append(&mut clock, 4, &confirmed).unwrap();
    let (documents, _) = Documents::from_log(&log);
    let document = documents.get(&id).unwrap();
    assert!(read.events(document).is_empty());
    let events = Rules::builtin().suggest(&builtin(), document, &invoice());
    let keys: Vec<&str> = events
        .iter()
        .map(|e| match e {
            DocumentEvent::DocumentField { key, .. } => key.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(keys, ["sender", "date"]);
}

#[test]
fn suggests_a_title() {
    let read = Rules::builtin().recognise(&builtin(), &invoice(), None);
    assert_eq!(read.title(&builtin()).as_deref(), Some("Invoice · Stadtwerke Musterstadt GmbH · 2026-11-05"));
    assert_eq!(Recognition::default().title(&builtin()), None);
}

#[test]
fn rules_are_data() {
    let rules = Rules::builtin();
    let json = serde_json::to_string(&rules).unwrap();
    assert!(json.contains(r#"{"key":"number","code":true,"find":[{"way":"label","labels":["rechnungsnummer""#));
    assert_eq!(serde_json::from_str::<Rules>(&json).unwrap(), rules);
    // Every rule names a kind and fields that aktujo-doc has.
    let kinds = builtin();
    for rules in &rules.kinds {
        let kind = kinds.iter().find(|k| k.id == rules.kind).unwrap_or_else(|| panic!("{}", rules.kind));
        for field in &rules.fields {
            assert!(kind.field(&field.key).is_some(), "{}.{}", rules.kind, field.key);
        }
    }
}

#[test]
fn recognises_documents_in_other_languages() {
    let facture = Extraction {
        pages: vec![page(
            &[
                ("Boulangerie Dupont SARL · 3 rue des Lilas · 75011 Paris", 0.1, 0.05),
                ("Facture N° F-2026-118", 0.1, 0.2),
                ("Date de facture : 5 novembre 2026", 0.1, 0.23),
                ("Total HT 100,00 €", 0.1, 0.5),
                ("Total TTC 1\u{202f}120,00 €", 0.1, 0.55),
                ("Date d'échéance : 05/12/2026", 0.1, 0.6),
            ],
            Method::TextLayer,
        )],
    };
    let rules = Rules { date_order: Some(DateOrder::DayFirst), ..Rules::builtin() };
    let read = rules.recognise(&builtin(), &facture, None);
    assert_eq!(read.kind.as_ref().map(|k| k.kind.as_str()), Some("invoice"));
    assert_eq!(value(&read, "issuer"), Some(&text("Boulangerie Dupont SARL")));
    assert_eq!(value(&read, "number"), Some(&text("F-2026-118")));
    assert_eq!(value(&read, "date"), Some(&date("2026-11-05")));
    assert_eq!(value(&read, "total"), Some(&money("1120.00")));
    assert_eq!(value(&read, "due"), Some(&date("2026-12-05")));
    // Without an order, 05/12/2026 is not read.
    let read = Rules::builtin().recognise(&builtin(), &facture, None);
    assert_eq!(value(&read, "due"), None);
}

/// A specialisation in code, as a program like spezado would bring it: the VAT ID of an invoice.
struct VatIds;

impl Recogniser for VatIds {
    fn recognise(&self, _: &[Kind], extraction: &Extraction, kind: Option<&str>) -> Recognition {
        let found = extraction.pages.iter().enumerate().find_map(|(p, page)| {
            page.lines.iter().enumerate().find_map(|(l, line)| {
                let at = crate::text::find_label(&line.text, "ust idnr")?;
                let id: String = line.text[at.end..].chars().filter(char::is_ascii_alphanumeric).take(11).collect();
                Some(Found {
                    key: "vat_id".into(),
                    value: text(&id),
                    confidence: 0.9,
                    rule: "spezado.vat_id".into(),
                    page: p,
                    line: l,
                })
            })
        });
        Recognition {
            // It does not guess kinds; it reads its field for the kind given, or as an invoice.
            kind: Some(Guess { kind: kind.unwrap_or("invoice").into(), score: 0.0, confidence: 0.0 }),
            kinds: vec![],
            fields: found.into_iter().collect(),
        }
    }
}

#[test]
fn programs_specialise_by_code_and_by_data() {
    // An invoice kind with one more field, as the program defines it.
    let mut kinds = builtin();
    let invoice_kind = kinds.iter_mut().find(|k| k.id == "invoice").unwrap();
    invoice_kind.fields.push(aktujo_doc::FieldDef {
        key: "vat_id".into(),
        name: "VAT ID".into(),
        field_type: FieldType::Text,
    });
    invoice_kind.fields.push(aktujo_doc::FieldDef {
        key: "customer".into(),
        name: "Customer number".into(),
        field_type: FieldType::Text,
    });
    // More rules as data, added to the built-in ones.
    let mut rules = Rules::builtin();
    let extra: Rules = serde_json::from_value(serde_json::json!({"kinds": [{
        "kind": "invoice",
        "words": [{"word": "kundennummer", "weight": 1.0}],
        "fields": [{"key": "customer", "code": true, "find": [{"way": "label", "labels": ["kundennummer"]}]}]
    }]}))
    .unwrap();
    rules.extend(extra);
    let mut document = invoice();
    document.pages[0].lines.push(Line { text: "Kundennummer 4711".into(), x0: 0.6, y0: 0.2, x1: 0.8, y1: 0.215 });
    let chain = Chain(vec![Box::new(rules), Box::new(VatIds)]);
    let read = chain.recognise(&kinds, &document, None);
    assert_eq!(read.kind.as_ref().map(|k| k.kind.as_str()), Some("invoice"));
    assert_eq!(value(&read, "customer"), Some(&text("4711")));
    assert_eq!(value(&read, "vat_id"), Some(&text("DE123456789")));
    assert_eq!(value(&read, "total"), Some(&money("119.00")));
    let vat = read.fields.iter().find(|f| f.key == "vat_id").unwrap();
    assert_eq!(vat.rule, "spezado.vat_id");
}

#[test]
fn merging_keeps_the_surer_kind_and_values() {
    let found = |key: &str, v: &str, confidence: f32| Found {
        key: key.into(),
        value: text(v),
        confidence,
        rule: format!("x.{key}"),
        page: 0,
        line: 0,
    };
    let guess = |kind: &str, confidence: f32| Some(Guess { kind: kind.into(), score: 3.0, confidence });
    let a = Recognition { kind: guess("letter", 0.4), kinds: vec![], fields: vec![found("sender", "A", 0.3)] };
    let b = Recognition {
        kind: guess("invoice", 0.6),
        kinds: vec![],
        fields: vec![found("issuer", "B", 0.6), found("number", "1", 0.8)],
    };
    let c = Recognition { kind: guess("invoice", 0.5), kinds: vec![], fields: vec![found("number", "2", 0.9)] };
    let merged = a.merge(b).merge(c);
    assert_eq!(merged.kind.unwrap().kind, "invoice");
    let values: Vec<(&str, &Value)> = merged.fields.iter().map(|f| (f.key.as_str(), &f.value)).collect();
    assert_eq!(values, [("issuer", &text("B")), ("number", &text("2"))]);
}
