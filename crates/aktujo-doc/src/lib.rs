// SPDX-License-Identifier: MIT
//! Documents on the aktujo core: a document is not a file. It has an identity, a title, a kind, fields, labels and
//! versions – each version an original stored by its hash. Everything that happens to a document is an event in the
//! log of its space; the state of all documents follows from the events in the agreed order (`docs/rfc/0002-
//! documents.md`). No I/O.
//!
//! Everything read from a document is a **suggestion** with its source; only a person **confirms** a value. A field
//! keeps both, and a suggestion never replaces a confirmed value.
//!
//! Nothing in it belongs to one trade: the kinds of documents and their fields are data of a space ([`Kind`]),
//! defined by the people and programs that use it (`docs/rfc/0005-pure-core.md`).

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use aktujo_base::{ParseError, Sha256Hash, as_text};
use aktujo_log::{EventHash, Hlc, Log};
use serde::{Deserialize, Serialize};

mod kinds;
mod merge;

pub use kinds::{
    FieldDef, FieldType, Find, KIND_EVENTS, Kind, KindError, KindEvent, KindSkipped, Kinds, MAX_ID, MAX_NAME,
    TemplateRef, Weighted,
};
pub use merge::{Conflict, Merged, merge};

/// The identity of a document: a UUID in lower case, given when it is added and never changed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(String);

impl DocumentId {
    /// A new identity: a UUID of version 7 from the time in milliseconds since 1970 and random bits.
    pub fn generate(unix_ms: u64) -> Self {
        DocumentId(aktujo_base::uuid7(unix_ms))
    }

    /// The identity as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for DocumentId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if aktujo_base::is_uuid(s) {
            Ok(DocumentId(s.to_owned()))
        } else {
            Err(ParseError::new("document identity", s))
        }
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

as_text!(DocumentId);

/// The value of a field. Dates, amounts and numbers are text, never binary floating point, so that every device
/// reads the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    /// Any text.
    Text(String),
    /// A day, `YYYY-MM-DD`.
    Date(String),
    /// An amount of money: a decimal number (`-12.50`) and a currency (ISO 4217, `EUR`).
    Money {
        /// The amount, a decimal number with a point.
        amount: String,
        /// The currency, three capital letters.
        currency: String,
    },
    /// A decimal number with a point.
    Number(String),
    /// Yes or no.
    Flag(bool),
}

/// Why a value is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValueError {
    /// Not a day `YYYY-MM-DD` that exists.
    #[error("not a date: {0:?}")]
    Date(String),
    /// Not a decimal number with a point.
    #[error("not a decimal number: {0:?}")]
    Number(String),
    /// Not three capital letters.
    #[error("not a currency: {0:?}")]
    Currency(String),
    /// A value of another type than the field's.
    #[error("the field {key:?} takes {expected:?}")]
    Type {
        /// The field.
        key: String,
        /// The type it takes.
        expected: FieldType,
    },
}

fn decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    !whole.is_empty()
        && !fraction.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

fn date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

impl Value {
    /// The type of the value.
    pub fn field_type(&self) -> FieldType {
        match self {
            Value::Text(_) => FieldType::Text,
            Value::Date(_) => FieldType::Date,
            Value::Money { .. } => FieldType::Money,
            Value::Number(_) => FieldType::Number,
            Value::Flag(_) => FieldType::Flag,
        }
    }

    /// Whether the value is well formed: a date that exists, a decimal number, a currency of three capital letters.
    pub fn check(&self) -> Result<(), ValueError> {
        match self {
            Value::Date(text) if !date(text) => Err(ValueError::Date(text.clone())),
            Value::Number(text) if !decimal(text) => Err(ValueError::Number(text.clone())),
            Value::Money { amount, .. } if !decimal(amount) => Err(ValueError::Number(amount.clone())),
            Value::Money { currency, .. }
                if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) =>
            {
                Err(ValueError::Currency(currency.clone()))
            }
            _ => Ok(()),
        }
    }
}

/// Where a suggestion came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Source {
    /// The document's own text layer.
    TextLayer,
    /// Optical character recognition.
    Ocr,
    /// A rule, by its name.
    Rule {
        /// The rule.
        name: String,
    },
    /// An AI the operator set up, by its name.
    Ai {
        /// The model or service.
        name: String,
    },
    /// An import from another program, by its name.
    Import {
        /// The program.
        name: String,
    },
}

