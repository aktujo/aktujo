// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the API shows and takes, described in OpenAPI. The document crates know nothing of the API, so their types
//! are mirrored here – in the same JSON where they have one.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use aktujo_app::{
    Bin, Document, Hit, Kind, Part, ReadState, Reading, Search as SearchQuery, SpaceInfo, Value, Version, date_of,
};

/// An error: what was refused or went wrong.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Problem {
    /// What happened, in English.
    pub error: String,
}

/// What this program is and can do.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StatusView {
    /// The version of the program.
    pub version: String,
    /// The device name this data folder writes under.
    pub device: String,
    /// Whether this build reads pages without a text layer (OCR); `false` in `aktujo-minimal`.
    pub ocr: bool,
    /// Why reading in the background stopped, if it did.
    pub reading_problem: Option<String>,
}

/// A space.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpaceView {
    /// The identity.
    pub id: String,
    /// Its name, once one arrived.
    pub name: Option<String>,
    /// Documents outside the bin.
    pub documents: usize,
    /// Documents in the bin.
    pub trashed: usize,
    /// The relay it is synchronised through, if it is.
    pub relay: Option<String>,
    /// What this device may do: `read`, `write` or `manage`; none for a space only on this device.
    pub rights: Option<String>,
    /// Whether this device asked to join and waits to be added.
    pub waiting: bool,
}

impl From<SpaceInfo> for SpaceView {
    fn from(space: SpaceInfo) -> Self {
        SpaceView {
            id: space.id.to_string(),
            name: space.name,
            documents: space.documents,
            trashed: space.trashed,
            relay: space.relay,
            rights: space.rights.and_then(|r| serde_json::to_value(r).ok()).and_then(|v| v.as_str().map(str::to_owned)),
            waiting: space.waiting,
        }
    }
}

/// A new space, or a new name for one.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpaceName {
    /// Its name: not empty, at most 100 characters.
    pub name: String,
}

/// The value of a field – the same JSON as in the events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ValueView {
    /// Any text.
    Text(String),
    /// A day, `YYYY-MM-DD`.
    Date(String),
    /// An amount of money.
    Money {
        /// A decimal number with a point, e.g. `-12.50`.
        amount: String,
        /// Three capital letters (ISO 4217), e.g. `EUR`.
        currency: String,
    },
    /// A decimal number with a point.
    Number(String),
    /// Yes or no.
    Flag(bool),
}

impl From<&Value> for ValueView {
    fn from(value: &Value) -> Self {
        match value.clone() {
            Value::Text(text) => ValueView::Text(text),
            Value::Date(day) => ValueView::Date(day),
            Value::Money { amount, currency } => ValueView::Money { amount, currency },
            Value::Number(number) => ValueView::Number(number),
            Value::Flag(flag) => ValueView::Flag(flag),
        }
    }
}

impl From<ValueView> for Value {
    fn from(value: ValueView) -> Self {
        match value {
            ValueView::Text(text) => Value::Text(text),
            ValueView::Date(day) => Value::Date(day),
            ValueView::Money { amount, currency } => Value::Money { amount, currency },
            ValueView::Number(number) => Value::Number(number),
            ValueView::Flag(flag) => Value::Flag(flag),
        }
    }
}

/// Where a document is in reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadStateView {
    /// Waiting to be read.
    Waiting,
    /// Being read now.
    Reading,
    /// Read.
    Done,
    /// Reading failed; the error is with the version.
    Failed,
}

impl From<ReadState> for ReadStateView {
    fn from(state: ReadState) -> Self {
        match state {
            ReadState::Waiting => ReadStateView::Waiting,
            ReadState::Reading => ReadStateView::Reading,
            ReadState::Done => ReadStateView::Done,
            ReadState::Failed => ReadStateView::Failed,
        }
    }
}

/// A part of an excerpt.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PartView {
    /// The text.
    pub text: String,
    /// Whether it matched a word looked for.
    pub hit: bool,
}

impl From<Part> for PartView {
    fn from(part: Part) -> Self {
        PartView { text: part.text, hit: part.hit }
    }
}

