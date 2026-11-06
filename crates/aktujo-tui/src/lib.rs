// SPDX-License-Identifier: AGPL-3.0-or-later
//! The terminal interface of aktujo: the archive with the keyboard, and with the mouse – a click does what the key or
//! the row under it does, the wheel moves like the arrow keys; `m` hands the mouse back to the terminal (to select
//! text). It works through the workspace of `aktujo-sdk` only, like every other interface, and speaks the language
//! of the environment where it has its texts (`aktujo-locales`), English otherwise.

use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use ratatui::{
    Frame,
    crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, MouseButton,
            MouseEvent, MouseEventKind,
        },
        execute,
    },
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};

use aktujo_sdk::{
    After, AppError, Bin, Document, DocumentId, Entry, FieldDef, FieldType, Kind, NewFile, ReadState, Reader, Rights,
    Search, SpaceInfo, Standing, Syncer, TemplateOrigin, Value, Watcher, Weighted, Worker, Workspace, date_of, kind_id,
};

mod texts;

pub use texts::Texts;

/// Why the interface ended with an error.
#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    /// The terminal.
    #[error("terminal: {0}")]
    Terminal(#[from] std::io::Error),
    /// The workspace.
    #[error(transparent)]
    Workspace(#[from] AppError),
}

/// The name of a field type as it is typed.
fn type_name(field_type: FieldType) -> &'static str {
    match field_type {
        FieldType::Text => "text",
        FieldType::Date => "date",
        FieldType::Money => "money",
        FieldType::Number => "number",
        FieldType::Flag => "flag",
    }
}

/// Edits the kinds of a space by a line of text (`tui.kindsHelp`):
///
/// ```text
/// + Letter                     a new kind
/// - letter                     removes a kind
/// letter = Business letter     renames it
/// letter + sender text Sender  adds a field: key, type (text, date, money, number, flag), name
/// letter - sender              removes a field
/// letter ~ invoice 6           a word that speaks for the kind, with its weight (0 removes it)
/// ```
fn edit_kinds(
    workspace: &mut Workspace,
    space: &aktujo_sdk::SpaceId,
    line: &str,
    texts: &Texts,
) -> Result<String, AppError> {
    let line = line.trim();
    let invalid = || AppError::NotFound(texts.get("tui.kindsHelp"));
    if line.is_empty() {
        return Ok(String::new());
    }
    if let Some(name) = line.strip_prefix('+') {
        let id = workspace.add_kind(space, name.trim())?;
        return Ok(texts.fill("tui.kindAdded", &[("id", &id)]));
    }
    if let Some(id) = line.strip_prefix('-') {
        workspace.remove_kind(space, id.trim())?;
        return Ok(String::new());
    }
    let (id, rest) = line.split_once(' ').ok_or_else(invalid)?;
    let mut kind: Kind = workspace.kind(space, id)?.clone();
    let rest = rest.trim();
    if let Some(name) = rest.strip_prefix('=') {
        kind.name = name.trim().to_owned();
    } else if let Some(field) = rest.strip_prefix('+') {
        let mut parts = field.split_whitespace();
        let key = kind_id(parts.next().ok_or_else(invalid)?);
        let field_type = match parts.next().ok_or_else(invalid)? {
            "text" => FieldType::Text,
            "date" => FieldType::Date,
            "money" => FieldType::Money,
            "number" => FieldType::Number,
            "flag" => FieldType::Flag,
            _ => return Err(invalid()),
        };
        let name: Vec<&str> = parts.collect();
        let name = if name.is_empty() { key.clone() } else { name.join(" ") };
        kind.fields.retain(|f| f.key != key);
        kind.fields.push(FieldDef::new(&key, &name, field_type));
    } else if let Some(key) = rest.strip_prefix('-') {
        let key = key.trim();
        kind.fields.retain(|f| f.key != key);
        for title in &mut kind.title {
            title.retain(|k| k != key);
        }
        kind.title.retain(|t| !t.is_empty());
    } else if let Some(word) = rest.strip_prefix('~') {
        let word = word.trim();
        let (word, weight) = match word.rsplit_once(' ') {
            Some((word, weight)) if weight.parse::<u32>().is_ok() => (word.trim(), weight.parse().expect("a number")),
            _ => (word, 1),
        };
        let word = word.to_lowercase();
        kind.words.retain(|w| w.word != word);
        if weight > 0 {
            kind.words.push(Weighted { word, weight });
        }
    } else {
        return Err(invalid());
    }
    workspace.define_kind(space, kind)?;
    Ok(String::new())
}

/// The actions at the bottom, each with its key and the key of its text – clickable as well.
const ACTIONS: [(char, &str); 28] = [
    ('/', "tui.search"),
    ('a', "tui.add"),
    ('t', "tui.title"),
    ('T', "tui.takeTitle"),
    ('k', "tui.kind"),
    ('c', "tui.chooseKind"),
    ('f', "tui.fields"),
    ('l', "tui.label"),
    ('u', "tui.unlabel"),
    ('d', "tui.trash"),
    ('b', "tui.showBin"),
    ('s', "tui.space"),
    ('n', "tui.newSpace"),
    ('A', "tui.kinds"),
    ('K', "tui.importKinds"),
    ('C', "tui.catalog"),
    ('W', "tui.watch"),
    ('I', "tui.improve"),
    ('E', "tui.export"),
    ('y', "tui.sync"),
    ('S', "tui.startSync"),
    ('i', "tui.invite"),
    ('j', "tui.join"),
    ('M', "tui.members"),
    ('X', "tui.removeMember"),
    ('N', "tui.deviceName"),
    ('m', "tui.mouse"),
    ('q', "tui.quit"),
];

/// The accent of aktujo/brand on dark – sealing-wax red, for areas only: the chosen row, the space and the keys,
/// with white text on it.
const ACCENT: Style = Style::new().bg(Color::Rgb(0xB3, 0x26, 0x2B)).fg(Color::White);

/// What is being typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prompt {
    Search,
    Add,
    Title,
    Label,
    Unlabel,
    NewSpace,
    StartSync,
    Invite,
    Join,
    RemoveMember,
    DeviceName,
    ChooseKind,
    ImportKinds,
    Kinds,
    TakeTemplate,
    Watch,
    Improve,
    Export,
}

