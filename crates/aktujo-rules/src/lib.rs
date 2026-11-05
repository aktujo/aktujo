// SPDX-License-Identifier: MIT
//! The kind and the fields of a document, read from its text – as **suggestions** for a person to confirm
//! (`docs/rfc/0003-recognition.md`). Input are the positioned lines of `aktujo-extract` and the kinds of a space;
//! output are a suggested kind and values for its fields as `aktujo-doc` writes them, each with a confidence, the
//! rule's name and the line it came from. No I/O.
//!
//! The crate knows no kinds of documents, words or labels: it is the mechanism only. What speaks for a kind and where
//! its fields are is data of the kind ([`Kind`]: words with weights, ways to find each field), defined by the people
//! and programs that use it. A program that needs more brings its own code behind the trait [`Recogniser`];
//! [`Chain`] combines them, and [`text`] has the readers of dates, amounts and labels to build on.

use aktujo_doc::{Document, DocumentEvent, DocumentId, FieldType, Find, Kind, Origin, Source, Value};
use aktujo_extract::{Extraction, Line, Method};
use serde::{Deserialize, Serialize};

pub mod text;

pub use text::DateOrder;

/// Something that recognises documents: [`Rules`], which reads the kinds as data, or a program's own code.
pub trait Recogniser: Send + Sync {
    /// The kind and its fields. With `kind` – one a person confirmed or chose – only the fields of that kind are
    /// read. `kinds` are the kinds of the space.
    fn recognise(&self, kinds: &[Kind], extraction: &Extraction, kind: Option<&str>) -> Recognition;

    /// What to suggest for `document`, as events: with a confirmed kind the fields of that kind, else the kind
    /// recognised and its fields ([`Recognition::events`]).
    fn suggest(&self, kinds: &[Kind], document: &Document, extraction: &Extraction) -> Vec<DocumentEvent> {
        self.recognise(kinds, extraction, document.kind.as_deref()).events(document)
    }
}

/// Several recognisers as one: each reads the document, and their results are merged ([`Recognition::merge`]) –
/// the rules of the kinds first, a program's own code after them.
#[derive(Default)]
pub struct Chain(pub Vec<Box<dyn Recogniser>>);

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Chain({} recognisers)", self.0.len())
    }
}

impl Recogniser for Chain {
    fn recognise(&self, kinds: &[Kind], extraction: &Extraction, kind: Option<&str>) -> Recognition {
        let results: Vec<Recognition> = self.0.iter().map(|r| r.recognise(kinds, extraction, kind)).collect();
        results.into_iter().reduce(Recognition::merge).unwrap_or_default()
    }
}

/// Recognition by the rules the kinds carry: the words that speak for a kind ([`Kind::words`]) and the ways to find
/// each field ([`aktujo_doc::FieldDef::find`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    /// How dates with slashes are read; without an order only those that cannot mean two days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_order: Option<DateOrder>,
}

/// A kind and how much the text speaks for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guess {
    /// The kind.
    pub kind: String,
    /// The sum of the weights of its words in the text; 0 for a kind given, not guessed.
    pub score: f32,
    /// How sure the guess is, 0–1: higher with the score and with the distance to the next kind; 1 for a kind
    /// given.
    pub confidence: f32,
}

/// A value found for a field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Found {
    /// The field.
    pub key: String,
    /// The value, as `aktujo-doc` writes it.
    pub value: Value,
    /// How sure it is, 0–1.
    pub confidence: f32,
    /// The rule that found it, `kind.key` (`invoice.total`) – the name of the source of the suggestion.
    pub rule: String,
    /// The page it is on, from 0.
    pub page: usize,
    /// The line on that page, from 0.
    pub line: usize,
}

/// What a recogniser recognised in a document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Recognition {
    /// The suggested kind, if the text speaks clearly enough for one, or the kind given.
    pub kind: Option<Guess>,
    /// Every kind the text speaks for at all, the most likely first.
    pub kinds: Vec<Guess>,
    /// The fields found for the kind.
    pub fields: Vec<Found>,
}

/// The score a kind needs to be suggested: the sum of the weights of its words in the text.
pub const MIN_SCORE: f32 = 6.0;
/// The name of the rule that suggests a kind.
pub const KIND_RULE: &str = "kind";
/// The pages that are read for the kind.
const KIND_PAGES: usize = 2;
/// Confidences of the ways to find a value.
const SAME_LINE: f32 = 0.8;
const NEIGHBOUR: f32 = 0.6;
const LARGEST: f32 = 0.7;
const FIRST: f32 = 0.35;
const MARKED: f32 = 0.6;
const TOPMOST: f32 = 0.3;
const PRESENT: f32 = 0.6;
/// A page read by OCR is less sure.
const OCR: f32 = 0.9;
/// Text values are cut at this many characters.
const MAX_TEXT: usize = 120;

