// SPDX-License-Identifier: MIT
//! The store in PostgreSQL, for servers: the same tables as in SQLite, in a schema of their own (`aktujo` unless
//! named otherwise), so that they stand beside a program's tables in the same database without meeting them.
//!
//! The client is synchronous, like the trait. It runs a small runtime of its own; from within an async runtime, call
//! it in `tokio::task::spawn_blocking` as any blocking I/O. Several servers may share one database: the schema is set
//! up under a lock, and member records are appended under a lock per space, so that two servers never store two
//! different records at the same place.
//!
//! The connection is encrypted with TLS as the URL's `sslmode` says: `disable` never, `prefer` (the default) if the
//! server offers it, `require` always. Whenever it is, the server's certificate is checked – against the roots of the
//! system, and a CA of the program's own ([`PgStore::connect_with_ca`]) for a database with a certificate of its
//! cluster. Stricter than libpq, whose `prefer` and `require` check nothing: a server with a certificate no one
//! trusts is refused, also under `prefer` – give its CA, or `sslmode=disable` within a private network.

use std::{collections::BTreeSet, sync::Mutex};

use aktujo_base::{Sha256Hash, SpaceId, to_root};
use aktujo_crypt::{Domain, MemberRecord};
use aktujo_log::Log;
use postgres::{Client, Transaction};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::{Backend, SCHEMA_VERSION, StoreError, parse};

/// The schema used unless another is named.
pub const DEFAULT_SCHEMA: &str = "aktujo";

/// TLS by rustls with ring: the roots of the system, and the CA certificates in `ca_pem` if given.
fn tls(ca_pem: Option<&[u8]>) -> Result<MakeRustlsConnect, StoreError> {
    use rustls::pki_types::{CertificateDer, pem::PemObject};
    let mut roots = rustls::RootCertStore::empty();
    // Certificates of the system that cannot be read are left out, as browsers do.
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    if let Some(pem) = ca_pem {
        let mut given = 0;
        for certificate in CertificateDer::pem_slice_iter(pem) {
            let certificate = certificate.map_err(|e| StoreError::Tls(format!("the CA: {e}")))?;
            roots.add(certificate).map_err(|e| StoreError::Tls(format!("the CA: {e}")))?;
            given += 1;
        }
        if given == 0 {
            return Err(StoreError::Tls("the CA: no certificate in it".into()));
        }
    }
    let config =
        rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| StoreError::Tls(e.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
    Ok(MakeRustlsConnect::new(config))
}

/// A name of a schema written into SQL as it is: so it must be a plain one.
fn check_schema(schema: &str) -> Result<(), StoreError> {
    let plain = schema.len() <= 63
        && schema.starts_with(|c: char| c.is_ascii_lowercase())
        && schema.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if plain { Ok(()) } else { Err(StoreError::Schema(schema.to_owned())) }
}

/// The store in PostgreSQL.
pub struct PgStore {
    client: Mutex<Client>,
    schema: String,
}

impl std::fmt::Debug for PgStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgStore").field("schema", &self.schema).finish_non_exhaustive()
    }
}

impl PgStore {
    /// Connects to the database at `url` (`postgres://user:password@host/database` or `host=… user=…`) and sets up
    /// the core's tables in the schema `aktujo`, if they are not there yet.
    pub fn connect(url: &str) -> Result<Self, StoreError> {
        Self::connect_in(url, DEFAULT_SCHEMA)
    }

    /// Like [`PgStore::connect`], with the tables in the schema `schema` – one per organisation, for example.
    pub fn connect_in(url: &str, schema: &str) -> Result<Self, StoreError> {
        check_schema(schema)?;
        Self::with_client(Client::connect(url, tls(None)?)?, schema)
    }

    /// Like [`PgStore::connect_in`], trusting the CA certificates in `ca_pem` (PEM) besides the system's – a
    /// cluster's own. Add `sslmode=require` to the URL so that the connection is never without TLS.
    pub fn connect_with_ca(url: &str, schema: &str, ca_pem: &[u8]) -> Result<Self, StoreError> {
        check_schema(schema)?;
        Self::with_client(Client::connect(url, tls(Some(ca_pem))?)?, schema)
    }