impl Prompt {
    fn text(self) -> &'static str {
        match self {
            Prompt::Search => "search",
            Prompt::Add => "tui.path",
            Prompt::Title => "doc.title",
            Prompt::Label => "doc.addLabel",
            Prompt::Unlabel => "tui.unlabel",
            Prompt::NewSpace => "space.name",
            Prompt::StartSync => "sync.relay",
            Prompt::Invite => "tui.rights",
            Prompt::Join => "sync.invitationField",
            Prompt::RemoveMember => "tui.removeMember",
            Prompt::DeviceName => "settings.deviceName",
            Prompt::ChooseKind => "tui.chooseKindPrompt",
            Prompt::ImportKinds => "tui.importKindsPrompt",
            Prompt::Kinds => "tui.kindsPrompt",
            Prompt::TakeTemplate => "tui.catalogPrompt",
            Prompt::Watch => "tui.watchPrompt",
            Prompt::Improve => "tui.improvePrompt",
            Prompt::Export => "tui.exportPrompt",
        }
    }
}

/// A document in the list.
#[derive(Debug, Clone)]
struct Row {
    id: DocumentId,
    title: String,
    date: String,
    /// The name of its kind, confirmed or suggested.
    kind: Option<String>,
    reading: ReadState,
}

/// The state of the interface; drawing it is [`draw`], changing it [`App::key`] and [`App::mouse`].
#[derive(Debug)]
pub struct App {
    workspace: Arc<Mutex<Workspace>>,
    worker: Option<Arc<Worker>>,
    syncer: Option<Arc<Syncer>>,
    watcher: Option<Arc<Watcher>>,
    /// What the right side shows instead of the document: the sync and the members of the space, read from its relay
    /// when asked for – not at every frame.
    members: Option<Vec<Line<'static>>>,
    /// The templates of the catalog view, by the number shown: their source and entry.
    templates: Vec<(String, Entry)>,
    /// For titles offered from what was read: the rules only.
    titles: Reader,
    texts: Texts,
    spaces: Vec<SpaceInfo>,
    space: usize,
    words: String,
    bin: bool,
    rows: Vec<Row>,
    selected: Option<usize>,
    input: Option<(Prompt, String)>,
    message: String,
    /// Whether the interface has the mouse.
    pub mouse: bool,
    /// Whether to end.
    pub quit: bool,
    /// Where the last frame put the list, the spaces and the actions, for clicks.
    list: Rect,
    tabs: Vec<(Rect, usize)>,
    actions: Vec<(Rect, char)>,
}

/// The media type of a file by its extension.
fn media_type(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    match extension.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// A value as it is read.
fn shown(value: &Value, texts: &Texts) -> String {
    match value {
        Value::Text(text) | Value::Date(text) | Value::Number(text) => text.clone(),
        Value::Money { amount, currency } => format!("{amount} {currency}"),
        Value::Flag(true) => texts.get("value.yes"),
        Value::Flag(false) => texts.get("value.no"),
    }
}

fn reading_key(state: ReadState) -> &'static str {
    match state {
        ReadState::Waiting => "reading.waiting",
        ReadState::Reading => "reading.reading",
        ReadState::Done => "reading.done",
        ReadState::Failed => "reading.failed",
    }
}

impl App {
    /// The interface on a workspace, in the language of `texts`; `worker` is woken when files are added, `syncer`
    /// after every change.
    pub fn new(
        workspace: Arc<Mutex<Workspace>>,
        worker: Option<Arc<Worker>>,
        syncer: Option<Arc<Syncer>>,
        texts: Texts,
    ) -> Self {
        let mut app = App {
            workspace,
            worker,
            syncer,
            watcher: None,
            members: None,
            templates: Vec::new(),
            titles: Reader::text_layer(),
            texts,
            spaces: Vec::new(),
            space: 0,
            words: String::new(),
            bin: false,
            rows: Vec::new(),
            selected: None,
            input: None,
            message: String::new(),
            mouse: true,
            quit: false,
            list: Rect::default(),
            tabs: Vec::new(),
            actions: Vec::new(),
        };
        app.reload();
        if app.spaces.is_empty() {
            app.message = app.texts.get("space.first");
            app.input = Some((Prompt::NewSpace, String::new()));
        }
        app
    }

    fn lock(&self) -> MutexGuard<'_, Workspace> {
        self.workspace.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The interface looks at the watched folders through `watcher` as soon as one is added.
    pub fn with_watcher(mut self, watcher: Option<Arc<Watcher>>) -> Self {
        self.watcher = watcher;
        self
    }

    /// Reads the spaces and the documents of the chosen one again, keeping the selected document.
    pub fn reload(&mut self) {
        let chosen = self.selected.and_then(|i| self.rows.get(i)).map(|r| r.id.clone());
        let workspace = self.workspace.clone();
        let workspace = workspace.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.spaces = workspace.spaces().unwrap_or_default();
        self.space = self.space.min(self.spaces.len().saturating_sub(1));
        self.rows.clear();
        if let Some(space) = self.spaces.get(self.space) {
            let kinds = workspace.kinds(&space.id).unwrap_or_default();
            let search = Search {
                words: self.words.clone(),
                spaces: vec![space.id.clone()],
                bin: if self.bin { Bin::Inside } else { Bin::Outside },
                ..Search::default()
            };
            match workspace.search(&search) {
                Ok(hits) => {
                    for hit in hits {
                        let Ok(document) = workspace.document(&hit.space, &hit.document) else { continue };
                        self.rows.push(Row {
                            id: document.id.clone(),
                            title: document.title.clone(),
                            date: date_of(document),
                            kind: document
                                .kind
                                .as_ref()
                                .or(document.kind_suggestion.as_ref().map(|s| &s.kind))
                                .map(|id| kind_name(&kinds, id, self.texts.language())),
                            reading: workspace.read_state(&hit.space, &hit.document).unwrap_or(ReadState::Done),
                        });
                    }
                }
                Err(error) => self.message = error.to_string(),
            }
        }
        self.selected = match chosen.and_then(|id| self.rows.iter().position(|r| r.id == id)) {
            Some(at) => Some(at),
            None if self.rows.is_empty() => None,
            None => Some(self.selected.unwrap_or(0).min(self.rows.len() - 1)),
        };
    }