impl Rules {
    /// How much the text of the first pages speaks for each of `kinds`: every kind with a score above 0, the most
    /// likely first.
    pub fn classify(&self, kinds: &[Kind], extraction: &Extraction) -> Vec<Guess> {
        let text: Vec<String> = extraction.pages.iter().take(KIND_PAGES).map(|p| p.text()).collect();
        let text = text::normalise(&text.join("\n"));
        let mut guesses: Vec<Guess> = kinds
            .iter()
            .map(|kind| {
                let score: u32 = kind
                    .words
                    .iter()
                    .filter(|w| {
                        let word = text::normalise(&w.word);
                        word.trim() != "" && text.contains(&word)
                    })
                    .map(|w| w.weight)
                    .sum();
                Guess { kind: kind.id.clone(), score: score as f32, confidence: 0.0 }
            })
            .filter(|g| g.score > 0.0)
            .collect();
        guesses.sort_by(|a, b| b.score.total_cmp(&a.score));
        let second = guesses.get(1).map_or(0.0, |g| g.score);
        if let Some(best) = guesses.first_mut() {
            best.confidence = ((1.0 - second / best.score) * (best.score / (2.0 * MIN_SCORE)).min(1.0)).clamp(0.0, 1.0);
        }
        guesses
    }

    /// The fields of `kind` that it says how to find.
    pub fn fields(&self, kind: &Kind, extraction: &Extraction) -> Vec<Found> {
        let reader = Reader { extraction, currency: text::main_currency(&extraction.text()), order: self.date_order };
        kind.fields
            .iter()
            .filter_map(|field| {
                let shape = Shape { field_type: field.field_type, code: field.code };
                let (value, confidence, page, line) = field.find.iter().find_map(|find| reader.find(find, shape))?;
                let factor = if extraction.pages[page].method == Method::Ocr { OCR } else { 1.0 };
                Some(Found {
                    key: field.key.clone(),
                    value,
                    confidence: confidence * factor,
                    rule: format!("{}.{}", kind.id, field.key),
                    page,
                    line,
                })
            })
            .collect()
    }
}

impl Recogniser for Rules {
    /// Without a kind given: the kind with the highest score if it reaches [`MIN_SCORE`] and no other kind has the
    /// same, then the fields of that kind.
    fn recognise(&self, kinds: &[Kind], extraction: &Extraction, kind: Option<&str>) -> Recognition {
        let (kind, guesses) = match kind {
            Some(kind) => (Some(Guess { kind: kind.to_owned(), score: 0.0, confidence: 1.0 }), vec![]),
            None => {
                let guesses = self.classify(kinds, extraction);
                let best = guesses
                    .first()
                    .filter(|g| g.score >= MIN_SCORE && guesses.get(1).is_none_or(|s| s.score < g.score))
                    .cloned();
                (best, guesses)
            }
        };
        let fields = kind
            .as_ref()
            .and_then(|g| kinds.iter().find(|k| k.id == g.kind))
            .map(|def| self.fields(def, extraction))
            .unwrap_or_default();
        Recognition { kind, kinds: guesses, fields }
    }
}

impl Recognition {
    /// Two results as one. The kind is the one more sure (on a tie this one's); the scores of a kind seen by both
    /// add up. Of the fields, only those found for that kind count, and of the same field the value more sure (on a
    /// tie this one's).
    pub fn merge(self, other: Recognition) -> Recognition {
        let mine = self.kind.as_ref().map_or(-1.0, |g| g.confidence);
        let theirs = other.kind.as_ref().map_or(-1.0, |g| g.confidence);
        let (kind, from) =
            if theirs > mine { (other.kind.clone(), [&other, &self]) } else { (self.kind.clone(), [&self, &other]) };
        let same = |r: &Recognition| r.kind.as_ref().map(|g| &g.kind) == kind.as_ref().map(|g| &g.kind);
        let mut fields: Vec<Found> = vec![];
        for result in from.into_iter().filter(|r| same(r)) {
            for found in &result.fields {
                match fields.iter_mut().find(|f| f.key == found.key) {
                    Some(f) if found.confidence > f.confidence => *f = found.clone(),
                    Some(_) => {}
                    None => fields.push(found.clone()),
                }
            }
        }
        let mut kinds: Vec<Guess> = self.kinds.clone();
        for guess in &other.kinds {
            match kinds.iter_mut().find(|g| g.kind == guess.kind) {
                Some(g) => g.score += guess.score,
                None => kinds.push(guess.clone()),
            }
        }
        kinds.sort_by(|a, b| b.score.total_cmp(&a.score));
        Recognition { kind, kinds, fields }
    }