    /// The store on a client connected already, with the tables in the schema `schema`.
    pub fn with_client(mut client: Client, schema: &str) -> Result<Self, StoreError> {
        check_schema(schema)?;
        let mut transaction = client.transaction()?;
        // One server sets the schema up at a time.
        transaction.execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&format!("aktujo-schema:{schema}")])?;
        transaction.batch_execute(&format!(
            "CREATE SCHEMA IF NOT EXISTS {schema};
             CREATE TABLE IF NOT EXISTS {schema}.schema_version (
                 one BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (one),
                 version BIGINT NOT NULL
             );"
        ))?;
        let version: Option<i64> =
            transaction.query_opt(&format!("SELECT version FROM {schema}.schema_version"), &[])?.map(|row| row.get(0));
        if let Some(version) = version.filter(|v| *v > SCHEMA_VERSION) {
            return Err(StoreError::Version(version));
        }
        transaction.batch_execute(&format!(
            "CREATE TABLE IF NOT EXISTS {schema}.settings (
                 space TEXT NOT NULL,          -- '' for settings of the device
                 key TEXT NOT NULL,
                 value TEXT NOT NULL,
                 PRIMARY KEY (space, key)
             );
             CREATE TABLE IF NOT EXISTS {schema}.events (
                 space TEXT NOT NULL,
                 hash TEXT NOT NULL,           -- sha256:… of the canonical encoding
                 canonical BYTEA NOT NULL,
                 PRIMARY KEY (space, hash)
             );
             CREATE TABLE IF NOT EXISTS {schema}.members (
                 space TEXT NOT NULL,
                 seq BIGINT NOT NULL,          -- the record's position, from 0
                 hash TEXT NOT NULL,
                 canonical BYTEA NOT NULL,
                 PRIMARY KEY (space, seq)
             );
             CREATE TABLE IF NOT EXISTS {schema}.originals (
                 space TEXT NOT NULL,
                 hash TEXT NOT NULL,           -- sha256:… of the bytes, kept in aktujo-blob
                 PRIMARY KEY (space, hash)
             );
             CREATE TABLE IF NOT EXISTS {schema}.sent (
                 space TEXT NOT NULL,
                 kind TEXT NOT NULL,           -- 'event' or 'original'
                 id TEXT NOT NULL,             -- its hash
                 PRIMARY KEY (space, kind, id)
             );"
        ))?;
        transaction.execute(
            &format!(
                "INSERT INTO {schema}.schema_version (version) VALUES ($1)
                 ON CONFLICT (one) DO UPDATE SET version = excluded.version"
            ),
            &[&SCHEMA_VERSION],
        )?;
        transaction.commit()?;
        Ok(PgStore { client: Mutex::new(client), schema: schema.to_owned() })
    }

    /// The schema the core's tables are in.
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// The client, for a program's own tables beside the core's – in a schema of the program's own. The core's tables
    /// are written through the store only.
    pub fn client(&self) -> std::sync::MutexGuard<'_, Client> {
        self.client.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn records_in(
        transaction: &mut Transaction<'_>,
        schema: &str,
        space: &SpaceId,
    ) -> Result<Vec<MemberRecord>, StoreError> {
        let rows = transaction.query(
            &format!("SELECT canonical FROM {schema}.members WHERE space = $1 ORDER BY seq"),
            &[&space.as_str()],
        )?;
        rows.iter()
            .map(|row| {
                aktujo_base::from_root(row.get::<_, &[u8]>(0))
                    .map_err(|error| StoreError::Record { space: space.clone(), error: error.to_string() })
            })
            .collect()
    }
}

impl Backend for PgStore {
    fn spaces(&self) -> Result<Vec<SpaceId>, StoreError> {
        let s = &self.schema;
        let rows = self
            .client()
            .query(&format!("SELECT space FROM {s}.events UNION SELECT space FROM {s}.members ORDER BY space"), &[])?;
        rows.iter().map(|row| parse(row.get::<_, String>(0))).collect()
    }

    fn load(&self, space: &SpaceId) -> Result<Log, StoreError> {
        let rows = self
            .client()
            .query(&format!("SELECT canonical FROM {}.events WHERE space = $1", self.schema), &[&space.as_str()])?;
        let mut log = Log::new();
        for row in &rows {
            log.receive(row.get::<_, &[u8]>(0)).map_err(|error| StoreError::Event { space: space.clone(), error })?;
        }
        Ok(log)
    }

    fn save(&mut self, logs: &[(&SpaceId, &Log)]) -> Result<usize, StoreError> {
        let schema = self.schema.clone();
        let mut client = self.client();
        let mut transaction = client.transaction()?;
        let insert = transaction.prepare(&format!(
            "INSERT INTO {schema}.events (space, hash, canonical) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
        ))?;
        let mut new = 0;
        for (space, log) in logs {
            for hash in log.hashes() {
                let canonical = log.canonical(hash).expect("a hash of the log");
                new += transaction.execute(&insert, &[&space.as_str(), &hash.to_string(), &canonical])?;
            }
        }
        transaction.commit()?;
        Ok(new as usize)
    }

