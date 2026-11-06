// SPDX-License-Identifier: AGPL-3.0-or-later
//! The open API of aktujo: HTTP and JSON under `/api/v1`, described in OpenAPI (`api/openapi.json`), and the web
//! app around it, embedded into the binary. Like every interface, it works through the workspace of `aktujo-app`
//! only.
//!
//! It listens on the device itself and answers only requests addressed to it there: the `Host` must be `localhost`,
//! `127.0.0.1` or `[::1]` (against pages that rebind their name to this device), and a request that changes
//! something must not come from another site (`Origin`, `Sec-Fetch-Site`). OPEN: authentication and TLS before it
//! listens anywhere else.

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, MutexGuard},
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use utoipa::OpenApi;

use aktujo_app::{AppError, DocumentId, NewFile, ReadState, Reader, SpaceId, Syncer, Worker, Workspace};

mod views;

pub use views::*;

/// The largest file taken in one request.
pub const UPLOAD_MAX: usize = 256 * 1024 * 1024;

/// The description of the API in OpenAPI.
#[derive(Debug, OpenApi)]
#[openapi(
    info(title = "aktujo", description = "The open API of aktujo: your documents, kept safe – end to end."),
    paths(
        status,
        spaces,
        found_space,
        rename_space,
        labels,
        kinds,
        add_kind,
        define_kind,
        remove_kind,
        import_kinds,
        sources,
        add_source,
        switch_source,
        remove_source,
        catalog,
        plan_template,
        take_template,
        search,
        add_document,
        document,
        original,
        text,
        add_version,
        set_title,
        set_kind,
        set_field,
        clear_field,
        add_label,
        remove_label,
        trash,
        restore,
        set_device_name,
        start_sync,
        sync,
        invite,
        join,
        recover,
        members,
        remove_member
    ),
    components(schemas(Problem))
)]
pub struct ApiDoc;

/// The description of the API in OpenAPI, as pretty JSON – what `api/openapi.json` holds.
pub fn openapi() -> String {
    ApiDoc::openapi().to_pretty_json().expect("the description in JSON")
}

/// An answer that went wrong: a status and what to say.
#[derive(Debug)]
pub struct ApiError(StatusCode, String);

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        let status = match &error {
            AppError::Invalid(_) | AppError::Value(_) | AppError::Kind(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::NotAllowed(_) => StatusCode::FORBIDDEN,
            AppError::Sync(_) | AppError::Catalog(_) => StatusCode::BAD_GATEWAY,
            AppError::Database(_)
            | AppError::Originals(_)
            | AppError::Keys(_)
            | AppError::Folder(_)
            | AppError::Reading(_)
            | AppError::KeyMissing(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError(status, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(Problem { error: self.1 })).into_response()
    }
}

fn not_found(what: &str) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, format!("no {what}"))
}

/// What runs in the background beside the API.
#[derive(Debug, Clone, Default)]
pub struct Background {
    /// Reading, woken when files are added.
    pub worker: Option<Arc<Worker>>,
    /// Synchronising, woken after every change.
    pub syncer: Option<Arc<Syncer>>,
    /// Whether the reader uses OCR.
    pub ocr: bool,
}

/// The state of the server.
#[derive(Clone)]
struct Shared {
    workspace: Arc<Mutex<Workspace>>,
    background: Background,
    /// For titles offered from what was read: the rules only, no models.
    titles: Arc<Reader>,
}

impl Shared {
    /// The workspace – also after a thread panicked while holding it: every command leaves it whole or not at all.
    fn lock(&self) -> MutexGuard<'_, Workspace> {
        self.workspace.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Reads what was added and synchronises.
    fn wake(&self) {
        if let Some(worker) = &self.background.worker {
            worker.wake();
        }
        self.changed();
    }

    /// Synchronises what changed.
    fn changed(&self) {
        if let Some(syncer) = &self.background.syncer {
            syncer.wake();
        }
    }

    /// Runs a command that talks to a relay, off the threads that serve requests.
    async fn blocking<T: Send + 'static>(
        &self,
        command: impl FnOnce(&mut Workspace) -> Result<T, AppError> + Send + 'static,
    ) -> Result<T, ApiError> {
        let shared = self.clone();
        let answer = tokio::task::spawn_blocking(move || command(&mut shared.lock())).await;
        let answer = answer.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
        if let Some(worker) = &self.background.worker {
            worker.wake();
        }
        Ok(answer)
    }
}

type Answer<T> = Result<Json<T>, ApiError>;