    /// Runs a command on the selected document and reports what went wrong.
    fn on_document(&mut self, command: impl FnOnce(&mut Workspace, &SpaceInfo, &Document) -> Result<(), AppError>) {
        let (Some(space), Some(row)) = (self.spaces.get(self.space), self.selected.and_then(|i| self.rows.get(i)))
        else {
            return;
        };
        let (space, id) = (space.clone(), row.id.clone());
        let result = {
            let mut workspace = self.lock();
            match workspace.document(&space.id, &id).cloned() {
                Ok(document) => command(&mut workspace, &space, &document),
                Err(error) => Err(error),
            }
        };
        if let Err(error) = result {
            self.message = error.to_string();
        }
        self.changed();
        self.reload();
    }

    fn wake(&self) {
        if let Some(worker) = &self.worker {
            worker.wake();
        }
        self.changed();
    }

    fn changed(&self) {
        if let Some(syncer) = &self.syncer {
            syncer.wake();
        }
    }

    /// Runs a command on the chosen space and shows what it says, or what went wrong.
    fn on_space(&mut self, command: impl FnOnce(&mut Workspace, &SpaceInfo) -> Result<String, AppError>) {
        let Some(space) = self.spaces.get(self.space).cloned() else { return };
        let result = command(&mut self.lock(), &space);
        match result {
            Ok(message) => self.message = message,
            Err(error) => self.message = error.to_string(),
        }
        if let Some(worker) = &self.worker {
            worker.wake();
        }
        self.reload();
        if self.members.is_some() {
            self.members = Some(self.sharing());
        }
    }