/// Whether a value is a suggestion or confirmed by a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum Origin {
    /// Read from the document; waits for a person.
    Suggested {
        /// Where it came from.
        #[serde(flatten)]
        source: Source,
    },
    /// Given or checked by a person – on the device that wrote the event.
    Confirmed,
}

/// An original of a document: its bytes are stored by their hash (`aktujo-blob`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Original {
    /// The hash of the bytes.
    pub hash: Sha256Hash,
    /// The file name it came with.
    pub name: String,
    /// The media type, e.g. `application/pdf`.
    pub media_type: String,
    /// The size in bytes.
    #[serde(deserialize_with = "aktujo_base::safe_u64")]
    pub size: u64,
}

/// What happens to documents – the payload of an event in the log of a space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DocumentEvent {
    /// A new document with its first version.
    DocumentAdded {
        /// The new identity.
        document: DocumentId,
        /// Its title.
        title: String,
        /// Its first version.
        original: Original,
    },
    /// A further version: a new scan, a corrected file.
    DocumentVersion {
        /// The document.
        document: DocumentId,
        /// The new version.
        original: Original,
    },
    /// A new title.
    DocumentTitle {
        /// The document.
        document: DocumentId,
        /// The title.
        title: String,
    },
    /// The kind, or none – suggested or confirmed, like a field.
    DocumentKind {
        /// The document.
        document: DocumentId,
        /// The identity of a kind, e.g. `invoice`.
        #[serde(rename = "document_kind")]
        kind: Option<String>,
        /// Suggested or confirmed.
        #[serde(flatten)]
        origin: Origin,
    },
    /// A value for a field, suggested or confirmed.
    DocumentField {
        /// The document.
        document: DocumentId,
        /// The field, e.g. `total`.
        key: String,
        /// The value.
        value: Value,
        /// Suggested or confirmed.
        #[serde(flatten)]
        origin: Origin,
    },
    /// A field emptied by a person: its confirmed value and its suggestion are gone.
    DocumentFieldCleared {
        /// The document.
        document: DocumentId,
        /// The field.
        key: String,
    },
    /// A label added.
    DocumentLabel {
        /// The document.
        document: DocumentId,
        /// The label.
        label: String,
    },
    /// A label removed.
    DocumentUnlabel {
        /// The document.
        document: DocumentId,
        /// The label.
        label: String,
    },
    /// Into the bin – nothing is deleted.
    DocumentTrashed {
        /// The document.
        document: DocumentId,
    },
    /// Out of the bin.
    DocumentRestored {
        /// The document.
        document: DocumentId,
    },
    /// The text of a version was read: the lines with their position, stored as an original of their own by hash,
    /// so that other devices need not read again. A later reading of the same version replaces it.
    DocumentRead {
        /// The document.
        document: DocumentId,
        /// The hash of the version's original that was read.
        version: Sha256Hash,
        /// The hash of what was read, if anything could be read.
        text: Option<Sha256Hash>,
        /// Whether the reader could use OCR; a version read without it may be read again by one that can.
        ocr: bool,
        /// How many pages were read.
        pages: u32,
        /// How many of them had no text that could be read.
        unread: u32,
        /// Why reading failed, if it did.
        error: Option<String>,
    },
}

/// The kinds of event this crate writes and reads; events of other kinds are left alone.
pub const KINDS: [&str; 11] = [
    "document_added",
    "document_version",
    "document_title",
    "document_kind",
    "document_field",
    "document_field_cleared",
    "document_label",
    "document_unlabel",
    "document_trashed",
    "document_restored",
    "document_read",
];

impl DocumentEvent {
    /// The document the event is about.
    pub fn document(&self) -> &DocumentId {
        match self {
            DocumentEvent::DocumentAdded { document, .. }
            | DocumentEvent::DocumentVersion { document, .. }
            | DocumentEvent::DocumentTitle { document, .. }
            | DocumentEvent::DocumentKind { document, .. }
            | DocumentEvent::DocumentField { document, .. }
            | DocumentEvent::DocumentFieldCleared { document, .. }
            | DocumentEvent::DocumentLabel { document, .. }
            | DocumentEvent::DocumentUnlabel { document, .. }
            | DocumentEvent::DocumentTrashed { document }
            | DocumentEvent::DocumentRestored { document }
            | DocumentEvent::DocumentRead { document, .. } => document,
        }
    }

