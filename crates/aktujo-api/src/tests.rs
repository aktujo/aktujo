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

    // A space has no kinds until someone defines or imports them.
    let kinds = format!("/api/v1/spaces/{space}/kinds");
    assert_eq!(call(&router, "GET", &kinds, None).await.1, json!([]));
    assert_eq!(
        call(&router, "PUT", &format!("{document}/kind"), Some(json!({"kind": "contract"}))).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, kind) = call(&router, "POST", &kinds, Some(json!({"name": "Contract"}))).await;
    assert_eq!((status, &kind), (StatusCode::OK, &json!({"id": "contract", "name": "Contract", "fields": []})));
    let contract = json!({"id": "contract", "name": "Contract", "names": {"de": "Vertrag"},
        "fields": [{"key": "deposit", "name": "Deposit", "type": "money",
            "find": [{"way": "label", "labels": ["kaution"]}]}],
        "words": [{"word": "mietvertrag", "weight": 6}]});
    let (status, kind) = call(&router, "PUT", &format!("{kinds}/contract"), Some(contract.clone())).await;
    assert_eq!((status, &kind), (StatusCode::OK, &contract));
    let mut renamed = contract.clone();
    renamed["id"] = json!("lease");
    let refused = call(&router, "PUT", &format!("{kinds}/contract"), Some(renamed)).await.0;
    assert_eq!(refused, StatusCode::UNPROCESSABLE_ENTITY);
    let bad = json!({"id": "contract", "name": "", "fields": []});
    assert_eq!(call(&router, "PUT", &format!("{kinds}/contract"), Some(bad)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let template = json!({"name": "Books", "kinds": [{"id": "book", "name": "Book",
        "fields": [{"key": "isbn", "name": "ISBN", "type": "text", "code": true}]}]});
    let (status, imported) =
        call(&router, "POST", &format!("/api/v1/spaces/{space}/kind-import"), Some(template)).await;
    assert_eq!((status, imported), (StatusCode::OK, json!({"changed": 1})));
    assert_eq!(call(&router, "GET", &kinds, None).await.1.as_array().unwrap().len(), 2);
    assert_eq!(call(&router, "DELETE", &format!("{kinds}/book"), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&router, "GET", &kinds, None).await.1, json!([contract]));

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
        "/api/v1/spaces/{space}/kinds/{kind}",
        "/api/v1/spaces/{space}/kind-import",
        "/api/v1/watches",
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

/// A catalog on a free port of this machine, its files in memory – changed by the test as a source would be.
async fn catalog_source(files: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>) -> String {
    let app = Router::new().fallback(move |uri: axum::http::Uri| {
        let files = files.clone();
        async move {
            match files.lock().unwrap().get(uri.path().trim_start_matches('/')) {
                Some(bytes) => (StatusCode::OK, bytes.clone()).into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

/// Puts a template with `kinds` into the catalog's files, as `tools/catalog.py` of aktujo/catalog writes them.
fn publish(files: &Mutex<std::collections::HashMap<String, Vec<u8>>>, kinds: Value) {
    let template =
        serde_json::to_vec(&json!({"author": "someone", "license": "MIT", "name": "Books", "kinds": kinds})).unwrap();
    let sha256 = aktujo_sdk::Sha256Hash::of(&template).hex();
    let catalog = json!({"format": 1, "source": {"name": "test"}, "templates": [{
        "id": "someone.books", "author": "someone", "path": "templates/someone/books/template.json",
        "sha256": sha256, "name": "Books", "kinds": [{"id": "book", "name": "Book"}]}]});
    let mut files = files.lock().unwrap();
    files.insert("templates/someone/books/template.json".into(), template);
    files.insert("catalog.json".into(), serde_json::to_vec(&catalog).unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn templates_from_a_catalog_are_taken_and_updated() {
    let files = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let book = json!({"id": "book", "name": "Book", "fields": [{"key": "isbn", "name": "ISBN", "type": "text"}]});
    publish(&files, json!([book]));
    let url = catalog_source(files.clone()).await;
    let router = server();
    let (_, space) = call(&router, "POST", "/api/v1/spaces", Some(json!({"name": "Books"}))).await;
    let space = space["id"].as_str().unwrap().to_owned();

    // The official catalog is switched off, so that the test reads nothing from the internet; ours is added.
    let (_, sources) = call(&router, "PUT", "/api/v1/sources", Some(json!({"url": DEFAULT, "on": false}))).await;
    assert_eq!(sources[0], json!({"url": DEFAULT, "on": false, "default": true}));
    let removed = call(&router, "DELETE", &format!("/api/v1/sources?url={DEFAULT}"), None).await.0;
    assert_eq!(removed, StatusCode::FORBIDDEN);
    let (status, sources) = call(&router, "POST", "/api/v1/sources", Some(json!({"url": url}))).await;
    assert_eq!((status, sources.as_array().unwrap().len()), (StatusCode::OK, 2));

    let catalog = format!("/api/v1/spaces/{space}/catalog");
    let (status, view) = call(&router, "GET", &catalog, None).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["sources"], json!([{"url": url, "name": "test", "description": "", "error": null}]));
    assert_eq!((&view["templates"][0]["standing"], &view["templates"][0]["official"]), (&json!("new"), &json!(false)));

    let which = json!({"source": url, "template": "someone.books"});
    let (_, plan) = call(&router, "POST", &format!("{catalog}/plan"), Some(which.clone())).await;
    assert_eq!(plan["kinds"][0]["step"], "new");
    let (_, taken) = call(&router, "POST", &format!("{catalog}/take"), Some(which.clone())).await;
    assert_eq!(taken, json!({"changed": 1}));
    assert_eq!(call(&router, "GET", &catalog, None).await.1["templates"][0]["standing"], "taken");

    // A person renames the kind; the template gets a field.
    let mut mine = book.clone();
    mine["name"] = json!("Volume");
    call(&router, "PUT", &format!("/api/v1/spaces/{space}/kinds/book"), Some(mine)).await;
    let mut newer = book.clone();
    newer["fields"].as_array_mut().unwrap().push(json!({"key": "year", "name": "Year", "type": "number"}));
    publish(&files, json!([newer]));
    assert_eq!(call(&router, "GET", &catalog, None).await.1["templates"][0]["standing"], "update");
    let (_, plan) = call(&router, "POST", &format!("{catalog}/plan"), Some(which.clone())).await;
    assert_eq!((&plan["kinds"][0]["step"], &plan["kinds"][0]["conflicts"]), (&json!("merge"), &json!([])));
    assert_eq!(plan["kinds"][0]["proposed"]["name"], "Volume");
    call(&router, "POST", &format!("{catalog}/take"), Some(which)).await;
    let (_, kinds) = call(&router, "GET", &format!("/api/v1/spaces/{space}/kinds"), None).await;
    assert_eq!((&kinds[0]["name"], kinds[0]["fields"].as_array().unwrap().len()), (&json!("Volume"), 2));
    assert_eq!(call(&router, "GET", &catalog, None).await.1["templates"][0]["standing"], "taken");

    // A source switched off is not read; one that cannot be read says why.
    let other = json!({"url": "http://127.0.0.1:9"});
    call(&router, "POST", "/api/v1/sources", Some(other)).await;
    call(&router, "PUT", "/api/v1/sources", Some(json!({"url": url, "on": false}))).await;
    let (_, view) = call(&router, "GET", &catalog, None).await;
    assert_eq!(view["templates"], json!([]));
    assert!(view["sources"][0]["error"].as_str().is_some());
}

const DEFAULT: &str = aktujo_sdk::DEFAULT_SOURCE;

#[tokio::test]
async fn watched_folders_through_the_api() {
    let workspace = Arc::new(Mutex::new(Workspace::in_memory("laptop").unwrap()));
    let router = router(workspace.clone(), Background::default());
    let (_, space) = call(&router, "POST", "/api/v1/spaces", Some(json!({"name": "Scans"}))).await;
    let space = space["id"].as_str().unwrap().to_owned();
    let folder = std::env::temp_dir().join(format!("aktujo-api-watch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let folder = folder.canonicalize().unwrap().to_string_lossy().into_owned();

    let refused = call(&router, "POST", "/api/v1/watches", Some(json!({"space": space, "folder": "scans"}))).await.0;
    assert_eq!(refused, StatusCode::UNPROCESSABLE_ENTITY, "not an absolute path");
    let (status, watches) =
        call(&router, "POST", "/api/v1/watches", Some(json!({"space": space, "folder": folder}))).await;
    assert_eq!(status, StatusCode::OK, "{watches}");
    let expected = json!([{"space": space, "folder": folder, "after": "leave", "on": true, "subfolders": false,
        "looked_at": null, "taken": 0, "problem": null}]);
    assert_eq!(watches, expected);

    std::fs::write(std::path::Path::new(&folder).join("letter.pdf"), b"%PDF-1.7 letter").unwrap();
    let mut looks = aktujo_sdk::Looks::default();
    aktujo_sdk::look(&workspace, &mut looks);
    assert_eq!(aktujo_sdk::look(&workspace, &mut looks), 1);
    let (_, watches) = call(&router, "GET", "/api/v1/watches", None).await;
    assert_eq!(watches[0]["taken"], 1);
    assert_eq!(call(&router, "GET", "/api/v1/spaces", None).await.1[0]["documents"], 1);

    let switch = json!({"space": space, "folder": folder, "on": false, "after": "move"});
    let (_, watches) = call(&router, "PUT", "/api/v1/watches", Some(switch)).await;
    assert_eq!((&watches[0]["on"], &watches[0]["after"]), (&json!(false), &json!("move")));
    let query = format!("/api/v1/watches?space={space}&folder={}", folder.replace('/', "%2F"));
    assert_eq!(call(&router, "DELETE", &query, None).await.1, json!([]));
    std::fs::remove_dir_all(&folder).unwrap();
}

#[cfg(feature = "scan")]
#[tokio::test(flavor = "multi_thread")]
async fn pages_are_shown_improved_and_combined() {
    fn png(width: u32, height: u32) -> &'static [u8] {
        let image = image::RgbImage::from_fn(width, height, |x, y| image::Rgb([(x * 3) as u8, (y * 3) as u8, 90]));
        let mut bytes = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes.leak()
    }
    let router = server();
    let (_, space) = call(&router, "POST", "/api/v1/spaces", Some(json!({"name": "Scans"}))).await;
    let space = space["id"].as_str().unwrap().to_owned();
    assert_eq!(call(&router, "GET", "/api/v1/status", None).await.1["improve"], true);
    let add = |name: &'static str, bytes: &'static [u8]| {
        let router = router.clone();
        let space = space.clone();
        async move {
            let request = request("POST", &format!("/api/v1/spaces/{space}/documents?name={name}"))
                .header(header::CONTENT_TYPE, "image/png")
                .body(Body::from(bytes))
                .unwrap();
            let (_, _, body) = send(&router, request).await;
            serde_json::from_slice::<Value>(&body).unwrap()["document"].as_str().unwrap().to_owned()
        }
    };
    let first = add("one.png", png(60, 80)).await;
    let second = add("two.png", png(80, 60)).await;
    let document = format!("/api/v1/spaces/{space}/documents/{first}");

    assert_eq!(call(&router, "GET", &format!("{document}/versions/1/pages"), None).await.1, json!({"pages": 1}));
    let (status, headers, bytes) =
        send(&router, request("GET", &format!("{document}/versions/1/pages/0?width=30")).body(Body::empty()).unwrap())
            .await;
    assert_eq!(
        (status, &headers[header::CONTENT_TYPE]),
        (StatusCode::OK, &"image/png".parse::<axum::http::HeaderValue>().unwrap())
    );
    assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 30);
    assert_eq!(
        call(&router, "GET", &format!("{document}/versions/1/pages/1"), None).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let (status, found) = call(&router, "GET", &format!("{document}/versions/1/pages/0/corners"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(found.get("corners").is_some(), "{found}");
    let half = json!({"corners": [{"points": [[0, 0], [0.5, 0], [0.5, 0.5], [0, 0.5]]}], "turns": [1]});
    let preview = request("POST", &format!("{document}/improve/preview?page=0&width=800"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(half.to_string()))
        .unwrap();
    let shown = image::load_from_memory(&send(&router, preview).await.2).unwrap();
    assert_eq!((shown.width(), shown.height()), (40, 30), "the top left quarter, turned");

    let steps = json!({"turns": [1], "colour": "grey"});
    let preview = request("POST", &format!("{document}/improve/preview?page=0&width=800"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(steps.to_string()))
        .unwrap();
    let (status, _, bytes) = send(&router, preview).await;
    let shown = image::load_from_memory(&bytes).unwrap();
    assert_eq!((status, shown.width(), shown.height()), (StatusCode::OK, 80, 60), "turned by a quarter");
    let (status, view) = call(&router, "POST", &format!("{document}/improve"), Some(steps)).await;
    assert_eq!((status, view["versions"].as_array().unwrap().len()), (StatusCode::OK, 2));
    assert_eq!(view["versions"][1]["media_type"], "image/png");

    let combine = json!({"documents": [first, second], "improve": {"colour": "black_white"}});
    let (status, view) = call(&router, "POST", &format!("/api/v1/spaces/{space}/combine"), Some(combine)).await;
    assert_eq!((status, &view["versions"][2]["media_type"]), (StatusCode::OK, &json!("application/pdf")));
    assert_eq!(call(&router, "GET", &format!("{document}/versions/3/pages"), None).await.1, json!({"pages": 2}));
    let one = json!({"documents": [first]});
    assert_eq!(
        call(&router, "POST", &format!("/api/v1/spaces/{space}/combine"), Some(one)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_space_is_exported_as_a_zip() {
    let router = server();
    let (_, space) = call(&router, "POST", "/api/v1/spaces", Some(json!({"name": "Home"}))).await;
    let space = space["id"].as_str().unwrap().to_owned();
    upload(&router, &format!("/api/v1/spaces/{space}/documents?name=lease.pdf"), b"%PDF-1.7 lease").await;
    let (status, headers, bytes) =
        send(&router, request("GET", &format!("/api/v1/spaces/{space}/export")).body(Body::empty()).unwrap()).await;
    assert_eq!(
        (status, &headers[header::CONTENT_TYPE]),
        (StatusCode::OK, &"application/zip".parse::<axum::http::HeaderValue>().unwrap())
    );
    let disposition = headers[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert!(disposition.starts_with("attachment; filename*=UTF-8''aktujo%20Home%20"), "{disposition}");
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/documents/0001 lease/v1 lease.pdf") && text.contains("%PDF-1.7 lease"));
    assert_eq!(call(&router, "GET", "/api/v1/spaces/nothing/export", None).await.0, StatusCode::NOT_FOUND);

    // The export comes back as a new space.
    let import = request("POST", "/api/v1/space-import?name=Home%20again")
        .header(header::CONTENT_TYPE, "application/zip")
        .body(Body::from(bytes))
        .unwrap();
    let (status, _, answer) = send(&router, import).await;
    let imported: Value = serde_json::from_slice(&answer).unwrap();
    assert_eq!(status, StatusCode::CREATED, "{imported}");
    assert_eq!((imported["documents"].as_u64(), imported["skipped"].as_array().map(Vec::len)), (Some(1), Some(0)));
    let spaces = call(&router, "GET", "/api/v1/spaces", None).await.1;
    let again = spaces.as_array().unwrap().iter().find(|s| s["name"] == "Home again").unwrap();
    assert_eq!((&again["id"], &again["documents"]), (&imported["space"], &json!(1)));
    let not_a_zip = request("POST", "/api/v1/space-import").body(Body::from("no zip")).unwrap();
    assert_eq!(send(&router, not_a_zip).await.0, StatusCode::UNPROCESSABLE_ENTITY);
}
