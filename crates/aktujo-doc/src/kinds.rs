// SPDX-License-Identifier: MIT
//! Kinds of documents as data of a space. Nothing is built in: a person or a program defines the kinds it needs –
//! letters, sheet music, research papers, the books of a collection, anything. A kind has a name, fields with their
//! types, the words that speak for it and the ways to find its fields in a text (read by `aktujo-rules`). Kinds are
//! events in the log of their space ([`KindEvent`]), so that every member has the same; [`Kinds::from_log`] folds
//! them.

use std::collections::{BTreeMap, BTreeSet};

use aktujo_base::Sha256Hash;
use aktujo_log::{EventHash, Log};
use serde::{Deserialize, Serialize};

use crate::{Value, ValueError};

/// The type of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// Any text.
    Text,
    /// A day.
    Date,
    /// An amount of money with its currency.
    Money,
    /// A decimal number.
    Number,
    /// Yes or no.
    Flag,
}

/// A way to find the value of a field in the text of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "way", rename_all = "snake_case")]
pub enum Find {
    /// After one of the labels, tried in order: on the same line, else on the line to its right or right below.
    Label {
        /// The labels, as words in any case (`order no` matches `Order-No.:`).
        labels: Vec<String>,
    },
    /// Among all values after any of the labels, the largest.
    Largest {
        /// The labels.
        labels: Vec<String>,
    },
    /// The first value of the field's type on the first page.
    First,
    /// The name in the letterhead, the sender line or the footer: the part of a line with one of the markers; else
    /// the topmost line of words.
    Sender {
        /// The markers, as words in any case.
        markers: Vec<String>,
    },
    /// Yes, if one of the phrases occurs – for flags.
    Present {
        /// The phrases, as words in any case.
        phrases: Vec<String>,
    },
}

/// A word or phrase and how much it speaks for a kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weighted {
    /// The word or phrase, matched as whole words in any case.
    pub word: String,
    /// Its weight, a whole number: events hold no floating-point numbers.
    pub weight: u32,
}

/// A field of a kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDef {
    /// The key in events, e.g. `author`.
    pub key: String,
    /// The name people read.
    pub name: String,
    /// The name in other languages, by language tag (`de`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    /// The type of its values.
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// For a text field: only the first word with a digit (a number or reference), not the rest of the line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
    /// The ways to find it in a text, tried in order; none, and it is only ever given by a person.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub find: Vec<Find>,
}

impl FieldDef {
    /// A field with a name and a type, found by no rule.
    pub fn new(key: &str, name: &str, field_type: FieldType) -> Self {
        FieldDef { key: key.into(), name: name.into(), names: BTreeMap::new(), field_type, code: false, find: vec![] }
    }

    /// The name in `language`, else the name.
    pub fn name_in(&self, language: &str) -> &str {
        self.names.get(language).unwrap_or(&self.name)
    }
}

/// A kind of document, defined by a person or a program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kind {
    /// The identity in events: lower-case letters, digits, `.`, `-` and `_`, e.g. `letter` or `books.edition`.
    pub id: String,
    /// The name people read.
    pub name: String,
    /// The name in other languages, by language tag (`de`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    /// Its fields, in the order they are shown.
    pub fields: Vec<FieldDef>,
    /// Words and phrases that speak for the kind; none, and it is never suggested, only chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Weighted>,
    /// The fields a suggested title is made of after the kind's name, the first with a value of each group:
    /// `[["sender"], ["date"]]` gives `Letter · Example Ltd · 2026-11-05`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<Vec<String>>,
}

/// The longest identity of a kind or key of a field.
pub const MAX_ID: usize = 64;
/// The longest name.
pub const MAX_NAME: usize = 100;