    /// Whether the values in the event are well formed.
    pub fn check(&self) -> Result<(), ValueError> {
        match self {
            DocumentEvent::DocumentField { value, .. } => value.check(),
            _ => Ok(()),
        }
    }
}

/// A version of a document: its original, when it was added and what was read from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The original.
    pub original: Original,
    /// When it was added, and on which device.
    pub added: Hlc,
    /// The latest reading of its text, if it was read.
    pub read: Option<Reading>,
}

/// What reading the text of a version gave ([`DocumentEvent::DocumentRead`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// The hash of what was read, if anything could be read.
    pub text: Option<Sha256Hash>,
    /// Whether the reader could use OCR.
    pub ocr: bool,
    /// How many pages were read.
    pub pages: u32,
    /// How many of them had no text that could be read.
    pub unread: u32,
    /// Why reading failed, if it did.
    pub error: Option<String>,
    /// When it was read, and on which device.
    pub at: Hlc,
}

/// A value and when it was set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmed {
    /// The value.
    pub value: Value,
    /// When it was confirmed, and on which device.
    pub at: Hlc,
}

/// A suggestion and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The value.
    pub value: Value,
    /// Where it came from.
    pub source: Source,
    /// When it was suggested, and on which device.
    pub at: Hlc,
}

/// A suggested kind and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindSuggestion {
    /// The identity of the kind.
    pub kind: String,
    /// Where it came from.
    pub source: Source,
    /// When it was suggested, and on which device.
    pub at: Hlc,
}

/// A field of a document: what a person confirmed and, apart from it, the latest suggestion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Field {
    /// The confirmed value, if a person gave one.
    pub confirmed: Option<Confirmed>,
    /// The latest suggestion not yet confirmed.
    pub suggestion: Option<Suggestion>,
}

impl Field {
    /// The confirmed value, or else the suggested one.
    pub fn value(&self) -> Option<&Value> {
        self.confirmed.as_ref().map(|c| &c.value).or(self.suggestion.as_ref().map(|s| &s.value))
    }
}

/// A document as the events of its space say it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The identity.
    pub id: DocumentId,
    /// The title.
    pub title: String,
    /// The kind a person confirmed, if any.
    pub kind: Option<String>,
    /// The latest suggested kind not yet confirmed.
    pub kind_suggestion: Option<KindSuggestion>,
    /// The versions, the first one first.
    pub versions: Vec<Version>,
    /// The fields by their key.
    pub fields: BTreeMap<String, Field>,
    /// The labels.
    pub labels: BTreeSet<String>,
    /// Whether it is in the bin.
    pub trashed: bool,
    /// When it was added.
    pub added: Hlc,
    /// When it last changed.
    pub changed: Hlc,
}

impl Document {
    /// The latest version.
    pub fn current(&self) -> &Version {
        self.versions.last().expect("a document has a version")
    }
}

/// An event about documents that could not be applied – reported, never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skipped {
    /// A document event that does not read as one.
    Unreadable {
        /// The event.
        event: EventHash,
        /// Why.
        reason: String,
    },
    /// An event about a document that was not added before it.
    UnknownDocument {
        /// The event.
        event: EventHash,
        /// The document.
        document: DocumentId,
    },
    /// A second `document_added` for the same identity.
    AddedTwice {
        /// The event.
        event: EventHash,
        /// The document.
        document: DocumentId,
    },
    /// A reading of a version the document does not have.
    UnknownVersion {
        /// The event.
        event: EventHash,
        /// The document.
        document: DocumentId,
        /// The version.
        version: Sha256Hash,
    },
    /// A value that is not well formed.
    BadValue {
        /// The event.
        event: EventHash,
        /// Why.
        reason: ValueError,
    },
}

/// The documents of a space, folded from its log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Documents {
    documents: BTreeMap<DocumentId, Document>,
}