    /// The suggestions for `document`, with the rules as their source: the kind (rule `kind`), unless a person
    /// confirmed one or it is suggested already, and the fields without a confirmed value whose suggestion is not
    /// already the same value. If a person confirmed another kind than the one here, nothing is suggested – read the
    /// document again with that kind ([`Recogniser::suggest`]).
    pub fn events(&self, document: &Document) -> Vec<DocumentEvent> {
        let recognised = self.kind.as_ref().map(|g| g.kind.as_str());
        if document.kind.is_some() && document.kind.as_deref() != recognised {
            return vec![];
        }
        let mut events = vec![];
        if let Some(kind) = recognised
            && document.kind.is_none()
            && document.kind_suggestion.as_ref().is_none_or(|s| s.kind != kind)
        {
            events.push(DocumentEvent::DocumentKind {
                document: document.id.clone(),
                kind: Some(kind.to_owned()),
                origin: Origin::Suggested { source: Source::Rule { name: KIND_RULE.into() } },
            });
        }
        events.extend(
            self.fields
                .iter()
                .filter(|found| {
                    document.fields.get(&found.key).is_none_or(|field| {
                        field.confirmed.is_none() && field.suggestion.as_ref().is_none_or(|s| s.value != found.value)
                    })
                })
                .map(|found| suggestion(&document.id, found)),
        );
        events
    }

    /// A title from the kind's name and the fields its title is made of ([`Kind::title`]), in `language` if the
    /// kind has a name in it: `Letter · Example Ltd · 2026-11-05`. The program offers it while the title is still
    /// the file name; `document_title` has no origin, so a person agrees to it.
    pub fn title(&self, kinds: &[Kind], language: &str) -> Option<String> {
        let kind = self.kind.as_ref().and_then(|g| kinds.iter().find(|k| k.id == g.kind))?;
        let first = |keys: &Vec<String>| {
            keys.iter().find_map(|key| {
                self.fields.iter().find(|f| &f.key == key).and_then(|f| match &f.value {
                    Value::Text(t) | Value::Date(t) | Value::Number(t) => Some(t.clone()),
                    _ => None,
                })
            })
        };
        let parts: Vec<String> =
            std::iter::once(kind.name_in(language).to_owned()).chain(kind.title.iter().filter_map(first)).collect();
        Some(parts.join(" · "))
    }
}

/// A found value as a suggestion for `document`, with the rule as its source.
pub fn suggestion(document: &DocumentId, found: &Found) -> DocumentEvent {
    DocumentEvent::DocumentField {
        document: document.clone(),
        key: found.key.clone(),
        value: found.value.clone(),
        origin: Origin::Suggested { source: Source::Rule { name: found.rule.clone() } },
    }
}

/// What kind of value a field takes.
#[derive(Clone, Copy)]
struct Shape {
    field_type: FieldType,
    code: bool,
}

/// Finds values in the lines of a document.
struct Reader<'a> {
    extraction: &'a Extraction,
    /// The currency named most often, for amounts without one.
    currency: Option<&'static str>,
    order: Option<DateOrder>,
}

/// A value, its confidence, its page and line.
type Hit = (Value, f32, usize, usize);

