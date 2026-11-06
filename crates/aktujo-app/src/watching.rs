// SPDX-License-Identifier: AGPL-3.0-or-later
//! Watched folders (`docs/rfc/0004-archive-0.1.md`, decision 10): a folder of this device given for a space – a
//! scanner's, a download folder – whose new files are taken in on their own, as if uploaded. They belong to the
//! device, not to the space: another device of the space has folders of its own.
//!
//! The folder is looked at every few seconds, not watched by the operating system: the same everywhere – also on
//! network shares, where file events are unreliable – and without a dependency. A file is taken once it stayed the
//! same (size and time of change) between two looks, so that one a scanner is still writing is left until it is
//! done. Only its top level is looked at; hidden and unfinished files (`.part`, `.tmp`, `.crdownload`, `~$…`) and files
//! of other types than PDF, PNG, JPEG and TIFF are left alone. What was taken is remembered by name, size and time of
//! change, so it is not read again; the same bytes twice are one document anyway. After taking a file, it is left
//! where it is (the default) or moved into the folder `added` inside the watched one – nothing is deleted.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, UNIX_EPOCH},
};

use aktujo_store::rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{AppError, NewFile, SpaceId, Worker, Workspace};

/// The setting with the watched folders of this device.
const WATCHES: &str = "watch.folders";
/// Where files are moved after they were taken, inside the watched folder.
pub const ADDED: &str = "added";
/// The largest file taken from a watched folder, as from an upload.
const MAX_FILE: u64 = 256 * 1024 * 1024;

/// What happens to a file once it was taken in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum After {
    /// It stays where it is; it is remembered, so it is not taken again.
    #[default]
    Leave,
    /// It is moved into the folder `added` inside the watched one.
    Move,
}

/// A watched folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    /// The space its files go to.
    pub space: SpaceId,
    /// The folder, as an absolute path.
    pub folder: PathBuf,
    /// What happens to a file once it was taken.
    #[serde(default)]
    pub after: After,
    /// Whether it is looked at.
    pub on: bool,
}

/// How the last look at a watched folder went.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Watched {
    /// When it ended, in milliseconds since 1970.
    pub at: u64,
    /// How many files were taken in since the program started.
    pub taken: usize,
    /// Why it failed, if it did: the folder is gone, a file could not be read or moved.
    pub problem: Option<String>,
}

/// The table of the files taken from watched folders.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS watch_taken (
    space TEXT NOT NULL,
    folder TEXT NOT NULL,
    name TEXT NOT NULL,
    size INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    PRIMARY KEY (space, folder, name)
);";

pub(crate) fn prepare(workspace: &Workspace) -> Result<(), AppError> {
    workspace.replica.store().connection().execute_batch(SCHEMA)?;
    Ok(())
}

/// The media type of a file that is taken from a watched folder, by its extension; none for any other.
pub fn media_type_of(name: &str) -> Option<&'static str> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match extension.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "tif" | "tiff" => "image/tiff",
        _ => return None,
    })
}

/// Whether a file is left alone however it is named: hidden, or still being written by a program that marks it.
fn unfinished(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    name.starts_with('.')
        || name.starts_with("~$")
        || [".part", ".tmp", ".crdownload", ".download", ".partial"].iter().any(|e| lower.ends_with(e))
}

/// A file as seen at a look: its name, size and time of change (milliseconds since 1970).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seen {
    size: u64,
    modified: i64,
}

fn seen(path: &Path) -> Option<Seen> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64;
    meta.is_file().then_some(Seen { size: meta.len(), modified })
}

impl Workspace {
    /// The watched folders of this device, in the order they were added.
    pub fn watches(&self) -> Result<Vec<Watch>, AppError> {
        match self.replica.store().setting(None, WATCHES)? {
            Some(json) => {
                serde_json::from_str(&json).map_err(|e| AppError::Folder(format!("the watched folders: {e}")))
            }
            None => Ok(vec![]),
        }
    }

    fn keep_watches(&mut self, watches: &[Watch]) -> Result<(), AppError> {
        let json = serde_json::to_string(watches).expect("watched folders are JSON");
        Ok(self.replica.store().set_setting(None, WATCHES, &json)?)
    }

