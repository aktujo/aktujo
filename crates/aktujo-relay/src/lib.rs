// SPDX-License-Identifier: MIT
// Taken over from spezado (crates/spezado-relay, AGPL-3.0-or-later) and made general; published here under MIT by
// its copyright holder, as planned for the core (docs/rfc/0001-architecture.md).
//! The relay of the aktujo core: it holds, per space, the ordered member list and the envelopes of events and
//! originals, and passes them on to the members. It cannot read them: it checks signatures, the rules of the member
//! list and who may write, never content (the mechanism of librotenado 0006, relays).
//!
//! | Request | |
//! |---|---|
//! | `GET /v1/spaces/{space}/members?after={seq}` | the records after `seq`, in order – open |
//! | `POST /v1/spaces/{space}/members` | a record: `201` when it is the next valid one, `409` when it does not follow the head, `403` when it is not valid; record 0 of a space not held is open |
//! | `POST /v1/spaces/{space}/envelopes` | an envelope: `201`, or `200` if held already |
//! | `GET /v1/spaces/{space}/envelopes?after={n}&limit={m}` | `{"envelopes": [{"position": …, "envelope": …}], "more": …}` |
//! | `POST /v1/spaces/{space}/requests` | a request to join – open |
//! | `GET /v1/spaces/{space}/requests` | the requests to join, for members who may manage |
//!
//! Every request but the open ones is signed (`aktujo_crypt::request_signature`). The relay speaks HTTP; it is meant
//! to run behind a reverse proxy that terminates TLS.

use std::{
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use aktujo_base::{SpaceId, from_root, to_root};
use aktujo_crypt::{
    Domain, Envelope, JoinRequest, Member, MemberId, MemberList, MemberRecord, MemberRole, RecordProblem, Rights,
    verify_request,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path as UrlPath, Query, State},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

/// The schema of the relay's database.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS records (
    space TEXT NOT NULL,
    seq INTEGER NOT NULL,
    canonical BLOB NOT NULL,
    PRIMARY KEY (space, seq)
) STRICT;
CREATE TABLE IF NOT EXISTS envelopes (
    space TEXT NOT NULL,
    position INTEGER NOT NULL,
    id TEXT NOT NULL,
    canonical BLOB NOT NULL,
    PRIMARY KEY (space, position),
    UNIQUE (space, id)
) STRICT;
CREATE TABLE IF NOT EXISTS requests (
    space TEXT NOT NULL,
    member TEXT NOT NULL,
    canonical BLOB NOT NULL,
    received INTEGER NOT NULL,
    PRIMARY KEY (space, member)
) STRICT;
PRAGMA user_version = 1;
";

/// How long a request to join is kept, in seconds.
const REQUEST_LIFETIME: u64 = 24 * 60 * 60;
/// How many requests to join a space may wait at once.
const MAX_REQUESTS: i64 = 20;
/// The largest request body: an original in an envelope.
const MAX_BODY: usize = 64 * 1024 * 1024;

/// The state of a relay: its database.
#[derive(Debug, Clone)]
pub struct Relay {
    domain: Domain,
    connection: Arc<Mutex<Connection>>,
}

/// An error answered to a request, as `{"error": …}`.
#[derive(Debug)]
pub struct RelayError(StatusCode, String);

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

impl From<rusqlite::Error> for RelayError {
    fn from(error: rusqlite::Error) -> Self {
        RelayError(StatusCode::INTERNAL_SERVER_ERROR, format!("database: {error}"))
    }
}

fn bad(message: impl Into<String>) -> RelayError {
    RelayError(StatusCode::BAD_REQUEST, message.into())
}

fn forbidden(message: impl Into<String>) -> RelayError {
    RelayError(StatusCode::FORBIDDEN, message.into())
}

/// Seconds since 1970-01-01 UTC.
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

fn space_id(text: &str) -> Result<SpaceId, RelayError> {
    text.parse().map_err(|_| bad("not the identity of a space"))
}

