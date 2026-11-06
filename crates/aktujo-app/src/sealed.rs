// SPDX-License-Identifier: AGPL-3.0-or-later
//! Originals in a folder, encrypted at rest (`docs/rfc/0004-archive-0.1.md`, decision 7).
//!
//! Each original is kept under the hash of its plain bytes, as in `aktujo-blob`'s folder, but as a random nonce of
//! 24 bytes followed by the bytes encrypted with XChaCha20-Poly1305; the hash is bound to them as associated data,
//! so that an original moved to another name does not open. On reading, the hash of the opened bytes is checked
//! again. OPEN: whether this moves into the core, beside `aktujo-blob`'s folder.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use aktujo_base::{Sha256Hash, random};
use aktujo_blob::{BlobError, Blobs};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};

/// Originals encrypted in a folder.
pub struct SealedBlobs {
    root: PathBuf,
    key: [u8; 32],
}

impl std::fmt::Debug for SealedBlobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedBlobs").field("root", &self.root).finish_non_exhaustive()
    }
}

impl SealedBlobs {
    /// The originals in `folder` under `key`; the folder is made if needed.
    pub fn open(folder: &Path, key: [u8; 32]) -> Result<Self, BlobError> {
        let root = folder.join("sha256");
        fs::create_dir_all(&root)?;
        Ok(SealedBlobs { root, key })
    }

    fn path(&self, hash: &Sha256Hash) -> PathBuf {
        let hex = hash.hex();
        self.root.join(&hex[..2]).join(hex)
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&self.key.into())
    }
}

impl Blobs for SealedBlobs {
    fn put(&self, bytes: &[u8]) -> Result<Sha256Hash, BlobError> {
        let hash = Sha256Hash::of(bytes);
        let path = self.path(&hash);
        if !path.exists() {
            let nonce: [u8; 24] = random();
            let aad = hash.to_string();
            let sealed = self
                .cipher()
                .encrypt(&XNonce::from(nonce), Payload { msg: bytes, aad: aad.as_bytes() })
                .expect("XChaCha20-Poly1305 encrypts any length");
            fs::create_dir_all(path.parent().expect("a file in a folder"))?;
            let suffix: [u8; 8] = random();
            let partial = path.with_extension(format!("partial-{}", hex::encode(suffix)));
            fs::write(&partial, [&nonce[..], &sealed].concat())?;
            fs::rename(&partial, &path)?;
        }
        Ok(hash)
    }

    fn get(&self, hash: &Sha256Hash) -> Result<Option<Vec<u8>>, BlobError> {
        let sealed = match fs::read(self.path(hash)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let damaged = || BlobError::Damaged(*hash);
        let (nonce, ciphertext) = sealed.split_at_checked(24).ok_or_else(damaged)?;
        let nonce: [u8; 24] = nonce.try_into().map_err(|_| damaged())?;
        let aad = hash.to_string();
        let bytes = self
            .cipher()
            .decrypt(&XNonce::from(nonce), Payload { msg: ciphertext, aad: aad.as_bytes() })
            .map_err(|_| damaged())?;
        if Sha256Hash::of(&bytes) == *hash { Ok(Some(bytes)) } else { Err(damaged()) }
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

    fn remove(&self, hash: &Sha256Hash) -> Result<bool, BlobError> {
        match fs::remove_file(self.path(hash)) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Scratch;

    #[test]
    fn originals_are_kept_encrypted_and_checked() {
        let scratch = Scratch::new("sealed");
        let blobs = SealedBlobs::open(&scratch.0, [5; 32]).unwrap();
        let hash = blobs.put(b"%PDF-1.7 a lease nobody else reads").unwrap();
        assert_eq!(blobs.put(b"%PDF-1.7 a lease nobody else reads").unwrap(), hash);
        assert_eq!(blobs.get(&hash).unwrap().as_deref(), Some(&b"%PDF-1.7 a lease nobody else reads"[..]));
        assert_eq!(blobs.list().unwrap(), vec![hash]);
        assert!(blobs.contains(&hash).unwrap());
        assert_eq!(blobs.get(&Sha256Hash::of(b"other")).unwrap(), None);

        let on_disk = fs::read(blobs.path(&hash)).unwrap();
        assert!(!on_disk.windows(5).any(|w| w == b"lease"));

        let other_key = SealedBlobs::open(&scratch.0, [6; 32]).unwrap();
        assert!(matches!(other_key.get(&hash), Err(BlobError::Damaged(h)) if h == hash));

        // An original moved to another name does not open.
        let other = Sha256Hash::of(b"other");
        fs::create_dir_all(blobs.path(&other).parent().unwrap()).unwrap();
        fs::copy(blobs.path(&hash), blobs.path(&other)).unwrap();
        assert!(matches!(blobs.get(&other), Err(BlobError::Damaged(_))));
    }
}