    /// Watches a folder for a space, switched on: it must be a folder of this device, given as an absolute path, and
    /// the device must be able to write in the space. Returns the folder as it is kept.
    pub fn add_watch(&mut self, space: &SpaceId, folder: &Path, after: After) -> Result<PathBuf, AppError> {
        self.may_write(space)?;
        if !folder.is_absolute() {
            return Err(AppError::Watch(format!("{} is not an absolute path", folder.display())));
        }
        let folder = folder.canonicalize().map_err(|e| AppError::Watch(format!("{}: {e}", folder.display())))?;
        if !folder.is_dir() {
            return Err(AppError::Watch(format!("{} is not a folder", folder.display())));
        }
        let mut watches = self.watches()?;
        match watches.iter_mut().find(|w| &w.space == space && w.folder == folder) {
            Some(watch) => {
                watch.after = after;
                watch.on = true;
            }
            None => watches.push(Watch { space: space.clone(), folder: folder.clone(), after, on: true }),
        }
        self.keep_watches(&watches)?;
        Ok(folder)
    }

    /// Switches a watched folder on or off, and sets what happens to a file once it was taken.
    pub fn set_watch(&mut self, space: &SpaceId, folder: &Path, on: bool, after: After) -> Result<(), AppError> {
        let mut watches = self.watches()?;
        let watch = watches
            .iter_mut()
            .find(|w| &w.space == space && w.folder == folder)
            .ok_or_else(|| AppError::NotFound(format!("{} is not watched", folder.display())))?;
        watch.on = on;
        watch.after = after;
        self.keep_watches(&watches)
    }

    /// Stops watching a folder; what was taken from it stays, and so do its files.
    pub fn remove_watch(&mut self, space: &SpaceId, folder: &Path) -> Result<(), AppError> {
        let mut watches = self.watches()?;
        let before = watches.len();
        watches.retain(|w| !(&w.space == space && w.folder == folder));
        if watches.len() == before {
            return Err(AppError::NotFound(format!("{} is not watched", folder.display())));
        }
        self.keep_watches(&watches)?;
        self.watched.remove(&(space.clone(), folder.to_path_buf()));
        let connection = self.replica.store().connection();
        let folder = folder.to_string_lossy();
        connection
            .execute("DELETE FROM watch_taken WHERE space = ?1 AND folder = ?2", params![space.as_str(), folder])?;
        Ok(())
    }

    /// How the last look at a watched folder went, if it was looked at since the program started.
    pub fn watched(&self, space: &SpaceId, folder: &Path) -> Option<&Watched> {
        self.watched.get(&(space.clone(), folder.to_path_buf()))
    }

