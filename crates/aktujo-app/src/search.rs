// SPDX-License-Identifier: AGPL-3.0-or-later
//! Finding documents (`docs/rfc/0004-archive-0.1.md`, decisions 2 and 3): a full-text index in SQLite FTS5 over the
//! title, the labels, the kind and the field values and the text read from the newest version – and filters by space,
//! kind, label, date and the bin over the state in memory.
//!
//! The index is a view: tables of the program in the database of the data folder, beside the core's and encrypted
//! with it. It is brought up to date for each space whenever its state is folded – on opening and after every change
//! or synchronisation – one document at a time, by a stamp of what was indexed; a change of [`FORMAT`] builds it
//! anew. Nothing in it is needed to rebuild it: it is made from the events and the text kept by hash.

use std::collections::BTreeMap;

use aktujo_doc::{Document, Value, builtin};
use aktujo_store::rusqlite::{self, Connection, params};

use crate::{AppError, DocumentId, Sha256Hash, SpaceId, Workspace};

/// The version of the index; another one in the database builds it anew.
const FORMAT: &str = "1";
/// The device setting the version is kept under.
const FORMAT_KEY: &str = "search.format";

const SCHEMA: &str = "
CREATE VIRTUAL TABLE search USING fts5(
    title, labels, fields, text,
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TABLE search_stamps (
    space TEXT NOT NULL,
    document TEXT NOT NULL,
    row INTEGER NOT NULL,         -- the rowid in `search`
    stamp TEXT NOT NULL,          -- sha256:… of what was indexed; '' to try again
    PRIMARY KEY (space, document)
) STRICT;
";

/// The weights of the columns for the order of hits: title, labels, fields, text.
const RANK: &str = "bm25(search, 10.0, 5.0, 3.0, 1.0)";

/// Where in the bin to look.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Bin {
    /// Documents outside the bin.
    #[default]
    Outside,
    /// Documents in the bin.
    Inside,
    /// Both.
    Both,
}

/// What to look for. The default finds every document outside the bin.
#[derive(Debug, Clone, Default)]
pub struct Search {
    /// Words, each to be found at the start of a word in the title, the labels, the kind, the field values or the
    /// text; `"two words"` in double quotes as a phrase. Case and accents do not count. Empty: no words.
    pub words: String,
    /// The spaces to look in; empty: all.
    pub spaces: Vec<SpaceId>,
    /// The kind – confirmed, or else suggested.
    pub kind: Option<String>,
    /// Labels a document must all have.
    pub labels: Vec<String>,
    /// The first day, `YYYY-MM-DD`, of the document's date ([`date_of`]).
    pub from: Option<String>,
    /// The last day, `YYYY-MM-DD`, of the document's date.
    pub to: Option<String>,
    /// Where in the bin to look.
    pub bin: Bin,
    /// At most this many hits; `None`: all.
    pub limit: Option<usize>,
}

/// A part of an excerpt: text, and whether it is what was looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// The text.
    pub text: String,
    /// Whether it matched a word looked for.
    pub hit: bool,
}

/// A document found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Its space.
    pub space: SpaceId,
    /// The document.
    pub document: DocumentId,
    /// Where the words were found, a few words around them – empty when no words were given.
    pub excerpt: Vec<Part>,
}

/// The date of a document: the value of its field `date` – confirmed, or else suggested – or else the day it was
/// added (UTC), `YYYY-MM-DD`.
pub fn date_of(document: &Document) -> String {
    match document.fields.get("date").and_then(|f| f.value()) {
        Some(Value::Date(day)) => day.clone(),
        _ => day_of(document.added.wall_ms),
    }
}