impl Relay {
    /// Opens the relay's database in `directory`, creating it if needed, for spaces of `domain`.
    pub fn open(domain: Domain, directory: &Path) -> Result<Self, rusqlite::Error> {
        std::fs::create_dir_all(directory).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let connection = Connection::open(directory.join("relay.sqlite"))?;
        connection.execute_batch("PRAGMA journal_mode = WAL;")?;
        connection.execute_batch(SCHEMA)?;
        Ok(Relay { domain, connection: Arc::new(Mutex::new(connection)) })
    }

    /// The member list of a space, as stored.
    fn members(&self, connection: &Connection, space: &SpaceId) -> Result<MemberList, RelayError> {
        let mut statement = connection.prepare("SELECT canonical FROM records WHERE space = ?1 ORDER BY seq")?;
        let rows = statement.query_map(params![space.as_str()], |row| row.get::<_, Vec<u8>>(0))?;
        let mut list = MemberList::new(self.domain, space.clone());
        for row in rows {
            let record: MemberRecord = from_root(&row?).map_err(|e| bad(e.to_string()))?;
            list.apply(record).map_err(|e| RelayError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        Ok(list)
    }

    /// The member who signed a request, if it is a current member of the space with at least `rights`.
    fn member(
        &self,
        space: &SpaceId,
        headers: &HeaderMap,
        method: &Method,
        uri: &Uri,
        rights: Rights,
    ) -> Result<(MemberList, Member), RelayError> {
        let header = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| RelayError(StatusCode::UNAUTHORIZED, "a signed request is needed".into()))?;
        let path = uri.path_and_query().map_or(uri.path(), |p| p.as_str());
        let id = verify_request(self.domain, header, method.as_str(), path, now())
            .ok_or_else(|| RelayError(StatusCode::UNAUTHORIZED, "the signature or its time is not valid".into()))?;
        let connection = self.connection.lock().expect("the relay's database");
        let list = self.members(&connection, space)?;
        let member = list.member(&id).cloned().ok_or_else(|| forbidden("not a member of this space"))?;
        if member.rights < rights {
            return Err(forbidden("not allowed"));
        }
        Ok((list, member))
    }
}

/// The routes of the relay (librotenado part 0006, relays).
pub fn router(relay: Relay) -> Router {
    Router::new()
        .route("/v1/spaces/{space}/members", get(members).post(add_record))
        .route("/v1/spaces/{space}/envelopes", get(envelopes).post(add_envelope))
        .route("/v1/spaces/{space}/requests", get(requests).post(add_request))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(relay)
}

#[derive(Debug, Deserialize)]
struct After {
    after: Option<i64>,
    limit: Option<i64>,
}

/// The records after `after`, in order – open.
async fn members(
    State(relay): State<Relay>,
    UrlPath(space): UrlPath<String>,
    Query(query): Query<After>,
) -> Result<Json<Vec<Value>>, RelayError> {
    let space = space_id(&space)?;
    let connection = relay.connection.lock().expect("the relay's database");
    let mut statement =
        connection.prepare("SELECT canonical FROM records WHERE space = ?1 AND seq > ?2 ORDER BY seq")?;
    let rows =
        statement.query_map(params![space.as_str(), query.after.unwrap_or(-1)], |row| row.get::<_, Vec<u8>>(0))?;
    let records = rows
        .map(|row| Ok(serde_json::from_slice(&row?).expect("a stored record is JSON")))
        .collect::<Result<Vec<Value>, rusqlite::Error>>()?;
    Ok(Json(records))
}

/// A record of the member list: the next valid one, or refused.
async fn add_record(
    State(relay): State<Relay>,
    UrlPath(space): UrlPath<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<StatusCode, RelayError> {
    let space = space_id(&space)?;
    let record: MemberRecord = from_root(&body).map_err(|e| bad(e.to_string()))?;
    if record.seq > 0 {
        relay.member(&space, &headers, &method, &uri, Rights::Manage)?;
    }
    let connection = relay.connection.lock().expect("the relay's database");
    let mut list = relay.members(&connection, &space)?;
    let seq = record.seq;
    let added: Vec<MemberId> = record.add.iter().map(|m| m.id).collect();
    let canonical = to_root(&record).expect("a record has a canonical encoding");
    match list.apply(record) {
        Ok(()) => {}
        Err(RecordProblem::NotNext) => return Err(RelayError(StatusCode::CONFLICT, "not the next record".into())),
        Err(problem) => return Err(forbidden(problem.to_string())),
    }
    connection.execute(
        "INSERT INTO records (space, seq, canonical) VALUES (?1, ?2, ?3)",
        params![space.as_str(), seq as i64, canonical],
    )?;
    for id in added {
        connection.execute(
            "DELETE FROM requests WHERE space = ?1 AND member = ?2",
            params![space.as_str(), id.to_string()],
        )?;
    }
    Ok(StatusCode::CREATED)
}

/// An envelope from a member who may write.
async fn add_envelope(
    State(relay): State<Relay>,
    UrlPath(space): UrlPath<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<StatusCode, RelayError> {
    let space = space_id(&space)?;
    let (list, _) = relay.member(&space, &headers, &method, &uri, Rights::Write)?;
    let envelope: Envelope = from_root(&body).map_err(|e| bad(e.to_string()))?;
    let author = list.member(&envelope.author).ok_or_else(|| forbidden("the author is not a member"))?;
    let writes = author.rights >= Rights::Write && author.role != MemberRole::Recovery;
    if envelope.space != space || !writes || !envelope.is_authentic() {
        return Err(forbidden("the envelope is not one a member may send"));
    }
    if list.key_index().is_none_or(|current| envelope.key > current) {
        return Err(forbidden("no such key yet"));
    }
    let connection = relay.connection.lock().expect("the relay's database");
    let known: Option<i64> = connection
        .query_row(
            "SELECT position FROM envelopes WHERE space = ?1 AND id = ?2",
            params![space.as_str(), envelope.id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    if known.is_some() {
        return Ok(StatusCode::OK);
    }
    let position: i64 = connection.query_row(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM envelopes WHERE space = ?1",
        params![space.as_str()],
        |row| row.get(0),
    )?;
    connection.execute(
        "INSERT INTO envelopes (space, position, id, canonical) VALUES (?1, ?2, ?3, ?4)",
        params![space.as_str(), position, envelope.id.to_string(), to_root(&envelope).expect("canonical")],
    )?;
    Ok(StatusCode::CREATED)
}

/// The envelopes after a position, for members.
async fn envelopes(
    State(relay): State<Relay>,
    UrlPath(space): UrlPath<String>,
    Query(query): Query<After>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<Value>, RelayError> {
    let space = space_id(&space)?;
    relay.member(&space, &headers, &method, &uri, Rights::Read)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    let connection = relay.connection.lock().expect("the relay's database");
    let mut statement = connection.prepare(
        "SELECT position, canonical FROM envelopes WHERE space = ?1 AND position > ?2 ORDER BY position LIMIT ?3",
    )?;
    let rows = statement.query_map(params![space.as_str(), query.after.unwrap_or(0), limit + 1], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    let mut envelopes = rows
        .map(|row| {
            let (position, canonical) = row?;
            let envelope: Value = serde_json::from_slice(&canonical).expect("a stored envelope is JSON");
            Ok(json!({"position": position, "envelope": envelope}))
        })
        .collect::<Result<Vec<Value>, rusqlite::Error>>()?;
    let more = envelopes.len() as i64 > limit;
    envelopes.truncate(limit as usize);
    Ok(Json(json!({"envelopes": envelopes, "more": more})))
}

/// A request to join – open; the inviting member checks its proof.
async fn add_request(
    State(relay): State<Relay>,
    UrlPath(space): UrlPath<String>,
    body: axum::body::Bytes,
) -> Result<StatusCode, RelayError> {
    let space = space_id(&space)?;
    let request: JoinRequest = from_root(&body).map_err(|e| bad(e.to_string()))?;
    let connection = relay.connection.lock().expect("the relay's database");
    let held: i64 =
        connection.query_row("SELECT COUNT(*) FROM records WHERE space = ?1", params![space.as_str()], |r| r.get(0))?;
    if held == 0 {
        return Err(RelayError(StatusCode::NOT_FOUND, "no such space here".into()));
    }
    connection
        .execute("DELETE FROM requests WHERE received < ?1", params![now().saturating_sub(REQUEST_LIFETIME) as i64])?;
    let waiting: i64 =
        connection
            .query_row("SELECT COUNT(*) FROM requests WHERE space = ?1", params![space.as_str()], |r| r.get(0))?;
    if waiting >= MAX_REQUESTS {
        return Err(RelayError(StatusCode::TOO_MANY_REQUESTS, "too many requests waiting".into()));
    }
    connection.execute(
        "INSERT OR REPLACE INTO requests (space, member, canonical, received) VALUES (?1, ?2, ?3, ?4)",
        params![space.as_str(), request.member.id.to_string(), to_root(&request).expect("canonical"), now() as i64],
    )?;
    Ok(StatusCode::CREATED)
}

/// The requests to join, for members who may manage.
async fn requests(
    State(relay): State<Relay>,
    UrlPath(space): UrlPath<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<Vec<Value>>, RelayError> {
    let space = space_id(&space)?;
    relay.member(&space, &headers, &method, &uri, Rights::Manage)?;
    let connection = relay.connection.lock().expect("the relay's database");
    let mut statement =
        connection.prepare("SELECT canonical FROM requests WHERE space = ?1 AND received >= ?2 ORDER BY received")?;
    let since = now().saturating_sub(REQUEST_LIFETIME) as i64;
    let rows = statement.query_map(params![space.as_str(), since], |row| row.get::<_, Vec<u8>>(0))?;
    let requests = rows
        .map(|row| Ok(serde_json::from_slice(&row?).expect("a stored request is JSON")))
        .collect::<Result<Vec<Value>, rusqlite::Error>>()?;
    Ok(Json(requests))
}

/// Opens the relay in `directory` and serves it at `address` until Ctrl-C, on a runtime of its own.
pub fn run(
    domain: Domain,
    directory: &Path,
    address: SocketAddr,
    started: impl FnOnce(SocketAddr),
) -> std::io::Result<()> {
    let relay = Relay::open(domain, directory).map_err(std::io::Error::other)?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(address).await?;
        started(listener.local_addr()?);
        axum::serve(listener, router(relay))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
    })
}

/// Serves the relay at `address` until Ctrl-C.
pub async fn serve(relay: Relay, address: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router(relay))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}

/// Serves the relay on a listener that is bound already, until the task ends – for tests and for embedding.
pub async fn serve_listener(relay: Relay, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    axum::serve(listener, router(relay)).await
}

#[cfg(test)]
mod tests {
    use aktujo_crypt::{MemberKeys, SpaceKey, request_signature};
    use aktujo_log::{Clock, Event};
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    const D: Domain = Domain::AKTUJO;

    /// A relay in a temporary folder, removed when dropped.
    struct Scratch(std::path::PathBuf, Router);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("aktujo-relay-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            let relay = Relay::open(D, &path).unwrap();
            Scratch(path, router(relay))
        }

        async fn send(
            &self,
            method: &str,
            path: &str,
            keys: Option<&MemberKeys>,
            body: Vec<u8>,
        ) -> (StatusCode, Value) {
            let mut request = Request::builder().method(method).uri(path);
            if let Some(keys) = keys {
                request = request.header("authorization", request_signature(keys, method, path, now()));
            }
            let response = self.1.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn space() -> SpaceId {
        "0192f1e4-0000-7000-8000-0000000000c1".parse().unwrap()
    }

    fn device(keys: &MemberKeys, name: &str) -> Member {
        Member::device(keys, name.parse().unwrap(), None, Rights::Manage)
    }

    fn founded(laptop: &MemberKeys) -> (MemberRecord, SpaceKey) {
        MemberRecord::found(&space(), laptop, vec![device(laptop, "laptop")])
    }

    #[tokio::test]
    async fn members_sync_envelopes_through_the_relay() {
        let relay = Scratch::new("sync");
        let (laptop, phone) = (MemberKeys::generate(D), MemberKeys::generate(D));
        let members = format!("/v1/spaces/{}/members", space());
        let envelopes = format!("/v1/spaces/{}/envelopes", space());

        let (first, key) = founded(&laptop);
        assert_eq!(relay.send("POST", &members, None, to_root(&first).unwrap()).await.0, StatusCode::CREATED);
        let mut list = MemberList::new(D, space());
        list.apply(first.clone()).unwrap();
        let second = list.add(&laptop, &key, vec![device(&phone, "phone")]);
        assert_eq!(relay.send("POST", &members, None, to_root(&second).unwrap()).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(relay.send("POST", &members, Some(&laptop), to_root(&second).unwrap()).await.0, StatusCode::CREATED);
        assert_eq!(relay.send("POST", &members, Some(&laptop), to_root(&first).unwrap()).await.0, StatusCode::CONFLICT);

        let mut clock = Clock::new("laptop".parse().unwrap());
        let event = Event::new(clock.now(1), 1, None, &serde_json::json!({"kind": "space_founded"})).unwrap();
        let envelope = Envelope::seal_event(&space(), 0, &key, &laptop, &event);
        let body = to_root(&envelope).unwrap();
        assert_eq!(relay.send("POST", &envelopes, Some(&laptop), body.clone()).await.0, StatusCode::CREATED);
        assert_eq!(relay.send("POST", &envelopes, Some(&laptop), body).await.0, StatusCode::OK);

        // The phone reads the member list (open), its keys, and the envelope.
        let (status, records) = relay.send("GET", &format!("{members}?after=-1"), None, vec![]).await;
        assert_eq!(status, StatusCode::OK);
        let records: Vec<MemberRecord> =
            records.as_array().unwrap().iter().map(|r| from_root(r.to_string().as_bytes()).unwrap()).collect();
        let list = MemberList::from_records(D, space(), &records).unwrap();
        let keys = list.keys(&phone).unwrap();
        let (status, page) = relay.send("GET", &format!("{envelopes}?after=0"), Some(&phone), vec![]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["more"], false);
        let received: Envelope = from_root(page["envelopes"][0]["envelope"].to_string().as_bytes()).unwrap();
        assert_eq!(Event::read(&received.open_event(&keys, &list).unwrap()).unwrap(), event);
    }

    #[tokio::test]
    async fn strangers_are_refused() {
        let relay = Scratch::new("strangers");
        let (laptop, stranger) = (MemberKeys::generate(D), MemberKeys::generate(D));
        let (first, key) = founded(&laptop);
        let members = format!("/v1/spaces/{}/members", space());
        relay.send("POST", &members, None, to_root(&first).unwrap()).await;
        let envelopes = format!("/v1/spaces/{}/envelopes", space());
        let (status, _) = relay.send("GET", &format!("{envelopes}?after=0"), Some(&stranger), vec![]).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let forged = Envelope::seal(&space(), 0, &key, &stranger, aktujo_crypt::EnvelopeContent::Original, b"x");
        let (status, _) = relay.send("POST", &envelopes, Some(&stranger), to_root(&forged).unwrap()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = relay.send("GET", "/v1/spaces/not-a-space/members", None, vec![]).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn requests_to_join_wait_for_a_manager() {
        let relay = Scratch::new("requests");
        let (laptop, phone) = (MemberKeys::generate(D), MemberKeys::generate(D));
        let requests = format!("/v1/spaces/{}/requests", space());
        let invitation = aktujo_crypt::Invitation::new("https://relay.example", space());
        let request = to_root(&invitation.request(device(&phone, "phone"))).unwrap();
        assert_eq!(relay.send("POST", &requests, None, request.clone()).await.0, StatusCode::NOT_FOUND);
        let (first, _) = founded(&laptop);
        relay.send("POST", &format!("/v1/spaces/{}/members", space()), None, to_root(&first).unwrap()).await;
        assert_eq!(relay.send("POST", &requests, None, request).await.0, StatusCode::CREATED);
        assert_eq!(relay.send("GET", &requests, Some(&phone), vec![]).await.0, StatusCode::FORBIDDEN);
        let (status, waiting) = relay.send("GET", &requests, Some(&laptop), vec![]).await;
        assert_eq!(status, StatusCode::OK);
        let waiting: aktujo_crypt::JoinRequest = from_root(waiting[0].to_string().as_bytes()).unwrap();
        assert!(invitation.proves(&waiting));
    }
}
