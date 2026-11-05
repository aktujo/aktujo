// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where the key of a data folder is kept, and the keys derived from it (`docs/rfc/0004-archive-0.1.md`, decision 7).
//!
//! A data folder has one secret of 32 random bytes, kept outside the folder – in the key store of the operating
//! system (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux). From it come the key of the
//! database (SQLCipher) and the key of the originals, by HKDF-SHA256 with a label each. A copy of the folder alone
//! reads as noise. OPEN: a file protected by a passphrase, where there is no key store (a server, a container).

use std::{collections::BTreeMap, sync::Mutex};

use hkdf::Hkdf;
use sha2::Sha256;

/// The service name entries are kept under in the key store of the operating system.
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

/// The key store of the operating system.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemKeys;

impl SystemKeys {
    fn entry(name: &str) -> Result<keyring::Entry, KeyError> {
        keyring::Entry::new(SERVICE, name).map_err(|e| KeyError(e.to_string()))
    }
}

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
    fn memory_keys_keep_what_they_get() {
        let keys = MemoryKeys::default();
        assert_eq!(keys.get("x").unwrap(), None);
        keys.set("x", &[3; 32]).unwrap();
        assert_eq!(keys.get("x").unwrap(), Some([3; 32]));
    }
}