/// The language the caller reads, from the first tag of `Accept-Language` (`de-DE` → `de`); `en` without one. A
/// suggested title takes the name of the kind in it, if the kind has one.
struct Language(String);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Language {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, _: &S) -> Result<Self, Self::Rejection> {
        let first = parts
            .headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split([',', ';', '-', '_']).next())
            .map(|tag| tag.trim().to_ascii_lowercase())
            .filter(|tag| (2..=3).contains(&tag.len()) && tag.bytes().all(|b| b.is_ascii_lowercase()));
        Ok(Language(first.unwrap_or_else(|| "en".into())))
    }
}

fn space(id: &str) -> Result<SpaceId, ApiError> {
    id.parse().map_err(|_| not_found("such space"))
}

fn ids(space_id: &str, document: &str) -> Result<(SpaceId, DocumentId), ApiError> {
    Ok((space(space_id)?, document.parse().map_err(|_| not_found("such document"))?))
}

fn document_view(
    workspace: &Workspace,
    shared: &Shared,
    language: &str,
    space: &SpaceId,
    id: &DocumentId,
) -> Answer<DocumentView> {
    let document = workspace.document(space, id)?;
    let reading = workspace.read_state(space, id)?;
    let title = workspace.suggested_title(space, id, &shared.titles, language)?;
    Ok(Json(DocumentView::of(space.as_str(), document, reading, title)))
}

/// What this program is and can do.
#[utoipa::path(get, path = "/api/v1/status", responses((status = 200, body = StatusView)))]
async fn status(State(shared): State<Shared>) -> Json<StatusView> {
    let (device, device_name) = {
        let workspace = shared.lock();
        (workspace.device().to_owned(), workspace.device_name().ok().flatten())
    };
    Json(StatusView {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        device,
        ocr: shared.background.ocr,
        reading_problem: shared.background.worker.as_ref().and_then(|w| w.problem()),
        device_name,
    })
}

/// Every space on this device.
#[utoipa::path(get, path = "/api/v1/spaces", responses((status = 200, body = Vec<SpaceView>)))]
async fn spaces(State(shared): State<Shared>) -> Answer<Vec<SpaceView>> {
    Ok(Json(shared.lock().spaces()?.into_iter().map(SpaceView::from).collect()))
}

/// Founds a space.
#[utoipa::path(post, path = "/api/v1/spaces", request_body = SpaceName,
    responses((status = 201, body = SpaceView), (status = 422, body = Problem)))]
async fn found_space(
    State(shared): State<Shared>,
    Json(new): Json<SpaceName>,
) -> Result<(StatusCode, Json<SpaceView>), ApiError> {
    let mut workspace = shared.lock();
    let id = workspace.found_space(&new.name)?;
    let space = workspace.spaces()?.into_iter().find(|s| s.id == id).ok_or_else(|| not_found("such space"))?;
    Ok((StatusCode::CREATED, Json(space.into())))
}

/// Gives a space a new name.
#[utoipa::path(put, path = "/api/v1/spaces/{space}", request_body = SpaceName,
    params(("space" = String, Path, description = "the space")),
    responses((status = 204), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn rename_space(
    State(shared): State<Shared>,
    Path(id): Path<String>,
    Json(new): Json<SpaceName>,
) -> Result<StatusCode, ApiError> {
    shared.lock().rename_space(&space(&id)?, &new.name)?;
    shared.changed();
    Ok(StatusCode::NO_CONTENT)
}

/// The labels used in a space, with how many documents outside the bin have each.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/labels", params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = Vec<LabelCount>), (status = 404, body = Problem)))]
async fn labels(State(shared): State<Shared>, Path(id): Path<String>) -> Answer<Vec<LabelCount>> {
    let labels = shared.lock().labels(&space(&id)?)?;
    Ok(Json(labels.into_iter().map(|(label, documents)| LabelCount { label, documents }).collect()))
}

/// The kinds of documents a space has – none until someone defines or imports them.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/kinds", params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = Vec<KindView>), (status = 404, body = Problem)))]
async fn kinds(State(shared): State<Shared>, Path(s): Path<String>) -> Answer<Vec<KindView>> {
    let kinds = shared.lock().kinds(&space(&s)?)?;
    Ok(Json(kinds.into_iter().map(KindView::from).collect()))
}

/// Adds a kind with a name; its identity is made from the name.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/kinds", request_body = NewKind,
    params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = KindView), (status = 403, body = Problem), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn add_kind(State(shared): State<Shared>, Path(s): Path<String>, Json(new): Json<NewKind>) -> Answer<KindView> {
    let space = space(&s)?;
    let mut workspace = shared.lock();
    let id = workspace.add_kind(&space, &new.name)?;
    shared.changed();
    Ok(Json(workspace.kind(&space, &id)?.clone().into()))
}

