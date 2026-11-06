// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exporting a space (`docs/rfc/0004-archive-0.1.md`, "Export"): everything, at any time, in aktujo's own open form –
//! readable without aktujo:
//!
//! ```text
//! README.txt                                    what this is
//! space.json                                    the space, its kinds, every document's state (format 1)
//! documents/0001 <title>/v1 <file name>         every version of every document, as it was added
//! documents/0001 <title>/v1 text.txt            what was read from it, pages split by a form feed
//! events.jsonl                                  the events of the space as they are, one per line, in their order
//! ```
//!
//! Written to a folder or as a ZIP ([`Folder`], [`Zip`]). Nothing secret is in it: no keys, no member list, no
//! recovery code – and nothing in it is encrypted: whoever has the export can read it.
//!
//! The workspace is locked to read the state, then once for each original, so that a large space does not hold it.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use aktujo_base::Sha256Hash;
use serde_json::{Value as Json, json};

use crate::{AppError, SpaceId, Workspace, search::day_of};

/// The form of `space.json`.
pub const FORMAT: u32 = 1;

/// Where an export is written: file by file, by its path inside the export (`/` between folders).
pub trait Sink {
    /// Writes one file.
    fn file(&mut self, path: &str, bytes: &[u8]) -> io::Result<()>;
    /// Ends the export.
    fn finish(&mut self) -> io::Result<()>;
}

/// An export into a folder that must not exist yet.
#[derive(Debug)]
pub struct Folder {
    root: PathBuf,
}

impl Folder {
    /// The export into `root`, made now; a folder that is there already is refused, so that nothing is overwritten.
    pub fn create(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root.parent().unwrap_or(Path::new(".")))?;
        fs::create_dir(root)?;
        Ok(Folder { root: root.to_path_buf() })
    }
}