    fn records(&self, space: &SpaceId) -> Result<Vec<MemberRecord>, StoreError> {
        let mut client = self.client();
        let mut transaction = client.transaction()?;
        let records = Self::records_in(&mut transaction, &self.schema, space)?;
        transaction.commit()?;
        Ok(records)
    }

    fn add_record(&mut self, domain: Domain, record: &MemberRecord) -> Result<(), StoreError> {
        let schema = self.schema.clone();
        let mut client = self.client();
        let mut transaction = client.transaction()?;
        // The records of a space are read and appended by one server at a time.
        transaction.execute(
            "SELECT pg_advisory_xact_lock(hashtext($1))",
            &[&format!("aktujo-members:{schema}:{}", record.space)],
        )?;
        let records = Self::records_in(&mut transaction, &schema, &record.space)?;
        let mut list = aktujo_crypt::MemberList::from_records(domain, record.space.clone(), &records)
            .map_err(|error| StoreError::Record { space: record.space.clone(), error: error.to_string() })?;
        list.apply(record.clone())?;
        let canonical = to_root(record).expect("a record has a canonical encoding");
        transaction.execute(
            &format!("INSERT INTO {schema}.members (space, seq, hash, canonical) VALUES ($1, $2, $3, $4)"),
            &[&record.space.as_str(), &(record.seq as i64), &record.hash().to_string(), &canonical],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn add_original(&self, space: &SpaceId, hash: &Sha256Hash) -> Result<(), StoreError> {
        self.client().execute(
            &format!("INSERT INTO {}.originals (space, hash) VALUES ($1, $2) ON CONFLICT DO NOTHING", self.schema),
            &[&space.as_str(), &hash.to_string()],
        )?;
        Ok(())
    }

    fn originals(&self, space: &SpaceId) -> Result<Vec<Sha256Hash>, StoreError> {
        let rows = self.client().query(
            &format!("SELECT hash FROM {}.originals WHERE space = $1 ORDER BY hash", self.schema),
            &[&space.as_str()],
        )?;
        rows.iter().map(|row| parse(row.get::<_, String>(0))).collect()
    }

    fn setting(&self, space: Option<&SpaceId>, key: &str) -> Result<Option<String>, StoreError> {
        let space = space.map_or("", SpaceId::as_str);
        let row = self.client().query_opt(
            &format!("SELECT value FROM {}.settings WHERE space = $1 AND key = $2", self.schema),
            &[&space, &key],
        )?;
        Ok(row.map(|row| row.get(0)))
    }

    fn set_setting(&self, space: Option<&SpaceId>, key: &str, value: &str) -> Result<(), StoreError> {
        let space = space.map_or("", SpaceId::as_str);
        self.client().execute(
            &format!(
                "INSERT INTO {}.settings (space, key, value) VALUES ($1, $2, $3)
                 ON CONFLICT (space, key) DO UPDATE SET value = excluded.value",
                self.schema
            ),
            &[&space, &key, &value],
        )?;
        Ok(())
    }

    fn sent(&self, space: &SpaceId, kind: &str) -> Result<BTreeSet<Sha256Hash>, StoreError> {
        let rows = self.client().query(
            &format!("SELECT id FROM {}.sent WHERE space = $1 AND kind = $2", self.schema),
            &[&space.as_str(), &kind],
        )?;
        rows.iter().map(|row| parse(row.get::<_, String>(0))).collect()
    }

    fn mark_sent(&self, space: &SpaceId, kind: &str, id: &Sha256Hash) -> Result<(), StoreError> {
        self.client().execute(
            &format!("INSERT INTO {}.sent (space, kind, id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING", self.schema),
            &[&space.as_str(), &kind, &id.to_string()],
        )?;
        Ok(())
    }
}

/// Against a database named by `AKTUJO_TEST_POSTGRES` (a URL); without it, these tests pass without doing anything.
/// Each test works in a schema of its own and removes it.
#[cfg(test)]
mod tests {
    use aktujo_crypt::{Member, MemberKeys, MemberList, Rights};

    use postgres::NoTls;

    use super::*;
    use crate::tests::{events_and_settings, members, originals};

    struct Scratch {
        url: String,
        schema: String,
    }

    impl Scratch {
        fn new(name: &str) -> Option<Self> {
            let url = std::env::var("AKTUJO_TEST_POSTGRES").ok()?;
            let schema = format!("test_{name}_{}", std::process::id());
            Client::connect(&url, NoTls)
                .unwrap()
                .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
                .unwrap();
            Some(Scratch { url, schema })
        }

        fn store(&self) -> PgStore {
            PgStore::connect_in(&self.url, &self.schema).unwrap()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Ok(mut client) = Client::connect(&self.url, NoTls) {
                let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema));
            }
        }
    }