/// Defines a kind anew – its name, fields and rules; the whole definition replaces the one before.
#[utoipa::path(put, path = "/api/v1/spaces/{space}/kinds/{kind}", request_body = KindView,
    params(("space" = String, Path, description = "the space"), ("kind" = String, Path, description = "the kind")),
    responses((status = 200, body = KindView), (status = 403, body = Problem), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn define_kind(
    State(shared): State<Shared>,
    Path((s, k)): Path<(String, String)>,
    Json(kind): Json<KindView>,
) -> Answer<KindView> {
    let space = space(&s)?;
    if kind.id != k {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "the identity of a kind does not change".into()));
    }
    let mut workspace = shared.lock();
    workspace.kind(&space, &k)?;
    workspace.define_kind(&space, kind.into())?;
    shared.changed();
    Ok(Json(workspace.kind(&space, &k)?.clone().into()))
}

/// Removes a kind. Documents keep it as their kind.
#[utoipa::path(delete, path = "/api/v1/spaces/{space}/kinds/{kind}",
    params(("space" = String, Path, description = "the space"), ("kind" = String, Path, description = "the kind")),
    responses((status = 204), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn remove_kind(
    State(shared): State<Shared>,
    Path((s, k)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    shared.lock().remove_kind(&space(&s)?, &k)?;
    shared.changed();
    Ok(StatusCode::NO_CONTENT)
}

/// Takes over a file of kinds, as it is: a template of a catalog, the kinds exported from another space, or a list of
/// kinds. Kinds new to the space are added; one a person changed is merged, if the space knows the template it came
/// from; any other of the same identity is replaced. A template remembers its SHA-256, so that it is found in a
/// catalog later.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/kind-import", request_body = KindImport,
    params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = Imported), (status = 403, body = Problem), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn import_kinds(State(shared): State<Shared>, Path(s): Path<String>, bytes: Bytes) -> Answer<Imported> {
    let changed = shared.lock().import_file(&space(&s)?, &bytes)?;
    shared.changed();
    Ok(Json(Imported { changed }))
}

/// The sources of templates on this device: the official catalog first.
#[utoipa::path(get, path = "/api/v1/sources", responses((status = 200, body = Vec<SourceView>)))]
async fn sources(State(shared): State<Shared>) -> Answer<Vec<SourceView>> {
    Ok(Json(shared.lock().sources()?.into_iter().map(SourceView::from).collect()))
}

/// Adds a source, switched on.
#[utoipa::path(post, path = "/api/v1/sources", request_body = NewSource,
    responses((status = 200, body = Vec<SourceView>), (status = 422, body = Problem)))]
async fn add_source(State(shared): State<Shared>, Json(new): Json<NewSource>) -> Answer<Vec<SourceView>> {
    let mut workspace = shared.lock();
    workspace.add_source(&new.url)?;
    Ok(Json(workspace.sources()?.into_iter().map(SourceView::from).collect()))
}

