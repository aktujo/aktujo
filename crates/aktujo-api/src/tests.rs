// SPDX-License-Identifier: AGPL-3.0-or-later
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::*;

fn server() -> Router {
    router(Arc::new(Mutex::new(Workspace::in_memory("laptop").unwrap())), Background::default())
}

async fn send(router: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes.to_vec())
}

fn request(method: &str, uri: &str) -> axum::http::request::Builder {
    Request::builder().method(method).uri(uri).header(header::HOST, "127.0.0.1:8071")
}

async fn call(router: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = request(method, uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map(|b| Body::from(b.to_string())).unwrap_or_default())
        .unwrap();
    let (status, _, bytes) = send(router, request).await;
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn upload(router: &Router, uri: &str, bytes: &'static [u8]) -> (StatusCode, Value) {
    let request = request("POST", uri).header(header::CONTENT_TYPE, "application/pdf").body(Body::from(bytes)).unwrap();
    let (status, _, bytes) = send(router, request).await;
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn the_archive_through_the_api() {
    let router = server();
    assert_eq!(call(&router, "GET", "/api/v1/spaces", None).await.1, json!([]));
    let (status, space) = call(&router, "POST", "/api/v1/spaces", Some(json!({"name": " Home "}))).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(space["name"], "Home");
    let space = space["id"].as_str().unwrap().to_owned();
    let (status, _) = call(&router, "POST", "/api/v1/spaces", Some(json!({"name": ""}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let documents = format!("/api/v1/spaces/{space}/documents");
    let (status, added) = upload(&router, &format!("{documents}?name=Mietvertrag%202026.pdf"), b"%PDF-1.7 lease").await;
    assert_eq!((status, &added["new"]), (StatusCode::CREATED, &json!(true)));
    let (status, again) = upload(&router, &format!("{documents}?name=copy.pdf"), b"%PDF-1.7 lease").await;
    assert_eq!((status, &again["document"]), (StatusCode::OK, &added["document"]));
    let document = format!("{documents}/{}", added["document"].as_str().unwrap());

    let (status, view) = call(&router, "GET", &document, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["title"], "Mietvertrag 2026");
    assert_eq!(view["reading"], "waiting");
    assert_eq!(view["versions"][0]["media_type"], "application/pdf");

    call(&router, "PUT", &format!("{document}/kind"), Some(json!({"kind": "contract"}))).await;
    let deposit = json!({"value": {"type": "money", "value": {"amount": "1500.00", "currency": "EUR"}}});
    let (status, view) = call(&router, "PUT", &format!("{document}/fields/deposit"), Some(deposit)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["kind"], "contract");
    assert_eq!(view["fields"][0]["confirmed"]["value"]["amount"], "1500.00");
    let bad = json!({"value": {"type": "date", "value": "2026-02-30"}});
    assert_eq!(
        call(&router, "PUT", &format!("{document}/fields/deposit"), Some(bad)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    call(&router, "POST", &format!("{document}/labels"), Some(json!({"label": "Wohnung"}))).await;
    let (_, view) = call(&router, "PUT", &format!("{document}/title"), Some(json!({"title": "Lease"}))).await;
    assert_eq!((&view["title"], &view["labels"]), (&json!("Lease"), &json!(["Wohnung"])));

    let (_, found) = call(&router, "POST", "/api/v1/search", Some(json!({"words": "lea"}))).await;
    assert_eq!(found[0]["title"], "Lease");
    assert_eq!(found[0]["excerpt"][0], json!({"text": "Lease", "hit": true}));
    let (_, found) =
        call(&router, "POST", "/api/v1/search", Some(json!({"labels": ["Wohnung"], "kind": "contract"}))).await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    let (_, labels) = call(&router, "GET", &format!("/api/v1/spaces/{space}/labels"), None).await;
    assert_eq!(labels, json!([{"label": "Wohnung", "documents": 1}]));

    let (status, view) =
        upload(&router, &format!("{document}/versions?name=signed.pdf"), b"%PDF-1.7 lease, signed").await;
    assert_eq!((status, view["versions"].as_array().unwrap().len()), (StatusCode::OK, 2));
    let (status, headers, bytes) =
        send(&router, request("GET", &format!("{document}/versions/1/file")).body(Body::empty()).unwrap()).await;
    assert_eq!((status, bytes.as_slice()), (StatusCode::OK, &b"%PDF-1.7 lease"[..]));
    assert_eq!(headers[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(headers[header::CONTENT_DISPOSITION], "inline; filename*=UTF-8''Mietvertrag%202026.pdf");
    assert_eq!(call(&router, "GET", &format!("{document}/versions/3/file"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&router, "GET", &format!("{document}/text"), None).await.0, StatusCode::NOT_FOUND);

    let (_, view) = call(&router, "POST", &format!("{document}/trash"), None).await;
    assert_eq!(view["trashed"], true);
    assert_eq!(call(&router, "POST", "/api/v1/search", Some(json!({}))).await.1, json!([]));
    let (_, found) = call(&router, "POST", "/api/v1/search", Some(json!({"bin": "inside"}))).await;
    assert_eq!(found[0]["trashed"], true);
    call(&router, "POST", &format!("{document}/restore"), None).await;
    assert_eq!(call(&router, "GET", "/api/v1/spaces", None).await.1[0]["documents"], 1);

    assert_eq!(call(&router, "GET", "/api/v1/spaces/nothing/labels", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&router, "GET", "/api/v1/nothing", None).await.0, StatusCode::NOT_FOUND);
    let (_, kinds) = call(&router, "GET", "/api/v1/kinds", None).await;
    assert!(kinds.as_array().unwrap().iter().any(|k| k["id"] == "invoice"));
    assert_eq!(call(&router, "GET", "/api/v1/status", None).await.1["ocr"], false);
}

#[tokio::test]
async fn other_hosts_and_other_sites_are_refused() {
    let router = server();
    let rebound = Request::get("/api/v1/spaces").header(header::HOST, "evil.example:8071").body(Body::empty()).unwrap();
    assert_eq!(send(&router, rebound).await.0, StatusCode::MISDIRECTED_REQUEST);
    for host in ["localhost:8071", "[::1]:8071", "127.0.0.1"] {
        let local = Request::get("/api/v1/spaces").header(header::HOST, host).body(Body::empty()).unwrap();
        assert_eq!(send(&router, local).await.0, StatusCode::OK, "{host}");
    }
    let foreign = request("POST", "/api/v1/spaces")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::from(r#"{"name":"x"}"#))
        .unwrap();
    assert_eq!(send(&router, foreign).await.0, StatusCode::FORBIDDEN);
    let own = request("POST", "/api/v1/spaces")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "http://127.0.0.1:8071")
        .header("sec-fetch-site", "same-origin")
        .body(Body::from(r#"{"name":"x"}"#))
        .unwrap();
    assert_eq!(send(&router, own).await.0, StatusCode::CREATED);
}

#[test]
fn the_description_names_every_route() {
    let description = openapi();
    for path in [
        "/api/v1/spaces",
        "/api/v1/search",
        "/api/v1/join",
        "/api/v1/spaces/{space}/members/{member}",
        "/api/v1/spaces/{space}/documents/{document}/fields/{key}",
    ] {
        assert!(description.contains(path), "{path}");
    }
}

/// A relay on a free port of this machine, in a folder of its own.
async fn relay() -> String {
    let folder = std::env::temp_dir().join(format!("aktujo-api-relay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let relay = aktujo_relay::Relay::open(aktujo_crypt::Domain::AKTUJO, &folder).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(aktujo_relay::serve_listener(relay, listener));
    format!("http://{address}")
}

#[tokio::test(flavor = "multi_thread")]
async fn sharing_through_the_api() {
    let url = relay().await;
    let laptop = server();
    let phone = router(Arc::new(Mutex::new(Workspace::in_memory("phone").unwrap())), Background::default());
    assert_eq!(call(&laptop, "PUT", "/api/v1/device", Some(json!({"name": "Laptop"}))).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&laptop, "GET", "/api/v1/status", None).await.1["device_name"], "Laptop");
    call(&phone, "PUT", "/api/v1/device", Some(json!({"name": "Phone"}))).await;

    let (_, space) = call(&laptop, "POST", "/api/v1/spaces", Some(json!({"name": "Home"}))).await;
    let space = space["id"].as_str().unwrap().to_owned();
    let (status, code) =
        call(&laptop, "POST", &format!("/api/v1/spaces/{space}/sync/start"), Some(json!({"relay": url}))).await;
    assert_eq!(status, StatusCode::OK, "{code}");
    assert!(code["recovery_code"].as_str().unwrap().len() > 20);
    let (_, spaces) = call(&laptop, "GET", "/api/v1/spaces", None).await;
    assert_eq!((&spaces[0]["rights"], &spaces[0]["sync_problem"]), (&json!("manage"), &Value::Null));
    assert!(spaces[0]["synced_at"].as_u64().is_some());

    let (_, invitation) =
        call(&laptop, "POST", &format!("/api/v1/spaces/{space}/invitations"), Some(json!({"rights": "write"}))).await;
    let (status, joined) = call(&phone, "POST", "/api/v1/join", Some(invitation.clone())).await;
    assert_eq!(status, StatusCode::OK, "{joined}");
    assert_eq!(joined["waiting"], true);
    let (_, report) = call(&laptop, "POST", &format!("/api/v1/spaces/{space}/sync"), None).await;
    assert_eq!(report["added"], json!(["Phone"]));
    let (_, report) = call(&phone, "POST", &format!("/api/v1/spaces/{space}/sync"), None).await;
    assert_eq!(report["waiting"], false);
    let (_, spaces) = call(&phone, "GET", "/api/v1/spaces", None).await;
    assert_eq!((&spaces[0]["name"], &spaces[0]["rights"]), (&json!("Home"), &json!("write")));

    let (_, members) = call(&laptop, "GET", &format!("/api/v1/spaces/{space}/members"), None).await;
    let members = members.as_array().unwrap();
    assert_eq!(members.len(), 3, "two devices and the recovery code");
    let phone_id = members.iter().find(|m| m["name"] == "Phone").unwrap()["id"].as_str().unwrap().to_owned();
    assert!(members.iter().any(|m| m["role"] == "recovery"));
    let path = format!("/api/v1/spaces/{space}/members/{phone_id}");
    assert_eq!(call(&laptop, "DELETE", &path, None).await.0, StatusCode::NO_CONTENT);
    let (status, _) = call(&phone, "POST", &format!("/api/v1/spaces/{space}/sync"), None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "removed");
    let (_, spaces) = call(&phone, "GET", "/api/v1/spaces", None).await;
    assert!(spaces[0]["sync_problem"].as_str().unwrap().contains("removed"));
}
