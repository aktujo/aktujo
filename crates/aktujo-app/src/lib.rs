// SPDX-License-Identifier: AGPL-3.0-or-later
//! The workspace of aktujo: every command and query. The terminal interface, the API and the command line work
//! through it only – never with the store or the domain on their own.
//!
//! A workspace is one data folder on one device (`docs/rfc/0004-archive-0.1.md`): its spaces with their documents,
//! kept in a replica of the core (`aktujo-sync`) – the events in a database encrypted with SQLCipher, the originals
//! encrypted in a folder beside it, the key in the key store of the operating system. A space can be synchronised
//! through a relay and shared with other people's devices, with rights.
//!
//! The state of each space is folded from its log when the workspace opens and again after every change; how far
//! that carries is to be measured (OPEN). Text is read in the background ([`Worker`]): with OCR and its models built
//! in, with the feature `ocr`; without it, from text layers only (`aktujo-minimal`).
//!
//! The items of the starter are still here, in memory, until the interfaces show the archive.

use std::{collections::BTreeMap, fs, io, path::Path, sync::Arc};

use aktujo_blob::{BlobError, Blobs, MemoryBlobs};
use aktujo_core::{DomainError, Item, SPACE_KINDS, SpaceEvent, label, space_name, title, title_of_file};
use aktujo_crypt::{Domain, RecoveryCode};
use aktujo_doc::{DocumentEvent, Documents, Origin, Original, Skipped, ValueError, builtin};
use aktujo_log::DeviceId;
use aktujo_store::Store as EventStore;
use aktujo_sync::{Replica, SyncError, now_ms};
use serde::{Deserialize, Serialize};

mod items;
mod keys;
#[cfg(feature = "ocr")]
mod models;
mod reading;
mod sealed;

pub use aktujo_base::{Sha256Hash, SpaceId};
pub use aktujo_crypt::{Member, Rights};
pub use aktujo_doc::{Document, DocumentId, Kind, Reading, Value};
pub use aktujo_extract::{Extraction, Line, Method, Page};
pub use aktujo_sync::SyncReport;
pub use items::StoreError;
pub use keys::{KeyError, KeyStore, MemoryKeys, SystemKeys};
pub use reading::{Job, ReadState, Reader, Worker, read_all};
pub use sealed::SealedBlobs;

/// The database of a data folder.
const DATABASE: &str = "aktujo.sqlite";
/// The description of a data folder: its device name and the name of its key.
const FOLDER: &str = "aktujo.json";
/// The folder of the originals in a data folder.
const ORIGINALS: &str = "originals";

/// Why a command did not happen.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// What was asked is not valid.
    #[error(transparent)]
    Invalid(#[from] DomainError),
    /// A value that is not well formed or does not fit its field.
    #[error(transparent)]
    Value(#[from] ValueError),
    /// The store of the starter's items.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The database of the data folder.
    #[error(transparent)]
    Database(#[from] aktujo_store::StoreError),
    /// The originals.
    #[error(transparent)]
    Originals(#[from] BlobError),
    /// Synchronising, or the replica.
    #[error(transparent)]
    Sync(#[from] SyncError),
    /// The key store.
    #[error(transparent)]
    Keys(#[from] KeyError),
    /// The data folder.
    #[error("the data folder: {0}")]
    Folder(String),
    /// Reading text: the models, or text kept in a form not known.
    #[error("reading: {0}")]
    Reading(String),
    /// The data folder is there, but its key is not in the key store.
    #[error("the key of this data folder ({0}) is not in the key store")]
    KeyMissing(String),
    /// No space, document, kind or original by this name.
    #[error("{0}")]
    NotFound(String),
    /// This device may not do this in the space.
    #[error("{0}")]
    NotAllowed(String),
}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        AppError::Folder(error.to_string())
    }
}

/// What a data folder says about itself; not secret – the secret is in the key store.
#[derive(Debug, Serialize, Deserialize)]
struct FolderInfo {
    /// The version of this description.
    format: u32,
    /// The device name this folder writes events under.
    device: DeviceId,
    /// The name of the secret in the key store, without its prefix.
    key: String,
}

/// A file to add: its name, its media type and its bytes.
#[derive(Debug, Clone, Copy)]
pub struct NewFile<'a> {
    /// The file name it came with.
    pub name: &'a str,
    /// The media type, e.g. `application/pdf`.
    pub media_type: &'a str,
    /// The bytes.
    pub bytes: &'a [u8],
}

/// What adding a file did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    /// The document the file is in.
    pub document: DocumentId,
    /// Whether it is a new document – false if the same bytes were in the space already.
    pub new: bool,
}

/// A space as the workspace lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceInfo {
    /// The identity.
    pub id: SpaceId,
    /// Its name, once one arrived.
    pub name: Option<String>,
    /// Documents outside the bin.
    pub documents: usize,
    /// Documents in the bin.
    pub trashed: usize,
    /// The relay it is synchronised through, if it is.
    pub relay: Option<String>,
    /// What this device may do – `None` for a space only on this device, and for one still waiting to be added.
    pub rights: Option<Rights>,
    /// Whether this device asked to join and waits to be added.
    pub waiting: bool,
}

