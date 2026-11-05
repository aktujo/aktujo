// SPDX-License-Identifier: AGPL-3.0-or-later
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use super::*;

async fn call(router: &Router, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, String) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map(|b| Body::from(b.to_owned())).unwrap_or_default())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn items_through_the_api() {
    let router = router(Workspace::new());
    let (status, body) = call(&router, "POST", "/api/v1/items", Some(r#"{"title":" tea "}"#)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body, r#"{"id":1,"title":"tea"}"#);
    let (status, _) = call(&router, "POST", "/api/v1/items", Some(r#"{"title":""}"#)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(call(&router, "GET", "/api/v1/items", None).await.1, r#"[{"id":1,"title":"tea"}]"#);
    assert_eq!(call(&router, "DELETE", "/api/v1/items/1", None).await.0, StatusCode::OK);
    assert_eq!(call(&router, "DELETE", "/api/v1/items/1", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&router, "GET", "/api/v1/nothing", None).await.0, StatusCode::NOT_FOUND);
}

#[test]
fn the_description_names_every_route() {
    let description = openapi();
    for path in ["/api/v1/items", "/api/v1/items/{id}"] {
        assert!(description.contains(path), "{path}");
    }
}