/// Why a kind is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KindError {
    /// An identity or key that is empty, too long or has other characters than `a-z 0-9 . - _`.
    #[error("not an identity: {0:?} (a-z, 0-9, '.', '-', '_'; at most 64)")]
    Id(String),
    /// A name that is empty or longer than 100 characters.
    #[error("not a name: {0:?} (1 to 100 characters)")]
    Name(String),
    /// Two fields with the same key.
    #[error("the field {0:?} is there twice")]
    TwiceField(String),
    /// A title made of a field the kind does not have.
    #[error("the title names the field {0:?}, which the kind does not have")]
    TitleField(String),
    /// A template whose base is another kind.
    #[error("the template's base is the kind {0:?}, not this one")]
    Base(String),
}

fn identity(id: &str) -> Result<(), KindError> {
    let fine = !id.is_empty()
        && id.len() <= MAX_ID
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b));
    if fine { Ok(()) } else { Err(KindError::Id(id.to_owned())) }
}

fn name(name: &str) -> Result<(), KindError> {
    let count = name.trim().chars().count();
    if (1..=MAX_NAME).contains(&count) { Ok(()) } else { Err(KindError::Name(name.to_owned())) }
}

impl Kind {
    /// A kind with a name and fields, found by no rule.
    pub fn new(id: &str, name: &str, fields: Vec<FieldDef>) -> Self {
        Kind { id: id.into(), name: name.into(), names: BTreeMap::new(), fields, words: vec![], title: vec![] }
    }

    /// The field with `key`.
    pub fn field(&self, key: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// The name in `language`, else the name.
    pub fn name_in(&self, language: &str) -> &str {
        self.names.get(language).unwrap_or(&self.name)
    }

    /// Whether the kind is well formed: identities, names, fields once each, a title of its own fields.
    pub fn check(&self) -> Result<(), KindError> {
        identity(&self.id)?;
        name(&self.name)?;
        self.names.values().try_for_each(|n| name(n))?;
        let mut keys = BTreeSet::new();
        for field in &self.fields {
            identity(&field.key)?;
            name(&field.name)?;
            field.names.values().try_for_each(|n| name(n))?;
            if !keys.insert(field.key.as_str()) {
                return Err(KindError::TwiceField(field.key.clone()));
            }
        }
        match self.title.iter().flatten().find(|key| !keys.contains(key.as_str())) {
            Some(key) => Err(KindError::TitleField(key.clone())),
            None => Ok(()),
        }
    }

    /// Whether `value` is well formed and fits the field `key`. A key the kind does not have takes any value.
    pub fn check_value(&self, key: &str, value: &Value) -> Result<(), ValueError> {
        value.check()?;
        match self.field(key) {
            Some(field) if field.field_type != value.field_type() => {
                Err(ValueError::Type { key: key.to_owned(), expected: field.field_type })
            }
            _ => Ok(()),
        }
    }
}

/// Where a kind came from: the template it was taken over from – from a catalog, or as a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateRef {
    /// The address of the catalog source; none for a template imported as a file and not yet found in a catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The template's ID in its catalog (`aktujo.paperwork`); none like the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The SHA-256 of the template's file, as it was taken over.
    pub sha256: Sha256Hash,
    /// The kind as the template defines it – what a person's changes are told apart from when it is updated.
    pub base: Box<Kind>,
}

/// What happens to the kinds of a space – the payload of an event in its log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[expect(clippy::large_enum_variant, reason = "kind events are few and read once; a box would only cost")]
pub enum KindEvent {
    /// A kind defined, or defined anew: the whole definition replaces the one before.
    KindDefined {
        /// The kind.
        definition: Kind,
        /// The template it was taken from, when it was – a person's change leaves this out, and the space keeps the
        /// template it knew.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        template: Option<TemplateRef>,
    },
    /// A kind removed. Documents keep it as their kind; it can be defined again.
    KindRemoved {
        /// The identity of the kind.
        id: String,
    },
}

/// The kinds of event about kinds; events of other kinds are left alone.
pub const KIND_EVENTS: [&str; 2] = ["kind_defined", "kind_removed"];

/// A kind event that could not be applied – reported, never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KindSkipped {
    /// An event that does not read as a kind event.
    Unreadable {
        /// The event.
        event: EventHash,
        /// Why.
        reason: String,
    },
    /// A definition that is not valid, or a template whose base is not the same kind.
    Invalid {
        /// The event.
        event: EventHash,
        /// Why.
        reason: KindError,
    },
}