/// A document in a list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocumentSummary {
    /// Its space.
    pub space: String,
    /// The identity.
    pub id: String,
    /// The title.
    pub title: String,
    /// The kind – confirmed, or else suggested.
    pub kind: Option<String>,
    /// Whether a person confirmed the kind.
    pub kind_confirmed: bool,
    /// The labels, in order.
    pub labels: Vec<String>,
    /// Its date, `YYYY-MM-DD`: the field `date`, or else the day it was added.
    pub date: String,
    /// When it was added, in milliseconds since 1970.
    pub added: u64,
    /// Whether it is in the bin.
    pub trashed: bool,
    /// Where it is in reading.
    pub reading: ReadStateView,
    /// How many versions it has.
    pub version_count: usize,
    /// Where the words were found – empty without words.
    pub excerpt: Vec<PartView>,
}

impl DocumentSummary {
    /// The summary of a document.
    pub fn of(space: &str, document: &Document, reading: ReadState, excerpt: Vec<Part>) -> Self {
        DocumentSummary {
            space: space.to_owned(),
            id: document.id.to_string(),
            title: document.title.clone(),
            kind: document.kind.clone().or(document.kind_suggestion.as_ref().map(|s| s.kind.clone())),
            kind_confirmed: document.kind.is_some(),
            labels: document.labels.iter().cloned().collect(),
            date: date_of(document),
            added: document.added.wall_ms,
            trashed: document.trashed,
            reading: reading.into(),
            version_count: document.versions.len(),
            excerpt: excerpt.into_iter().map(PartView::from).collect(),
        }
    }

    /// The summary of a hit.
    pub fn of_hit(hit: Hit, document: &Document, reading: ReadState) -> Self {
        Self::of(hit.space.as_str(), document, reading, hit.excerpt)
    }
}

/// A field of a document.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldView {
    /// The key, e.g. `total`.
    pub key: String,
    /// The value a person confirmed.
    pub confirmed: Option<ValueView>,
    /// The latest suggestion not yet confirmed.
    pub suggestion: Option<ValueView>,
}

/// What reading a version gave.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadingView {
    /// Whether the reader could use OCR.
    pub ocr: bool,
    /// How many pages there are.
    pub pages: u32,
    /// How many pages gave no text.
    pub unread: u32,
    /// Why reading failed, if it did.
    pub error: Option<String>,
}

impl From<&Reading> for ReadingView {
    fn from(reading: &Reading) -> Self {
        ReadingView { ocr: reading.ocr, pages: reading.pages, unread: reading.unread, error: reading.error.clone() }
    }
}

/// A version of a document.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VersionView {
    /// Its number, the first version 1.
    pub number: usize,
    /// The hash of its original.
    pub hash: String,
    /// The file name it came with.
    pub name: String,
    /// The media type.
    pub media_type: String,
    /// The size in bytes.
    pub size: u64,
    /// When it was added, in milliseconds since 1970.
    pub added: u64,
    /// What reading it gave, if it was read.
    pub read: Option<ReadingView>,
}

impl VersionView {
    fn of(number: usize, version: &Version) -> Self {
        VersionView {
            number,
            hash: version.original.hash.to_string(),
            name: version.original.name.clone(),
            media_type: version.original.media_type.clone(),
            size: version.original.size,
            added: version.added.wall_ms,
            read: version.read.as_ref().map(ReadingView::from),
        }
    }
}

/// A document with everything about it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocumentView {
    /// What a list shows of it.
    #[serde(flatten)]
    pub summary: DocumentSummary,
    /// A better title, offered while the title is still the file's.
    pub title_suggestion: Option<String>,
    /// The kind suggested, while one is not confirmed.
    pub kind_suggestion: Option<String>,
    /// The fields, in the order of their keys.
    pub fields: Vec<FieldView>,
    /// The versions, the first one first.
    pub versions: Vec<VersionView>,
}