    /// Adds a file, or every file in a folder, to the chosen space.
    fn add(&mut self, path: &str) {
        let Some(space) = self.spaces.get(self.space).map(|s| s.id.clone()) else { return };
        let path = Path::new(path.trim());
        let files: Vec<_> = if path.is_dir() {
            match fs::read_dir(path) {
                Ok(entries) => entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect(),
                Err(error) => {
                    self.message = format!("{}: {error}", path.display());
                    return;
                }
            }
        } else {
            vec![path.to_owned()]
        };
        let (mut added, mut known) = (0, 0);
        for file in files {
            let bytes = match fs::read(&file) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.message = format!("{}: {error}", file.display());
                    continue;
                }
            };
            let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let new_file = NewFile { name: &name, media_type: media_type(&file), bytes: &bytes };
            let result = self.lock().add_document(&space, new_file);
            match result {
                Ok(done) if done.new => added += 1,
                Ok(_) => known += 1,
                Err(error) => self.message = error.to_string(),
            }
        }
        if added + known > 0 {
            let mut message = self.texts.fill("doc.added", &[("n", &added.to_string())]);
            if known > 0 {
                message = format!("{message}, {}", self.texts.fill("doc.known", &[("n", &known.to_string())]));
            }
            self.message = message;
        }
        self.wake();
        self.reload();
    }

    fn enter(&mut self, prompt: Prompt, text: String) {
        match prompt {
            Prompt::Search => {
                self.words = text;
                self.selected = Some(0);
                self.reload();
            }
            Prompt::Add => self.add(&text),
            Prompt::Title => self.on_document(|w, s, d| w.set_title(&s.id, &d.id, &text)),
            Prompt::Label => self.on_document(|w, s, d| w.add_label(&s.id, &d.id, &text)),
            Prompt::Unlabel => self.on_document(|w, s, d| w.remove_label(&s.id, &d.id, text.trim())),
            Prompt::StartSync => {
                let texts = self.texts.clone();
                self.on_space(|w, s| {
                    let code = w.start_sync(&s.id, &text, None)?;
                    Ok(texts.fill("tui.codeShown", &[("code", &code)]))
                });
            }
            Prompt::Invite => {
                let rights = match text.trim() {
                    "r" => Rights::Read,
                    "w" => Rights::Write,
                    "m" => Rights::Manage,
                    _ => {
                        self.message = self.texts.get("tui.rights");
                        return;
                    }
                };
                let texts = self.texts.clone();
                self.on_space(|w, s| {
                    let invitation = w.invite(&s.id, rights)?;
                    Ok(texts.fill("tui.invitationShown", &[("invitation", &invitation)]))
                });
            }
            Prompt::Join => {
                let result = self.lock().join(&text, None);
                match result {
                    Ok(id) => {
                        self.message = self.texts.get("sync.waiting");
                        self.reload();
                        self.space = self.spaces.iter().position(|s| s.id == id).unwrap_or(0);
                        self.reload();
                    }
                    Err(error) => self.message = error.to_string(),
                }
            }
            Prompt::RemoveMember => self.on_space(|w, s| w.remove_member(&s.id, text.trim())),
            Prompt::ChooseKind => {
                let language = self.texts.language().to_owned();
                self.on_document(|w, s, d| {
                    let wanted = text.trim().to_lowercase();
                    if wanted.is_empty() {
                        return w.set_kind(&s.id, &d.id, None);
                    }
                    let kinds = w.kinds(&s.id)?;
                    let kind = kinds
                        .iter()
                        .find(|k| k.id == wanted || k.name_in(&language).to_lowercase() == wanted)
                        .ok_or_else(|| AppError::NotFound(format!("no kind {:?}", text.trim())))?;
                    w.set_kind(&s.id, &d.id, Some(&kind.id))
                });
            }
            Prompt::Improve => self.improve(&text),
            Prompt::Export => {
                let Some(space) = self.spaces.get(self.space).cloned() else { return };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64);
                let folder =
                    std::path::Path::new(text.trim()).join(aktujo_sdk::export_name(space.name.as_deref(), now));
                let result = aktujo_sdk::Folder::create(&folder)
                    .map_err(AppError::from)
                    .and_then(|mut sink| aktujo_sdk::export(&self.workspace, &space.id, &mut sink));
                self.message = match result {
                    Ok(exported) => self.texts.fill(
                        "tui.exported",
                        &[("folder", &folder.display().to_string()), ("n", &exported.documents.to_string())],
                    ),
                    Err(error) => error.to_string(),
                };
            }
            Prompt::Watch => {
                let texts = self.texts.clone();
                self.on_space(|w, s| {
                    let folder = text.trim();
                    if folder.is_empty() {
                        for watch in w.watches()?.into_iter().filter(|watch| watch.space == s.id) {
                            w.remove_watch(&s.id, &watch.folder)?;
                        }
                        return Ok(texts.get("tui.watchNone"));
                    }
                    // `…/**`: with the subfolders.
                    let (folder, subfolders) = match folder.strip_suffix("**") {
                        Some(folder) => (folder.trim_end_matches(['/', '\\']), true),
                        None => (folder, false),
                    };
                    let folder = w.add_watch(&s.id, std::path::Path::new(folder), After::Leave, subfolders)?;
                    Ok(texts.fill("tui.watching", &[("folders", &folder.display().to_string())]))
                });
                if let Some(watcher) = &self.watcher {
                    watcher.wake();
                }
            }
            Prompt::TakeTemplate => {
                let chosen = text.trim().parse::<usize>().ok().and_then(|n| self.templates.get(n.wrapping_sub(1)));
                let Some((source, entry)) = chosen.cloned() else {
                    self.message = self.texts.get("tui.catalogNone");
                    return;
                };
                let texts = self.texts.clone();
                let template = aktujo_sdk::fetch_template(&source, &entry);
                self.on_space(|w, s| {
                    let origin = TemplateOrigin { source: Some(source.clone()), id: Some(entry.id.clone()) };
                    let plan = w.plan(&s.id, origin, &template?)?;
                    let changed = w.apply(&s.id, &plan, &Default::default())?;
                    let conflicts: usize = plan.kinds.iter().map(|k| k.conflicts.len()).sum();
                    let values = [("n", changed.to_string()), ("conflicts", conflicts.to_string())];
                    let values: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
                    Ok(texts.fill("tui.catalogTaken", &values))
                });
                self.members = None;
            }
            Prompt::Kinds => {
                let texts = self.texts.clone();
                self.on_space(|w, s| edit_kinds(w, &s.id, &text, &texts));
                self.members = Some(self.kinds_panel());
                self.input = Some((Prompt::Kinds, String::new()));
            }
            Prompt::ImportKinds => {
                let texts = self.texts.clone();
                self.on_space(|w, s| {
                    let path = text.trim();
                    let bytes = std::fs::read(path).map_err(|e| AppError::NotFound(format!("{path}: {e}")))?;
                    let changed = w.import_file(&s.id, &bytes)?;
                    Ok(texts.fill("kinds.imported", &[("n", &changed.to_string())]))
                });
            }
            Prompt::DeviceName => {
                let result = self.lock().set_device_name(&text);
                self.message = match result {
                    Ok(()) => String::new(),
                    Err(error) => error.to_string(),
                };
            }
            Prompt::NewSpace => {
                let result = self.lock().found_space(&text);
                match result {
                    Ok(id) => {
                        self.message.clear();
                        self.reload();
                        self.space = self.spaces.iter().position(|s| s.id == id).unwrap_or(0);
                        self.reload();
                    }
                    Err(error) => self.message = error.to_string(),
                }
            }
        }
    }

    /// Handles a key.
    pub fn key(&mut self, key: KeyEvent) {
        if let Some((prompt, input)) = &mut self.input {
            match key.code {
                KeyCode::Enter => {
                    let (prompt, text) = (*prompt, std::mem::take(input));
                    self.input = None;
                    self.enter(prompt, text);
                }
                KeyCode::Esc => self.input = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            }
            return;
        }
        let count = self.rows.len();
        let prompt = |app: &mut App, prompt: Prompt, text: String| app.input = Some((prompt, text));
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('m') => self.mouse = !self.mouse,
            KeyCode::Char('n') => prompt(self, Prompt::NewSpace, String::new()),
            KeyCode::Char('j') => prompt(self, Prompt::Join, String::new()),
            KeyCode::Char('N') => {
                let name = self.lock().device_name().ok().flatten().unwrap_or_default();
                prompt(self, Prompt::DeviceName, name);
            }
            KeyCode::Char('s') if !self.spaces.is_empty() => {
                self.space = (self.space + 1) % self.spaces.len();
                self.selected = Some(0);
                self.reload();
            }
            _ if self.spaces.is_empty() => self.message = self.texts.get("tui.noSpace"),
            KeyCode::Char('/') => prompt(self, Prompt::Search, self.words.clone()),
            KeyCode::Char('y') => {
                let texts = self.texts.clone();
                self.on_space(|w, s| {
                    let report = w.sync(&s.id)?;
                    let (received, sent) = (report.received.to_string(), report.sent.to_string());
                    let mut message = texts.fill("sync.report", &[("received", &received), ("sent", &sent)]);
                    if !report.added.is_empty() {
                        message =
                            format!("{message} – {}", texts.fill("sync.added", &[("names", &report.added.join(", "))]));
                    }
                    if report.waiting {
                        message = texts.get("sync.waiting");
                    }
                    Ok(message)
                });
            }
            KeyCode::Char('S') => prompt(self, Prompt::StartSync, String::new()),
            KeyCode::Char('i') => prompt(self, Prompt::Invite, String::new()),
            KeyCode::Char('X') => prompt(self, Prompt::RemoveMember, String::new()),
            KeyCode::Char('M') => {
                self.members = match self.members {
                    Some(_) => None,
                    None => Some(self.sharing()),
                }
            }
            KeyCode::Char('a') => prompt(self, Prompt::Add, String::new()),
            KeyCode::Char('K') => prompt(self, Prompt::ImportKinds, String::new()),
            KeyCode::Char('A') if !self.spaces.is_empty() => {
                self.members = Some(self.kinds_panel());
                self.message = self.texts.get("tui.kindsHelp");
                prompt(self, Prompt::Kinds, String::new());
            }
            KeyCode::Char('E') => prompt(self, Prompt::Export, ".".into()),
            KeyCode::Char('W') => {
                let space = self.spaces.get(self.space).map(|s| s.id.clone());
                let watched: Vec<String> = self
                    .lock()
                    .watches()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|w| Some(&w.space) == space.as_ref())
                    .map(|w| w.folder.display().to_string())
                    .collect();
                if !watched.is_empty() {
                    self.message = self.texts.fill("tui.watching", &[("folders", &watched.join(", "))]);
                }
                prompt(self, Prompt::Watch, String::new());
            }
            KeyCode::Char('C') => {
                // Read now, when asked – the sources of this device that are on.
                let lines = self.catalog();
                self.members = Some(lines);
                if !self.templates.is_empty() {
                    prompt(self, Prompt::TakeTemplate, String::new());
                }
            }
            KeyCode::Char('b') => {
                self.bin = !self.bin;
                self.selected = Some(0);
                self.reload();
            }
            KeyCode::Down if count > 0 => self.selected = Some(self.selected.map_or(0, |s| (s + 1).min(count - 1))),
            KeyCode::Up if count > 0 => self.selected = Some(self.selected.map_or(0, |s| s.saturating_sub(1))),
            _ if self.selected.is_none() => {}
            KeyCode::Char('t') => {
                let title = self.selected.and_then(|i| self.rows.get(i)).map(|r| r.title.clone()).unwrap_or_default();
                prompt(self, Prompt::Title, title);
            }
            KeyCode::Char('l') => prompt(self, Prompt::Label, String::new()),
            KeyCode::Char('I') => prompt(self, Prompt::Improve, String::new()),
            KeyCode::Char('c') => {
                let kinds = self.spaces.get(self.space).and_then(|s| self.lock().kinds(&s.id).ok()).unwrap_or_default();
                let language = self.texts.language().to_owned();
                let names: Vec<&str> = kinds.iter().map(|k| k.name_in(&language)).collect();
                self.message = if names.is_empty() { self.texts.get("kinds.none") } else { names.join(", ") };
                prompt(self, Prompt::ChooseKind, String::new());
            }
            KeyCode::Char('u') => prompt(self, Prompt::Unlabel, String::new()),
            KeyCode::Char('T') => {
                let titles = Reader::text_layer();
                let language = self.texts.language().to_owned();
                self.on_document(|w, s, d| match w.suggested_title(&s.id, &d.id, &titles, &language)? {
                    Some(title) => w.set_title(&s.id, &d.id, &title),
                    None => Ok(()),
                });
            }
            KeyCode::Char('k') => self.on_document(|w, s, d| match &d.kind_suggestion {
                Some(suggestion) if d.kind.is_none() => w.set_kind(&s.id, &d.id, Some(&suggestion.kind)),
                _ => Ok(()),
            }),
            KeyCode::Char('f') => self.on_document(|w, s, d| {
                for (key, field) in &d.fields {
                    if let Some(suggestion) = &field.suggestion {
                        w.set_field(&s.id, &d.id, key, suggestion.value.clone())?;
                    }
                }
                Ok(())
            }),
            KeyCode::Char('d') | KeyCode::Delete => {
                self.on_document(|w, s, d| if d.trashed { w.restore(&s.id, &d.id) } else { w.trash(&s.id, &d.id) });
                self.wake();
            }
            _ => {}
        }
    }

    /// Handles the mouse.
    pub fn mouse(&mut self, event: MouseEvent) {
        let at = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollDown => self.key(KeyCode::Down.into()),
            MouseEventKind::ScrollUp => self.key(KeyCode::Up.into()),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(&(_, key)) = self.actions.iter().find(|(area, _)| area.contains(at)) {
                    self.key(KeyCode::Char(key).into());
                } else if let Some(&(_, space)) = self.tabs.iter().find(|(area, _)| area.contains(at)) {
                    self.space = space;
                    self.selected = Some(0);
                    self.reload();
                } else if self.input.is_none() && self.list.contains(at) {
                    let row = usize::from(at.y - self.list.y).checked_sub(1);
                    if let Some(row) = row.filter(|row| *row < self.rows.len()) {
                        self.selected = Some(row);
                    }
                }
            }
            _ => {}
        }
    }

    /// The lines that describe the sync of the chosen space and its members.
    /// Improves the newest version of the chosen document by letters: `d` straighten, `g` grey, `s` black and
    /// white, each `r` a quarter turn of every page clockwise.
    #[cfg(feature = "scan")]
    fn improve(&mut self, letters: &str) {
        let improve = aktujo_sdk::Improve {
            deskew: letters.contains('d'),
            colour: if letters.contains('s') {
                aktujo_sdk::Colour::BlackWhite
            } else if letters.contains('g') {
                aktujo_sdk::Colour::Grey
            } else {
                aktujo_sdk::Colour::Keep
            },
            turns: vec![(letters.matches('r').count() % 4) as u8; 1000],
            ..aktujo_sdk::Improve::default()
        };
        let texts = self.texts.clone();
        self.message.clear();
        self.on_document(|w, s, d| {
            let file = w.version_file(&s.id, &d.id, None)?;
            let written = aktujo_sdk::improved(&file, &improve)?;
            w.add_improved(&s.id, &d.id, &file.name, &written)
        });
        if self.message.is_empty() {
            self.message = texts.get("tui.improved");
        }
        if let Some(worker) = &self.worker {
            worker.wake();
        }
    }

    #[cfg(not(feature = "scan"))]
    fn improve(&mut self, _: &str) {
        self.message = self.texts.get("tui.noImprove");
    }

    /// The templates of the sources that are on, numbered, with how each stands in the space.
    fn catalog(&mut self) -> Vec<Line<'static>> {
        let texts = self.texts.clone();
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut lines = vec![Line::styled(texts.get("catalog.title"), bold)];
        self.templates.clear();
        let Some(space) = self.spaces.get(self.space).map(|s| s.id.clone()) else { return lines };
        let sources = self.lock().sources().unwrap_or_default();
        // Read without the workspace.
        let read: Vec<_> =
            sources.into_iter().filter(|s| s.on).map(|s| (aktujo_sdk::fetch_catalog(&s.url), s)).collect();
        let language = texts.language().to_owned();
        for (catalog, source) in read {
            let catalog = match catalog {
                Ok(catalog) => catalog,
                Err(error) => {
                    let problem = error.to_string();
                    lines.push(Line::from(
                        texts.fill("catalog.unreadable", &[("source", &source.url), ("problem", &problem)]),
                    ));
                    continue;
                }
            };
            for entry in catalog.templates {
                let standing = self.lock().standing(&space, &source.url, &entry).unwrap_or(Standing::New);
                let mark = match standing {
                    Standing::New => String::new(),
                    Standing::Taken => format!(" ({})", texts.get("catalog.standing.taken")),
                    Standing::Update => format!(" ({})", texts.get("catalog.standing.update")),
                };
                let official = source.is_default() && entry.author == aktujo_sdk::OFFICIAL_AUTHOR;
                let by = if official { texts.get("catalog.official") } else { entry.author.clone() };
                let name = entry.names.get(&language).unwrap_or(&entry.name).clone();
                let kinds: Vec<&str> =
                    entry.kinds.iter().map(|k| k.names.get(&language).unwrap_or(&k.name).as_str()).collect();
                let number = self.templates.len() + 1;
                lines.push(Line::from(format!("{number}. {name}{mark} – {by}")));
                lines.push(Line::from(format!("   {}", kinds.join(", "))));
                self.templates.push((source.url.clone(), entry));
            }
        }
        if self.templates.is_empty() {
            lines.push(Line::from(texts.get("catalog.empty")));
        }
        lines
    }

    /// The kinds of the space, with their fields and words – beside the list while they are edited.
    fn kinds_panel(&self) -> Vec<Line<'static>> {
        let texts = &self.texts;
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut lines = vec![Line::styled(texts.get("kinds.title"), bold)];
        let Some(space) = self.spaces.get(self.space) else { return lines };
        let kinds = self.lock().kinds(&space.id).unwrap_or_default();
        if kinds.is_empty() {
            lines.push(Line::from(texts.get("kinds.none")));
        }
        let language = texts.language().to_owned();
        for kind in kinds {
            lines.push(Line::styled(format!("{} ({})", kind.name_in(&language), kind.id), bold));
            for field in &kind.fields {
                lines.push(Line::from(format!(
                    "  {} – {}, {}",
                    field.key,
                    field.name_in(&language),
                    type_name(field.field_type)
                )));
            }
            if !kind.words.is_empty() {
                let words: Vec<String> = kind.words.iter().map(|w| format!("{} {}", w.word, w.weight)).collect();
                lines.push(Line::from(format!("  ≈ {}", words.join(", "))));
            }
        }
        lines
    }

    fn sharing(&self) -> Vec<Line<'static>> {
        let texts = &self.texts;
        let Some(space) = self.spaces.get(self.space) else { return Vec::new() };
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut lines = vec![Line::styled(texts.get("sync.title"), bold)];
        let Some(relay) = &space.relay else {
            lines.push(Line::from(texts.get("sync.notYet")));
            return lines;
        };
        lines.push(Line::from(texts.fill("sync.via", &[("relay", relay)])));
        if let Some(rights) = space.rights {
            let key = match rights {
                Rights::Read => "rights.read",
                Rights::Write => "rights.write",
                Rights::Manage => "rights.manage",
            };
            lines.push(Line::from(texts.get(key)));
        }
        if space.waiting {
            lines.push(Line::from(texts.get("sync.waiting")));
            return lines;
        }
        if let Some(problem) = space.synced.as_ref().and_then(|s| s.problem.as_ref()) {
            lines.push(Line::from(texts.fill("sync.problem", &[("problem", problem)])));
        }
        lines.push(Line::default());
        lines.push(Line::styled(texts.get("sync.members"), bold));
        let (members, device) = {
            let workspace = self.lock();
            (workspace.members(&space.id), workspace.device().to_owned())
        };
        match members {
            Ok(members) => {
                for member in members {
                    let name = match (&member.name, &member.device) {
                        _ if member.role == aktujo_sdk::MemberRole::Recovery => texts.get("sync.code"),
                        (Some(name), _) => name.clone(),
                        (None, Some(device)) => device.as_str().to_owned(),
                        (None, None) => "…".to_owned(),
                    };
                    let mine = member.device.as_ref().is_some_and(|d| d.as_str() == device);
                    let me = if mine { format!(" ({})", texts.get("sync.thisDevice")) } else { String::new() };
                    let rights = match member.rights {
                        Rights::Read => "rights.read",
                        Rights::Write => "rights.write",
                        Rights::Manage => "rights.manage",
                    };
                    lines.push(Line::from(format!("{name}{me} – {}", texts.get(rights))));
                }
            }
            Err(error) => lines.push(Line::from(error.to_string())),
        }
        lines
    }

    /// The lines that describe the selected document.
    fn details(&self) -> Vec<Line<'static>> {
        if let Some(lines) = &self.members {
            return lines.clone();
        }
        let texts = &self.texts;
        let (Some(space), Some(row)) = (self.spaces.get(self.space), self.selected.and_then(|i| self.rows.get(i)))
        else {
            return vec![Line::from(texts.get("doc.none"))];
        };
        let workspace = self.lock();
        let Ok(document) = workspace.document(&space.id, &row.id) else { return Vec::new() };
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let dim = Style::default().add_modifier(Modifier::DIM);
        let suggested = format!(" ({})", texts.get("doc.suggested"));
        let mut lines = vec![Line::styled(document.title.clone(), bold)];
        if let Ok(Some(title)) = workspace.suggested_title(&space.id, &row.id, &self.titles, texts.language()) {
            lines.push(Line::styled(texts.fill("doc.titleSuggestion", &[("title", &title)]), dim));
        }
        if document.trashed {
            lines.push(Line::from(texts.get("doc.inBin")));
        }
        lines.push(Line::from(format!("{}: {}", texts.get("doc.date"), row.date)));
        let kinds = workspace.kinds(&space.id).unwrap_or_default();
        let language = texts.language();
        let kind = match (&document.kind, &document.kind_suggestion) {
            (Some(kind), _) => kind_name(&kinds, kind, language),
            (None, Some(suggestion)) => format!("{}{suggested}", kind_name(&kinds, &suggestion.kind, language)),
            (None, None) => texts.get("doc.noKind"),
        };
        let of_kind = document.kind.as_ref().or(document.kind_suggestion.as_ref().map(|s| &s.kind));
        let of_kind = of_kind.and_then(|id| kinds.iter().find(|k| &k.id == id));
        lines.push(Line::from(format!("{}: {kind}", texts.get("doc.kind"))));
        let mut reading = texts.get(reading_key(row.reading));
        if let Some(read) = &document.current().read {
            if read.ocr {
                reading = format!("{reading}, {}", texts.get("reading.ocr"));
            }
            if read.unread > 0 {
                let values = [("unread", read.unread.to_string()), ("pages", read.pages.to_string())];
                let values: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
                reading = format!("{reading} – {}", texts.fill("reading.unread", &values));
            }
            if let Some(error) = &read.error {
                reading = format!("{reading}: {error}");
            }
        }
        lines.push(Line::styled(reading, dim));
        if !document.fields.is_empty() {
            lines.push(Line::default());
            lines.push(Line::styled(texts.get("doc.fields"), bold));
            for (key, field) in &document.fields {
                let name = of_kind.and_then(|k| k.field(key)).map_or(key.as_str(), |f| f.name_in(language));
                let value = match (&field.confirmed, &field.suggestion) {
                    (_, Some(suggestion)) => format!("{}{suggested}", shown(&suggestion.value, texts)),
                    (Some(confirmed), None) => shown(&confirmed.value, texts),
                    (None, None) => continue,
                };
                lines.push(Line::from(format!("{name}: {value}")));
            }
        }
        if !document.labels.is_empty() {
            lines.push(Line::default());
            let labels: Vec<&str> = document.labels.iter().map(String::as_str).collect();
            lines.push(Line::from(format!("{}: {}", texts.get("doc.labels"), labels.join(", "))));
        }
        lines.push(Line::default());
        lines.push(Line::styled(texts.get("doc.versions"), bold));
        for (i, version) in document.versions.iter().enumerate() {
            let number = texts.fill("doc.version", &[("n", &(i + 1).to_string())]);
            let kb = version.original.size.div_ceil(1024);
            lines.push(Line::from(format!("{number}: {} ({kb} KB)", version.original.name)));
        }
        lines
    }
}