impl Sink for Folder {
    fn file(&mut self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let target = path.split('/').fold(self.root.clone(), |p, part| p.join(part));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, bytes)
    }

    fn finish(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// An export as a ZIP: the files stored as they are – documents are compressed already – under `prefix/`. Beyond
/// 4 GiB or 65,535 files it is a ZIP64, which every current unzip reads.
#[derive(Debug)]
pub struct Zip<W: Write> {
    out: W,
    prefix: String,
    offset: u64,
    central: Vec<u8>,
    count: u64,
}

impl<W: Write> Zip<W> {
    /// A ZIP written to `out`, its files in the folder `prefix`.
    pub fn new(out: W, prefix: &str) -> Self {
        Zip { out, prefix: prefix.to_owned(), offset: 0, central: Vec::new(), count: 0 }
    }

    /// What was written to.
    pub fn into_inner(self) -> W {
        self.out
    }
}

/// The largest size and offset a ZIP holds without ZIP64; it stands for "in the ZIP64 field".
const MAX32: u64 = 0xFFFF_FFFF;
/// The most files a ZIP holds without ZIP64.
const MAX16: u64 = 0xFFFF;

impl<W: Write> Sink for Zip<W> {
    fn file(&mut self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let name = format!("{}/{path}", self.prefix);
        let name = name.as_bytes();
        let size = bytes.len() as u64;
        let offset = self.offset;
        self.count += 1;
        let crc = crc32fast::hash(bytes);
        let name_len = u16::try_from(name.len()).map_err(|_| io::Error::other("a path too long"))?;
        let large = size >= MAX32;
        // The ZIP64 field of the local header: both sizes.
        let mut local_extra = Vec::new();
        if large {
            local_extra.extend_from_slice(&1u16.to_le_bytes());
            local_extra.extend_from_slice(&16u16.to_le_bytes());
            local_extra.extend_from_slice(&size.to_le_bytes());
            local_extra.extend_from_slice(&size.to_le_bytes());
        }
        // The ZIP64 field of the central directory: the sizes and the offset, those that do not fit.
        let mut central_extra = Vec::new();
        if large {
            central_extra.extend_from_slice(&size.to_le_bytes());
            central_extra.extend_from_slice(&size.to_le_bytes());
        }
        if offset >= MAX32 {
            central_extra.extend_from_slice(&offset.to_le_bytes());
        }
        if !central_extra.is_empty() {
            let mut field = 1u16.to_le_bytes().to_vec();
            field.extend_from_slice(&(central_extra.len() as u16).to_le_bytes());
            field.extend_from_slice(&central_extra);
            central_extra = field;
        }
        let version: u16 = if central_extra.is_empty() { 20 } else { 45 };
        let size32 = size.min(MAX32) as u32;
        // The name in UTF-8 (bit 11), stored, no time (1980-01-01).
        let common = |header: &mut Vec<u8>, extra: usize| {
            header.extend_from_slice(&version.to_le_bytes());
            header.extend_from_slice(&0x0800u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0x0021u16.to_le_bytes());
            header.extend_from_slice(&crc.to_le_bytes());
            header.extend_from_slice(&size32.to_le_bytes());
            header.extend_from_slice(&size32.to_le_bytes());
            header.extend_from_slice(&name_len.to_le_bytes());
            header.extend_from_slice(&(extra as u16).to_le_bytes());
        };
        let mut local = 0x0403_4b50u32.to_le_bytes().to_vec();
        common(&mut local, local_extra.len());
        local.extend_from_slice(name);
        local.extend_from_slice(&local_extra);
        self.out.write_all(&local)?;
        self.out.write_all(bytes)?;
        self.offset += (local.len() + bytes.len()) as u64;

        self.central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        self.central.extend_from_slice(&version.to_le_bytes());
        common(&mut self.central, central_extra.len());
        self.central.extend_from_slice(&[0; 6]); // comment length, disk, internal attributes
        self.central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        self.central.extend_from_slice(&(offset.min(MAX32) as u32).to_le_bytes());
        self.central.extend_from_slice(name);
        self.central.extend_from_slice(&central_extra);
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        let start = self.offset;
        let length = self.central.len() as u64;
        self.out.write_all(&self.central)?;
        if self.count > MAX16 || start >= MAX32 || length >= MAX32 {
            // The ZIP64 end of the central directory, and where it is.
            let at = start + length;
            let mut record = 0x0606_4b50u32.to_le_bytes().to_vec();
            record.extend_from_slice(&44u64.to_le_bytes());
            record.extend_from_slice(&45u16.to_le_bytes());
            record.extend_from_slice(&45u16.to_le_bytes());
            record.extend_from_slice(&[0; 8]); // this disk, the disk of the directory
            record.extend_from_slice(&self.count.to_le_bytes());
            record.extend_from_slice(&self.count.to_le_bytes());
            record.extend_from_slice(&length.to_le_bytes());
            record.extend_from_slice(&start.to_le_bytes());
            record.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
            record.extend_from_slice(&0u32.to_le_bytes());
            record.extend_from_slice(&at.to_le_bytes());
            record.extend_from_slice(&1u32.to_le_bytes());
            self.out.write_all(&record)?;
        }
        let mut end = 0x0605_4b50u32.to_le_bytes().to_vec();
        end.extend_from_slice(&[0; 4]); // this disk, the disk of the directory
        end.extend_from_slice(&(self.count.min(MAX16) as u16).to_le_bytes());
        end.extend_from_slice(&(self.count.min(MAX16) as u16).to_le_bytes());
        end.extend_from_slice(&(length.min(MAX32) as u32).to_le_bytes());
        end.extend_from_slice(&(start.min(MAX32) as u32).to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        self.out.write_all(&end)?;
        self.out.flush()
    }
}

/// A name for a file or folder on any system: without the characters some forbid, at most 80 characters.
fn safe(name: &str) -> String {
    let cleaned: String =
        name.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).take(80).collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_owned();
    if cleaned.is_empty() { "_".into() } else { cleaned }
}

/// The name of an export of a space: `aktujo <space> <day>`.
pub fn export_name(space: Option<&str>, unix_ms: u64) -> String {
    format!("aktujo {} {}", safe(space.unwrap_or("space")), day_of(unix_ms))
}

const README: &str = "aktujo – an export of one space
================================

space.json      the space, its kinds (with the templates they came from) and the state of every document: title, kind, fields (confirmed and
                suggested), labels, whether it is in the bin, and its versions with their files
documents/      every version of every document as it was added (v1, v2 …), and what was read from it (vN text.txt,
                pages split by a form feed)
events.jsonl    the events of the space as they are, one per line, in their order – everything above follows from them

Nothing here is encrypted: whoever has this export can read it. No keys are in it.
The form is described at https://aktujo.dev.

aktujo – Export eines Space
===========================

space.json      der Space, seine Arten und der Stand jedes Dokuments; documents/ jede Version jedes Dokuments und der
                daraus gelesene Text; events.jsonl die Ereignisse des Space, wie sie sind.
Nichts hier ist verschlüsselt: Wer diesen Export hat, kann ihn lesen.
";

/// What an export contains: the files made from the state, and the originals to copy, by their path.
struct Plan {
    files: Vec<(String, Vec<u8>)>,
    originals: Vec<(String, Sha256Hash)>,
    documents: usize,
}

impl Workspace {
    fn export_plan(&self, space: &SpaceId) -> Result<Plan, AppError> {
        let view = self.view(space)?;
        let mut documents: Vec<_> = view.documents.active().chain(view.documents.trashed()).collect();
        documents.sort_by(|a, b| a.added.cmp(&b.added));
        let mut files = vec![("README.txt".to_owned(), README.as_bytes().to_vec())];
        let mut originals = Vec::new();
        let mut states = Vec::new();
        for (i, document) in documents.iter().enumerate() {
            let folder = format!("documents/{:04} {}", i + 1, safe(&document.title));
            let mut versions = Vec::new();
            for (n, version) in document.versions.iter().enumerate() {
                let file = format!("{folder}/v{} {}", n + 1, safe(&version.original.name));
                originals.push((file.clone(), version.original.hash));
                let text = match version.read.as_ref().and_then(|r| r.text) {
                    Some(hash) => match self.stored_text(&hash)? {
                        Some(read) => {
                            let path = format!("{folder}/v{} text.txt", n + 1);
                            let pages: Vec<String> = read.pages.iter().map(|p| p.text()).collect();
                            files.push((path.clone(), pages.join("\n\u{c}\n").into_bytes()));
                            Some(path)
                        }
                        None => None,
                    },
                    None => None,
                };
                versions.push(json!({
                    "number": n + 1,
                    "name": version.original.name,
                    "media_type": version.original.media_type,
                    "size": version.original.size,
                    "sha256": version.original.hash.to_string(),
                    "added": version.added.wall_ms,
                    "file": file,
                    "text": text,
                }));
            }
            let fields: serde_json::Map<String, Json> = document
                .fields
                .iter()
                .map(|(key, field)| {
                    let confirmed = field.confirmed.as_ref().map(|c| &c.value);
                    let suggested = field.suggestion.as_ref().map(|s| &s.value);
                    (key.clone(), json!({"confirmed": confirmed, "suggested": suggested}))
                })
                .collect();
            states.push(json!({
                "id": document.id.as_str(),
                "title": document.title,
                "folder": folder,
                "kind": document.kind,
                "kind_suggested": document.kind_suggestion.as_ref().map(|s| &s.kind),
                "fields": fields,
                "labels": document.labels,
                "trashed": document.trashed,
                "added": document.added.wall_ms,
                "changed": document.changed.wall_ms,
                "versions": versions,
            }));
        }
        let state = json!({
            "format": FORMAT,
            "program": format!("aktujo {}", env!("CARGO_PKG_VERSION")),
            "exported": aktujo_sync::now_ms(),
            "space": {"id": space.as_str(), "name": view.name},
            "kinds": view.kinds.to_vec(),
            "templates": view
                .kinds
                .to_vec()
                .iter()
                .filter_map(|kind| view.kinds.template(&kind.id).map(|t| (kind.id.clone(), json!(t))))
                .collect::<serde_json::Map<String, Json>>(),
            "documents": states,
        });
        files.push(("space.json".into(), serde_json::to_vec_pretty(&state).expect("JSON")));
        let log = self.replica.log(space)?;
        let mut events = Vec::new();
        for (hash, _) in log.ordered() {
            if let Some(canonical) = log.canonical(&hash) {
                events.extend_from_slice(canonical);
                events.push(b'\n');
            }
        }
        files.push(("events.jsonl".into(), events));
        Ok(Plan { files, originals, documents: documents.len() })
    }
}

/// What an export wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exported {
    /// The documents, in the bin or not.
    pub documents: usize,
    /// The files, the README and `space.json` included.
    pub files: usize,
    /// Originals whose bytes are not on this device (yet) – their files are missing; `space.json` names them.
    pub missing: usize,
}