impl DocumentView {
    /// Everything about a document.
    pub fn of(space: &str, document: &Document, reading: ReadState, title_suggestion: Option<String>) -> Self {
        DocumentView {
            summary: DocumentSummary::of(space, document, reading, Vec::new()),
            title_suggestion,
            kind_suggestion: document.kind_suggestion.as_ref().map(|s| s.kind.clone()),
            fields: document
                .fields
                .iter()
                .map(|(key, field)| FieldView {
                    key: key.clone(),
                    confirmed: field.confirmed.as_ref().map(|c| ValueView::from(&c.value)),
                    suggestion: field.suggestion.as_ref().map(|s| ValueView::from(&s.value)),
                })
                .collect(),
            versions: document.versions.iter().enumerate().map(|(i, v)| VersionView::of(i + 1, v)).collect(),
        }
    }
}

/// What adding a file did.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddedView {
    /// The document the file is in.
    pub document: String,
    /// Whether it is new – false if the same bytes were in the space already.
    pub new: bool,
}

/// A new title.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TitleChange {
    /// Not empty, at most 200 characters.
    pub title: String,
}

/// The kind a person confirms.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KindChange {
    /// The identity of a kind ([`KindView`]), or none: the document has no kind.
    pub kind: Option<String>,
}

/// A value a person confirms.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldChange {
    /// The value; it must fit the field of the document's kind.
    pub value: ValueView,
}

/// A label to add.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LabelChange {
    /// Not empty, at most 100 characters.
    pub label: String,
}

/// A label and how many documents outside the bin have it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LabelCount {
    /// The label.
    pub label: String,
    /// How many documents have it.
    pub documents: usize,
}

/// A field of a kind.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldDefView {
    /// The key, e.g. `total`.
    pub key: String,
    /// The name, in English.
    pub name: String,
    /// The type of its values: `text`, `date`, `money`, `number` or `flag`.
    #[serde(rename = "type")]
    pub field_type: String,
}

/// A kind of document.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KindView {
    /// The identity, e.g. `invoice`.
    pub id: String,
    /// The name, in English.
    pub name: String,
    /// Its fields, in the order they are shown.
    pub fields: Vec<FieldDefView>,
}

impl From<Kind> for KindView {
    fn from(kind: Kind) -> Self {
        KindView {
            id: kind.id,
            name: kind.name,
            fields: kind
                .fields
                .into_iter()
                .map(|f| FieldDefView {
                    key: f.key,
                    name: f.name,
                    field_type: serde_json::to_value(f.field_type)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                })
                .collect(),
        }
    }
}

/// Where in the bin to look.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BinView {
    /// Documents outside the bin.
    #[default]
    Outside,
    /// Documents in the bin.
    Inside,
    /// Both.
    Both,
}

/// What to look for; every part may be left out.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct SearchRequest {
    /// Words, each found at the start of a word; `"…"` for a phrase. Case and accents do not count.
    pub words: String,
    /// The spaces to look in; none: all.
    pub spaces: Vec<String>,
    /// The kind – confirmed, or else suggested.
    pub kind: Option<String>,
    /// Labels a document must all have.
    pub labels: Vec<String>,
    /// The first day of the document's date, `YYYY-MM-DD`.
    pub from: Option<String>,
    /// The last day of the document's date, `YYYY-MM-DD`.
    pub to: Option<String>,
    /// Where in the bin to look.
    pub bin: BinView,
    /// At most this many documents.
    pub limit: Option<usize>,
}

impl SearchRequest {
    /// The search of the workspace; `None` names a space that is not an identity.
    pub fn query(self) -> Option<SearchQuery> {
        let spaces = self.spaces.iter().map(|s| s.parse().ok()).collect::<Option<Vec<_>>>()?;
        Some(SearchQuery {
            words: self.words,
            spaces,
            kind: self.kind,
            labels: self.labels,
            from: self.from,
            to: self.to,
            bin: match self.bin {
                BinView::Outside => Bin::Outside,
                BinView::Inside => Bin::Inside,
                BinView::Both => Bin::Both,
            },
            limit: self.limit,
        })
    }
}

/// A page of the text that was read.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PageText {
    /// How it was read: `text_layer`, `ocr` or `none`.
    pub method: String,
    /// Its lines, from the top.
    pub text: String,
}