/// The state of one space, folded from its log.
#[derive(Debug, Default)]
struct View {
    name: Option<String>,
    documents: Documents,
    skipped: Vec<Skipped>,
}

/// One workspace: the data and what can be done with it.
#[derive(Debug)]
pub struct Workspace {
    replica: Replica,
    views: BTreeMap<SpaceId, View>,
    /// The document being read now, if any.
    reading: Option<(SpaceId, DocumentId)>,
    items: items::Store,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// A new, empty workspace in memory, gone when dropped.
    pub fn new() -> Self {
        Self::in_memory("this-device").expect("a workspace in memory opens")
    }

    /// A workspace in memory, writing under the device name `device` – for tests and trials.
    pub fn in_memory(device: &str) -> Result<Self, AppError> {
        let device = device.parse().map_err(|_| AppError::Folder(format!("{device:?} is not a device name")))?;
        let store = EventStore::in_memory()?;
        Self::on(Replica::open(Domain::AKTUJO, device, store, Arc::new(MemoryBlobs::new()))?)
    }

    /// The workspace in the data folder `folder`, made if it is not there: the database encrypted with SQLCipher,
    /// the originals encrypted beside it, the secret of both in `keys`.
    pub fn open(folder: &Path, keys: &dyn KeyStore) -> Result<Self, AppError> {
        fs::create_dir_all(folder)?;
        let info_path = folder.join(FOLDER);
        let info: FolderInfo = match fs::read(&info_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| AppError::Folder(format!("{} cannot be read: {e}", info_path.display())))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if folder.join(DATABASE).exists() {
                    return Err(AppError::Folder(format!("{} is missing", info_path.display())));
                }
                let device: [u8; 4] = aktujo_base::random();
                let key: [u8; 16] = aktujo_base::random();
                let info = FolderInfo {
                    format: 1,
                    device: format!("device-{}", hex::encode(device)).parse().expect("a valid device name"),
                    key: hex::encode(key),
                };
                fs::write(&info_path, serde_json::to_vec_pretty(&info).expect("JSON"))?;
                info
            }
            Err(error) => return Err(error.into()),
        };
        if info.format != 1 {
            return Err(AppError::Folder(format!("format {} of the data folder is not known", info.format)));
        }
        let name = format!("data-{}", info.key);
        let secret = match keys.get(&name)? {
            Some(secret) => secret,
            None if folder.join(DATABASE).exists() => return Err(AppError::KeyMissing(name)),
            None => {
                let secret: [u8; 32] = aktujo_base::random();
                keys.set(&name, &secret)?;
                if keys.get(&name)? != Some(secret) {
                    return Err(AppError::Keys(KeyError("the key store did not keep the key".into())));
                }
                secret
            }
        };
        let derived = keys::FolderKeys::derive(&secret);
        let store = EventStore::open_encrypted(&folder.join(DATABASE), &derived.database)?;
        let blobs: Arc<dyn Blobs> = Arc::new(SealedBlobs::open(&folder.join(ORIGINALS), derived.originals)?);
        Self::on(Replica::open(Domain::AKTUJO, info.device, store, blobs)?)
    }

    fn on(replica: Replica) -> Result<Self, AppError> {
        let mut workspace =
            Workspace { replica, views: BTreeMap::new(), reading: None, items: items::Store::default() };
        for space in workspace.replica.spaces()? {
            workspace.refresh(&space)?;
        }
        Ok(workspace)
    }

    /// The device name this workspace writes events under.
    pub fn device(&self) -> &str {
        self.replica.device().as_str()
    }

    /// Folds the state of a space from its log again.
    fn refresh(&mut self, space: &SpaceId) -> Result<(), AppError> {
        let log = self.replica.log(space)?;
        let (documents, skipped) = Documents::from_log(&log);
        let mut name = None;
        for (_, event) in log.ordered() {
            if SPACE_KINDS.contains(&event.kind.as_str())
                && let Ok(SpaceEvent::SpaceNamed { name: named }) = event.payload::<SpaceEvent>()
            {
                name = Some(named);
            }
        }
        self.views.insert(space.clone(), View { name, documents, skipped });
        Ok(())
    }

    fn view(&self, space: &SpaceId) -> Result<&View, AppError> {
        self.views.get(space).ok_or_else(|| AppError::NotFound(format!("no space {space}")))
    }

    /// Whether this device may write in the space: always in one only here; in a synchronised one with the right
    /// to write, once it was added.
    fn may_write(&self, space: &SpaceId) -> Result<(), AppError> {
        self.view(space)?;
        if self.replica.relay(space)?.is_none() {
            return Ok(());
        }
        match self.replica.rights(space)? {
            Some(rights) if rights >= Rights::Write => Ok(()),
            Some(_) => Err(AppError::NotAllowed("this device may only read in this space".into())),
            None => Err(AppError::NotAllowed("this device waits to be added to this space".into())),
        }
    }

    fn write(&mut self, space: &SpaceId, payload: &impl Serialize) -> Result<(), AppError> {
        self.may_write(space)?;
        self.replica.write(space, payload)?;
        self.refresh(space)
    }

    // Spaces

    /// Every space on this device, in the order of their identities – that is, of when they were founded.
    pub fn spaces(&self) -> Result<Vec<SpaceInfo>, AppError> {
        let mut spaces = Vec::new();
        for (id, view) in &self.views {
            let relay = self.replica.relay(id)?;
            let rights = self.replica.rights(id)?;
            spaces.push(SpaceInfo {
                id: id.clone(),
                name: view.name.clone(),
                documents: view.documents.active().count(),
                trashed: view.documents.trashed().count(),
                waiting: relay.is_some() && rights.is_none(),
                relay,
                rights,
            });
        }
        Ok(spaces)
    }

    /// Founds a space with a name, on this device only until it is synchronised.
    pub fn found_space(&mut self, name: &str) -> Result<SpaceId, AppError> {
        let name = space_name(name)?;
        let space = self.replica.new_space();
        self.views.insert(space.clone(), View::default());
        self.write(&space, &SpaceEvent::SpaceNamed { name })?;
        Ok(space)
    }

    /// Gives a space a new name.
    pub fn rename_space(&mut self, space: &SpaceId, name: &str) -> Result<(), AppError> {
        let name = space_name(name)?;
        if self.view(space)?.name.as_deref() == Some(name.as_str()) {
            return Ok(());
        }
        self.write(space, &SpaceEvent::SpaceNamed { name })
    }

    /// Events about documents in a space that could not be applied – reported, never resolved silently.
    pub fn problems(&self, space: &SpaceId) -> Result<&[Skipped], AppError> {
        Ok(&self.view(space)?.skipped)
    }

    // Documents

    /// The documents of a space outside the bin, the newest first.
    pub fn documents(&self, space: &SpaceId) -> Result<Vec<&Document>, AppError> {
        let mut documents: Vec<&Document> = self.view(space)?.documents.active().collect();
        documents.sort_by(|a, b| b.added.cmp(&a.added));
        Ok(documents)
    }

    /// The documents of a space in the bin, the newest first.
    pub fn trashed(&self, space: &SpaceId) -> Result<Vec<&Document>, AppError> {
        let mut documents: Vec<&Document> = self.view(space)?.documents.trashed().collect();
        documents.sort_by(|a, b| b.added.cmp(&a.added));
        Ok(documents)
    }

    /// A document of a space, in the bin or not.
    pub fn document(&self, space: &SpaceId, id: &DocumentId) -> Result<&Document, AppError> {
        self.view(space)?.documents.get(id).ok_or_else(|| AppError::NotFound(format!("no document {id}")))
    }

    /// The bytes of an original.
    pub fn original(&self, hash: &Sha256Hash) -> Result<Vec<u8>, AppError> {
        self.replica.original(hash)?.ok_or_else(|| AppError::NotFound(format!("no original {hash}")))
    }

    /// The kinds of document there are.
    pub fn kinds(&self) -> Vec<Kind> {
        builtin()
    }

    fn original_of(&self, space: &SpaceId, file: &NewFile<'_>) -> Result<Original, AppError> {
        let name = file.name.trim();
        let media_type = file.media_type.trim();
        if name.is_empty() || media_type.is_empty() {
            return Err(AppError::Folder("a file needs a name and a media type".into()));
        }
        let hash = self.replica.put_original(space, file.bytes)?;
        Ok(Original { hash, name: name.to_owned(), media_type: media_type.to_owned(), size: file.bytes.len() as u64 })
    }

    /// Adds a file as a new document, titled after its name. The same bytes twice in a space are one document: then
    /// nothing is written and that document is returned.
    pub fn add_document(&mut self, space: &SpaceId, file: NewFile<'_>) -> Result<Added, AppError> {
        self.may_write(space)?;
        let hash = Sha256Hash::of(file.bytes);
        let view = self.view(space)?;
        if let Some(document) = view
            .documents
            .active()
            .chain(view.documents.trashed())
            .find(|d| d.versions.iter().any(|v| v.original.hash == hash))
        {
            return Ok(Added { document: document.id.clone(), new: false });
        }
        let original = self.original_of(space, &file)?;
        let document = DocumentId::generate(now_ms());
        let title = title_of_file(file.name);
        self.write(space, &DocumentEvent::DocumentAdded { document: document.clone(), title, original })?;
        Ok(Added { document, new: true })
    }

    /// Adds a file as the newest version of a document.
    pub fn add_version(&mut self, space: &SpaceId, id: &DocumentId, file: NewFile<'_>) -> Result<(), AppError> {
        self.may_write(space)?;
        let hash = Sha256Hash::of(file.bytes);
        if self.document(space, id)?.current().original.hash == hash {
            return Ok(());
        }
        let original = self.original_of(space, &file)?;
        self.write(space, &DocumentEvent::DocumentVersion { document: id.clone(), original })
    }

    /// Gives a document a new title.
    pub fn set_title(&mut self, space: &SpaceId, id: &DocumentId, text: &str) -> Result<(), AppError> {
        let title = title(text)?;
        if self.document(space, id)?.title == title {
            return Ok(());
        }
        self.write(space, &DocumentEvent::DocumentTitle { document: id.clone(), title })
    }

    /// Confirms the kind of a document – or that it has none.
    pub fn set_kind(&mut self, space: &SpaceId, id: &DocumentId, kind: Option<&str>) -> Result<(), AppError> {
        self.document(space, id)?;
        if let Some(kind) = kind
            && !builtin().iter().any(|k| k.id == kind)
        {
            return Err(AppError::NotFound(format!("no kind {kind:?}")));
        }
        let event = DocumentEvent::DocumentKind {
            document: id.clone(),
            kind: kind.map(str::to_owned),
            origin: Origin::Confirmed,
        };
        self.write(space, &event)
    }

    /// Confirms the value of a field; it must fit the field of the document's kind, if it has one.
    pub fn set_field(&mut self, space: &SpaceId, id: &DocumentId, key: &str, value: Value) -> Result<(), AppError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::NotFound("a field needs a key".into()));
        }
        let document = self.document(space, id)?;
        let kind = document.kind.as_ref().or(document.kind_suggestion.as_ref().map(|s| &s.kind));
        match kind.and_then(|kind| builtin().into_iter().find(|k| &k.id == kind)) {
            Some(kind) => kind.check(key, &value)?,
            None => value.check()?,
        }
        let event = DocumentEvent::DocumentField {
            document: id.clone(),
            key: key.to_owned(),
            value,
            origin: Origin::Confirmed,
        };
        self.write(space, &event)
    }

    /// Empties a field: its confirmed value and its suggestion.
    pub fn clear_field(&mut self, space: &SpaceId, id: &DocumentId, key: &str) -> Result<(), AppError> {
        if !self.document(space, id)?.fields.contains_key(key) {
            return Ok(());
        }
        self.write(space, &DocumentEvent::DocumentFieldCleared { document: id.clone(), key: key.to_owned() })
    }

    /// Adds a label to a document.
    pub fn add_label(&mut self, space: &SpaceId, id: &DocumentId, text: &str) -> Result<(), AppError> {
        let label = label(text)?;
        if self.document(space, id)?.labels.contains(&label) {
            return Ok(());
        }
        self.write(space, &DocumentEvent::DocumentLabel { document: id.clone(), label })
    }

    /// Takes a label off a document.
    pub fn remove_label(&mut self, space: &SpaceId, id: &DocumentId, label: &str) -> Result<(), AppError> {
        if !self.document(space, id)?.labels.contains(label) {
            return Ok(());
        }
        self.write(space, &DocumentEvent::DocumentUnlabel { document: id.clone(), label: label.to_owned() })
    }

    /// Puts a document in the bin – nothing is deleted.
    pub fn trash(&mut self, space: &SpaceId, id: &DocumentId) -> Result<(), AppError> {
        if self.document(space, id)?.trashed {
            return Ok(());
        }
        self.write(space, &DocumentEvent::DocumentTrashed { document: id.clone() })
    }

    /// Takes a document out of the bin.
    pub fn restore(&mut self, space: &SpaceId, id: &DocumentId) -> Result<(), AppError> {
        if !self.document(space, id)?.trashed {
            return Ok(());
        }
        self.write(space, &DocumentEvent::DocumentRestored { document: id.clone() })
    }

    // Sync and sharing

    /// Starts synchronising a space through a relay, as this device with a name people read. Returns the recovery
    /// code: the person keeps it on paper – without it and without a device, the space is lost.
    pub fn start_sync(&mut self, space: &SpaceId, relay: &str, name: Option<&str>) -> Result<String, AppError> {
        self.view(space)?;
        let code = self.replica.start_sync(space, relay, name)?;
        self.refresh(space)?;
        Ok(code.to_string())
    }

    /// An invitation for another device – the person's own or someone else's – with what it may do, good for a day.
    pub fn invite(&self, space: &SpaceId, rights: Rights) -> Result<String, AppError> {
        self.view(space)?;
        Ok(self.replica.invite(space, rights)?)
    }

    /// Asks to join a space with an invitation; its documents arrive once a device that may manage it added this
    /// one, at a later synchronisation.
    pub fn join(&mut self, invitation: &str, name: Option<&str>) -> Result<SpaceId, AppError> {
        let space = self.replica.join(invitation, name)?;
        self.refresh(&space)?;
        Ok(space)
    }

    /// Brings a space back to this device with its recovery code, when no other device of it is left.
    pub fn recover(
        &mut self,
        relay: &str,
        space: &SpaceId,
        code: &str,
        name: Option<&str>,
    ) -> Result<SyncReport, AppError> {
        let code: RecoveryCode = code.parse().map_err(SyncError::from)?;
        let report = self.replica.recover(relay, space, &code, name)?;
        self.refresh(space)?;
        Ok(report)
    }

    /// The members of a synchronised space, as its relay lists them.
    pub fn members(&self, space: &SpaceId) -> Result<Vec<Member>, AppError> {
        self.view(space)?;
        Ok(self.replica.members(space)?)
    }

    /// Removes a member – by its name, device name or identity; it gets nothing written after. Returns its name.
    pub fn remove_member(&mut self, space: &SpaceId, who: &str) -> Result<String, AppError> {
        self.view(space)?;
        Ok(self.replica.remove_member(space, who)?)
    }

    /// Synchronises a space with its relay.
    pub fn sync(&mut self, space: &SpaceId) -> Result<SyncReport, AppError> {
        self.view(space)?;
        let report = self.replica.sync(space)?;
        self.refresh(space)?;
        Ok(report)
    }

    /// Synchronises every space that is synchronised.
    pub fn sync_all(&mut self) -> Result<BTreeMap<SpaceId, SyncReport>, AppError> {
        let reports = self.replica.sync_all()?;
        for space in reports.keys() {
            self.refresh(space)?;
        }
        Ok(reports)
    }

    // The starter's items

    /// Adds an item.
    pub fn add_item(&mut self, text: &str) -> Result<Item, AppError> {
        Ok(self.items.insert(title(text)?))
    }

    /// Removes an item.
    pub fn remove_item(&mut self, id: u64) -> Result<Item, AppError> {
        Ok(self.items.remove(id)?)
    }

    /// Every item.
    pub fn items(&self) -> &[Item] {
        self.items.items()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A folder of its own for a test, removed after it.
    pub(crate) struct Scratch(pub PathBuf);

    impl Scratch {
        pub(crate) fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("aktujo-app-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            Scratch(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn pdf(bytes: &[u8]) -> NewFile<'_> {
        NewFile { name: "Lease 2026.pdf", media_type: "application/pdf", bytes }
    }

    fn money(amount: &str) -> Value {
        Value::Money { amount: amount.into(), currency: "EUR".into() }
    }

    #[test]
    fn items_are_added_and_removed() {
        let mut workspace = Workspace::new();
        let item = workspace.add_item(" bread ").unwrap();
        assert_eq!(item.title, "bread");
        assert!(matches!(workspace.add_item(""), Err(AppError::Invalid(DomainError::EmptyTitle))));
        assert_eq!(workspace.remove_item(item.id).unwrap(), item);
        assert!(matches!(workspace.remove_item(item.id), Err(AppError::Store(StoreError::NotFound(_)))));
        assert!(workspace.items().is_empty());
    }

    #[test]
    fn spaces_are_founded_named_and_listed() {
        let mut workspace = Workspace::in_memory("laptop").unwrap();
        assert!(workspace.spaces().unwrap().is_empty());
        let home = workspace.found_space(" Home ").unwrap();
        let work = workspace.found_space("Business").unwrap();
        workspace.rename_space(&work, "Business Ltd").unwrap();
        let spaces = workspace.spaces().unwrap();
        assert_eq!(spaces.iter().map(|s| s.name.as_deref().unwrap()).collect::<Vec<_>>(), ["Home", "Business Ltd"]);
        assert!(spaces.iter().all(|s| s.relay.is_none() && s.rights.is_none() && !s.waiting));
        assert!(matches!(workspace.found_space("  "), Err(AppError::Invalid(DomainError::EmptySpaceName))));
        assert!(workspace.documents(&home).unwrap().is_empty());
        let unknown: SpaceId = "0192f1e4-0000-7000-8000-0000000000b1".parse().unwrap();
        assert!(matches!(workspace.documents(&unknown), Err(AppError::NotFound(_))));
    }

    #[test]
    fn a_document_is_added_labelled_versioned_and_binned() {
        let mut workspace = Workspace::in_memory("laptop").unwrap();
        let space = workspace.found_space("Home").unwrap();
        let added = workspace.add_document(&space, pdf(b"%PDF-1.7 lease")).unwrap();
        assert!(added.new);
        let id = added.document;
        assert_eq!(
            workspace.add_document(&space, pdf(b"%PDF-1.7 lease")).unwrap(),
            Added { document: id.clone(), new: false }
        );

        let document = workspace.document(&space, &id).unwrap();
        assert_eq!(document.title, "Lease 2026");
        assert_eq!(workspace.original(&document.current().original.hash).unwrap(), b"%PDF-1.7 lease");

        workspace.set_title(&space, &id, "Lease of the flat").unwrap();
        workspace.add_label(&space, &id, " Flat ").unwrap();
        workspace.add_label(&space, &id, "Flat").unwrap();
        workspace.set_kind(&space, &id, Some("contract")).unwrap();
        assert!(matches!(workspace.set_kind(&space, &id, Some("spaceship")), Err(AppError::NotFound(_))));
        workspace.set_field(&space, &id, "deposit", money("1500.00")).unwrap();
        assert!(matches!(
            workspace.set_field(&space, &id, "total", Value::Date("2026-02-30".into())),
            Err(AppError::Value(_))
        ));
        workspace.add_version(&space, &id, pdf(b"%PDF-1.7 lease, signed")).unwrap();

        let document = workspace.document(&space, &id).unwrap();
        assert_eq!(document.title, "Lease of the flat");
        assert_eq!(document.labels.iter().collect::<Vec<_>>(), ["Flat"]);
        assert_eq!(document.kind.as_deref(), Some("contract"));
        assert_eq!(document.fields["deposit"].value(), Some(&money("1500.00")));
        assert_eq!(document.versions.len(), 2);

        workspace.remove_label(&space, &id, "Flat").unwrap();
        workspace.clear_field(&space, &id, "deposit").unwrap();
        workspace.trash(&space, &id).unwrap();
        assert!(workspace.documents(&space).unwrap().is_empty());
        assert_eq!(workspace.trashed(&space).unwrap().len(), 1);
        assert_eq!(workspace.spaces().unwrap()[0].trashed, 1);
        // The same bytes again find the document in the bin.
        assert!(!workspace.add_document(&space, pdf(b"%PDF-1.7 lease")).unwrap().new);
        workspace.restore(&space, &id).unwrap();
        let document = workspace.document(&space, &id).unwrap();
        assert!(document.labels.is_empty() && !document.fields.contains_key("deposit") && !document.trashed);
        assert!(workspace.problems(&space).unwrap().is_empty());
    }

    #[test]
    fn a_data_folder_is_encrypted_and_opens_only_with_its_key() {
        let scratch = Scratch::new("folder");
        let keys = MemoryKeys::default();
        let (space, id) = {
            let mut workspace = Workspace::open(&scratch.0, &keys).unwrap();
            let space = workspace.found_space("Home").unwrap();
            let id = workspace.add_document(&space, pdf(b"%PDF-1.7 a secret lease")).unwrap().document;
            workspace.set_title(&space, &id, "Confidential lease").unwrap();
            (space, id)
        };
        let reopened = Workspace::open(&scratch.0, &keys).unwrap();
        assert_eq!(reopened.document(&space, &id).unwrap().title, "Confidential lease");
        assert!(reopened.device().starts_with("device-"));

        // Nothing in the folder reads as what was written.
        let mut files = vec![scratch.0.clone()];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            } else {
                let bytes = fs::read(&path).unwrap();
                for word in [&b"Confidential"[..], b"secret lease", b"Home"] {
                    assert!(!bytes.windows(word.len()).any(|w| w == word), "{} holds {word:?}", path.display());
                }
            }
        }
        assert!(matches!(Workspace::open(&scratch.0, &MemoryKeys::default()), Err(AppError::KeyMissing(_))));
    }
}