/// Switches a source on or off – the official catalog too.
#[utoipa::path(put, path = "/api/v1/sources", request_body = SourceSwitch,
    responses((status = 200, body = Vec<SourceView>), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn switch_source(State(shared): State<Shared>, Json(switch): Json<SourceSwitch>) -> Answer<Vec<SourceView>> {
    let mut workspace = shared.lock();
    workspace.set_source(&switch.url, switch.on)?;
    Ok(Json(workspace.sources()?.into_iter().map(SourceView::from).collect()))
}

/// Removes a source; the official catalog can only be switched off.
#[utoipa::path(delete, path = "/api/v1/sources", params(SourceQuery),
    responses((status = 200, body = Vec<SourceView>), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn remove_source(State(shared): State<Shared>, Query(query): Query<SourceQuery>) -> Answer<Vec<SourceView>> {
    let mut workspace = shared.lock();
    workspace.remove_source(&query.url)?;
    Ok(Json(workspace.sources()?.into_iter().map(SourceView::from).collect()))
}

fn blocking_failed(error: tokio::task::JoinError) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

/// The templates of the sources that are on, read now, with how each stands in the space: new, taken over, or with
/// an update. A source that cannot be read is named with why.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/catalog", params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = CatalogView), (status = 404, body = Problem)))]
async fn catalog(State(shared): State<Shared>, Path(s): Path<String>) -> Answer<CatalogView> {
    let space = space(&s)?;
    let sources: Vec<aktujo_app::Source> = {
        let workspace = shared.lock();
        workspace.kinds(&space)?;
        workspace.sources()?.into_iter().filter(|s| s.on).collect()
    };
    // Read without the workspace: a slow source holds nothing.
    let fetched = tokio::task::spawn_blocking(move || {
        sources.into_iter().map(|s| (aktujo_app::fetch_catalog(&s.url), s)).collect::<Vec<_>>()
    })
    .await
    .map_err(blocking_failed)?;
    let workspace = shared.lock();
    let mut view = CatalogView { sources: vec![], templates: vec![] };
    for (result, source) in fetched {
        match result {
            Err(error) => view.sources.push(ReadSource {
                url: source.url,
                name: None,
                description: None,
                error: Some(error.to_string()),
            }),
            Ok(catalog) => {
                for entry in catalog.templates {
                    let standing = match workspace.standing(&space, &source.url, &entry)? {
                        aktujo_app::Standing::New => StandingView::New,
                        aktujo_app::Standing::Taken => StandingView::Taken,
                        aktujo_app::Standing::Update => StandingView::Update,
                    };
                    view.templates.push(TemplateListing {
                        source: source.url.clone(),
                        official: source.is_default() && entry.author == aktujo_app::OFFICIAL_AUTHOR,
                        kinds: entry
                            .kinds
                            .into_iter()
                            .map(|k| ListedKindView { id: k.id, name: k.name, names: k.names })
                            .collect(),
                        id: entry.id,
                        author: entry.author,
                        license: entry.license,
                        name: entry.name,
                        names: entry.names,
                        description: entry.description,
                        descriptions: entry.descriptions,
                        tags: entry.tags,
                        standing,
                    });
                }
                view.sources.push(ReadSource {
                    url: source.url,
                    name: Some(catalog.source.name),
                    description: Some(catalog.source.description),
                    error: None,
                });
            }
        }
    }
    Ok(Json(view))
}

/// A template of a source that is on, read now and checked.
async fn fetched(shared: &Shared, source: &str, id: &str) -> Result<aktujo_app::Template, ApiError> {
    let url = aktujo_app::source_url(source)?;
    if !shared.lock().sources()?.iter().any(|s| s.url == url && s.on) {
        return Err(not_found("such source switched on"));
    }
    let id = id.to_owned();
    let template = tokio::task::spawn_blocking(move || {
        let catalog = aktujo_app::fetch_catalog(&url)?;
        let entry = catalog
            .templates
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| AppError::NotFound(format!("no template {id} in {url}")))?;
        aktujo_app::fetch_template(&url, entry)
    })
    .await
    .map_err(blocking_failed)??;
    Ok(template)
}

fn origin(choice: &TemplateChoice) -> Result<aktujo_app::TemplateOrigin, ApiError> {
    Ok(aktujo_app::TemplateOrigin {
        source: Some(aktujo_app::source_url(&choice.source)?),
        id: Some(choice.template.clone()),
    })
}

/// What taking a template over would do in the space, kind by kind – new, the same, replaced, merged with a person's
/// changes (and where both changed an item), or gone from the template. Nothing is written.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/catalog/plan", request_body = TemplateChoice,
    params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = PlanView), (status = 404, body = Problem), (status = 502, body = Problem)))]
async fn plan_template(
    State(shared): State<Shared>,
    Path(s): Path<String>,
    Json(choice): Json<TemplateChoice>,
) -> Answer<PlanView> {
    let space = space(&s)?;
    let template = fetched(&shared, &choice.source, &choice.template).await?;
    let plan = shared.lock().plan(&space, origin(&choice)?, &template)?;
    Ok(Json(PlanView {
        source: choice.source,
        template: choice.template,
        kinds: plan.kinds.into_iter().map(PlannedView::from).collect(),
    }))
}

/// Takes a template over as chosen, kind by kind – the first time or as an update. The template is read again and
/// planned anew, so what is taken is what the source has now.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/catalog/take", request_body = TemplateChoice,
    params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = Imported), (status = 403, body = Problem), (status = 404, body = Problem), (status = 502, body = Problem)))]