    fn taken(&self, watch: &Watch) -> Result<BTreeMap<String, Seen>, AppError> {
        let connection = self.replica.store().connection();
        let mut statement =
            connection.prepare("SELECT name, size, modified FROM watch_taken WHERE space = ?1 AND folder = ?2")?;
        let folder = watch.folder.to_string_lossy();
        let rows = statement.query_map(params![watch.space.as_str(), folder], |row| {
            Ok((row.get::<_, String>(0)?, Seen { size: row.get::<_, i64>(1)? as u64, modified: row.get(2)? }))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn remember(&self, watch: &Watch, name: &str, seen: Option<Seen>) -> Result<(), AppError> {
        let connection = self.replica.store().connection();
        let folder = watch.folder.to_string_lossy();
        match seen {
            Some(seen) => connection.execute(
                "INSERT OR REPLACE INTO watch_taken (space, folder, name, size, modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![watch.space.as_str(), folder, name, seen.size as i64, seen.modified],
            )?,
            None => connection.execute(
                "DELETE FROM watch_taken WHERE space = ?1 AND folder = ?2 AND name = ?3",
                params![watch.space.as_str(), folder, name],
            )?,
        };
        Ok(())
    }

    fn report(&mut self, watch: &Watch, taken: usize, problem: Option<String>) {
        let entry = self.watched.entry((watch.space.clone(), watch.folder.clone())).or_default();
        entry.at = aktujo_sync::now_ms();
        entry.taken += taken;
        entry.problem = problem;
    }
}

/// What a looker remembers between two looks: the files as they were, per folder.
#[derive(Debug, Default)]
pub struct Looks {
    before: BTreeMap<PathBuf, BTreeMap<String, Seen>>,
}

/// Looks at every watched folder that is on once, taking in what is new and stayed the same since the last look –
/// the workspace locked only to read the folders and to add a file. Returns how many files were taken.
pub fn look(workspace: &Mutex<Workspace>, looks: &mut Looks) -> usize {
    let lock = || workspace.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let watches = lock().watches().unwrap_or_default();
    let mut total = 0;
    for watch in watches.into_iter().filter(|w| w.on) {
        let (taken, problem) = look_at(workspace, &watch, looks);
        total += taken;
        lock().report(&watch, taken, problem);
    }
    looks.before.retain(|folder, _| folder.exists());
    total
}

fn look_at(workspace: &Mutex<Workspace>, watch: &Watch, looks: &mut Looks) -> (usize, Option<String>) {
    let lock = || workspace.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let entries = match fs::read_dir(&watch.folder) {
        Ok(entries) => entries,
        Err(error) => return (0, Some(format!("{}: {error}", watch.folder.display()))),
    };
    let mut now = BTreeMap::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else { continue };
        if unfinished(&name) || media_type_of(&name).is_none() {
            continue;
        }
        if let Some(seen) = seen(&entry.path()) {
            now.insert(name, seen);
        }
    }
    let before = looks.before.insert(watch.folder.clone(), now.clone()).unwrap_or_default();
    let taken_before = match lock().taken(watch) {
        Ok(taken) => taken,
        Err(error) => return (0, Some(error.to_string())),
    };
    let mut taken = 0;
    let mut problem = None;
    for (name, seen) in &now {
        // Stayed the same since the last look, and not taken in this state before.
        if before.get(name) != Some(seen) || taken_before.get(name) == Some(seen) {
            continue;
        }
        let path = watch.folder.join(name);
        if seen.size > MAX_FILE {
            problem = Some(format!("{}: larger than {MAX_FILE} bytes", path.display()));
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                problem = Some(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let media_type = media_type_of(name).expect("only known types are looked at");
        let added = {
            let mut workspace = lock();
            let added = workspace.add_document(&watch.space, NewFile { name, media_type, bytes: &bytes });
            added.and_then(|added| workspace.remember(watch, name, Some(*seen)).map(|()| added))
        };
        match added {
            Ok(added) => {
                taken += usize::from(added.new);
                if watch.after == After::Move
                    && let Err(error) = move_away(&watch.folder, name)
                {
                    problem = Some(format!("{}: {error}", path.display()));
                }
            }
            Err(error) => problem = Some(format!("{}: {error}", path.display())),
        }
    }
    // Forget what is no longer there, so that a file of the same name later is taken again.
    for name in taken_before.keys().filter(|name| !now.contains_key(*name)) {
        let _ = lock().remember(watch, name, None);
    }
    (taken, problem)
}

/// Moves a taken file into `added` inside its folder, under a free name.
fn move_away(folder: &Path, name: &str) -> std::io::Result<()> {
    let target = folder.join(ADDED);
    fs::create_dir_all(&target)?;
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let free = (1..)
        .map(|n| if n == 1 { name.to_owned() } else { format!("{stem} ({n}).{extension}") })
        .map(|candidate| target.join(candidate))
        .find(|candidate| !candidate.exists())
        .expect("a free name");
    fs::rename(folder.join(name), free)
}

/// Looking at the watched folders on a thread of its own: every `every` and whenever it is woken. It stops when
/// dropped, after the look it is in.
#[derive(Debug)]
pub struct Watcher {
    wake: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Watcher {
    /// Starts looking at the watched folders of `workspace`; `worker` is woken when a file was taken.
    pub fn start(workspace: Arc<Mutex<Workspace>>, worker: Option<Arc<Worker>>, every: Duration) -> Self {
        let (wake, woken) = mpsc::channel::<()>();
        let thread = thread::Builder::new()
            .name("aktujo-watch".into())
            .spawn(move || {
                let mut looks = Looks::default();
                loop {
                    if look(&workspace, &mut looks) > 0
                        && let Some(worker) = &worker
                    {
                        worker.wake();
                    }
                    match woken.recv_timeout(every) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                    while woken.try_recv().is_ok() {}
                }
            })
            .expect("a thread starts");
        Watcher { wake: Some(wake), thread: Some(thread) }
    }

    /// Looks now.
    pub fn wake(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.send(());
        }
    }
}

impl Drop for Watcher {
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

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("aktujo-watch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder.canonicalize().unwrap()
    }

    #[test]
    fn files_are_taken_once_they_stay_the_same() {
        let workspace = Mutex::new(Workspace::in_memory("laptop").unwrap());
        let dir = folder("take");
        let space = {
            let mut w = workspace.lock().unwrap();
            let space = w.found_space("Home").unwrap();
            assert_eq!(w.add_watch(&space, &dir, After::Leave).unwrap(), dir);
            assert!(w.add_watch(&space, Path::new("relative"), After::Leave).is_err());
            space
        };
        fs::write(dir.join("scan 1.pdf"), b"%PDF-1.7 one").unwrap();
        fs::write(dir.join("notes.txt"), b"not a document").unwrap();
        fs::write(dir.join(".hidden.pdf"), b"%PDF-1.7 hidden").unwrap();
        fs::write(dir.join("scan 2.pdf.part"), b"%PDF-1.7 half").unwrap();
        let mut looks = Looks::default();
        assert_eq!(look(&workspace, &mut looks), 0, "first seen, not yet the same twice");
        assert_eq!(look(&workspace, &mut looks), 1);
        assert_eq!(look(&workspace, &mut looks), 0, "taken once");
        {
            let w = workspace.lock().unwrap();
            let documents = w.documents(&space).unwrap();
            assert_eq!(documents.iter().map(|d| d.title.as_str()).collect::<Vec<_>>(), ["scan 1"]);
            let watched = w.watched(&space, &dir).unwrap();
            assert_eq!((watched.taken, &watched.problem), (1, &None));
        }
        // Left where it was; after a restart it is not taken again either.
        assert!(dir.join("scan 1.pdf").exists());
        let mut fresh = Looks::default();
        look(&workspace, &mut fresh);
        assert_eq!(look(&workspace, &mut fresh), 0);
        // A changed file is a new document.
        fs::write(dir.join("scan 1.pdf"), b"%PDF-1.7 one, rescanned").unwrap();
        look(&workspace, &mut looks);
        look(&workspace, &mut looks);
        assert_eq!(workspace.lock().unwrap().documents(&space).unwrap().len(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn taken_files_can_be_moved_and_folders_switched_off() {
        let workspace = Mutex::new(Workspace::in_memory("laptop").unwrap());
        let dir = folder("move");
        let space = {
            let mut w = workspace.lock().unwrap();
            let space = w.found_space("Home").unwrap();
            w.add_watch(&space, &dir, After::Move).unwrap();
            space
        };
        fs::create_dir_all(dir.join(ADDED)).unwrap();
        fs::write(dir.join(ADDED).join("scan.pdf"), b"older").unwrap();
        fs::write(dir.join("scan.pdf"), b"%PDF-1.7 two").unwrap();
        let mut looks = Looks::default();
        look(&workspace, &mut looks);
        assert_eq!(look(&workspace, &mut looks), 1);
        assert!(!dir.join("scan.pdf").exists());
        assert!(dir.join(ADDED).join("scan (2).pdf").exists(), "a free name beside the older one");

        workspace.lock().unwrap().set_watch(&space, &dir, false, After::Move).unwrap();
        fs::write(dir.join("other.png"), b"png").unwrap();
        look(&workspace, &mut looks);
        assert_eq!(look(&workspace, &mut looks), 0, "switched off");

        fs::remove_dir_all(&dir).unwrap();
        workspace.lock().unwrap().set_watch(&space, &dir, true, After::Move).unwrap();
        look(&workspace, &mut looks);
        assert!(workspace.lock().unwrap().watched(&space, &dir).unwrap().problem.is_some(), "the folder is gone");
        workspace.lock().unwrap().remove_watch(&space, &dir).unwrap();
        assert!(workspace.lock().unwrap().watches().unwrap().is_empty());
    }
}