/// The kinds of a space, folded from its log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kinds {
    kinds: BTreeMap<String, Kind>,
    templates: BTreeMap<String, TemplateRef>,
}

impl Kinds {
    /// The kinds in the log, from its events in the agreed order, and the kind events that did not apply.
    pub fn from_log(log: &Log) -> (Self, Vec<KindSkipped>) {
        let mut kinds = Kinds::default();
        let mut skipped = Vec::new();
        for (hash, event) in log.ordered() {
            if !KIND_EVENTS.contains(&event.kind.as_str()) {
                continue;
            }
            match event.payload::<KindEvent>() {
                Ok(payload) => {
                    if let Err(reason) = kinds.apply(payload) {
                        skipped.push(KindSkipped::Invalid { event: hash, reason });
                    }
                }
                Err(error) => skipped.push(KindSkipped::Unreadable { event: hash, reason: error.to_string() }),
            }
        }
        (kinds, skipped)
    }

    /// Applies one event.
    pub fn apply(&mut self, payload: KindEvent) -> Result<(), KindError> {
        match payload {
            KindEvent::KindDefined { definition, template } => {
                definition.check()?;
                if let Some(template) = template {
                    template.base.check()?;
                    if template.base.id != definition.id {
                        return Err(KindError::Base(template.base.id.clone()));
                    }
                    self.templates.insert(definition.id.clone(), template);
                }
                self.kinds.insert(definition.id.clone(), definition);
            }
            KindEvent::KindRemoved { id } => {
                self.kinds.remove(&id);
                self.templates.remove(&id);
            }
        }
        Ok(())
    }

    /// The kind with `id`.
    pub fn get(&self, id: &str) -> Option<&Kind> {
        self.kinds.get(id)
    }

    /// The template the kind `id` was taken from, if it was.
    pub fn template(&self, id: &str) -> Option<&TemplateRef> {
        self.templates.get(id)
    }

    /// All kinds, by identity.
    pub fn iter(&self) -> impl Iterator<Item = &Kind> {
        self.kinds.values()
    }

    /// All kinds, by identity, as a list.
    pub fn to_vec(&self) -> Vec<Kind> {
        self.kinds.values().cloned().collect()
    }

    /// How many there are.
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use aktujo_log::{Clock, DeviceId};

    use super::*;

    fn letter() -> Kind {
        let mut kind = Kind::new(
            "letter",
            "Letter",
            vec![FieldDef::new("sender", "Sender", FieldType::Text), FieldDef::new("date", "Date", FieldType::Date)],
        );
        kind.names.insert("de".into(), "Brief".into());
        kind.title = vec![vec!["sender".into()], vec!["date".into()]];
        kind
    }

    #[test]
    fn a_value_must_fit_its_field() {
        let letter = letter();
        assert!(letter.check_value("date", &Value::Date("2026-11-05".into())).is_ok());
        assert_eq!(
            letter.check_value("date", &Value::Text("5 Nov".into())),
            Err(ValueError::Type { key: "date".into(), expected: FieldType::Date })
        );
        assert!(letter.check_value("note", &Value::Text("anything".into())).is_ok());
        assert!(letter.check_value("note", &Value::Date("2026-02-30".into())).is_err());
    }

    #[test]
    fn a_kind_is_checked() {
        assert_eq!(letter().check(), Ok(()));
        assert_eq!(letter().name_in("de"), "Brief");
        assert_eq!(letter().name_in("fr"), "Letter");
        let mut bad = letter();
        bad.id = "Letter".into();
        assert_eq!(bad.check(), Err(KindError::Id("Letter".into())));
        let mut bad = letter();
        bad.name = " ".into();
        assert!(matches!(bad.check(), Err(KindError::Name(_))));
        let mut bad = letter();
        bad.fields.push(FieldDef::new("date", "Again", FieldType::Text));
        assert_eq!(bad.check(), Err(KindError::TwiceField("date".into())));
        let mut bad = letter();
        bad.title.push(vec!["author".into()]);
        assert_eq!(bad.check(), Err(KindError::TitleField("author".into())));
    }

