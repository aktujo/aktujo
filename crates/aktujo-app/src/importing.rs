// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reading an export back in (`exporting.rs`) – from a folder or a ZIP – as a new space.
//!
//! What comes back is the state, not the history: the kinds (with the templates they came from), every document with
//! its versions in their order, its title, its confirmed kind and fields, its labels and whether it is in the bin.
//! The events of the export are not replayed – they belong to the space they were written in, its members and
//! keys – so the new space starts its own history today. Suggestions are not taken over: the text is read anew.
//!
//! The workspace is locked once for each document, so that a large import does not hold it.

use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
};

use aktujo_doc::{DocumentId, Kind, TemplateRef, Value};
use serde::Deserialize;

use crate::{AppError, NewFile, SpaceId, Workspace, exporting::FORMAT};

/// Files of an export, by their path inside it (`/` between folders).
pub trait Files {
    /// The bytes of a file, none if there is no such file.
    fn read(&mut self, path: &str) -> io::Result<Option<Vec<u8>>>;
}

/// An export in a folder: the folder itself, or the one folder in it that holds `space.json`.
#[derive(Debug)]
pub struct FolderFiles {
    root: PathBuf,
}

impl FolderFiles {
    /// The export in `folder`.
    pub fn open(folder: &Path) -> io::Result<Self> {
        if folder.join("space.json").is_file() {
            return Ok(FolderFiles { root: folder.to_path_buf() });
        }
        let inner: Vec<PathBuf> = fs::read_dir(folder)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join("space.json").is_file())
            .collect();
        match inner.as_slice() {
            [one] => Ok(FolderFiles { root: one.clone() }),
            _ => Err(not_an_export()),
        }
    }
}

fn not_an_export() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "not an export of aktujo: there is no space.json")
}

impl Files for FolderFiles {
    fn read(&mut self, path: &str) -> io::Result<Option<Vec<u8>>> {
        if path.split('/').any(|part| part == ".." || part.is_empty()) {
            return Ok(None);
        }
        let file = path.split('/').fold(self.root.clone(), |p, part| p.join(part));
        match fs::read(file) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// A file in a ZIP.
#[derive(Debug, Clone, Copy)]
struct Entry {
    offset: u64,
    method: u16,
    compressed: u64,
    size: u64,
    crc: u32,
}

/// An export as a ZIP – stored or deflated, ZIP64 too – read where its files are, not all at once. The files are
/// those under the folder that holds `space.json`.
#[derive(Debug)]
pub struct ZipFiles<R: Read + Seek> {
    input: R,
    prefix: String,
    entries: BTreeMap<String, Entry>,
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("not a ZIP that can be read: {what}"))
}

fn u16_at(bytes: &[u8], at: usize) -> io::Result<u16> {
    bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| bad("cut short"))
}

fn u32_at(bytes: &[u8], at: usize) -> io::Result<u32> {
    bytes.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes"))).ok_or_else(|| bad("cut short"))
}

fn u64_at(bytes: &[u8], at: usize) -> io::Result<u64> {
    bytes.get(at..at + 8).map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes"))).ok_or_else(|| bad("cut short"))
}