async fn take_template(
    State(shared): State<Shared>,
    Path(s): Path<String>,
    Json(choice): Json<TemplateChoice>,
) -> Answer<Imported> {
    let space = space(&s)?;
    let template = fetched(&shared, &choice.source, &choice.template).await?;
    let choices = choice.choices.iter().map(|(id, c)| (id.clone(), (*c).into())).collect();
    let changed = {
        let mut workspace = shared.lock();
        let plan = workspace.plan(&space, origin(&choice)?, &template)?;
        workspace.apply(&space, &plan, &choices)?
    };
    shared.changed();
    Ok(Json(Imported { changed }))
}

/// Finds documents: with words, the best hits first; without, the newest first.
#[utoipa::path(post, path = "/api/v1/search", request_body = SearchRequest,
    responses((status = 200, body = Vec<DocumentSummary>), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn search(State(shared): State<Shared>, Json(request): Json<SearchRequest>) -> Answer<Vec<DocumentSummary>> {
    let query = request.query().ok_or_else(|| not_found("such space"))?;
    let workspace = shared.lock();
    let mut found = Vec::new();
    for hit in workspace.search(&query)? {
        let document = workspace.document(&hit.space, &hit.document)?;
        let reading = workspace.read_state(&hit.space, &hit.document).unwrap_or(ReadState::Done);
        found.push(DocumentSummary::of_hit(hit, document, reading));
    }
    Ok(Json(found))
}

/// The file name of an upload.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
struct FileName {
    /// The name the file came with.
    name: String,
}

fn media_type(headers: &HeaderMap) -> &str {
    headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream")
}

/// Adds a file as a new document – the bytes as the body, its media type as `Content-Type`. The same bytes twice
/// in a space are one document.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/documents", params(("space" = String, Path, description = "the space"), FileName),
    request_body(content = Vec<u8>, content_type = "application/octet-stream", description = "the file"),
    responses((status = 201, body = AddedView), (status = 200, body = AddedView, description = "the same bytes were there already"),
        (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn add_document(
    State(shared): State<Shared>,
    Path(id): Path<String>,
    Query(file): Query<FileName>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<(StatusCode, Json<AddedView>), ApiError> {
    let new_file = NewFile { name: &file.name, media_type: media_type(&headers), bytes: &bytes };
    let added = shared.lock().add_document(&space(&id)?, new_file)?;
    shared.wake();
    let status = if added.new { StatusCode::CREATED } else { StatusCode::OK };
    Ok((status, Json(AddedView { document: added.document.to_string(), new: added.new })))
}

/// A document with everything about it.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/documents/{document}",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document")),
    responses((status = 200, body = DocumentView), (status = 404, body = Problem)))]
async fn document(
    State(shared): State<Shared>,
    Language(language): Language,
    Path((s, d)): Path<(String, String)>,
) -> Answer<DocumentView> {
    let (space, id) = ids(&s, &d)?;
    document_view(&shared.lock(), &shared, &language, &space, &id)
}

/// Whether to download a file rather than show it.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
struct Download {
    /// `true` to download it.
    #[serde(default)]
    download: bool,
}

/// Media types shown in the browser; every other file is downloaded.
const SHOWN: [&str; 5] = ["application/pdf", "image/png", "image/jpeg", "image/webp", "image/gif"];

/// The file of a version, with its media type.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/documents/{document}/versions/{number}/file",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"),
        ("number" = usize, Path, description = "the version, the first 1"), Download),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>), (status = 404, body = Problem)))]
async fn original(
    State(shared): State<Shared>,
    Path((s, d, number)): Path<(String, String, usize)>,
    Query(download): Query<Download>,
) -> Result<Response, ApiError> {
    let (space, id) = ids(&s, &d)?;
    let workspace = shared.lock();
    let document = workspace.document(&space, &id)?;
    let version =
        number.checked_sub(1).and_then(|i| document.versions.get(i)).ok_or_else(|| not_found("such version"))?;
    let original = version.original.clone();
    let bytes = workspace.original(&original.hash)?;
    drop(workspace);
    let shown = !download.download && SHOWN.contains(&original.media_type.as_str());
    let media_type = if SHOWN.contains(&original.media_type.as_str()) || original.media_type.starts_with("text/plain") {
        original.media_type.clone()
    } else {
        "application/octet-stream".to_owned()
    };
    // The name as RFC 5987 gives it, so that any character survives.
    let name: String = original
        .name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let disposition = format!("{}; filename*=UTF-8''{name}", if shown { "inline" } else { "attachment" });
    Ok((
        [
            (header::CONTENT_TYPE, media_type),
            (header::CONTENT_DISPOSITION, disposition),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'".to_owned(),
            ),
            (header::CACHE_CONTROL, "private, no-cache".to_owned()),
        ],
        bytes,
    )
        .into_response())
}

