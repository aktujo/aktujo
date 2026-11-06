// SPDX-License-Identifier: AGPL-3.0-or-later
//! The web app of aktujo, built in: its page, scripts, styles and fonts, served from the binary with a
//! Content-Security-Policy that allows nothing from elsewhere. Around a router – the API's in `aktujo serve` –, it
//! answers every path that router does not know. It talks to aktujo through the API only.

use axum::{
    Router,
    http::header,
    response::{Html, IntoResponse, Response},
};

/// `router` with the web app around it: every path it does not know is a file of the app or the app itself.
pub fn around<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router.fallback(web_app)
}

/// The web app, built by `pnpm build` in `web/` of the repository into this crate (`web/` here) and embedded; a build
/// without it says so.
#[derive(rust_embed::Embed)]
#[folder = "web"]
#[allow_missing = true]
struct WebApp;

/// What the web app may load: itself only – no script, style, font or connection elsewhere, no frames.
const POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

/// The page shown where the web app is not built into the binary.
const NO_WEB_APP: &str = "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>aktujo</title>\
<body style=\"font-family:system-ui;max-width:40rem;margin:4rem auto;padding:0 1rem\"><h1>aktujo</h1>\
<p>The API runs at <code>/api/v1</code>, described at <code>/api/v1/openapi.json</code>. This build has no web app \
– run <code>pnpm build</code> in <code>web/</code> and build again.</p>";

/// A file of the web app; any other path is the app itself, which routes by the address.
async fn web_app(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (file, path) = match WebApp::get(path) {
        Some(file) if !path.is_empty() => (file, path),
        _ => match WebApp::get("index.html") {
            Some(file) => (file, "index.html"),
            None => return Html(NO_WEB_APP).into_response(),
        },
    };
    // Built files carry their hash in their name and never change; the page itself is asked for every time.
    let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    (
        [
            (header::CONTENT_TYPE, file.metadata.mimetype().to_owned()),
            (header::CACHE_CONTROL, cache.to_owned()),
            (header::CONTENT_SECURITY_POLICY, POLICY.to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::REFERRER_POLICY, "no-referrer".to_owned()),
        ],
        file.data,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request, routing::get};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn get_page(router: &Router, uri: &str) -> Response {
        router.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap()
    }

    #[tokio::test]
    async fn the_router_first_then_the_app() {
        let router = around(Router::new().route("/api/v1/status", get(|| async { "ok" })));
        let api = get_page(&router, "/api/v1/status").await;
        assert_eq!(api.into_body().collect().await.unwrap().to_bytes(), "ok");
        for path in ["/", "/spaces/x/documents/y"] {
            let page = get_page(&router, path).await;
            assert_eq!(page.status(), 200);
            assert!(page.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"));
            if WebApp::get("index.html").is_some() {
                assert_eq!(page.headers()[header::CONTENT_SECURITY_POLICY], POLICY);
            }
        }
    }
}
