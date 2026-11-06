// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the API shows and takes, described in OpenAPI. The document crates know nothing of the API, so their types
//! are mirrored here – in the same JSON where they have one.

use std::collections::BTreeMap;

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
    /// Whether this build improves scans and shows pages; `false` in `aktujo-minimal`.
    pub improve: bool,
    /// Why reading in the background stopped, if it did.
    pub reading_problem: Option<String>,
    /// The name people read for this device, if it was given one.
    pub device_name: Option<String>,
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
    /// When the last synchronisation since the program started ended, in milliseconds since 1970.
    pub synced_at: Option<u64>,
    /// Why it failed, if it did.
    pub sync_problem: Option<String>,
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
            synced_at: space.synced.as_ref().map(|s| s.at),
            sync_problem: space.synced.and_then(|s| s.problem),
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

/// The type of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldTypeView {
    /// Any text.
    Text,
    /// A day, `YYYY-MM-DD`.
    Date,
    /// An amount of money with its currency.
    Money,
    /// A decimal number.
    Number,
    /// Yes or no.
    Flag,
}

/// A way to find the value of a field in the text of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "way", rename_all = "snake_case")]
pub enum FindView {
    /// After one of the labels: on the same line, else on the line to its right or right below.
    Label {
        /// The labels, as words in any case.
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
        /// The markers.
        markers: Vec<String>,
    },
    /// Yes, if one of the phrases occurs – for flags.
    Present {
        /// The phrases.
        phrases: Vec<String>,
    },
}

/// A word or phrase and how much it speaks for a kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WeightedView {
    /// The word or phrase, matched as whole words in any case.
    pub word: String,
    /// Its weight, a whole number; a kind is suggested from a sum of 6.
    pub weight: u32,
}

/// A field of a kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FieldDefView {
    /// The key in events: `a-z`, `0-9`, `.`, `-`, `_`.
    pub key: String,
    /// The name people read.
    pub name: String,
    /// The name in other languages, by language tag (`de`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    /// The type of its values.
    #[serde(rename = "type")]
    pub field_type: FieldTypeView,
    /// For a text field: only the first word with a digit (a number or reference).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
    /// The ways to find it in a text, tried in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub find: Vec<FindView>,
}

/// A kind of document, as the space defines it – the same JSON as in its events and in a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct KindView {
    /// The identity: `a-z`, `0-9`, `.`, `-`, `_`; at most 64.
    pub id: String,
    /// The name people read.
    pub name: String,
    /// The name in other languages, by language tag (`de`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    /// Its fields, in the order they are shown.
    pub fields: Vec<FieldDefView>,
    /// Words and phrases that speak for the kind; none, and it is only ever chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WeightedView>,
    /// The fields a suggested title is made of after the name, the first with a value of each group.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<Vec<String>>,
}

impl From<Kind> for KindView {
    fn from(kind: Kind) -> Self {
        // The same JSON on both sides.
        serde_json::from_value(serde_json::to_value(kind).expect("a kind is JSON")).expect("the same JSON")
    }
}

impl From<KindView> for Kind {
    fn from(view: KindView) -> Self {
        serde_json::from_value(serde_json::to_value(view).expect("a kind is JSON")).expect("the same JSON")
    }
}

/// A new kind: only its name; fields and rules are added by changing it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewKind {
    /// The name people read.
    pub name: String,
}

/// Kinds to take over – a template, or the kinds of another space. A template file of `aktujo/catalog` has the same
/// shape; other members of it are left alone.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KindImport {
    /// The kinds.
    pub kinds: Vec<KindView>,
}

/// How many kinds an import changed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Imported {
    /// The kinds new or different from before.
    pub changed: usize,
}

/// A source of templates on this device.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceView {
    /// Its address.
    pub url: String,
    /// Whether it is read.
    pub on: bool,
    /// Whether it is the official catalog – it can be switched off, not removed.
    pub default: bool,
}

impl From<aktujo_app::Source> for SourceView {
    fn from(source: aktujo_app::Source) -> Self {
        SourceView { default: source.is_default(), url: source.url, on: source.on }
    }
}

/// A source to add.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewSource {
    /// A repository on GitHub, or any HTTPS address of a folder with a `catalog.json`.
    pub url: String,
}

/// A source switched on or off.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceSwitch {
    /// Its address.
    pub url: String,
    /// Whether it is read.
    pub on: bool,
}

/// The source to remove.
#[derive(Debug, Clone, Deserialize, utoipa::IntoParams)]
pub struct SourceQuery {
    /// Its address.
    pub url: String,
}

/// A source as it was read for the catalog view.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadSource {
    /// Its address.
    pub url: String,
    /// Its name, if it could be read.
    pub name: Option<String>,
    /// What it offers.
    pub description: Option<String>,
    /// Why it could not be read.
    pub error: Option<String>,
}

/// A kind as a catalog lists it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListedKindView {
    /// Its identity.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its name in other languages.
    pub names: BTreeMap<String, String>,
}