/// Exports a space to `sink`: the state at once, then each original, the workspace locked only to read it.
pub fn export(workspace: &Mutex<Workspace>, space: &SpaceId, sink: &mut dyn Sink) -> Result<Exported, AppError> {
    let lock = || workspace.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let plan = lock().export_plan(space)?;
    let mut written = 0;
    let mut missing = 0;
    for (path, hash) in &plan.originals {
        let bytes = lock().replica.original(hash)?;
        match bytes {
            Some(bytes) => {
                sink.file(path, &bytes)?;
                written += 1;
            }
            None => missing += 1,
        }
    }
    for (path, bytes) in &plan.files {
        sink.file(path, bytes)?;
        written += 1;
    }
    sink.finish()?;
    Ok(Exported { documents: plan.documents, files: written, missing })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewFile;

    fn workspace() -> (Mutex<Workspace>, SpaceId) {
        let mut w = Workspace::in_memory("laptop").unwrap();
        let space = w.found_space("Home / Office").unwrap();
        let add = |w: &mut Workspace, name: &str, bytes: &[u8]| {
            w.add_document(&space, NewFile { name, media_type: "application/pdf", bytes }).unwrap().document
        };
        let lease = add(&mut w, "lease.pdf", b"%PDF-1.7 lease");
        w.add_label(&space, &lease, "Flat").unwrap();
        w.add_version(
            &space,
            &lease,
            NewFile { name: "lease signed.pdf", media_type: "application/pdf", bytes: b"%PDF-1.7 signed" },
        )
        .unwrap();
        let old = add(&mut w, "old: bill?.pdf", b"%PDF-1.7 old");
        w.trash(&space, &old).unwrap();
        (Mutex::new(w), space)
    }

    #[test]
    fn a_space_is_exported_to_a_folder_readable_without_aktujo() {
        let (workspace, space) = workspace();
        let root = std::env::temp_dir().join(format!("aktujo-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut folder = Folder::create(&root).unwrap();
        let exported = export(&workspace, &space, &mut folder).unwrap();
        assert_eq!(exported, Exported { documents: 2, files: 6, missing: 0 });
        assert_eq!(fs::read(root.join("documents/0001 lease/v2 lease signed.pdf")).unwrap(), b"%PDF-1.7 signed");
        assert!(root.join("documents/0002 old_ bill_/v1 old_ bill_.pdf").exists(), "unsafe characters replaced");
        let state: Json = serde_json::from_slice(&fs::read(root.join("space.json")).unwrap()).unwrap();
        assert_eq!((state["format"].as_u64(), &state["space"]["name"]), (Some(1), &json!("Home / Office")));
        assert_eq!(state["documents"][0]["labels"], json!(["Flat"]));
        assert_eq!(state["documents"][1]["trashed"], true);
        assert_eq!(state["documents"][0]["versions"][1]["file"], "documents/0001 lease/v2 lease signed.pdf");
        let events = fs::read_to_string(root.join("events.jsonl")).unwrap();
        assert_eq!(events.lines().count(), 6, "named, two added, a version, a label, the bin");
        assert!(events.lines().all(|line| serde_json::from_str::<Json>(line).is_ok()));
        assert!(Folder::create(&root).is_err(), "nothing is overwritten");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_space_is_exported_as_a_zip() {
        let (workspace, space) = workspace();
        let mut zip = Zip::new(Vec::new(), "aktujo Home 2026-11-06");
        export(&workspace, &space, &mut zip).unwrap();
        let bytes = zip.into_inner();
        // The end of the central directory: six files, and each name in it.
        let end = &bytes[bytes.len() - 22..];
        assert_eq!(&end[..4], &[0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 6);
        // Walk the central directory: each entry points at a local header with the same name.
        let start = u32::from_le_bytes(end[16..20].try_into().unwrap()) as usize;
        let mut at = start;
        let mut names = vec![];
        for _ in 0..6 {
            assert_eq!(&bytes[at..at + 4], &[0x50, 0x4b, 0x01, 0x02]);
            let name_len = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
            let offset = u32::from_le_bytes(bytes[at + 42..at + 46].try_into().unwrap()) as usize;
            let name = &bytes[at + 46..at + 46 + name_len];
            assert_eq!(&bytes[offset..offset + 4], &[0x50, 0x4b, 0x03, 0x04]);
            assert_eq!(&bytes[offset + 30..offset + 30 + name_len], name);
            names.push(String::from_utf8(name.to_vec()).unwrap());
            at += 46 + name_len;
        }
        assert_eq!(at, bytes.len() - 22);
        assert!(names.contains(&"aktujo Home 2026-11-06/space.json".to_owned()), "{names:?}");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("aktujo Home 2026-11-06/space.json"));
        assert!(text.contains("%PDF-1.7 signed"), "stored as it is");
        assert_eq!(export_name(Some("Home / Office"), 0), "aktujo Home _ Office 1970-01-01");
    }
}