/// The text read from the newest version, page by page.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/documents/{document}/text",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document")),
    responses((status = 200, body = Vec<PageText>), (status = 404, body = Problem, description = "no document, or not read yet")))]
async fn text(State(shared): State<Shared>, Path((s, d)): Path<(String, String)>) -> Answer<Vec<PageText>> {
    let (space, id) = ids(&s, &d)?;
    let extraction = shared.lock().read_text(&space, &id)?.ok_or_else(|| not_found("text read yet"))?;
    Ok(Json(
        extraction
            .pages
            .iter()
            .map(|page| PageText {
                method: serde_json::to_value(page.method)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                text: page.text(),
            })
            .collect(),
    ))
}

/// Adds a file as the newest version of a document – the bytes as the body, its media type as `Content-Type`.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/documents/{document}/versions",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"), FileName),
    request_body(content = Vec<u8>, content_type = "application/octet-stream", description = "the file"),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn add_version(
    State(shared): State<Shared>,
    Language(language): Language,
    Path((s, d)): Path<(String, String)>,
    Query(file): Query<FileName>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Answer<DocumentView> {
    let (space, id) = ids(&s, &d)?;
    let new_file = NewFile { name: &file.name, media_type: media_type(&headers), bytes: &bytes };
    let mut workspace = shared.lock();
    workspace.add_version(&space, &id, new_file)?;
    shared.wake();
    document_view(&workspace, &shared, &language, &space, &id)
}

/// Runs a command on a document and answers with the document as it is then.
fn change(
    shared: &Shared,
    language: &str,
    (s, d): (String, String),
    command: impl FnOnce(&mut Workspace, &SpaceId, &DocumentId) -> Result<(), AppError>,
) -> Answer<DocumentView> {
    let (space, id) = ids(&s, &d)?;
    let mut workspace = shared.lock();
    command(&mut workspace, &space, &id)?;
    shared.changed();
    document_view(&workspace, shared, language, &space, &id)
}

/// Gives a document a new title.
#[utoipa::path(put, path = "/api/v1/spaces/{space}/documents/{document}/title", request_body = TitleChange,
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn set_title(
    State(shared): State<Shared>,
    Language(language): Language,
    Path(path): Path<(String, String)>,
    Json(change_to): Json<TitleChange>,
) -> Answer<DocumentView> {
    change(&shared, &language, path, |w, s, d| w.set_title(s, d, &change_to.title))
}

/// Confirms the kind of a document – or that it has none.
#[utoipa::path(put, path = "/api/v1/spaces/{space}/documents/{document}/kind", request_body = KindChange,
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn set_kind(
    State(shared): State<Shared>,
    Language(language): Language,
    Path(path): Path<(String, String)>,
    Json(change_to): Json<KindChange>,
) -> Answer<DocumentView> {
    change(&shared, &language, path, |w, s, d| w.set_kind(s, d, change_to.kind.as_deref()))
}

/// Confirms the value of a field.
#[utoipa::path(put, path = "/api/v1/spaces/{space}/documents/{document}/fields/{key}", request_body = FieldChange,
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"),
        ("key" = String, Path, description = "the field")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn set_field(
    State(shared): State<Shared>,
    Language(language): Language,
    Path((s, d, key)): Path<(String, String, String)>,
    Json(change_to): Json<FieldChange>,
) -> Answer<DocumentView> {
    change(&shared, &language, (s, d), |w, s, d| w.set_field(s, d, &key, change_to.value.into()))
}

/// Empties a field: its confirmed value and its suggestion.
#[utoipa::path(delete, path = "/api/v1/spaces/{space}/documents/{document}/fields/{key}",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"),
        ("key" = String, Path, description = "the field")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn clear_field(
    State(shared): State<Shared>,
    Language(language): Language,
    Path((s, d, key)): Path<(String, String, String)>,
) -> Answer<DocumentView> {
    change(&shared, &language, (s, d), |w, s, d| w.clear_field(s, d, &key))
}

/// Adds a label to a document.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/documents/{document}/labels", request_body = LabelChange,
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn add_label(
    State(shared): State<Shared>,
    Language(language): Language,
    Path(path): Path<(String, String)>,
    Json(change_to): Json<LabelChange>,
) -> Answer<DocumentView> {
    change(&shared, &language, path, |w, s, d| w.add_label(s, d, &change_to.label))
}

/// Takes a label off a document.
#[utoipa::path(delete, path = "/api/v1/spaces/{space}/documents/{document}/labels/{label}",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"),
        ("label" = String, Path, description = "the label")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn remove_label(
    State(shared): State<Shared>,
    Language(language): Language,
    Path((s, d, label)): Path<(String, String, String)>,
) -> Answer<DocumentView> {
    change(&shared, &language, (s, d), |w, s, d| w.remove_label(s, d, &label))
}

/// Puts a document in the bin – nothing is deleted.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/documents/{document}/trash",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn trash(
    State(shared): State<Shared>,
    Language(language): Language,
    Path(path): Path<(String, String)>,
) -> Answer<DocumentView> {
    change(&shared, &language, path, |w, s, d| w.trash(s, d))
}

