// SPDX-License-Identifier: MIT
//! Where the key of a data folder is kept, and the keys derived from it (`docs/rfc/0004-archive-0.1.md`, decision 7).
//!
//! A data folder has one secret of 32 random bytes, kept outside the folder – in the key store of the operating
//! system (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux). From it come the key of the
//! database (SQLCipher) and the key of the originals, by HKDF-SHA256 with a label each. A copy of the folder alone
//! reads as noise. Where there is no key store – a server, a container – the secret can be kept in a file of its own
//! instead ([`FileKeys`]), outside the data folder. OPEN: that file protected by a passphrase.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use hkdf::Hkdf;
use sha2::Sha256;

/// The service name entries are kept under in the key store of the operating system.
#[cfg(feature = "system-keys")]
const SERVICE: &str = "aktujo";

/// Why a key could not be read or kept.
#[derive(Debug, thiserror::Error)]
#[error("the key store: {0}")]
pub struct KeyError(pub String);

/// A place that keeps secrets of 32 bytes by name.
pub trait KeyStore: Send + Sync {
    /// The secret kept under `name`, if any.
    fn get(&self, name: &str) -> Result<Option<[u8; 32]>, KeyError>;
    /// Keeps `secret` under `name`.
    fn set(&self, name: &str, secret: &[u8; 32]) -> Result<(), KeyError>;
}

/// The key store of the operating system (feature `system-keys`).
#[cfg(feature = "system-keys")]
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemKeys;

#[cfg(feature = "system-keys")]
impl SystemKeys {
    fn entry(name: &str) -> Result<keyring::Entry, KeyError> {
        keyring::Entry::new(SERVICE, name).map_err(|e| KeyError(e.to_string()))
    }
}

#[cfg(feature = "system-keys")]
impl KeyStore for SystemKeys {
    fn get(&self, name: &str) -> Result<Option<[u8; 32]>, KeyError> {
        match Self::entry(name)?.get_secret() {
            Ok(secret) => {
                let secret: [u8; 32] =
                    secret.try_into().map_err(|_| KeyError(format!("the entry {name:?} is not 32 bytes")))?;
                Ok(Some(secret))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(KeyError(error.to_string())),
        }
    }

    fn set(&self, name: &str, secret: &[u8; 32]) -> Result<(), KeyError> {
        Self::entry(name)?.set_secret(secret).map_err(|e| KeyError(e.to_string()))
    }
}

/// The secret of one data folder in a file of its own, as 64 hexadecimal digits – for a server or a container,
/// where there is no key store: a secret mounted into the container, say. Whoever has the file and the data folder
/// can read the data, so keep the file apart from the folder and its backups. The file holds one secret, whatever
/// the name; a file that is there is never overwritten.
#[derive(Debug, Clone)]
pub struct FileKeys {
    path: PathBuf,
}

impl FileKeys {
    /// The secret in the file at `path`, made by the first [`KeyStore::set`] if it is not there.
    pub fn new(path: &Path) -> Self {
        FileKeys { path: path.to_path_buf() }
    }
}

impl KeyStore for FileKeys {
    fn get(&self, _name: &str) -> Result<Option<[u8; 32]>, KeyError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(KeyError(format!("{}: {error}", self.path.display()))),
        };
        let mut secret = [0u8; 32];
        hex::decode_to_slice(text.trim(), &mut secret)
            .map_err(|_| KeyError(format!("{} does not hold 64 hexadecimal digits", self.path.display())))?;
        Ok(Some(secret))
    }

    fn set(&self, _name: &str, secret: &[u8; 32]) -> Result<(), KeyError> {
        let failed = |error: io::Error| KeyError(format!("{}: {error}", self.path.display()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&self.path).map_err(failed)?;
        io::Write::write_all(&mut file, format!("{}\n", hex::encode(secret)).as_bytes()).map_err(failed)
    }
}

/// Secrets in memory, gone when dropped – for tests and for a workspace that is not kept.
#[derive(Debug, Default)]
pub struct MemoryKeys(Mutex<BTreeMap<String, [u8; 32]>>);

impl KeyStore for MemoryKeys {
    fn get(&self, name: &str) -> Result<Option<[u8; 32]>, KeyError> {
        Ok(self.0.lock().expect("not poisoned").get(name).copied())
    }

    fn set(&self, name: &str, secret: &[u8; 32]) -> Result<(), KeyError> {
        self.0.lock().expect("not poisoned").insert(name.to_owned(), *secret);
        Ok(())
    }
}

/// The keys of a data folder, derived from its secret.
pub(crate) struct FolderKeys {
    /// The key of the database.
    pub database: [u8; 32],
    /// The key of the originals.
    pub originals: [u8; 32],
}

impl std::fmt::Debug for FolderKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FolderKeys(…)")
    }
}

impl FolderKeys {
    pub(crate) fn derive(secret: &[u8; 32]) -> Self {
        let key = |label: &str| {
            let mut out = [0u8; 32];
            Hkdf::<Sha256>::new(None, secret).expand(label.as_bytes(), &mut out).expect("32 bytes from HKDF-SHA256");
            out
        };
        FolderKeys { database: key("aktujo/data database"), originals: key("aktujo/data originals") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_differ_and_follow_from_the_secret() {
        let a = FolderKeys::derive(&[1; 32]);
        let b = FolderKeys::derive(&[1; 32]);
        let c = FolderKeys::derive(&[2; 32]);
        assert_ne!(a.database, a.originals);
        assert_eq!(a.database, b.database);
        assert_ne!(a.database, c.database);
    }

    #[test]
    fn file_keys_keep_one_secret_and_never_overwrite_it() {
        let path = std::env::temp_dir().join(format!("aktujo-key-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        let keys = FileKeys::new(&path);
        assert_eq!(keys.get("x").unwrap(), None);
        keys.set("x", &[7; 32]).unwrap();
        assert_eq!(keys.get("anything").unwrap(), Some([7; 32]));
        assert!(keys.set("x", &[8; 32]).is_err());
        assert_eq!(keys.get("x").unwrap(), Some([7; 32]));
        fs::write(&path, "not hex").unwrap();
        assert!(keys.get("x").is_err());
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn memory_keys_keep_what_they_get() {
        let keys = MemoryKeys::default();
        assert_eq!(keys.get("x").unwrap(), None);
        keys.set("x", &[3; 32]).unwrap();
        assert_eq!(keys.get("x").unwrap(), Some([3; 32]));
    }
}