impl<R: Read + Seek> ZipFiles<R> {
    /// The export in the ZIP `input`.
    pub fn open(mut input: R) -> io::Result<Self> {
        let len = input.seek(SeekFrom::End(0))?;
        // The end of the central directory is in the last 22 bytes and a comment of up to 65,535.
        let tail_len = len.min(22 + 0xFFFF);
        input.seek(SeekFrom::Start(len - tail_len))?;
        let mut tail = vec![0; tail_len as usize];
        input.read_exact(&mut tail)?;
        let end = (0..tail.len().saturating_sub(21))
            .rev()
            .find(|&i| tail[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
            .ok_or_else(|| bad("no end of the central directory"))?;
        let mut count = u64::from(u16_at(&tail, end + 10)?);
        let mut length = u64::from(u32_at(&tail, end + 12)?);
        let mut start = u64::from(u32_at(&tail, end + 16)?);
        // ZIP64: the locator just before the end points at the ZIP64 end.
        if end >= 20 && tail[end - 20..end - 16] == [0x50, 0x4b, 0x06, 0x07] {
            let at = u64_at(&tail, end - 12)?;
            input.seek(SeekFrom::Start(at))?;
            let mut record = [0u8; 56];
            input.read_exact(&mut record)?;
            if record[..4] != [0x50, 0x4b, 0x06, 0x06] {
                return Err(bad("no ZIP64 end of the central directory"));
            }
            count = u64_at(&record, 32)?;
            length = u64_at(&record, 40)?;
            start = u64_at(&record, 48)?;
        }
        if start.checked_add(length).is_none_or(|stop| stop > len) {
            return Err(bad("the central directory is outside the file"));
        }
        input.seek(SeekFrom::Start(start))?;
        let mut central = vec![0; usize::try_from(length).map_err(|_| bad("too large"))?];
        input.read_exact(&mut central)?;
        let mut entries = BTreeMap::new();
        let mut at = 0;
        for _ in 0..count {
            if u32_at(&central, at)? != 0x0201_4b50 {
                return Err(bad("a damaged central directory"));
            }
            let method = u16_at(&central, at + 10)?;
            let crc = u32_at(&central, at + 16)?;
            let mut compressed = u64::from(u32_at(&central, at + 20)?);
            let mut size = u64::from(u32_at(&central, at + 24)?);
            let name_len = usize::from(u16_at(&central, at + 28)?);
            let extra_len = usize::from(u16_at(&central, at + 30)?);
            let comment_len = usize::from(u16_at(&central, at + 32)?);
            let mut offset = u64::from(u32_at(&central, at + 42)?);
            let name = central.get(at + 46..at + 46 + name_len).ok_or_else(|| bad("cut short"))?;
            let name = String::from_utf8_lossy(name).replace('\\', "/");
            let extra =
                central.get(at + 46 + name_len..at + 46 + name_len + extra_len).ok_or_else(|| bad("cut short"))?;
            // The ZIP64 field holds, in this order, those of size, compressed size and offset that did not fit.
            let mut field = 0;
            while field + 4 <= extra.len() {
                let id = u16_at(extra, field)?;
                let field_len = usize::from(u16_at(extra, field + 2)?);
                if id == 1 {
                    let mut value = field + 4;
                    for slot in [&mut size, &mut compressed, &mut offset] {
                        if *slot == MAX32 {
                            *slot = u64_at(extra, value)?;
                            value += 8;
                        }
                    }
                }
                field += 4 + field_len;
            }
            entries.insert(name, Entry { offset, method, compressed, size, crc });
            at += 46 + name_len + extra_len + comment_len;
        }
        let state = entries
            .keys()
            .filter(|name| {
                name.as_str() == "space.json" || (name.ends_with("/space.json") && name.matches('/').count() == 1)
            })
            .min_by_key(|name| name.len())
            .ok_or_else(not_an_export)?;
        let prefix = state.trim_end_matches("space.json").to_owned();
        Ok(ZipFiles { input, prefix, entries })
    }
}

const MAX32: u64 = 0xFFFF_FFFF;

impl<R: Read + Seek> Files for ZipFiles<R> {
    fn read(&mut self, path: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(entry) = self.entries.get(&format!("{}{path}", self.prefix)).copied() else { return Ok(None) };
        self.input.seek(SeekFrom::Start(entry.offset))?;
        let mut header = [0u8; 30];
        self.input.read_exact(&mut header)?;
        if header[..4] != [0x50, 0x4b, 0x03, 0x04] {
            return Err(bad("a damaged local header"));
        }
        let skip = u64::from(u16_at(&header, 26)?) + u64::from(u16_at(&header, 28)?);
        self.input.seek(SeekFrom::Current(skip as i64))?;
        let mut compressed = vec![0; usize::try_from(entry.compressed).map_err(|_| bad("too large"))?];
        self.input.read_exact(&mut compressed)?;
        let bytes = match entry.method {
            0 => compressed,
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&compressed, entry.size as usize)
                .map_err(|_| bad("a damaged deflated file"))?,
            _ => return Err(bad("a file compressed in a way other than deflate")),
        };
        if bytes.len() as u64 != entry.size || crc32fast::hash(&bytes) != entry.crc {
            return Err(bad(&format!("{path} is damaged")));
        }
        Ok(Some(bytes))
    }
}

/// An error reading an export: what is not an export or is damaged is [`AppError::Import`], anything else what it is.
pub fn import_error(error: io::Error) -> AppError {
    if error.kind() == io::ErrorKind::InvalidData || error.kind() == io::ErrorKind::UnexpectedEof {
        AppError::Import(error.to_string())
    } else {
        AppError::from(error)
    }
}

/// `space.json`, as far as an import needs it.
#[derive(Debug, Deserialize)]
struct State {
    format: u32,
    space: StateSpace,
    #[serde(default)]
    kinds: Vec<Kind>,
    #[serde(default)]
    templates: BTreeMap<String, TemplateRef>,
    #[serde(default)]
    documents: Vec<StateDocument>,
}

#[derive(Debug, Deserialize)]
struct StateSpace {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StateDocument {
    title: String,
    kind: Option<String>,
    #[serde(default)]
    fields: BTreeMap<String, StateField>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    trashed: bool,
    versions: Vec<StateVersion>,
}

#[derive(Debug, Deserialize)]
struct StateField {
    confirmed: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct StateVersion {
    name: String,
    media_type: String,
    file: String,
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The new space.
    pub space: SpaceId,
    /// The kinds defined in it.
    pub kinds: usize,
    /// The documents added.
    pub documents: usize,
    /// Versions whose file is not in the export – the export was made on a device that did not have them.
    pub missing: usize,
    /// What could not be taken over: a kind or a value the space refused, a document with no file at all.
    pub skipped: Vec<String>,
}

/// Reads the export in `files` into a new space named `name` – or as the exported one was.
pub fn import(workspace: &Mutex<Workspace>, files: &mut dyn Files, name: Option<&str>) -> Result<Imported, AppError> {
    let lock = || workspace.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let bytes = files.read("space.json").map_err(import_error)?.ok_or_else(|| import_error(not_an_export()))?;
    let state: State = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Import(format!("space.json cannot be read: {error}")))?;
    if state.format != FORMAT {
        return Err(AppError::Import(format!(
            "space.json is in form {}, this aktujo reads form {FORMAT}",
            state.format
        )));
    }
    let name = name.map(str::to_owned).or(state.space.name).unwrap_or_else(|| "Imported".into());
    let space = lock().found_space(&name)?;
    let mut imported = Imported { space: space.clone(), kinds: 0, documents: 0, missing: 0, skipped: Vec::new() };