impl Documents {
    /// The documents in the log, from its events in the agreed order, and the document events that did not apply.
    pub fn from_log(log: &Log) -> (Self, Vec<Skipped>) {
        let mut documents = Documents::default();
        let mut skipped = Vec::new();
        for (hash, event) in log.ordered() {
            if !KINDS.contains(&event.kind.as_str()) {
                continue;
            }
            match event.payload::<DocumentEvent>() {
                Ok(payload) => {
                    if let Err(problem) = documents.apply(hash, &event.time, payload) {
                        skipped.push(problem);
                    }
                }
                Err(error) => skipped.push(Skipped::Unreadable { event: hash, reason: error.to_string() }),
            }
        }
        (documents, skipped)
    }

    /// Applies one event written at `at`.
    pub fn apply(&mut self, event: EventHash, at: &Hlc, payload: DocumentEvent) -> Result<(), Skipped> {
        payload.check().map_err(|reason| Skipped::BadValue { event, reason })?;
        if let DocumentEvent::DocumentAdded { document, title, original } = payload {
            if self.documents.contains_key(&document) {
                return Err(Skipped::AddedTwice { event, document });
            }
            let version = Version { original, added: at.clone(), read: None };
            self.documents.insert(
                document.clone(),
                Document {
                    id: document,
                    title,
                    kind: None,
                    kind_suggestion: None,
                    versions: vec![version],
                    fields: BTreeMap::new(),
                    labels: BTreeSet::new(),
                    trashed: false,
                    added: at.clone(),
                    changed: at.clone(),
                },
            );
            return Ok(());
        }
        let Some(doc) = self.documents.get_mut(payload.document()) else {
            return Err(Skipped::UnknownDocument { event, document: payload.document().clone() });
        };
        if let DocumentEvent::DocumentRead { document, version, text, ocr, pages, unread, error } = payload {
            // Reading is not a change by a person: `changed` stays. The newest version of that original takes it.
            let Some(read) = doc.versions.iter_mut().rev().find(|v| v.original.hash == version) else {
                return Err(Skipped::UnknownVersion { event, document, version });
            };
            read.read = Some(Reading { text, ocr, pages, unread, error, at: at.clone() });
            return Ok(());
        }
        doc.changed = at.clone();
        match payload {
            DocumentEvent::DocumentAdded { .. } | DocumentEvent::DocumentRead { .. } => unreachable!("handled above"),
            DocumentEvent::DocumentVersion { original, .. } => {
                doc.versions.push(Version { original, added: at.clone(), read: None })
            }
            DocumentEvent::DocumentTitle { title, .. } => doc.title = title,
            DocumentEvent::DocumentKind { kind, origin, .. } => {
                match origin {
                    Origin::Confirmed => {
                        doc.kind = kind;
                        doc.kind_suggestion = None;
                    }
                    Origin::Suggested { source } => {
                        doc.kind_suggestion = kind
                            .filter(|k| doc.kind.as_ref() != Some(k))
                            .map(|kind| KindSuggestion { kind, source, at: at.clone() });
                    }
                }
            }
            DocumentEvent::DocumentField { key, value, origin, .. } => {
                let field = doc.fields.entry(key).or_default();
                match origin {
                    Origin::Confirmed => {
                        field.confirmed = Some(Confirmed { value, at: at.clone() });
                        field.suggestion = None;
                    }
                    Origin::Suggested { source } => {
                        let same = field.confirmed.as_ref().is_some_and(|c| c.value == value);
                        if !same {
                            field.suggestion = Some(Suggestion { value, source, at: at.clone() });
                        }
                    }
                }
            }
            DocumentEvent::DocumentFieldCleared { key, .. } => {
                doc.fields.remove(&key);
            }
            DocumentEvent::DocumentLabel { label, .. } => {
                doc.labels.insert(label);
            }
            DocumentEvent::DocumentUnlabel { label, .. } => {
                doc.labels.remove(&label);
            }
            DocumentEvent::DocumentTrashed { .. } => doc.trashed = true,
            DocumentEvent::DocumentRestored { .. } => doc.trashed = false,
        }
        Ok(())
    }

    /// A document by its identity, also one in the bin.
    pub fn get(&self, id: &DocumentId) -> Option<&Document> {
        self.documents.get(id)
    }

    /// All documents not in the bin, by identity (in order of being added, as identities are UUIDs of version 7).
    pub fn active(&self) -> impl Iterator<Item = &Document> {
        self.documents.values().filter(|d| !d.trashed)
    }

