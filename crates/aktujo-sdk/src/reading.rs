// SPDX-License-Identifier: MIT
//! Reading in the background (`docs/rfc/0004-archive-0.1.md`, decisions 3, 4 and 6): the text of each document's
//! newest version is read with `aktujo-extract`, kept as an original of its own by hash – the positioned lines as JSON
//! – and linked to the version by a `document_read` event; then `aktujo-rules` suggests the kind and the fields.
//!
//! The queue is the state itself: a version waits while it has no reading – or, for a reader that can use OCR, while
//! its reading was made without OCR and left pages unread. So nothing is lost when the program stops, and a version
//! read by `aktujo-minimal` is read again by the full build where that helps. Reading happens outside the lock of the
//! workspace: [`Workspace::next_to_read`] hands out a [`Job`], [`Reader::read`] reads it, and
//! [`Workspace::finish_reading`] writes what came of it. [`Worker`] does this on a thread of its own.

use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
};

use aktujo_doc::{Document, DocumentEvent};
use aktujo_extract::{Extraction, Extractor, Method, Options, Page, Pure};
use aktujo_rules::{Recogniser, Rules};
use serde::{Deserialize, Serialize};

use crate::{AppError, DocumentId, Sha256Hash, SpaceId, Workspace};

/// The format of the text that was read, as it is kept.
const TEXT_FORMAT: u32 = 1;

/// What was read, as it is kept: the pages with their positioned lines.
#[derive(Debug, Serialize, Deserialize)]
struct StoredText {
    format: u32,
    pages: Vec<Page>,
}

/// Where a document is in reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadState {
    /// Its newest version waits to be read.
    Waiting,
    /// It is being read now.
    Reading,
    /// It was read.
    Done,
    /// Reading failed.
    Failed,
}

/// A version to read, with its bytes.
#[derive(Clone)]
pub struct Job {
    /// The space.
    pub space: SpaceId,
    /// The document.
    pub document: DocumentId,
    /// The hash of the version's original.
    pub version: Sha256Hash,
    /// Its media type.
    pub media_type: String,
    bytes: Vec<u8>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job").field("document", &self.document).field("version", &self.version).finish_non_exhaustive()
    }
}

/// What reads text and recognises documents.
pub struct Reader {
    extractor: Box<dyn Extractor>,
    recogniser: Box<dyn Recogniser>,
    options: Options,
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader").field("ocr", &self.ocr()).finish_non_exhaustive()
    }
}

impl Reader {
    /// A reader of the given extractor and recogniser.
    pub fn new(extractor: Box<dyn Extractor>, recogniser: Box<dyn Recogniser>) -> Self {
        Reader { extractor, recogniser, options: Options::default() }
    }

    /// The reader of this build: the backend `pure` with the models built in, if the build has OCR, and the rules
    /// of the kinds of each space.
    pub fn builtin() -> Result<Self, AppError> {
        #[cfg(feature = "ocr")]
        let pure = Pure::with_models(crate::models::MODELS).map_err(|e| AppError::Reading(e.to_string()))?;
        #[cfg(not(feature = "ocr"))]
        let pure = Pure::new();
        Ok(Reader::new(Box::new(pure), Box::new(Rules::default())))
    }

    /// A reader of text layers only, with the rules of the kinds – what `aktujo-minimal` reads with.
    pub fn text_layer() -> Self {
        Reader::new(Box::new(Pure::new()), Box::new(Rules::default()))
    }

    /// Whether this reader can use OCR.
    pub fn ocr(&self) -> bool {
        self.extractor.abilities().ocr
    }

    /// Reads a job – the slow part, outside the workspace.
    pub fn read(&self, job: &Job) -> Result<Extraction, String> {
        self.extractor.extract(&job.bytes, &job.media_type, &self.options).map_err(|e| e.to_string())
    }
}

/// Whether the newest version of a document waits for a reader that can (`ocr`) or cannot use OCR.
fn waits(document: &Document, ocr: bool) -> bool {
    match &document.current().read {
        None => true,
        Some(reading) => ocr && !reading.ocr && (reading.unread > 0 || reading.error.is_some()),
    }
}