/// Takes a document out of the bin.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/documents/{document}/restore",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem)))]
async fn restore(
    State(shared): State<Shared>,
    Language(language): Language,
    Path(path): Path<(String, String)>,
) -> Answer<DocumentView> {
    let answer = change(&shared, &language, path, |w, s, d| w.restore(s, d));
    shared.wake();
    answer
}

/// Gives this device a name people read – the other members of a space see it.
#[utoipa::path(put, path = "/api/v1/device", request_body = DeviceName,
    responses((status = 204), (status = 422, body = Problem)))]
async fn set_device_name(State(shared): State<Shared>, Json(new): Json<DeviceName>) -> Result<StatusCode, ApiError> {
    shared.lock().set_device_name(&new.name)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Starts synchronising a space through a relay; answers with its recovery code, which is shown only now.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/sync/start", request_body = StartSync,
    params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = RecoveryCodeView), (status = 404, body = Problem), (status = 502, body = Problem)))]
async fn start_sync(
    State(shared): State<Shared>,
    Path(id): Path<String>,
    Json(start): Json<StartSync>,
) -> Answer<RecoveryCodeView> {
    let space = space(&id)?;
    let code = shared.blocking(move |w| w.start_sync(&space, &start.relay, None)).await?;
    Ok(Json(RecoveryCodeView { recovery_code: code }))
}

/// Synchronises a space now.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/sync", params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = SyncReportView), (status = 404, body = Problem), (status = 502, body = Problem)))]
async fn sync(State(shared): State<Shared>, Path(id): Path<String>) -> Answer<SyncReportView> {
    let space = space(&id)?;
    Ok(Json(shared.blocking(move |w| w.sync(&space)).await?.into()))
}

/// An invitation for another device – the person's own or someone else's – with what it may do, good for a day.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/invitations", request_body = NewInvitation,
    params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = InvitationView), (status = 404, body = Problem), (status = 502, body = Problem)))]
async fn invite(
    State(shared): State<Shared>,
    Path(id): Path<String>,
    Json(new): Json<NewInvitation>,
) -> Answer<InvitationView> {
    let space = space(&id)?;
    let invitation = shared.lock().invite(&space, new.rights.into())?;
    Ok(Json(InvitationView { invitation }))
}

/// Asks to join a space with an invitation; its documents arrive once a device that may manage it added this one.
#[utoipa::path(post, path = "/api/v1/join", request_body = Join,
    responses((status = 200, body = SpaceView), (status = 422, body = Problem), (status = 502, body = Problem)))]
async fn join(State(shared): State<Shared>, Json(request): Json<Join>) -> Answer<SpaceView> {
    let space = shared
        .blocking(move |w| {
            let id = w.join(&request.invitation, None)?;
            w.spaces()?.into_iter().find(|s| s.id == id).ok_or_else(|| AppError::NotFound("the space".into()))
        })
        .await?;
    Ok(Json(space.into()))
}

/// Brings a space back to this device with its recovery code, when no other device of it is left.
#[utoipa::path(post, path = "/api/v1/recover", request_body = Recover,
    responses((status = 200, body = SpaceView), (status = 404, body = Problem), (status = 502, body = Problem)))]
async fn recover(State(shared): State<Shared>, Json(request): Json<Recover>) -> Answer<SpaceView> {
    let id = space(request.space.trim())?;
    let space = shared
        .blocking(move |w| {
            w.recover(&request.relay, &id, &request.code, None)?;
            w.spaces()?.into_iter().find(|s| s.id == id).ok_or_else(|| AppError::NotFound("the space".into()))
        })
        .await?;
    Ok(Json(space.into()))
}