    /// The documents in the bin.
    pub fn trashed(&self) -> impl Iterator<Item = &Document> {
        self.documents.values().filter(|d| d.trashed)
    }

    /// How many documents there are, in the bin or not.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use aktujo_log::{Clock, DeviceId};

    use super::*;

    fn clock(name: &str) -> Clock {
        Clock::new(name.parse::<DeviceId>().unwrap())
    }

    fn original(bytes: &[u8]) -> Original {
        Original {
            hash: Sha256Hash::of(bytes),
            name: "scan.pdf".into(),
            media_type: "application/pdf".into(),
            size: bytes.len() as u64,
        }
    }

    fn read(document: &DocumentId, version: &[u8], text: Option<&[u8]>) -> DocumentEvent {
        DocumentEvent::DocumentRead {
            document: document.clone(),
            version: Sha256Hash::of(version),
            text: text.map(Sha256Hash::of),
            ocr: true,
            pages: 1,
            unread: 0,
            error: None,
        }
    }

    fn money(amount: &str) -> Value {
        Value::Money { amount: amount.into(), currency: "EUR".into() }
    }

    fn field(document: &DocumentId, key: &str, value: Value, origin: Origin) -> DocumentEvent {
        DocumentEvent::DocumentField { document: document.clone(), key: key.into(), value, origin }
    }

    #[test]
    fn an_event_reads_as_written() {
        let id: DocumentId = "019a5d2e-7c3b-7d4e-8f00-0123456789ab".parse().unwrap();
        let mut log = Log::new();
        let mut laptop = clock("laptop");
        let event = field(&id, "total", money("119.00"), Origin::Suggested { source: Source::Ocr });
        let hash = log.append(&mut laptop, 1_000, &event).unwrap();
        let canonical = std::str::from_utf8(log.canonical(&hash).unwrap()).unwrap();
        assert_eq!(
            canonical,
            r#"{"document":"019a5d2e-7c3b-7d4e-8f00-0123456789ab","key":"total","kind":"document_field","origin":"suggested","prev":null,"seq":1,"source":"ocr","time":{"counter":0,"device":"laptop","wall_ms":1000},"type":"event","value":{"type":"money","value":{"amount":"119.00","currency":"EUR"}}}"#
        );
        assert_eq!(log.get(&hash).unwrap().payload::<DocumentEvent>().unwrap(), event);
    }

    #[test]
    fn every_kind_is_listed() {
        let id = DocumentId::generate(1);
        let events = [
            DocumentEvent::DocumentAdded { document: id.clone(), title: "t".into(), original: original(b"a") },
            DocumentEvent::DocumentVersion { document: id.clone(), original: original(b"b") },
            DocumentEvent::DocumentTitle { document: id.clone(), title: "t".into() },
            DocumentEvent::DocumentKind { document: id.clone(), kind: None, origin: Origin::Confirmed },
            field(&id, "k", Value::Flag(true), Origin::Confirmed),
            DocumentEvent::DocumentFieldCleared { document: id.clone(), key: "k".into() },
            DocumentEvent::DocumentLabel { document: id.clone(), label: "l".into() },
            DocumentEvent::DocumentUnlabel { document: id.clone(), label: "l".into() },
            DocumentEvent::DocumentTrashed { document: id.clone() },
            DocumentEvent::DocumentRestored { document: id.clone() },
            read(&id, b"b", Some(b"text")),
        ];
        let kinds: Vec<String> =
            events.iter().map(|e| serde_json::to_value(e).unwrap()["kind"].as_str().unwrap().to_owned()).collect();
        assert_eq!(kinds, KINDS);
    }