impl Workspace {
    /// Where a document is in reading, as a reader of this build sees it.
    pub fn read_state(&self, space: &SpaceId, id: &DocumentId) -> Result<ReadState, AppError> {
        let document = self.document(space, id)?;
        if self.reading.as_ref().is_some_and(|(s, d)| s == space && d == id) {
            return Ok(ReadState::Reading);
        }
        Ok(match &document.current().read {
            _ if waits(document, cfg!(feature = "ocr")) => ReadState::Waiting,
            Some(reading) if reading.error.is_some() => ReadState::Failed,
            _ => ReadState::Done,
        })
    }

    /// The next version to read, the documents added first first: of documents outside the bin, in spaces this
    /// device may write in, whose original is here. It counts as being read until [`Workspace::finish_reading`].
    pub fn next_to_read(&mut self, ocr: bool) -> Result<Option<Job>, AppError> {
        let mut waiting: Vec<(&SpaceId, &Document)> = Vec::new();
        for (space, view) in &self.views {
            if self.may_write(space).is_ok() {
                waiting.extend(view.documents.active().filter(|d| waits(d, ocr)).map(|d| (space, d)));
            }
        }
        waiting.sort_by(|a, b| a.1.added.cmp(&b.1.added));
        for (space, document) in waiting {
            let original = &document.current().original;
            if let Some(bytes) = self.replica.original(&original.hash)? {
                let job = Job {
                    space: space.clone(),
                    document: document.id.clone(),
                    version: original.hash,
                    media_type: original.media_type.clone(),
                    bytes,
                };
                self.reading = Some((job.space.clone(), job.document.clone()));
                return Ok(Some(job));
            }
        }
        Ok(None)
    }

    /// Writes what reading a job gave: the text as an original, the `document_read` event, and – for the newest
    /// version – the suggestions of the reader's rules.
    pub fn finish_reading(
        &mut self,
        job: Job,
        result: Result<Extraction, String>,
        reader: &Reader,
    ) -> Result<(), AppError> {
        self.reading = None;
        let document = self.document(&job.space, &job.document)?;
        if !document.versions.iter().any(|v| v.original.hash == job.version) {
            return Ok(());
        }
        let ocr = reader.ocr();
        let event = match &result {
            Ok(extraction) => {
                let stored = StoredText { format: TEXT_FORMAT, pages: extraction.pages.clone() };
                let json = serde_json::to_vec(&stored).expect("text as JSON");
                let text = self.replica.put_original(&job.space, &json)?;
                DocumentEvent::DocumentRead {
                    document: job.document.clone(),
                    version: job.version,
                    text: Some(text),
                    ocr,
                    pages: extraction.pages.len() as u32,
                    unread: extraction.pages.iter().filter(|p| p.method == Method::None).count() as u32,
                    error: None,
                }
            }
            Err(error) => DocumentEvent::DocumentRead {
                document: job.document.clone(),
                version: job.version,
                text: None,
                ocr,
                pages: 0,
                unread: 0,
                error: Some(error.clone()),
            },
        };
        self.write(&job.space, &event)?;
        let Ok(extraction) = result else { return Ok(()) };
        let document = self.document(&job.space, &job.document)?;
        if document.current().original.hash != job.version {
            return Ok(());
        }
        let suggestions = reader.recogniser.suggest(&self.kinds(&job.space)?, document, &extraction);
        for suggestion in suggestions {
            self.write(&job.space, &suggestion)?;
        }
        Ok(())
    }

    /// The text read from the newest version of a document, if it was read.
    pub fn read_text(&self, space: &SpaceId, id: &DocumentId) -> Result<Option<Extraction>, AppError> {
        let document = self.document(space, id)?;
        let Some(hash) = document.current().read.as_ref().and_then(|r| r.text) else {
            return Ok(None);
        };
        self.stored_text(&hash)
    }

    /// The text kept under `hash`, if its bytes are on this device.
    pub(crate) fn stored_text(&self, hash: &Sha256Hash) -> Result<Option<Extraction>, AppError> {
        let Some(bytes) = self.replica.original(hash)? else {
            return Ok(None);
        };
        let stored: StoredText =
            serde_json::from_slice(&bytes).map_err(|e| AppError::Reading(format!("the text {hash}: {e}")))?;
        if stored.format != TEXT_FORMAT {
            return Err(AppError::Reading(format!("format {} of the text {hash} is not known", stored.format)));
        }
        Ok(Some(Extraction { pages: stored.pages }))
    }