/// How a template stands in a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StandingView {
    /// None of its kinds came from it.
    New,
    /// Its kinds came from it as it is now.
    Taken,
    /// Its kinds came from an earlier version.
    Update,
}

/// A template of a source, with how it stands in the space.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TemplateListing {
    /// The source.
    pub source: String,
    /// Whether it is official: by `aktujo`, from the official catalog.
    pub official: bool,
    /// Its ID, `<author>.<name>`.
    pub id: String,
    /// Who made it.
    pub author: String,
    /// Its licence.
    pub license: Option<String>,
    /// Its name.
    pub name: String,
    /// Its name in other languages.
    pub names: BTreeMap<String, String>,
    /// What it is for.
    pub description: String,
    /// That in other languages.
    pub descriptions: BTreeMap<String, String>,
    /// Its tags.
    pub tags: Vec<String>,
    /// Its kinds.
    pub kinds: Vec<ListedKindView>,
    /// How it stands in the space.
    pub standing: StandingView,
}

/// The templates of the sources that are on, for a space – read when asked.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CatalogView {
    /// The sources that were read, with what went wrong.
    pub sources: Vec<ReadSource>,
    /// Their templates.
    pub templates: Vec<TemplateListing>,
}

/// A template of a source.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TemplateChoice {
    /// The source's address.
    pub source: String,
    /// The template's ID.
    pub template: String,
    /// What to do with each kind, by its identity; `take` for one not named.
    #[serde(default)]
    pub choices: BTreeMap<String, ChoiceView>,
}

/// What to do with a kind of a template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceView {
    /// What the plan proposes: add, replace, merge; a kind gone from the template is kept.
    Take,
    /// The template's kind as it is.
    Theirs,
    /// The space's kind as it is; a new one is not added.
    Keep,
    /// A kind gone from the template is removed.
    Remove,
}

impl From<ChoiceView> for aktujo_app::Choice {
    fn from(choice: ChoiceView) -> Self {
        match choice {
            ChoiceView::Take => aktujo_app::Choice::Take,
            ChoiceView::Theirs => aktujo_app::Choice::Theirs,
            ChoiceView::Keep => aktujo_app::Choice::Keep,
            ChoiceView::Remove => aktujo_app::Choice::Remove,
        }
    }
}

/// What taking a template over does to a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepView {
    /// Added.
    New,
    /// Already as the template has it.
    Same,
    /// Replaced by the template's.
    Replace,
    /// Changed by a person: merged.
    Merge,
    /// No longer in the template: offered to remove.
    Gone,
}

/// An item both changed differently: the space's version stays.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConflictView {
    /// What it is: `name`, `names.de`, `field.total`, `word.invoice`, `title`.
    pub item: String,
    /// The space's version; `null` where the space removed it.
    #[schema(value_type = Object)]
    pub mine: serde_json::Value,
    /// The template's version; `null` where the template removed it.
    #[schema(value_type = Object)]
    pub theirs: serde_json::Value,
}

/// One kind of a plan.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlannedView {
    /// Its identity.
    pub id: String,
    /// What happens to it.
    pub step: StepView,
    /// The space's kind.
    pub mine: Option<KindView>,
    /// The template's kind.
    pub theirs: Option<KindView>,
    /// The kind as it would be.
    pub proposed: Option<KindView>,
    /// Items both changed differently.
    pub conflicts: Vec<ConflictView>,
}

impl From<aktujo_app::Planned> for PlannedView {
    fn from(planned: aktujo_app::Planned) -> Self {
        PlannedView {
            id: planned.id,
            step: match planned.step {
                aktujo_app::Step::New => StepView::New,
                aktujo_app::Step::Same => StepView::Same,
                aktujo_app::Step::Replace => StepView::Replace,
                aktujo_app::Step::Merge => StepView::Merge,
                aktujo_app::Step::Gone => StepView::Gone,
            },
            mine: planned.mine.map(KindView::from),
            theirs: planned.theirs.map(KindView::from),
            proposed: planned.proposed.map(KindView::from),
            conflicts: planned
                .conflicts
                .into_iter()
                .map(|c| ConflictView { item: c.item, mine: c.mine, theirs: c.theirs })
                .collect(),
        }
    }
}

/// What taking a template over would do.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlanView {
    /// The source.
    pub source: String,
    /// The template.
    pub template: String,
    /// Every kind of the template, then those gone from it.
    pub kinds: Vec<PlannedView>,
}

/// What happens to a file of a watched folder once it was taken.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AfterView {
    /// It stays where it is.
    #[default]
    Leave,
    /// It is moved into the folder `added` inside the watched one.
    Move,
}

impl From<AfterView> for aktujo_app::After {
    fn from(after: AfterView) -> Self {
        match after {
            AfterView::Leave => aktujo_app::After::Leave,
            AfterView::Move => aktujo_app::After::Move,
        }
    }
}