    #[test]
    fn suggestions_never_replace_what_a_person_confirmed() {
        let id = DocumentId::generate(1);
        let mut log = Log::new();
        let mut laptop = clock("laptop");
        let added =
            DocumentEvent::DocumentAdded { document: id.clone(), title: "Invoice".into(), original: original(b"x") };
        log.append(&mut laptop, 1, &added).unwrap();
        log.append(&mut laptop, 2, &field(&id, "total", money("110.00"), Origin::Suggested { source: Source::Ocr }))
            .unwrap();
        log.append(&mut laptop, 3, &field(&id, "total", money("119.00"), Origin::Confirmed)).unwrap();
        let (documents, skipped) = Documents::from_log(&log);
        assert!(skipped.is_empty());
        let total = &documents.get(&id).unwrap().fields["total"];
        assert_eq!(total.confirmed.as_ref().unwrap().value, money("119.00"));
        assert_eq!(total.suggestion, None);

        let rule = Source::Rule { name: "invoice-total".into() };
        log.append(&mut laptop, 4, &field(&id, "total", money("119.00"), Origin::Suggested { source: rule.clone() }))
            .unwrap();
        log.append(&mut laptop, 5, &field(&id, "total", money("9.00"), Origin::Suggested { source: rule.clone() }))
            .unwrap();
        let (documents, _) = Documents::from_log(&log);
        let total = &documents.get(&id).unwrap().fields["total"];
        assert_eq!(total.value(), Some(&money("119.00")));
        assert_eq!(total.suggestion.as_ref().map(|s| (&s.value, &s.source)), Some((&money("9.00"), &rule)));
    }

    #[test]
    fn a_kind_is_suggested_and_confirmed_like_a_field() {
        let id = DocumentId::generate(1);
        let mut log = Log::new();
        let mut laptop = clock("laptop");
        let added =
            DocumentEvent::DocumentAdded { document: id.clone(), title: "Scan".into(), original: original(b"x") };
        let kind = |kind: Option<&str>, origin: Origin| DocumentEvent::DocumentKind {
            document: id.clone(),
            kind: kind.map(Into::into),
            origin,
        };
        let rule = Source::Rule { name: "kind".into() };
        log.append(&mut laptop, 1, &added).unwrap();
        log.append(&mut laptop, 2, &kind(Some("invoice"), Origin::Suggested { source: rule.clone() })).unwrap();
        let (documents, _) = Documents::from_log(&log);
        let doc = documents.get(&id).unwrap();
        assert_eq!(doc.kind, None);
        assert_eq!(doc.kind_suggestion.as_ref().map(|s| (s.kind.as_str(), &s.source)), Some(("invoice", &rule)));

        log.append(&mut laptop, 3, &kind(Some("receipt"), Origin::Confirmed)).unwrap();
        log.append(&mut laptop, 4, &kind(Some("receipt"), Origin::Suggested { source: rule.clone() })).unwrap();
        let (documents, _) = Documents::from_log(&log);
        let doc = documents.get(&id).unwrap();
        assert_eq!((doc.kind.as_deref(), &doc.kind_suggestion), (Some("receipt"), &None));
        let json = serde_json::to_value(kind(Some("invoice"), Origin::Suggested { source: rule })).unwrap();
        assert_eq!(json["origin"], "suggested");
        assert_eq!(json["source"], "rule");
    }

    #[test]
    fn two_devices_agree_after_merging() {
        let id = DocumentId::generate(1);
        let (mut laptop, mut phone) = (clock("laptop"), clock("phone"));
        let mut a = Log::new();
        let added =
            DocumentEvent::DocumentAdded { document: id.clone(), title: "Lease".into(), original: original(b"x") };
        a.append(&mut laptop, 1, &added).unwrap();
        let mut b = a.clone();
        a.append(&mut laptop, 10, &DocumentEvent::DocumentTitle { document: id.clone(), title: "Lease 2026".into() })
            .unwrap();
        a.append(&mut laptop, 11, &DocumentEvent::DocumentLabel { document: id.clone(), label: "home".into() })
            .unwrap();
        b.append(&mut phone, 20, &DocumentEvent::DocumentTitle { document: id.clone(), title: "Flat".into() }).unwrap();
        b.append(&mut phone, 21, &DocumentEvent::DocumentVersion { document: id.clone(), original: original(b"y") })
            .unwrap();
        a.merge(&b);
        b.merge(&a);
        let (left, _) = Documents::from_log(&a);
        let (right, _) = Documents::from_log(&b);
        assert_eq!(left, right);
        let doc = left.get(&id).unwrap();
        assert_eq!(doc.title, "Flat");
        assert_eq!(doc.versions.len(), 2);
        assert_eq!(doc.current().original.hash, Sha256Hash::of(b"y"));
        assert!(doc.labels.contains("home"));
    }