    /// A better title than the file name, from the kind's name (in `language`, if it has one) and the fields its
    /// title is made of – offered while the title is still the file's; a person takes it with
    /// [`Workspace::set_title`].
    pub fn suggested_title(
        &self,
        space: &SpaceId,
        id: &DocumentId,
        reader: &Reader,
        language: &str,
    ) -> Result<Option<String>, AppError> {
        let document = self.document(space, id)?;
        if document.title != aktujo_core::title_of_file(&document.versions[0].original.name) {
            return Ok(None);
        }
        let Some(extraction) = self.read_text(space, id)? else {
            return Ok(None);
        };
        let kinds = self.kinds(space)?;
        let kind = document.kind.as_deref().or(document.kind_suggestion.as_ref().map(|s| s.kind.as_str()));
        let title = reader.recogniser.recognise(&kinds, &extraction, kind).title(&kinds, language);
        Ok(title.and_then(|t| aktujo_core::title(&t).ok()).filter(|t| *t != document.title))
    }
}

/// Reads every version that waits, one after the other, the workspace locked only between them. Returns how many
/// were read.
pub fn read_all(workspace: &Mutex<Workspace>, reader: &Reader) -> Result<usize, AppError> {
    let mut count = 0;
    loop {
        let job = workspace.lock().expect("not poisoned").next_to_read(reader.ocr())?;
        let Some(job) = job else { return Ok(count) };
        let result = reader.read(&job);
        workspace.lock().expect("not poisoned").finish_reading(job, result, reader)?;
        count += 1;
    }
}

/// Reading on a thread of its own: it reads what waits when it starts and whenever it is woken – after adding, after
/// a sync. It stops when dropped, after the document it is reading.
#[derive(Debug)]
pub struct Worker {
    wake: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
    problem: Arc<Mutex<Option<String>>>,
}

impl Worker {
    /// Starts reading for `workspace` with `reader`.
    pub fn start(workspace: Arc<Mutex<Workspace>>, reader: Reader) -> Self {
        let (wake, woken) = mpsc::channel::<()>();
        let problem = Arc::new(Mutex::new(None));
        let seen = problem.clone();
        let thread = thread::Builder::new()
            .name("aktujo-reading".into())
            .spawn(move || {
                loop {
                    let result = read_all(&workspace, &reader);
                    *seen.lock().expect("not poisoned") = result.err().map(|e| e.to_string());
                    if woken.recv().is_err() {
                        return;
                    }
                    while woken.try_recv().is_ok() {}
                }
            })
            .expect("a thread starts");
        Worker { wake: Some(wake), thread: Some(thread), problem }
    }