/// The day (UTC) of milliseconds since 1970, `YYYY-MM-DD` – after H. Hinnant, "chrono-Compatible Low-Level Date
/// Algorithms", `civil_from_days`.
fn day_of(ms: u64) -> String {
    let z = (ms / 86_400_000) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The words of a search as an FTS5 query: each word or phrase quoted and matched at the start of words, all of them
/// needed. `None` when there are no words.
fn fts_query(words: &str) -> Option<String> {
    let mut terms = Vec::new();
    for (i, part) in words.split('"').enumerate() {
        let quoted = i % 2 == 1;
        let pieces: Vec<&str> = if quoted { vec![part] } else { part.split_whitespace().collect() };
        for piece in pieces {
            if piece.split_whitespace().next().is_none() {
                continue;
            }
            terms.push(format!("\"{}\"*", piece.split_whitespace().collect::<Vec<_>>().join(" ")));
        }
    }
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

/// Markers of hits in an excerpt, as SQLite gives it; control characters that are not in indexed text.
const START: char = '\u{1}';
const END: char = '\u{2}';

/// An excerpt in parts, on one line.
fn excerpt(snippet: &str) -> Vec<Part> {
    let snippet = snippet.replace('\n', " ");
    let mut parts = Vec::new();
    for (i, piece) in snippet.split(START).enumerate() {
        let (hit, rest) = match piece.split_once(END) {
            Some((hit, rest)) if i > 0 => (hit, rest),
            _ => ("", piece),
        };
        if !hit.is_empty() {
            parts.push(Part { text: hit.to_owned(), hit: true });
        }
        if !rest.is_empty() {
            parts.push(Part { text: rest.to_owned(), hit: false });
        }
    }
    parts
}

/// What is indexed of a document, but its text.
struct Entry {
    title: String,
    labels: String,
    fields: String,
    text: Option<Sha256Hash>,
}

impl Entry {
    fn of(document: &Document, kinds: &[aktujo_doc::Kind]) -> Self {
        let mut fields = Vec::new();
        let kind = document.kind.as_ref().or(document.kind_suggestion.as_ref().map(|s| &s.kind));
        if let Some(kind) = kind {
            fields.push(kinds.iter().find(|k| &k.id == kind).map_or(kind.clone(), |k| format!("{} {}", k.id, k.name)));
        }
        for field in document.fields.values() {
            match field.value() {
                Some(Value::Text(text) | Value::Date(text) | Value::Number(text)) => fields.push(text.clone()),
                Some(Value::Money { amount, currency }) => fields.push(format!("{amount} {currency}")),
                Some(Value::Flag(_)) | None => {}
            }
        }
        Entry {
            title: document.title.clone(),
            labels: document.labels.iter().cloned().collect::<Vec<_>>().join("\n"),
            fields: fields.join("\n"),
            text: document.current().read.as_ref().and_then(|r| r.text),
        }
    }

    fn stamp(&self) -> String {
        let text = self.text.map(|t| t.to_string()).unwrap_or_default();
        let joined = [self.title.as_str(), &self.labels, &self.fields, &text].join("\u{0}");
        Sha256Hash::of(joined.as_bytes()).to_string()
    }
}

/// Makes the tables of the index, anew when they are of another format.
pub(crate) fn prepare(workspace: &Workspace) -> Result<(), AppError> {
    let store = workspace.replica.store();
    if store.setting(None, FORMAT_KEY)?.as_deref() == Some(FORMAT) {
        return Ok(());
    }
    let connection = store.connection();
    connection.execute_batch("DROP TABLE IF EXISTS search; DROP TABLE IF EXISTS search_stamps;")?;
    connection.execute_batch(SCHEMA)?;
    store.set_setting(None, FORMAT_KEY, FORMAT)?;
    Ok(())
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        AppError::Database(error.into())
    }
}

fn stamps(connection: &Connection, space: &SpaceId) -> Result<BTreeMap<String, (i64, String)>, AppError> {
    let mut statement = connection.prepare("SELECT document, row, stamp FROM search_stamps WHERE space = ?1")?;
    let rows = statement.query_map(params![space.as_str()], |row| Ok((row.get(0)?, (row.get(1)?, row.get(2)?))))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

impl Workspace {
    /// Brings the index of a space up to date with its state.
    pub(crate) fn index(&self, space: &SpaceId) -> Result<(), AppError> {
        let view = self.view(space)?;
        let connection = self.replica.store().connection();
        let mut stamps = stamps(connection, space)?;
        let kinds = builtin();
        let transaction = connection.unchecked_transaction()?;
        for document in view.documents.active().chain(view.documents.trashed()) {
            let entry = Entry::of(document, &kinds);
            let mut stamp = entry.stamp();
            let previous = stamps.remove(document.id.as_str());
            if previous.as_ref().is_some_and(|(_, s)| *s == stamp) {
                continue;
            }
            if let Some((row, _)) = previous {
                transaction.execute("DELETE FROM search WHERE rowid = ?1", params![row])?;
            }
            let text = match entry.text {
                // A text not known or not readable is left out; it is not tried again until the stamp changes.
                Some(hash) => match self.stored_text(&hash) {
                    Ok(Some(extraction)) => extraction.text(),
                    // Its bytes have not arrived yet: tried again at the next change.
                    Ok(None) => {
                        stamp.clear();
                        String::new()
                    }
                    Err(_) => String::new(),
                },
                None => String::new(),
            };
            transaction.execute(
                "INSERT INTO search (title, labels, fields, text) VALUES (?1, ?2, ?3, ?4)",
                params![entry.title, entry.labels, entry.fields, text],
            )?;
            transaction.execute(
                "INSERT OR REPLACE INTO search_stamps (space, document, row, stamp) VALUES (?1, ?2, ?3, ?4)",
                params![space.as_str(), document.id.as_str(), transaction.last_insert_rowid(), stamp],
            )?;
        }
        for (document, (row, _)) in stamps {
            transaction.execute("DELETE FROM search WHERE rowid = ?1", params![row])?;
            transaction.execute(
                "DELETE FROM search_stamps WHERE space = ?1 AND document = ?2",
                params![space.as_str(), document],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Finds documents: with words, the best hits first; without, the newest first.
    pub fn search(&self, search: &Search) -> Result<Vec<Hit>, AppError> {
        for day in [&search.from, &search.to].into_iter().flatten() {
            Value::Date(day.clone()).check()?;
        }
        for space in &search.spaces {
            self.view(space)?;
        }
        let wanted = |space: &SpaceId, document: &Document| {
            (search.spaces.is_empty() || search.spaces.contains(space))
                && match search.bin {
                    Bin::Outside => !document.trashed,
                    Bin::Inside => document.trashed,
                    Bin::Both => true,
                }
                && search.kind.as_ref().is_none_or(|kind| {
                    document.kind.as_ref().or(document.kind_suggestion.as_ref().map(|s| &s.kind)) == Some(kind)
                })
                && search.labels.iter().all(|label| document.labels.contains(label))
                && (search.from.is_none() && search.to.is_none() || {
                    let date = date_of(document);
                    search.from.as_ref().is_none_or(|from| date >= *from)
                        && search.to.as_ref().is_none_or(|to| date <= *to)
                })
        };
        let limit = search.limit.unwrap_or(usize::MAX);
        let mut hits = Vec::new();
        let Some(query) = fts_query(&search.words) else {
            let mut found: Vec<(&SpaceId, &Document)> = Vec::new();
            for (space, view) in &self.views {
                found.extend(
                    view.documents
                        .active()
                        .chain(view.documents.trashed())
                        .filter(|d| wanted(space, d))
                        .map(|d| (space, d)),
                );
            }
            found.sort_by(|a, b| b.1.added.cmp(&a.1.added));
            return Ok(found
                .into_iter()
                .take(limit)
                .map(|(space, d)| Hit { space: space.clone(), document: d.id.clone(), excerpt: Vec::new() })
                .collect());
        };
        let connection = self.replica.store().connection();
        let mut statement = connection.prepare(&format!(
            "SELECT s.space, s.document, snippet(search, -1, '{START}', '{END}', '…', 12)
             FROM search JOIN search_stamps s ON s.row = search.rowid
             WHERE search MATCH ?1 ORDER BY {RANK}"
        ))?;
        let mut rows = statement.query(params![query])?;
        while hits.len() < limit
            && let Some(row) = rows.next()?
        {
            let (space, document, snippet): (String, String, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
            let (Ok(space), Ok(document)) = (space.parse::<SpaceId>(), document.parse::<DocumentId>()) else {
                continue;
            };
            let Some(found) = self.views.get(&space).and_then(|v| v.documents.get(&document)) else { continue };
            if wanted(&space, found) {
                hits.push(Hit { space, document, excerpt: excerpt(&snippet) });
            }
        }
        Ok(hits)
    }

    /// The labels used in a space, with how many documents outside the bin have each – for filters.
    pub fn labels(&self, space: &SpaceId) -> Result<BTreeMap<String, usize>, AppError> {
        let mut labels = BTreeMap::new();
        for document in self.view(space)?.documents.active() {
            for label in &document.labels {
                *labels.entry(label.clone()).or_insert(0) += 1;
            }
        }
        Ok(labels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewFile, tests::Scratch};

    #[test]
    fn days_are_counted_from_1970() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(1_793_836_800_000), "2026-11-05");
        assert_eq!(day_of(1_709_251_199_000), "2024-02-29");
        assert_eq!(day_of(1_709_251_200_000), "2024-03-01");
    }

    #[test]
    fn words_become_a_query_that_cannot_break() {
        assert_eq!(fts_query("  "), None);
        assert_eq!(fts_query("Miete juli"), Some("\"Miete\"* AND \"juli\"*".into()));
        assert_eq!(fts_query("\"electricity bill\" 2026"), Some("\"electricity bill\"* AND \"2026\"*".into()));
        assert_eq!(fts_query("a\"b OR c"), Some("\"a\"* AND \"b OR c\"*".into()));
        assert_eq!(fts_query("NOT * ( )"), Some("\"NOT\"* AND \"*\"* AND \"(\"* AND \")\"*".into()));
    }

    #[test]
    fn excerpts_mark_the_hits() {
        let parts = excerpt("…the \u{1}rent\u{2}\nfor \u{1}July\u{2}");
        let marked: Vec<(&str, bool)> = parts.iter().map(|p| (p.text.as_str(), p.hit)).collect();
        assert_eq!(marked, [("…the ", false), ("rent", true), (" for ", false), ("July", true)]);
    }

    fn file<'a>(name: &'a str, bytes: &'a [u8]) -> NewFile<'a> {
        NewFile { name, media_type: "application/pdf", bytes }
    }

    fn found(workspace: &Workspace, search: Search) -> Vec<DocumentId> {
        workspace.search(&search).unwrap().into_iter().map(|h| h.document).collect()
    }

    fn words(workspace: &Workspace, words: &str) -> Vec<DocumentId> {
        found(workspace, Search { words: words.into(), ..Search::default() })
    }

    #[test]
    fn titles_labels_kinds_and_fields_are_found_and_filtered() {
        let mut workspace = Workspace::in_memory("laptop").unwrap();
        let home = workspace.found_space("Home").unwrap();
        let work = workspace.found_space("Business").unwrap();
        let lease = workspace.add_document(&home, file("Mietvertrag Wohnung.pdf", b"1")).unwrap().document;
        let bill = workspace.add_document(&home, file("Stromrechnung.pdf", b"2")).unwrap().document;
        let tax = workspace.add_document(&work, file("Steuerbescheid.pdf", b"3")).unwrap().document;

        workspace.add_label(&home, &lease, "Wohnung").unwrap();
        workspace.add_label(&home, &bill, "Wohnung").unwrap();
        workspace.add_label(&home, &bill, "Strom").unwrap();
        workspace.set_kind(&home, &bill, Some("invoice")).unwrap();
        workspace.set_field(&home, &bill, "date", Value::Date("2026-07-01".into())).unwrap();
        workspace.set_field(&home, &bill, "issuer", Value::Text("Stadtwerke Köln".into())).unwrap();

        // Prefixes, case and accents do not count; every word is needed.
        assert_eq!(words(&workspace, "miet"), vec![lease.clone()]);
        assert_eq!(words(&workspace, "koln"), vec![bill.clone()]);
        assert_eq!(words(&workspace, "INVOICE stadt"), vec![bill.clone()]);
        assert_eq!(words(&workspace, "invoice miete"), Vec::<DocumentId>::new());
        // The title weighs more than a label.
        assert_eq!(words(&workspace, "wohnung"), [lease.clone(), bill.clone()]);

        let all = workspace.search(&Search::default()).unwrap();
        assert_eq!(all.iter().map(|h| &h.document).collect::<Vec<_>>(), [&tax, &bill, &lease]);
        assert_eq!(found(&workspace, Search { spaces: vec![work.clone()], ..Search::default() }), vec![tax.clone()]);
        assert_eq!(found(&workspace, Search { kind: Some("invoice".into()), ..Search::default() }), vec![bill.clone()]);
        assert_eq!(
            found(&workspace, Search { labels: vec!["Wohnung".into(), "Strom".into()], ..Search::default() }),
            vec![bill.clone()]
        );
        let july = Search { from: Some("2026-07-01".into()), to: Some("2026-07-31".into()), ..Search::default() };
        assert_eq!(found(&workspace, july), vec![bill.clone()]);
        assert_eq!(found(&workspace, Search { limit: Some(1), ..Search::default() }), vec![tax.clone()]);
        assert!(matches!(
            workspace.search(&Search { from: Some("July".into()), ..Search::default() }),
            Err(AppError::Value(_))
        ));
        assert_eq!(workspace.labels(&home).unwrap(), BTreeMap::from([("Strom".into(), 1), ("Wohnung".into(), 2)]));

        // A change is found at once; the bin is looked in only when asked.
        workspace.set_title(&home, &lease, "Lease of the flat").unwrap();
        assert!(words(&workspace, "miet").is_empty());
        workspace.trash(&home, &lease).unwrap();
        assert!(words(&workspace, "lease").is_empty());
        let in_bin = Search { words: "lease".into(), bin: Bin::Inside, ..Search::default() };
        assert_eq!(found(&workspace, in_bin), vec![lease.clone()]);
        assert_eq!(found(&workspace, Search { bin: Bin::Both, ..Search::default() }).len(), 3);
    }

    #[test]
    fn the_index_follows_a_data_folder_and_is_built_anew() {
        let scratch = Scratch::new("search");
        let keys = crate::MemoryKeys::default();
        let id = {
            let mut workspace = Workspace::open(&scratch.0, &keys).unwrap();
            let space = workspace.found_space("Home").unwrap();
            workspace.add_document(&space, file("Kaufvertrag.pdf", b"1")).unwrap().document
        };
        let workspace = Workspace::open(&scratch.0, &keys).unwrap();
        assert_eq!(words(&workspace, "kaufvertrag"), vec![id.clone()]);
        // An index of another format is dropped and built again from the state.
        workspace.replica.store().set_setting(None, FORMAT_KEY, "0").unwrap();
        workspace.replica.store().connection().execute("DELETE FROM search", []).unwrap();
        drop(workspace);
        let workspace = Workspace::open(&scratch.0, &keys).unwrap();
        assert_eq!(words(&workspace, "kaufvertrag"), [id]);
    }
}