    #[test]
    fn events_and_settings_are_kept_per_space_and_survive_reconnecting() {
        let Some(scratch) = Scratch::new("events") else { return };
        events_and_settings(scratch.store());
        let again = scratch.store();
        assert_eq!(again.spaces().unwrap().len(), 2);
    }

    #[test]
    fn originals_belong_to_spaces() {
        let Some(scratch) = Scratch::new("originals") else { return };
        originals(scratch.store());
    }

    #[test]
    fn member_records_are_stored_only_in_order_and_valid() {
        let Some(scratch) = Scratch::new("members") else { return };
        members(scratch.store());
    }

    #[test]
    fn two_servers_never_store_two_records_at_one_place() {
        let Some(scratch) = Scratch::new("servers") else { return };
        let domain = Domain::AKTUJO;
        let space: SpaceId = "0192f1e4-0000-7000-8000-000000000001".parse().unwrap();
        let laptop = MemberKeys::generate(domain);
        let device = |keys: &MemberKeys, name: &str| Member::device(keys, name.parse().unwrap(), None, Rights::Manage);
        let (first, key) = MemberRecord::found(&space, &laptop, vec![device(&laptop, "laptop")]);
        let mut one = scratch.store();
        one.add_record(domain, &first).unwrap();
        let list: MemberList = one.member_list(domain, &space).unwrap();
        // Two different records for the same place, written by two servers at once: one is stored, one refused.
        let records: Vec<_> = (0..2)
            .map(|n| list.add(&laptop, &key, vec![device(&MemberKeys::generate(domain), &format!("phone{n}"))]))
            .collect();
        let results: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = records
                .iter()
                .map(|record| {
                    let scratch = &scratch;
                    scope.spawn(move || scratch.store().add_record(domain, record))
                })
                .collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).collect()
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(one.records(&space).unwrap().len(), 2);
    }

    #[test]
    fn a_plain_schema_name_and_a_known_version_are_required() {
        assert!(matches!(PgStore::connect_in("host=nowhere", "Robert'); DROP"), Err(StoreError::Schema(_))));
        let Some(scratch) = Scratch::new("version") else { return };
        drop(scratch.store());
        Client::connect(&scratch.url, NoTls)
            .unwrap()
            .execute(&format!("UPDATE {}.schema_version SET version = 99", scratch.schema), &[])
            .unwrap();
        assert!(matches!(PgStore::connect_in(&scratch.url, &scratch.schema), Err(StoreError::Version(99))));
    }

    #[test]
    fn a_ca_that_is_none_is_refused() {
        assert!(matches!(tls(Some(b"not a certificate")), Err(StoreError::Tls(_))));
        assert!(tls(None).is_ok());
    }

    /// Against a database with TLS: `AKTUJO_TEST_POSTGRES_TLS` (a URL with `sslmode=require`) and
    /// `AKTUJO_TEST_POSTGRES_CA` (the file of the CA of its certificate, PEM); without them, nothing is done.
    #[test]
    fn over_tls_the_server_is_checked() {
        let (Ok(url), Ok(ca)) = (std::env::var("AKTUJO_TEST_POSTGRES_TLS"), std::env::var("AKTUJO_TEST_POSTGRES_CA"))
        else {
            return;
        };
        let ca = std::fs::read(ca).unwrap();
        let schema = format!("test_tls_{}", std::process::id());
        let store = PgStore::connect_with_ca(&url, &schema, &ca).unwrap();
        store.set_setting(None, "over", "tls").unwrap();
        let encrypted: bool =
            store.client().query_one("SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()", &[]).unwrap().get(0);
        assert!(encrypted, "the connection is encrypted");
        // The CA is not one of the system's: without it, the server is not trusted.
        assert!(matches!(PgStore::connect_in(&url, &schema), Err(StoreError::Postgres(_))));
        store.client().batch_execute(&format!("DROP SCHEMA {schema} CASCADE")).unwrap();
    }
}
