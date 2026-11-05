// SPDX-License-Identifier: MIT
//! Originals in an object store – S3 and everything `object_store` reaches: `sha256/<two digits>/<64 digits>` below
//! a prefix of the caller's choice.
//!
//! [`Blobs`] is synchronous; the object store is not. Every call runs on a small runtime of the store's own, on a
//! helper thread, so that it works from plain threads as well as from within another async runtime.

use std::sync::Arc;

use futures_util::StreamExt;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload, path::Path};

use crate::{BlobError, Blobs, Sha256Hash, checked};

/// Where an S3 bucket is and how to reach it.
#[derive(Clone, PartialEq, Eq)]
pub struct S3Config {
    /// The bucket.
    pub bucket: String,
    /// The region, e.g. `eu-central-1`.
    pub region: String,
    /// The endpoint of self-hosted object storage (MinIO, Garage, Ceph); none for AWS.
    pub endpoint: Option<String>,
    /// The access key.
    pub access_key_id: String,
    /// The secret key.
    pub secret_access_key: String,
    /// Whether plain HTTP is allowed – for a store on the same machine only.
    pub allow_http: bool,
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field("allow_http", &self.allow_http)
            .finish_non_exhaustive()
    }
}

/// Originals in an object store.
pub struct ObjectBlobs {
    inner: Arc<dyn ObjectStore>,
    prefix: String,
    runtime: tokio::runtime::Runtime,
}

impl std::fmt::Debug for ObjectBlobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectBlobs").field("inner", &self.inner.to_string()).field("prefix", &self.prefix).finish()
    }
}

fn store_error(error: object_store::Error) -> BlobError {
    BlobError::Object(error.to_string())
}

impl ObjectBlobs {
    /// Originals in any object store, below `prefix` (empty for the top).
    pub fn new(inner: Arc<dyn ObjectStore>, prefix: &str) -> Result<Self, BlobError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("aktujo-blob")
            .enable_all()
            .build()?;
        Ok(ObjectBlobs { inner, prefix: prefix.trim_matches('/').to_owned(), runtime })
    }

    /// Originals in an S3 bucket, below `prefix`.
    #[cfg(feature = "s3")]
    pub fn s3(config: &S3Config, prefix: &str) -> Result<Self, BlobError> {
        let mut builder = object_store::aws::AmazonS3Builder::new()
            .with_region(&config.region)
            .with_bucket_name(&config.bucket)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key)
            .with_allow_http(config.allow_http);
        if let Some(endpoint) = &config.endpoint {
            // Self-hosted object storage is addressed by path, not by a bucket subdomain.
            builder = builder.with_endpoint(endpoint).with_virtual_hosted_style_request(false);
        }
        Self::new(Arc::new(builder.build().map_err(store_error)?), prefix)
    }

    fn root(&self) -> Path {
        if self.prefix.is_empty() { Path::from("sha256") } else { Path::from(format!("{}/sha256", self.prefix)) }
    }

    fn path(&self, hash: &Sha256Hash) -> Path {
        let hex = hash.hex();
        self.root().join(&hex[..2]).join(hex.as_str())
    }

    /// Runs `work` on the store's runtime, on a helper thread.
    fn run<T: Send>(&self, work: impl Future<Output = T> + Send) -> T {
        std::thread::scope(|scope| scope.spawn(|| self.runtime.block_on(work)).join().expect("object store call"))
    }
}

impl Blobs for ObjectBlobs {
    fn put(&self, bytes: &[u8]) -> Result<Sha256Hash, BlobError> {
        let hash = Sha256Hash::of(bytes);
        if !self.contains(&hash)? {
            let path = self.path(&hash);
            let payload = PutPayload::from(bytes.to_vec());
            self.run(async { self.inner.put(&path, payload).await }).map_err(store_error)?;
        }
        Ok(hash)
    }

    fn get(&self, hash: &Sha256Hash) -> Result<Option<Vec<u8>>, BlobError> {
        let path = self.path(hash);
        let bytes = self.run(async {
            match self.inner.get(&path).await {
                Ok(result) => result.bytes().await.map(Some),
                Err(object_store::Error::NotFound { .. }) => Ok(None),
                Err(error) => Err(error),
            }
        });
        bytes.map_err(store_error)?.map(|bytes| checked(hash, bytes.to_vec())).transpose()
    }

    fn contains(&self, hash: &Sha256Hash) -> Result<bool, BlobError> {
        let path = self.path(hash);
        match self.run(async { self.inner.head(&path).await }) {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(error) => Err(store_error(error)),
        }
    }

    fn list(&self) -> Result<Vec<Sha256Hash>, BlobError> {
        let root = self.root();
        let found: Vec<Result<String, object_store::Error>> = self.run(async {
            self.inner
                .list(Some(&root))
                .map(|meta| meta.map(|meta| meta.location.filename().unwrap_or("").to_owned()))
                .collect()
                .await
        });
        let mut hashes = Vec::new();
        for name in found {
            if let Ok(hash) = format!("sha256:{}", name.map_err(store_error)?).parse() {
                hashes.push(hash);
            }
        }
        hashes.sort();
        Ok(hashes)
    }
}

#[cfg(test)]
mod tests {
    use object_store::memory::InMemory;

    use super::*;

    #[test]
    fn in_an_object_store() {
        let inner = Arc::new(InMemory::new());
        let store = ObjectBlobs::new(inner.clone(), "/archive/").unwrap();
        crate::tests::behaves(&store);
        let hash = Sha256Hash::of(b"<Invoice/>");
        let at = Path::from(format!("archive/sha256/{}/{}", &hash.hex()[..2], hash.hex()));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async { inner.put(&at, PutPayload::from(b"changed".to_vec())).await }).unwrap();
        assert!(matches!(store.get(&hash), Err(BlobError::Damaged(_))));
    }

    #[test]
    fn works_from_within_another_runtime() {
        let store = ObjectBlobs::new(Arc::new(InMemory::new()), "").unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let hash = runtime.block_on(async { store.put(b"inside").unwrap() });
        assert_eq!(store.get(&hash).unwrap().as_deref(), Some(&b"inside"[..]));
    }

    #[test]
    fn s3_secrets_are_not_shown() {
        let config = S3Config {
            bucket: "archive".into(),
            region: "eu-central-1".into(),
            endpoint: None,
            access_key_id: "key".into(),
            secret_access_key: "secret".into(),
            allow_http: false,
        };
        let shown = format!("{config:?}");
        assert!(!shown.contains("\"secret\"") && !shown.contains("\"key\""), "{shown}");
    }
}
