// SPDX-License-Identifier: MIT
//! Originals stored by their SHA-256 hash, on any storage: in memory, in a folder, and – with the feature `object`
//! or `s3` – in an object store such as S3. OPEN: kernoid.
//!
//! A store keeps bytes under the hash of those bytes and checks them on every read, so damaged or changed bytes are
//! reported, never returned. Storing the same bytes again changes nothing. Whether the bytes are plain or an
//! encrypted envelope is the caller's choice; the store only sees bytes.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::RwLock,
};

pub use aktujo_base::Sha256Hash;

#[cfg(feature = "object")]
mod object;

#[cfg(feature = "object")]
pub use object::{ObjectBlobs, S3Config};
#[cfg(feature = "object")]
pub use object_store;

/// Why a store failed.
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    /// The file system.
    #[error("file system: {0}")]
    Io(#[from] io::Error),
    /// The object store.
    #[error("object store: {0}")]
    Object(String),
    /// Stored bytes that do not have the hash they are stored under.
    #[error("the original {0} is damaged")]
    Damaged(Sha256Hash),
}

/// Where originals are kept, by their hash.
pub trait Blobs: Send + Sync {
    /// Stores bytes under their hash and returns it. Storing them again changes nothing.
    fn put(&self, bytes: &[u8]) -> Result<Sha256Hash, BlobError>;

    /// The bytes stored under `hash`, checked against it; none if there are none.
    fn get(&self, hash: &Sha256Hash) -> Result<Option<Vec<u8>>, BlobError>;

    /// Whether bytes are stored under `hash`, without reading or checking them.
    fn contains(&self, hash: &Sha256Hash) -> Result<bool, BlobError>;

    /// The hashes of everything stored, in order.
    fn list(&self) -> Result<Vec<Sha256Hash>, BlobError>;
}

/// The bytes if they have the hash, or [`BlobError::Damaged`].
fn checked(hash: &Sha256Hash, bytes: Vec<u8>) -> Result<Vec<u8>, BlobError> {
    if Sha256Hash::of(&bytes) == *hash { Ok(bytes) } else { Err(BlobError::Damaged(*hash)) }
}

/// Originals in memory, gone when the program ends – for tests and for what is only passed through.
#[derive(Debug, Default)]
pub struct MemoryBlobs {
    blobs: RwLock<BTreeMap<Sha256Hash, Vec<u8>>>,
}

impl MemoryBlobs {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Blobs for MemoryBlobs {
    fn put(&self, bytes: &[u8]) -> Result<Sha256Hash, BlobError> {
        let hash = Sha256Hash::of(bytes);
        self.blobs.write().expect("not poisoned").entry(hash).or_insert_with(|| bytes.to_vec());
        Ok(hash)
    }

    fn get(&self, hash: &Sha256Hash) -> Result<Option<Vec<u8>>, BlobError> {
        let bytes = self.blobs.read().expect("not poisoned").get(hash).cloned();
        bytes.map(|bytes| checked(hash, bytes)).transpose()
    }

    fn contains(&self, hash: &Sha256Hash) -> Result<bool, BlobError> {
        Ok(self.blobs.read().expect("not poisoned").contains_key(hash))
    }

    fn list(&self) -> Result<Vec<Sha256Hash>, BlobError> {
        Ok(self.blobs.read().expect("not poisoned").keys().copied().collect())
    }
}

/// Originals as files in a folder: `sha256/<first two digits>/<all 64 digits>`.
///
/// A file is written next to its place and renamed into it, so that a file under a hash is always complete.
#[derive(Debug, Clone)]
pub struct FolderBlobs {
    root: PathBuf,
}

impl FolderBlobs {
    /// The store in `folder`, created if needed.
    pub fn open(folder: &Path) -> Result<Self, BlobError> {
        let root = folder.join("sha256");
        fs::create_dir_all(&root)?;
        Ok(FolderBlobs { root })
    }

    fn path(&self, hash: &Sha256Hash) -> PathBuf {
        let hex = hash.hex();
        self.root.join(&hex[..2]).join(hex)
    }
}

impl Blobs for FolderBlobs {
    fn put(&self, bytes: &[u8]) -> Result<Sha256Hash, BlobError> {
        let hash = Sha256Hash::of(bytes);
        let path = self.path(&hash);
        if !path.exists() {
            fs::create_dir_all(path.parent().expect("a file in a folder"))?;
            let partial = path.with_extension(format!("partial-{}", hex_suffix()));
            fs::write(&partial, bytes)?;
            fs::rename(&partial, &path)?;
        }
        Ok(hash)
    }

    fn get(&self, hash: &Sha256Hash) -> Result<Option<Vec<u8>>, BlobError> {
        match fs::read(self.path(hash)) {
            Ok(bytes) => checked(hash, bytes).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn contains(&self, hash: &Sha256Hash) -> Result<bool, BlobError> {
        Ok(self.path(hash).is_file())
    }

    fn list(&self) -> Result<Vec<Sha256Hash>, BlobError> {
        let mut hashes = Vec::new();
        for folder in fs::read_dir(&self.root)? {
            let folder = folder?;
            if !folder.file_type()?.is_dir() {
                continue;
            }
            for file in fs::read_dir(folder.path())? {
                let name = file?.file_name();
                if let Some(hash) = name.to_str().and_then(|name| format!("sha256:{name}").parse().ok()) {
                    hashes.push(hash);
                }
            }
        }
        hashes.sort();
        Ok(hashes)
    }
}

/// A random suffix for a file being written, so that two writers never share one.
fn hex_suffix() -> String {
    let bytes: [u8; 8] = aktujo_base::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A temporary folder, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("aktujo-blob-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            Scratch(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    pub(crate) fn behaves(store: &dyn Blobs) {
        let hash = store.put(b"<Invoice/>").unwrap();
        assert_eq!(hash, Sha256Hash::of(b"<Invoice/>"));
        assert_eq!(store.put(b"<Invoice/>").unwrap(), hash);
        assert_eq!(store.get(&hash).unwrap().as_deref(), Some(&b"<Invoice/>"[..]));
        assert!(store.contains(&hash).unwrap());
        let other = Sha256Hash::of(b"other");
        assert_eq!(store.get(&other).unwrap(), None);
        assert!(!store.contains(&other).unwrap());
        let empty = store.put(b"").unwrap();
        let mut expected = vec![hash, empty];
        expected.sort();
        assert_eq!(store.list().unwrap(), expected);
    }

    #[test]
    fn in_memory() {
        behaves(&MemoryBlobs::new());
    }

    #[test]
    fn in_a_folder() {
        let scratch = Scratch::new("folder");
        let store = FolderBlobs::open(&scratch.0).unwrap();
        behaves(&store);
        let reopened = FolderBlobs::open(&scratch.0).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 2);
    }

    #[test]
    fn damaged_bytes_are_reported() {
        let scratch = Scratch::new("damaged");
        let store = FolderBlobs::open(&scratch.0).unwrap();
        let hash = store.put(b"original").unwrap();
        fs::write(store.path(&hash), b"changed").unwrap();
        assert!(matches!(store.get(&hash), Err(BlobError::Damaged(h)) if h == hash));
    }
}