/// The name of the kind `id` in `language`, from the kinds of its space; the identity if the space has no such kind.
fn kind_name(kinds: &[Kind], id: &str, language: &str) -> String {
    kinds.iter().find(|k| k.id == id).map_or(id, |k| k.name_in(language)).to_owned()
}

/// Draws the interface and remembers where the clickable parts are.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let [tabs, body, input, bar] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(3), Constraint::Length(3), Constraint::Length(1)])
            .areas(frame.area());
    let [list, detail] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(body);
    let highlight = ACCENT;

    app.tabs.clear();
    let mut spans = vec![Span::raw(" aktujo  ")];
    let mut x = tabs.x + 9;
    for (i, space) in app.spaces.iter().enumerate() {
        let marker = match (&space.relay, &space.synced) {
            (None, _) => "",
            _ if space.waiting => " …",
            (Some(_), Some(synced)) if synced.problem.is_some() => " !",
            (Some(_), _) => " ⇅",
        };
        let text = format!(" {}{marker} ", space.name.as_deref().unwrap_or("…"));
        let width = text.chars().count() as u16;
        app.tabs.push((Rect::new(x, tabs.y, width, 1), i));
        spans.push(if i == app.space { Span::styled(text, highlight) } else { Span::raw(text) });
        spans.push(Span::raw(" "));
        x += width + 1;
    }
    frame.render_widget(Line::from(spans), tabs);

    let rows: Vec<ListItem> = app
        .rows
        .iter()
        .map(|row| {
            let marker = match row.reading {
                ReadState::Waiting | ReadState::Reading => "… ",
                ReadState::Failed => "! ",
                ReadState::Done => "  ",
            };
            let kind = row.kind.as_deref().map(|k| format!(" · {k}")).unwrap_or_default();
            ListItem::new(format!("{marker}{}  {}{kind}", row.date, row.title))
        })
        .collect();
    let mut title = app.texts.get(if app.bin { "bin.inside" } else { "bin.outside" });
    if !app.words.is_empty() {
        title = format!("{title} – {}: {}", app.texts.get("search"), app.words);
    }
    let mut state = ListState::default().with_selected(app.selected);
    frame.render_stateful_widget(
        List::new(rows).block(Block::bordered().title(format!(" {title} "))).highlight_style(highlight),
        list,
        &mut state,
    );
    app.list = list;
    frame.render_widget(Paragraph::new(app.details()).wrap(Wrap { trim: false }).block(Block::bordered()), detail);

    let text = match &app.input {
        Some((prompt, input)) => {
            format!("{}: {input}▏ ({})", app.texts.get(prompt.text()), app.texts.get("tui.enter"))
        }
        None => app.message.clone(),
    };
    frame.render_widget(Paragraph::new(text).block(Block::bordered()), input);

    app.actions.clear();
    let mut spans = Vec::new();
    let mut x = bar.x;
    for (key, label) in ACTIONS {
        let label = match key {
            'm' if !app.mouse => "tui.mouseOff",
            'b' if app.bin => "tui.showDocuments",
            _ => label,
        };
        let text = format!(" {key} {} ", app.texts.get(label));
        let width = text.chars().count() as u16;
        app.actions.push((Rect::new(x, bar.y, width, 1), key));
        spans.push(Span::styled(text, highlight));
        spans.push(Span::raw(" "));
        x += width + 1;
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }), bar);
}