    {
        let mut workspace = lock();
        for kind in state.kinds {
            let id = kind.id.clone();
            let template = state.templates.get(&id).cloned();
            match kind.check().map_err(AppError::from).and_then(|()| workspace.write_kind(&space, kind, template)) {
                Ok(()) => imported.kinds += 1,
                Err(error) => imported.skipped.push(format!("the kind {id}: {error}")),
            }
        }
    }

    for document in state.documents {
        let mut id: Option<DocumentId> = None;
        for version in &document.versions {
            let Some(bytes) = files.read(&version.file).map_err(import_error)? else {
                imported.missing += 1;
                continue;
            };
            let file = NewFile { name: &version.name, media_type: &version.media_type, bytes: &bytes };
            let mut workspace = lock();
            match &id {
                None => id = Some(workspace.add_document(&space, file)?.document),
                Some(id) => workspace.add_version(&space, id, file)?,
            }
        }
        let Some(id) = id else {
            imported.skipped.push(format!("the document {:?}: none of its files is in the export", document.title));
            continue;
        };
        imported.documents += 1;
        let mut workspace = lock();
        let mut note = |what: String, result: Result<(), AppError>| {
            if let Err(error) = result {
                imported.skipped.push(format!("{:?}, {what}: {error}", document.title));
            }
        };
        note("the title".into(), workspace.set_title(&space, &id, &document.title));
        if let Some(kind) = &document.kind {
            note(format!("the kind {kind}"), workspace.set_kind(&space, &id, Some(kind)));
        }
        for (key, field) in document.fields {
            if let Some(value) = field.confirmed {
                let result = workspace.set_field(&space, &id, &key, value);
                note(format!("the field {key}"), result);
            }
        }
        for label in &document.labels {
            note(format!("the label {label}"), workspace.add_label(&space, &id, label));
        }
        if document.trashed {
            note("the bin".into(), workspace.trash(&space, &id));
        }
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::exporting::{Folder, Sink, Zip, export};

    fn space() -> (Mutex<Workspace>, SpaceId) {
        let mut w = Workspace::in_memory("laptop").unwrap();
        let space = w.found_space("Home").unwrap();
        let mut kind = Kind::new("letter", "Letter", vec![]);
        kind.fields.push(aktujo_doc::FieldDef::new("sender", "Sender", aktujo_doc::FieldType::Text));
        w.define_kind(&space, kind).unwrap();
        let add = |w: &mut Workspace, name: &str, bytes: &[u8]| {
            w.add_document(&space, NewFile { name, media_type: "application/pdf", bytes }).unwrap().document
        };
        let lease = add(&mut w, "lease.pdf", b"%PDF-1.7 lease");
        w.add_version(
            &space,
            &lease,
            NewFile { name: "signed.pdf", media_type: "application/pdf", bytes: b"%PDF-1.7 s" },
        )
        .unwrap();
        w.set_title(&space, &lease, "The lease").unwrap();
        w.set_kind(&space, &lease, Some("letter")).unwrap();
        w.set_field(&space, &lease, "sender", Value::Text("Landlord".into())).unwrap();
        w.add_label(&space, &lease, "Flat").unwrap();
        let old = add(&mut w, "old.pdf", b"%PDF-1.7 old");
        w.trash(&space, &old).unwrap();
        (Mutex::new(w), space)
    }

    fn check(workspace: &Mutex<Workspace>, imported: &Imported) {
        assert_eq!((imported.kinds, imported.documents, imported.missing), (1, 2, 0));
        assert!(imported.skipped.is_empty(), "{:?}", imported.skipped);
        let w = workspace.lock().unwrap();
        let documents = w.documents(&imported.space).unwrap();
        assert_eq!(documents.len(), 1);
        let lease = documents[0];
        assert_eq!(lease.title, "The lease");
        assert_eq!(lease.versions.len(), 2);
        assert_eq!(w.original(&lease.current().original.hash).unwrap(), b"%PDF-1.7 s");
        assert_eq!(lease.kind.as_deref(), Some("letter"));
        assert_eq!(lease.fields["sender"].value(), Some(&Value::Text("Landlord".into())));
        assert_eq!(lease.labels.iter().collect::<Vec<_>>(), ["Flat"]);
        assert_eq!(w.trashed(&imported.space).unwrap().len(), 1);
    }

    #[test]
    fn an_export_in_a_folder_comes_back_as_a_new_space() {
        let (workspace, space) = space();
        let root = std::env::temp_dir().join(format!("aktujo-import-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let export_root = root.join("aktujo Home 2026-11-06");
        export(&workspace, &space, &mut Folder::create(&export_root).unwrap()).unwrap();
        // The folder of the export, or the folder it is in.
        let mut files = FolderFiles::open(&root).unwrap();
        let imported = import(&workspace, &mut files, Some("Home again")).unwrap();
        assert_ne!(imported.space, space);
        check(&workspace, &imported);
        assert_eq!(files.read("../space.json").unwrap(), None, "nothing outside the export");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_export_as_a_zip_comes_back_as_a_new_space() {
        let (workspace, space) = space();
        let mut zip = Zip::new(Vec::new(), "aktujo Home 2026-11-06");
        export(&workspace, &space, &mut zip).unwrap();
        let mut files = ZipFiles::open(Cursor::new(zip.into_inner())).unwrap();
        let imported = import(&workspace, &mut files, None).unwrap();
        check(&workspace, &imported);
        let w = workspace.lock().unwrap();
        assert_eq!(w.spaces().unwrap().iter().filter(|s| s.name.as_deref() == Some("Home")).count(), 2);
    }

    #[test]
    fn a_zip_of_many_files_is_a_zip64_and_reads_back() {
        let mut zip = Zip::new(Vec::new(), "x");
        zip.file("space.json", b"{}").unwrap();
        for i in 0..70_000u32 {
            zip.file(&format!("f{i}"), &i.to_le_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let bytes = zip.into_inner();
        let end = &bytes[bytes.len() - 22..];
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 0xFFFF, "the count is in the ZIP64 end");
        let mut files = ZipFiles::open(Cursor::new(bytes)).unwrap();
        assert_eq!(files.entries.len(), 70_001);
        assert_eq!(files.read("f69999").unwrap(), Some(69_999u32.to_le_bytes().to_vec()));
        assert_eq!(files.read("space.json").unwrap().as_deref(), Some(&b"{}"[..]));
    }

    #[test]
    fn a_deflated_zip_reads_too() {
        // As a person might pack an export folder again: one file, deflated.
        let data = b"{\"format\": 1} {\"format\": 1} {\"format\": 1}".repeat(20);
        let packed = miniz_oxide::deflate::compress_to_vec(&data, 6);
        let name = b"e/space.json";
        let crc = crc32fast::hash(&data);
        let mut zip = vec![];
        let header = |zip: &mut Vec<u8>, signature: u32| {
            zip.extend_from_slice(&signature.to_le_bytes());
            if signature == 0x0201_4b50 {
                zip.extend_from_slice(&20u16.to_le_bytes());
            }
            zip.extend_from_slice(&20u16.to_le_bytes());
            zip.extend_from_slice(&0u16.to_le_bytes());
            zip.extend_from_slice(&8u16.to_le_bytes());
            zip.extend_from_slice(&[0; 4]);
            zip.extend_from_slice(&crc.to_le_bytes());
            zip.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            zip.extend_from_slice(&(data.len() as u32).to_le_bytes());
            zip.extend_from_slice(&(name.len() as u16).to_le_bytes());
            zip.extend_from_slice(&0u16.to_le_bytes());
        };
        header(&mut zip, 0x0403_4b50);
        zip.extend_from_slice(name);
        zip.extend_from_slice(&packed);
        let start = zip.len() as u32;
        header(&mut zip, 0x0201_4b50);
        zip.extend_from_slice(&[0; 10]);
        zip.extend_from_slice(&0u32.to_le_bytes());
        zip.extend_from_slice(name);
        let length = zip.len() as u32 - start;
        zip.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        zip.extend_from_slice(&[0; 4]);
        zip.extend_from_slice(&1u16.to_le_bytes());
        zip.extend_from_slice(&1u16.to_le_bytes());
        zip.extend_from_slice(&length.to_le_bytes());
        zip.extend_from_slice(&start.to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        let mut files = ZipFiles::open(Cursor::new(zip)).unwrap();
        assert_eq!(files.read("space.json").unwrap(), Some(data));
    }
}