    #[test]
    fn kinds_are_events_of_their_space() {
        let mut log = Log::new();
        let mut laptop = Clock::new("laptop".parse::<DeviceId>().unwrap());
        let book = Kind::new("book", "Book", vec![FieldDef::new("author", "Author", FieldType::Text)]);
        log.append(&mut laptop, 1, &KindEvent::KindDefined { definition: letter(), template: None }).unwrap();
        log.append(&mut laptop, 2, &KindEvent::KindDefined { definition: book.clone(), template: None }).unwrap();
        let mut renamed = letter();
        renamed.name = "Mail".into();
        log.append(&mut laptop, 3, &KindEvent::KindDefined { definition: renamed, template: None }).unwrap();
        log.append(&mut laptop, 4, &KindEvent::KindRemoved { id: "book".into() }).unwrap();
        let mut bad = book;
        bad.id = "".into();
        let wrong = log.append(&mut laptop, 5, &KindEvent::KindDefined { definition: bad, template: None }).unwrap();
        let (kinds, skipped) = Kinds::from_log(&log);
        assert_eq!(kinds.len(), 1);
        assert_eq!(kinds.get("letter").map(|k| k.name.as_str()), Some("Mail"));
        assert_eq!(skipped, vec![KindSkipped::Invalid { event: wrong, reason: KindError::Id("".into()) }]);
    }

    #[test]
    fn a_kind_remembers_its_template_until_it_is_removed() {
        let mut log = Log::new();
        let mut laptop = Clock::new("laptop".parse::<DeviceId>().unwrap());
        let template = TemplateRef {
            source: Some("https://github.com/aktujo/catalog".into()),
            id: Some("aktujo.paperwork".into()),
            sha256: Sha256Hash::of(b"template"),
            base: Box::new(letter()),
        };
        let taken = KindEvent::KindDefined { definition: letter(), template: Some(template.clone()) };
        log.append(&mut laptop, 1, &taken).unwrap();
        let mut changed = letter();
        changed.name = "Mail".into();
        log.append(&mut laptop, 2, &KindEvent::KindDefined { definition: changed, template: None }).unwrap();
        let (kinds, skipped) = Kinds::from_log(&log);
        assert!(skipped.is_empty());
        assert_eq!(kinds.get("letter").map(|k| k.name.as_str()), Some("Mail"));
        assert_eq!(kinds.template("letter"), Some(&template), "a person's change keeps the template");
        let json = serde_json::to_value(&taken).unwrap();
        assert_eq!(json["template"]["id"], "aktujo.paperwork");
        assert_eq!(json["template"]["base"]["name"], "Letter");

        let mut other = template.clone();
        other.base.id = "book".into();
        let wrong = log
            .append(&mut laptop, 3, &KindEvent::KindDefined { definition: letter(), template: Some(other) })
            .unwrap();
        log.append(&mut laptop, 4, &KindEvent::KindRemoved { id: "letter".into() }).unwrap();
        let (kinds, skipped) = Kinds::from_log(&log);
        assert_eq!(kinds.template("letter"), None);
        assert_eq!(skipped, vec![KindSkipped::Invalid { event: wrong, reason: KindError::Base("book".into()) }]);
    }

    #[test]
    fn a_kind_reads_as_written() {
        let json = serde_json::to_value(KindEvent::KindDefined { definition: letter(), template: None }).unwrap();
        assert_eq!(json["kind"], "kind_defined");
        assert_eq!(json["definition"]["fields"][1], serde_json::json!({"key": "date", "name": "Date", "type": "date"}));
        assert_eq!(json["definition"]["names"], serde_json::json!({"de": "Brief"}));
        let back: KindEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, KindEvent::KindDefined { definition: letter(), template: None });
    }
}