/// Runs the terminal interface on a workspace until it is left, in the language of the environment.
pub fn run(
    workspace: Arc<Mutex<Workspace>>,
    worker: Option<Arc<Worker>>,
    syncer: Option<Arc<Syncer>>,
    watcher: Option<Arc<Watcher>>,
) -> Result<(), TuiError> {
    let mut app = App::new(workspace, worker, syncer, Texts::from_environment()).with_watcher(watcher);
    let mut terminal = ratatui::try_init()?;
    let mut mouse = false;
    let result = (|| -> Result<(), TuiError> {
        while !app.quit {
            if app.mouse != mouse {
                mouse = app.mouse;
                if mouse {
                    execute!(std::io::stdout(), EnableMouseCapture)?;
                } else {
                    execute!(std::io::stdout(), DisableMouseCapture)?;
                }
            }
            terminal.draw(|frame| draw(frame, &mut app))?;
            // Without input, the list is read again each second – so that reading in the background shows.
            if !event::poll(Duration::from_secs(1))? {
                app.reload();
                continue;
            }
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.key(key),
                Event::Mouse(event) if app.mouse => app.mouse(event),
                _ => {}
            }
        }
        Ok(())
    })();
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, crossterm::event::KeyModifiers};

    use super::*;

    fn click(column: u16, row: u16) -> MouseEvent {
        MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column, row, modifiers: KeyModifiers::NONE }
    }

    fn typed(app: &mut App, text: &str) {
        for c in text.chars() {
            app.key(KeyCode::Char(c).into());
        }
        app.key(KeyCode::Enter.into());
    }

    fn screen(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>() + "\n")
            .collect()
    }

    #[test]
    fn the_archive_with_keys_and_clicks() {
        let folder = std::env::temp_dir().join(format!("aktujo-tui-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("Mietvertrag.pdf"), b"%PDF-1.7 lease").unwrap();
        fs::write(folder.join("Stromrechnung.pdf"), b"%PDF-1.7 bill").unwrap();

        let workspace = Arc::new(Mutex::new(Workspace::in_memory("laptop").unwrap()));
        let mut app = App::new(workspace.clone(), None, None, Texts::of("de"));
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        // The first start asks for a space.
        assert_eq!(app.input.as_ref().map(|(p, _)| *p), Some(Prompt::NewSpace));
        typed(&mut app, "Zuhause");
        app.key(KeyCode::Char('a').into());
        typed(&mut app, folder.to_str().unwrap());
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.message, "2 hinzugefügt");
        app.key(KeyCode::Char('a').into());
        typed(&mut app, folder.join("Mietvertrag.pdf").to_str().unwrap());
        assert_eq!(app.message, "0 hinzugefügt, 1 waren schon da");

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("Zuhause"), "{}", screen(&terminal));
        // Search, label and rename the hit.
        app.key(KeyCode::Char('/').into());
        typed(&mut app, "miet");
        assert_eq!(app.rows.len(), 1);
        app.key(KeyCode::Char('l').into());
        typed(&mut app, "Wohnung");
        app.key(KeyCode::Char('t').into());
        for _ in 0.."Mietvertrag".len() {
            app.key(KeyCode::Backspace.into());
        }
        typed(&mut app, "Lease");
        assert!(app.rows.is_empty(), "the search finds the old title no more");
        app.key(KeyCode::Char('/').into());
        for _ in 0..4 {
            app.key(KeyCode::Backspace.into());
        }
        typed(&mut app, "wohnung");
        assert_eq!(app.rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["Lease"]);

        // Into the bin with a key, and the bin shown with a click on its action.
        app.key(KeyCode::Char('d').into());
        assert!(app.rows.is_empty());
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let (area, _) = *app.actions.iter().find(|(_, key)| *key == 'b').unwrap();
        app.mouse(click(area.x, area.y));
        assert!(app.bin);
        assert_eq!(app.rows.len(), 1);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("Im Papierkorb"), "{}", screen(&terminal));

        // A second space, chosen by a click on its tab.
        app.key(KeyCode::Char('n').into());
        typed(&mut app, "Firma");
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let (area, first) =
            app.tabs.iter().copied().find(|(_, i)| app.spaces[*i].name.as_deref() == Some("Zuhause")).unwrap();
        app.mouse(click(area.x, area.y));
        assert_eq!(app.space, first);
        let (area, _) = *app.actions.iter().find(|(_, key)| *key == 'q').unwrap();
        app.mouse(click(area.x + 1, area.y));
        assert!(app.quit);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn kinds_are_edited_by_lines() {
        let workspace = Arc::new(Mutex::new(Workspace::in_memory("laptop").unwrap()));
        let mut app = App::new(workspace.clone(), None, None, Texts::of("en"));
        typed(&mut app, "Home");
        app.key(KeyCode::Char('A').into());
        assert_eq!(app.input.as_ref().map(|(p, _)| *p), Some(Prompt::Kinds));
        typed(&mut app, "+ Letter");
        assert_eq!(app.message, "Kind letter added");
        typed(&mut app, "letter = Business letter");
        typed(&mut app, "letter + sender text Sender");
        typed(&mut app, "letter + due day Due");
        assert!(app.message.starts_with("+ Name adds a kind"), "an unknown type: the help, {}", app.message);
        typed(&mut app, "letter + due date Due");
        typed(&mut app, "letter ~ dear sir 4");
        typed(&mut app, "letter ~ invoice");
        typed(&mut app, "letter ~ invoice 0");
        let space = app.spaces[0].id.clone();
        let kind = workspace.lock().unwrap().kind(&space, "letter").unwrap().clone();
        assert_eq!(kind.name, "Business letter");
        assert_eq!(
            kind.fields.iter().map(|f| (f.key.as_str(), f.field_type)).collect::<Vec<_>>(),
            [("sender", FieldType::Text), ("due", FieldType::Date)]
        );
        assert_eq!(kind.words, [Weighted { word: "dear sir".into(), weight: 4 }]);
        typed(&mut app, "letter - sender");
        assert_eq!(workspace.lock().unwrap().kind(&space, "letter").unwrap().fields.len(), 1);
        let panel: Vec<String> = app.members.clone().unwrap().iter().map(|l| l.to_string()).collect();
        assert!(panel.contains(&"Business letter (letter)".to_owned()), "{panel:?}");
        typed(&mut app, "- letter");
        assert!(workspace.lock().unwrap().kinds(&space).unwrap().is_empty());
        app.key(KeyCode::Esc.into());
        assert!(app.input.is_none());
    }

    /// A relay on a free port of this machine, in a folder of its own; stops with the test.
    fn relay() -> String {
        let folder = std::env::temp_dir().join(format!("aktujo-tui-relay-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let relay = aktujo_relay::Relay::open(aktujo_crypt::Domain::AKTUJO, &folder).unwrap();
        let (ready, address) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                ready.send(listener.local_addr().unwrap()).unwrap();
                aktujo_relay::serve_listener(relay, listener).await.unwrap();
            });
        });
        format!("http://{}", address.recv().unwrap())
    }

    #[test]
    fn a_space_is_synchronised_and_shared() {
        let url = relay();
        let workspace = Arc::new(Mutex::new(Workspace::in_memory("laptop").unwrap()));
        let mut app = App::new(workspace.clone(), None, None, Texts::of("en"));
        typed(&mut app, "Home");
        app.key(KeyCode::Char('N').into());
        typed(&mut app, "Laptop");
        app.key(KeyCode::Char('S').into());
        typed(&mut app, &url);
        assert!(app.message.starts_with("Recovery code"), "{}", app.message);
        app.key(KeyCode::Char('i').into());
        typed(&mut app, "x");
        assert_eq!(app.message, "Rights: r read, w write, m manage");
        app.key(KeyCode::Char('i').into());
        typed(&mut app, "r");
        assert!(app.message.starts_with(&format!("Invitation: {url}#join=")), "{}", app.message);
        app.key(KeyCode::Char('y').into());
        assert_eq!(app.message, "0 received, 0 sent");
        app.key(KeyCode::Char('M').into());
        let shown: Vec<String> = app.details().iter().map(|l| l.to_string()).collect();
        assert!(shown.contains(&"Laptop (this device) – May manage".to_owned()), "{shown:?}");
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("Home ⇅"), "{}", screen(&terminal));
    }
}