/// A watched folder of this device.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WatchView {
    /// The space its files go to.
    pub space: String,
    /// The folder.
    pub folder: String,
    /// What happens to a file once it was taken.
    pub after: AfterView,
    /// Whether it is looked at.
    pub on: bool,
    /// When it was last looked at, in milliseconds since 1970, since the program started.
    pub looked_at: Option<u64>,
    /// How many files were taken from it since the program started.
    pub taken: usize,
    /// Why the last look failed, if it did.
    pub problem: Option<String>,
}

impl WatchView {
    /// A watched folder and how the last look at it went.
    pub fn of(watch: aktujo_app::Watch, watched: Option<aktujo_app::Watched>) -> Self {
        WatchView {
            space: watch.space.to_string(),
            folder: watch.folder.to_string_lossy().into_owned(),
            after: match watch.after {
                aktujo_app::After::Leave => AfterView::Leave,
                aktujo_app::After::Move => AfterView::Move,
            },
            on: watch.on,
            looked_at: watched.as_ref().map(|w| w.at),
            taken: watched.as_ref().map_or(0, |w| w.taken),
            problem: watched.and_then(|w| w.problem),
        }
    }
}

/// A folder to watch.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewWatch {
    /// The space its files go to.
    pub space: String,
    /// The folder, as an absolute path on this device.
    pub folder: String,
    /// What happens to a file once it was taken; by default it stays.
    #[serde(default)]
    pub after: AfterView,
}

/// A watched folder switched on or off.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WatchSwitch {
    /// The space.
    pub space: String,
    /// The folder, as it is listed.
    pub folder: String,
    /// Whether it is looked at.
    pub on: bool,
    /// What happens to a file once it was taken.
    #[serde(default)]
    pub after: AfterView,
}

/// The watched folder to stop watching.
#[derive(Debug, Clone, Deserialize, utoipa::IntoParams)]
pub struct WatchQuery {
    /// The space.
    pub space: String,
    /// The folder, as it is listed.
    pub folder: String,
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

/// The name people read for this device.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceName {
    /// Not empty, at most 100 characters.
    pub name: String,
}

/// Where to synchronise a space through.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StartSync {
    /// The address of the relay, e.g. `http://192.168.1.10:8072`.
    pub relay: String,
}

/// The recovery code of a space: shown once, kept on paper.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecoveryCodeView {
    /// The code.
    pub recovery_code: String,
}

/// What a synchronisation did.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SyncReportView {
    /// Events and originals received.
    pub received: usize,
    /// Events and originals sent.
    pub sent: usize,
    /// Devices added by invitation, by name.
    pub added: Vec<String>,
    /// Whether this device still waits to be added.
    pub waiting: bool,
}

impl From<aktujo_app::SyncReport> for SyncReportView {
    fn from(report: aktujo_app::SyncReport) -> Self {
        SyncReportView { received: report.received, sent: report.sent, added: report.added, waiting: report.waiting }
    }
}

/// What may be done in a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RightsView {
    /// Read.
    Read,
    /// Read and write.
    Write,
    /// Read, write, and add and remove members.
    Manage,
}

impl From<RightsView> for aktujo_app::Rights {
    fn from(rights: RightsView) -> Self {
        match rights {
            RightsView::Read => aktujo_app::Rights::Read,
            RightsView::Write => aktujo_app::Rights::Write,
            RightsView::Manage => aktujo_app::Rights::Manage,
        }
    }
}

impl From<aktujo_app::Rights> for RightsView {
    fn from(rights: aktujo_app::Rights) -> Self {
        match rights {
            aktujo_app::Rights::Read => RightsView::Read,
            aktujo_app::Rights::Write => RightsView::Write,
            aktujo_app::Rights::Manage => RightsView::Manage,
        }
    }
}

/// An invitation to make.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewInvitation {
    /// What the invited device may do.
    pub rights: RightsView,
}

/// An invitation: text to pass on, good for a day.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvitationView {
    /// The invitation – the relay's address with the space and a secret.
    pub invitation: String,
}

/// Joining a space.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Join {
    /// The invitation as it was passed on.
    pub invitation: String,
}

/// Bringing a space back with its recovery code.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Recover {
    /// The address of the relay.
    pub relay: String,
    /// The identity of the space.
    pub space: String,
    /// The recovery code.
    pub code: String,
}

/// A member of a space.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MemberView {
    /// Its identity.
    pub id: String,
    /// What people call it.
    pub name: Option<String>,
    /// The device name it writes under; none for the recovery code.
    pub device: Option<String>,
    /// `device`, `backend` or `recovery`.
    pub role: String,
    /// What it may do.
    pub rights: RightsView,
}

impl From<aktujo_app::Member> for MemberView {
    fn from(member: aktujo_app::Member) -> Self {
        MemberView {
            id: member.id.to_string(),
            name: member.name,
            device: member.device.map(|d| d.as_str().to_owned()),
            role: serde_json::to_value(member.role)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default(),
            rights: member.rights.into(),
        }
    }
}