    #[test]
    fn the_bin_keeps_documents() {
        let id = DocumentId::generate(1);
        let mut log = Log::new();
        let mut laptop = clock("laptop");
        let added =
            DocumentEvent::DocumentAdded { document: id.clone(), title: "Old".into(), original: original(b"x") };
        log.append(&mut laptop, 1, &added).unwrap();
        log.append(&mut laptop, 2, &DocumentEvent::DocumentTrashed { document: id.clone() }).unwrap();
        let (documents, _) = Documents::from_log(&log);
        assert_eq!((documents.active().count(), documents.trashed().count()), (0, 1));
        log.append(&mut laptop, 3, &DocumentEvent::DocumentRestored { document: id }).unwrap();
        assert_eq!(Documents::from_log(&log).0.active().count(), 1);
    }

    #[test]
    fn what_does_not_apply_is_reported_and_other_kinds_are_left_alone() {
        let id = DocumentId::generate(1);
        let mut log = Log::new();
        let mut laptop = clock("laptop");
        #[derive(Serialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        enum Other {
            BookingAdded { amount: String },
        }
        log.append(&mut laptop, 1, &Other::BookingAdded { amount: "1".into() }).unwrap();
        let orphan = log.append(&mut laptop, 2, &DocumentEvent::DocumentTrashed { document: id.clone() }).unwrap();
        let added = DocumentEvent::DocumentAdded { document: id.clone(), title: "A".into(), original: original(b"x") };
        log.append(&mut laptop, 3, &added).unwrap();
        let twice = log.append(&mut laptop, 4, &added).unwrap();
        let bad = log
            .append(&mut laptop, 5, &field(&id, "date", Value::Date("2026-02-30".into()), Origin::Confirmed))
            .unwrap();
        let (documents, skipped) = Documents::from_log(&log);
        assert_eq!(documents.len(), 1);
        assert_eq!(
            skipped,
            vec![
                Skipped::UnknownDocument { event: orphan, document: id.clone() },
                Skipped::AddedTwice { event: twice, document: id },
                Skipped::BadValue { event: bad, reason: ValueError::Date("2026-02-30".into()) },
            ]
        );
    }

    #[test]
    fn a_reading_belongs_to_its_version_and_changes_nothing_else() {
        let id = DocumentId::generate(1);
        let mut log = Log::new();
        let mut laptop = clock("laptop");
        let added = DocumentEvent::DocumentAdded { document: id.clone(), title: "A".into(), original: original(b"x") };
        log.append(&mut laptop, 1, &added).unwrap();
        log.append(&mut laptop, 2, &DocumentEvent::DocumentVersion { document: id.clone(), original: original(b"y") })
            .unwrap();
        log.append(&mut laptop, 3, &read(&id, b"x", None)).unwrap();
        log.append(&mut laptop, 4, &read(&id, b"x", Some(b"lines"))).unwrap();
        let stray = log.append(&mut laptop, 5, &read(&id, b"z", Some(b"lines"))).unwrap();
        let (documents, skipped) = Documents::from_log(&log);
        let doc = documents.get(&id).unwrap();
        assert_eq!(doc.versions[0].read.as_ref().unwrap().text, Some(Sha256Hash::of(b"lines")));
        assert_eq!(doc.versions[1].read, None);
        assert_eq!(doc.changed.wall_ms, 2);
        assert_eq!(
            skipped,
            vec![Skipped::UnknownVersion { event: stray, document: id, version: Sha256Hash::of(b"z") }]
        );
    }

    #[test]
    fn values_are_checked() {
        assert!(Value::Date("2024-02-29".into()).check().is_ok());
        assert!(Value::Date("2026-13-01".into()).check().is_err());
        assert!(Value::Number("-0.5".into()).check().is_ok());
        for bad in ["1,5", ".5", "1.", "1e3", ""] {
            assert!(Value::Number(bad.into()).check().is_err(), "{bad}");
        }
        assert_eq!(
            Value::Money { amount: "1.00".into(), currency: "eur".into() }.check(),
            Err(ValueError::Currency("eur".into()))
        );
    }

    #[test]
    fn identities_are_uuids() {
        let id = DocumentId::generate(1_700_000_000_000);
        assert_eq!(id.as_str().parse::<DocumentId>().unwrap(), id);
        assert_eq!(&id.as_str()[14..15], "7");
        assert!("not-a-uuid".parse::<DocumentId>().is_err());
    }
}
