// SPDX-License-Identifier: AGPL-3.0-or-later
//! The open API of aktujo: HTTP and JSON under `/api/v1`, described in OpenAPI (`api/openapi.json`), and the web
//! app around it, embedded into the binary. Like every interface, it works through the workspace of `aktujo-app`
//! only. It listens on the device itself; OPEN: authentication and TLS before it listens anywhere else.

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{delete, get},
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use aktujo_app::{AppError, Workspace};

/// The description of the API in OpenAPI.
#[derive(Debug, OpenApi)]
#[openapi(
    info(title = "aktujo", description = "The open API of aktujo: your documents, kept safe – end to end."),
    paths(items, add_item, remove_item),
    components(schemas(Problem))
)]
pub struct ApiDoc;

/// The description of the API in OpenAPI, as pretty JSON – what `api/openapi.json` holds.
pub fn openapi() -> String {
    ApiDoc::openapi().to_pretty_json().expect("the description in JSON")
}

/// An item as the API shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ItemView {
    /// Unique within the workspace.
    pub id: u64,
    /// What the item is called.
    pub title: String,
}

impl From<&aktujo_core::Item> for ItemView {
    fn from(item: &aktujo_core::Item) -> Self {
        ItemView { id: item.id, title: item.title.clone() }
    }
}

/// A new item.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewItem {
    /// What it is called: not empty, at most 200 characters.
    pub title: String,
}

/// An error: what was refused or went wrong.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Problem {
    /// What happened, in English.
    pub error: String,
}

/// An answer that went wrong: a status and what to say.
#[derive(Debug)]
pub struct ApiError(StatusCode, String);

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        let status = match &error {
            AppError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Store(_) => StatusCode::NOT_FOUND,
        };
        ApiError(status, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(Problem { error: self.1 })).into_response()
    }
}

/// The state of the server: one workspace.
type Shared = Arc<Mutex<Workspace>>;

/// Every item.
#[utoipa::path(get, path = "/api/v1/items", responses((status = 200, body = Vec<ItemView>)))]
async fn items(State(workspace): State<Shared>) -> Json<Vec<ItemView>> {
    Json(workspace.lock().expect("the workspace").items().iter().map(ItemView::from).collect())
}

/// Adds an item.
#[utoipa::path(post, path = "/api/v1/items", request_body = NewItem,
    responses((status = 201, body = ItemView), (status = 422, body = Problem)))]
async fn add_item(
    State(workspace): State<Shared>,
    Json(new): Json<NewItem>,
) -> Result<(StatusCode, Json<ItemView>), ApiError> {
    let item = workspace.lock().expect("the workspace").add_item(&new.title)?;
    Ok((StatusCode::CREATED, Json(ItemView::from(&item))))
}

/// Removes an item.
#[utoipa::path(delete, path = "/api/v1/items/{id}", params(("id" = u64, Path, description = "the item")),
    responses((status = 200, body = ItemView), (status = 404, body = Problem)))]
async fn remove_item(State(workspace): State<Shared>, Path(id): Path<u64>) -> Result<Json<ItemView>, ApiError> {
    let item = workspace.lock().expect("the workspace").remove_item(id)?;
    Ok(Json(ItemView::from(&item)))
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// The web app, built by `pnpm build` in `web/` and embedded into the binary; a build without it serves the API alone.
#[derive(rust_embed::Embed)]
#[folder = "../../web/dist"]
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

/// The router: the API under `/api/v1`, and the web app around it.
pub fn router(workspace: Workspace) -> Router {
    let api = Router::new()
        .route("/openapi.json", get(openapi_json))
        .route("/items", get(items).post(add_item))
        .route("/items/{id}", delete(remove_item))
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "no such route of the API".into()) });
    Router::new().nest("/api/v1", api).fallback(web_app).with_state(Arc::new(Mutex::new(workspace)))
}

/// Serves a workspace on the device itself, at `port`, until interrupted; `ready` is told the address.
pub async fn serve(workspace: Workspace, port: u16, ready: impl FnOnce(SocketAddr)) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    ready(listener.local_addr()?);
    axum::serve(listener, router(workspace))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}

#[cfg(test)]
mod tests;