impl Reader<'_> {
    fn lines(&self) -> impl Iterator<Item = (usize, usize, &Line)> {
        self.extraction
            .pages
            .iter()
            .enumerate()
            .flat_map(|(p, page)| page.lines.iter().enumerate().map(move |(l, line)| (p, l, line)))
    }

    fn find(&self, find: &Find, shape: Shape) -> Option<Hit> {
        match find {
            Find::Label { labels } => labels.iter().find_map(|label| {
                self.lines().find_map(|(p, l, line)| {
                    let at = text::find_label(&line.text, label)?;
                    self.after(p, l, line, at, shape)
                })
            }),
            Find::Largest { labels } => {
                let mut values = vec![];
                for label in labels {
                    for (p, l, line) in self.lines() {
                        let Some(at) = text::find_label(&line.text, label) else { continue };
                        let found = self.all(&line.text[at.end..], shape);
                        if found.is_empty() {
                            values.extend(self.after(p, l, line, at, shape));
                        } else {
                            values.extend(found.into_iter().map(|v| (v, LARGEST, p, l)));
                        }
                    }
                }
                values
                    .into_iter()
                    .max_by(|a, b| text::amount(&a.0).total_cmp(&text::amount(&b.0)))
                    .map(|(v, _, p, l)| (v, LARGEST, p, l))
            }
            Find::First => {
                let page = self.extraction.pages.first()?;
                page.lines
                    .iter()
                    .enumerate()
                    .find_map(|(l, line)| self.value(&line.text, shape).map(|v| (v, FIRST, 0, l)))
            }
            Find::Sender { markers } => self.sender(markers),
            Find::Present { phrases } => self.lines().find_map(|(p, l, line)| {
                phrases.iter().any(|ph| text::contains(&line.text, ph)).then_some((Value::Flag(true), PRESENT, p, l))
            }),
        }
    }

    /// The value after the label at `at` in `line`: on the same line, else on the line to its right, else right below.
    fn after(&self, p: usize, l: usize, line: &Line, at: std::ops::Range<usize>, shape: Shape) -> Option<Hit> {
        if let Some(value) = self.value(&line.text[at.end..], shape) {
            return Some((value, SAME_LINE, p, l));
        }
        let lines = &self.extraction.pages[p].lines;
        // Where the label is on the line, by its share of the characters.
        let chars = line.text.chars().count().max(1) as f32;
        let share = |byte: usize| line.text[..byte].chars().count() as f32 / chars;
        let label_x0 = line.x0 + (line.x1 - line.x0) * share(at.start);
        let label_x1 = line.x0 + (line.x1 - line.x0) * share(at.end);
        let centre = (line.y0 + line.y1) / 2.0;
        let height = line.height().max(0.005);
        let right = lines
            .iter()
            .enumerate()
            .filter(|(i, o)| *i != l && o.x0 >= label_x1 - 0.005 && ((o.y0 + o.y1) / 2.0 - centre).abs() < height * 0.6)
            .min_by(|a, b| a.1.x0.total_cmp(&b.1.x0));
        let below = lines
            .iter()
            .enumerate()
            .filter(|(i, o)| {
                *i != l
                    && o.y0 >= line.y1 - height * 0.3
                    && o.y0 - line.y1 < height * 2.0
                    && o.x0 < label_x1 + 0.1
                    && o.x1 > label_x0
            })
            .min_by(|a, b| a.1.y0.total_cmp(&b.1.y0));
        [right, below].into_iter().flatten().find_map(|(i, o)| self.value(&o.text, shape).map(|v| (v, NEIGHBOUR, p, i)))
    }

    /// The first value of the shape in `text`.
    fn value(&self, text: &str, shape: Shape) -> Option<Value> {
        self.all(text, shape).into_iter().next()
    }

    /// Every value of the shape in `text`, in order.
    fn all(&self, line: &str, shape: Shape) -> Vec<Value> {
        match shape.field_type {
            FieldType::Date => text::dates(line, self.order).into_iter().map(Value::Date).collect(),
            FieldType::Money => text::amounts(line, self.currency),
            FieldType::Number => text::number(line).into_iter().map(Value::Number).collect(),
            FieldType::Text if shape.code => text::code_word(line).into_iter().map(Value::Text).collect(),
            FieldType::Text => text::clean(line)
                .map(|t| Value::Text(t.chars().take(MAX_TEXT).collect::<String>().trim_end().to_owned()))
                .into_iter()
                .collect(),
            FieldType::Flag => vec![],
        }
    }

    /// The sender on the first page: a marked line in the top third, then in the footer; else the topmost
    /// line of words.
    fn sender(&self, markers: &[String]) -> Option<Hit> {
        let page = self.extraction.pages.first()?;
        let lines: Vec<(usize, &Line)> = page.lines.iter().enumerate().collect();
        let top = lines.iter().filter(|(_, l)| l.y0 < 0.35);
        let footer = lines.iter().filter(|(_, l)| l.y0 > 0.85);
        for (i, line) in top.chain(footer) {
            for marker in markers {
                if let Some(at) = text::find_label(&line.text, marker)
                    && let Some(name) = text::clean(text::segment(&line.text, at.start))
                {
                    return Some((Value::Text(name), MARKED, 0, *i));
                }
            }
        }
        lines
            .iter()
            .filter(|(_, l)| l.y0 < 0.25)
            .find(|(_, l)| {
                let letters = l.text.chars().filter(|c| c.is_alphabetic()).count();
                let digits = l.text.chars().filter(char::is_ascii_digit).count();
                letters >= 3 && digits * 2 < letters && l.words() <= 8 && text::dates(&l.text, self.order).is_empty()
            })
            .and_then(|(i, l)| text::clean(&l.text).map(|t| (Value::Text(t), TOPMOST, 0, *i)))
    }
}

#[cfg(test)]
mod tests;