/// The members of a synchronised space, as its relay lists them.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/members", params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = Vec<MemberView>), (status = 404, body = Problem), (status = 502, body = Problem)))]
async fn members(State(shared): State<Shared>, Path(id): Path<String>) -> Answer<Vec<MemberView>> {
    let space = space(&id)?;
    let members = shared.blocking(move |w| w.members(&space)).await?;
    Ok(Json(members.into_iter().map(MemberView::from).collect()))
}

/// Removes a member by its identity; it gets nothing written after.
#[utoipa::path(delete, path = "/api/v1/spaces/{space}/members/{member}",
    params(("space" = String, Path, description = "the space"), ("member" = String, Path, description = "the member's identity")),
    responses((status = 204), (status = 404, body = Problem), (status = 502, body = Problem)))]
async fn remove_member(
    State(shared): State<Shared>,
    Path((id, member)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let space = space(&id)?;
    shared.blocking(move |w| w.remove_member(&space, &member)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Whether a `Host` names this device.
fn local_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().map(|h| format!("[{h}]")).unwrap_or_default(),
        None => host.split(':').next().unwrap_or_default().to_owned(),
    };
    matches!(name.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

/// Answers only requests addressed to this device, and changes only for pages of its own.
async fn guard(request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
    if !local_host(&host) {
        return ApiError(StatusCode::MISDIRECTED_REQUEST, "this server answers on this device only".into())
            .into_response();
    }
    let changes = !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok());
    let foreign = origin.is_some_and(|o| o != format!("http://{host}"))
        || site.is_some_and(|s| s == "cross-site" || s == "same-site");
    if changes && foreign {
        return ApiError(StatusCode::FORBIDDEN, "changes are taken from this device's own pages only".into())
            .into_response();
    }
    next.run(request).await
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

/// The router: the API under `/api/v1`, and the web app around it, with what runs in the background.
pub fn router(workspace: Arc<Mutex<Workspace>>, background: Background) -> Router {
    let shared = Shared { workspace, background, titles: Arc::new(Reader::text_layer()) };
    let document = "/spaces/{space}/documents/{document}";
    let api = Router::new()
        .route("/openapi.json", get(openapi_json))
        .route("/status", get(status))
        .route("/spaces", get(spaces).post(found_space))
        .route("/spaces/{space}", put(rename_space))
        .route("/spaces/{space}/labels", get(labels))
        .route("/spaces/{space}/kinds", get(kinds).post(add_kind))
        .route("/spaces/{space}/kind-import", post(import_kinds))
        .route("/sources", get(sources).post(add_source).put(switch_source).delete(remove_source))
        .route("/spaces/{space}/catalog", get(catalog))
        .route("/spaces/{space}/catalog/plan", post(plan_template))
        .route("/spaces/{space}/catalog/take", post(take_template))
        .route("/spaces/{space}/kinds/{kind}", put(define_kind).delete(remove_kind))
        .route("/search", post(search))
        .route("/spaces/{space}/documents", post(add_document))
        .route(document, get(self::document))
        .route(&format!("{document}/versions"), post(add_version))
        .route(&format!("{document}/versions/{{number}}/file"), get(original))
        .route(&format!("{document}/text"), get(text))
        .route(&format!("{document}/title"), put(set_title))
        .route(&format!("{document}/kind"), put(set_kind))
        .route(&format!("{document}/fields/{{key}}"), put(set_field).delete(clear_field))
        .route(&format!("{document}/labels"), post(add_label))
        .route(&format!("{document}/labels/{{label}}"), axum::routing::delete(remove_label))
        .route(&format!("{document}/trash"), post(trash))
        .route(&format!("{document}/restore"), post(restore))
        .route("/device", put(set_device_name))
        .route("/spaces/{space}/sync/start", post(start_sync))
        .route("/spaces/{space}/sync", post(sync))
        .route("/spaces/{space}/invitations", post(invite))
        .route("/spaces/{space}/members", get(members))
        .route("/spaces/{space}/members/{member}", axum::routing::delete(remove_member))
        .route("/join", post(join))
        .route("/recover", post(recover))
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "no such route of the API".into()) })
        .layer(DefaultBodyLimit::max(UPLOAD_MAX));
    Router::new().nest("/api/v1", api).fallback(web_app).layer(middleware::from_fn(guard)).with_state(shared)
}

/// Serves a workspace on the device itself, at `port`, until interrupted; `ready` is told the address.
pub async fn serve(
    workspace: Arc<Mutex<Workspace>>,
    background: Background,
    port: u16,
    ready: impl FnOnce(SocketAddr),
) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    ready(listener.local_addr()?);
    axum::serve(listener, router(workspace, background))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}

#[cfg(test)]
mod tests;