    /// Reads what waits now.
    pub fn wake(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.send(());
        }
    }

    /// Why the last round of reading stopped, if it failed.
    pub fn problem(&self) -> Option<String> {
        self.problem.lock().expect("not poisoned").clone()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.wake = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, NewFile, Value};

    /// A one-page PDF; `lines` in Helvetica from the top, or a grey box without text if there are none.
    fn pdf(lines: &[&str]) -> Vec<u8> {
        let content = if lines.is_empty() {
            "0.5 g 100 100 200 200 re f".to_owned()
        } else {
            let shown: Vec<String> = lines.iter().map(|l| format!("({l}) Tj 0 -30 Td")).collect();
            format!("BT /F1 18 Tf 72 720 Td {} ET", shown.join(" "))
        };
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R \
             >> >> >>"
                .to_owned(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![];
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        out.extend(
            format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes(),
        );
        out
    }

    fn file<'a>(name: &'a str, bytes: &'a [u8]) -> NewFile<'a> {
        NewFile { name, media_type: "application/pdf", bytes }
    }

    const INVOICE: [&str; 6] = [
        "Stadtwerke Musterstadt GmbH",
        "Invoice",
        "Invoice number: RE-2026-0042",
        "Invoice date: 05.11.2026",
        "Total amount: 119.00 EUR",
        "Thank you for your order.",
    ];

    /// A kind as a person or a template defines it – a space has none of its own.
    fn invoice() -> Kind {
        serde_json::from_value(serde_json::json!({
            "id": "invoice", "name": "Invoice", "names": {"de": "Rechnung"},
            "fields": [
                {"key": "issuer", "name": "Issuer", "type": "text", "find": [{"way": "sender", "markers": ["gmbh"]}]},
                {"key": "number", "name": "Number", "type": "text", "code": true,
                 "find": [{"way": "label", "labels": ["invoice number"]}]},
                {"key": "date", "name": "Date", "type": "date", "find": [{"way": "label", "labels": ["invoice date"]}]},
                {"key": "total", "name": "Total", "type": "money",
                 "find": [{"way": "largest", "labels": ["total amount"]}]}
            ],
            "words": [{"word": "invoice", "weight": 6}, {"word": "invoice number", "weight": 4}],
            "title": [["issuer"], ["date"]]
        }))
        .unwrap()
    }

    #[test]
    fn without_kinds_nothing_is_suggested() {
        let workspace = Mutex::new(Workspace::in_memory("laptop").unwrap());
        let (space, id) = {
            let mut w = workspace.lock().unwrap();
            let space = w.found_space("Home").unwrap();
            (space.clone(), w.add_document(&space, file("scan.pdf", &pdf(&INVOICE))).unwrap().document)
        };
        assert_eq!(read_all(&workspace, &Reader::text_layer()).unwrap(), 1);
        let w = workspace.lock().unwrap();
        let document = w.document(&space, &id).unwrap();
        assert_eq!((&document.kind_suggestion, document.fields.len()), (&None, 0));
    }

    #[test]
    fn a_document_is_read_and_its_kind_and_fields_suggested() {
        let workspace = Mutex::new(Workspace::in_memory("laptop").unwrap());
        let reader = Reader::text_layer();
        let (space, id) = {
            let mut w = workspace.lock().unwrap();
            let space = w.found_space("Home").unwrap();
            assert_eq!(w.import_kinds(&space, vec![invoice()]).unwrap(), 1);
            let id = w.add_document(&space, file("scan_0001.pdf", &pdf(&INVOICE))).unwrap().document;
            assert_eq!(w.read_state(&space, &id).unwrap(), ReadState::Waiting);
            (space, id)
        };
        assert_eq!(read_all(&workspace, &reader).unwrap(), 1);
        assert_eq!(read_all(&workspace, &reader).unwrap(), 0, "nothing is read twice");

        let w = workspace.lock().unwrap();
        assert_eq!(w.read_state(&space, &id).unwrap(), ReadState::Done);
        let document = w.document(&space, &id).unwrap();
        let reading = document.current().read.clone().unwrap();
        assert_eq!((reading.pages, reading.unread, reading.ocr, reading.error), (1, 0, false, None));
        let text = w.read_text(&space, &id).unwrap().unwrap();
        assert_eq!(text.pages[0].method, Method::TextLayer);
        assert!(text.text().contains("Total amount: 119.00 EUR"), "{}", text.text());
        // The text is kept as an original by its hash, so that it synchronises.
        assert!(w.original(&reading.text.unwrap()).is_ok());

        assert_eq!(document.kind_suggestion.as_ref().map(|s| s.kind.as_str()), Some("invoice"));
        let total = document.fields.get("total").and_then(|f| f.suggestion.as_ref()).map(|s| &s.value);
        assert_eq!(total, Some(&Value::Money { amount: "119.00".into(), currency: "EUR".into() }));
        assert_eq!(document.kind, None, "suggested, not confirmed");
        let title = w.suggested_title(&space, &id, &reader, "en").unwrap().unwrap();
        assert_eq!(title, "Invoice · Stadtwerke Musterstadt GmbH · 2026-11-05");
        let title = w.suggested_title(&space, &id, &reader, "de").unwrap().unwrap();
        assert!(title.starts_with("Rechnung · "), "{title}");

        // What was read is found, with the words around it.
        let search = crate::Search { words: "RE-2026-0042".into(), ..crate::Search::default() };
        let hits = w.search(&search).unwrap();
        assert_eq!(hits.iter().map(|h| &h.document).collect::<Vec<_>>(), [&id]);
        assert!(hits[0].excerpt.iter().any(|p| p.hit && p.text == "RE-2026-0042"), "{:?}", hits[0].excerpt);
        let search = crate::Search { words: "thank order".into(), ..crate::Search::default() };
        assert_eq!(w.search(&search).unwrap().len(), 1);
    }

    #[test]
    fn a_page_without_text_waits_for_ocr_and_failures_are_kept() {
        let workspace = Mutex::new(Workspace::in_memory("laptop").unwrap());
        let (space, scan, broken) = {
            let mut w = workspace.lock().unwrap();
            let space = w.found_space("Home").unwrap();
            let scan = w.add_document(&space, file("scan.pdf", &pdf(&[]))).unwrap().document;
            let broken = w.add_document(&space, file("broken.pdf", b"not a PDF")).unwrap().document;
            (space, scan, broken)
        };
        assert_eq!(read_all(&workspace, &Reader::text_layer()).unwrap(), 2);
        {
            let w = workspace.lock().unwrap();
            let reading = w.document(&space, &scan).unwrap().current().read.clone().unwrap();
            assert_eq!((reading.pages, reading.unread, reading.ocr), (1, 1, false));
            let failed = w.document(&space, &broken).unwrap().current().read.clone().unwrap();
            assert!(failed.error.is_some() && failed.text.is_none());
            if !cfg!(feature = "ocr") {
                assert_eq!(w.read_state(&space, &broken).unwrap(), ReadState::Failed);
            }
        }
        // A reader with OCR reads both again: the page without text and the one that failed.
        #[cfg(feature = "ocr")]
        {
            let reader = Reader::builtin().unwrap();
            assert!(reader.ocr());
            assert_eq!(read_all(&workspace, &reader).unwrap(), 2);
            assert_eq!(read_all(&workspace, &reader).unwrap(), 0);
            let w = workspace.lock().unwrap();
            assert!(w.document(&space, &scan).unwrap().current().read.as_ref().unwrap().ocr);
            assert_eq!(w.read_state(&space, &scan).unwrap(), ReadState::Done);
            assert_eq!(w.read_state(&space, &broken).unwrap(), ReadState::Failed);
        }
    }

    #[test]
    fn a_new_version_is_read_and_the_bin_is_not() {
        let workspace = Mutex::new(Workspace::in_memory("laptop").unwrap());
        let reader = Reader::text_layer();
        let (space, id) = {
            let mut w = workspace.lock().unwrap();
            let space = w.found_space("Home").unwrap();
            let id = w.add_document(&space, file("a.pdf", &pdf(&["first"]))).unwrap().document;
            let binned = w.add_document(&space, file("b.pdf", &pdf(&["binned"]))).unwrap().document;
            w.trash(&space, &binned).unwrap();
            (space, id)
        };
        assert_eq!(read_all(&workspace, &reader).unwrap(), 1);
        workspace.lock().unwrap().add_version(&space, &id, file("a2.pdf", &pdf(&["second"]))).unwrap();
        assert_eq!(read_all(&workspace, &reader).unwrap(), 1);
        let w = workspace.lock().unwrap();
        assert_eq!(w.read_text(&space, &id).unwrap().unwrap().text(), "second");
        assert!(w.document(&space, &id).unwrap().versions[0].read.is_some());
    }

    #[test]
    fn the_worker_reads_in_the_background() {
        let workspace = Arc::new(Mutex::new(Workspace::in_memory("laptop").unwrap()));
        let worker = Worker::start(workspace.clone(), Reader::text_layer());
        let (space, id) = {
            let mut w = workspace.lock().unwrap();
            let space = w.found_space("Home").unwrap();
            (space.clone(), w.add_document(&space, file("a.pdf", &pdf(&INVOICE))).unwrap().document)
        };
        worker.wake();
        let start = std::time::Instant::now();
        while workspace.lock().unwrap().read_state(&space, &id).unwrap() != ReadState::Done {
            assert!(start.elapsed().as_secs() < 30, "read in time");
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(worker.problem(), None);
        drop(worker);
    }
}
