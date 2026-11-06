// SPDX-License-Identifier: AGPL-3.0-or-later
//! Looking at the pages of a version and improving them – only in the full build (feature `scan`); `aktujo-minimal`
//! has no `aktujo-scan`, and these routes are not there.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

use aktujo_sdk::{DocumentId, Improve, VersionFile};

use crate::{ApiError, DocumentView, Language, Problem, Shared, document_view, ids, space};

/// The description of these routes, merged into the API's.
#[derive(Debug, OpenApi)]
#[openapi(paths(pages, page, corners, preview, improve, combine))]
pub(crate) struct ScanDoc;

/// These routes on `api`.
pub(crate) fn routes(api: Router<Shared>) -> Router<Shared> {
    let document = "/spaces/{space}/documents/{document}";
    api.route(&format!("{document}/versions/{{number}}/pages"), get(pages))
        .route(&format!("{document}/versions/{{number}}/pages/{{page}}"), get(page))
        .route(&format!("{document}/versions/{{number}}/pages/{{page}}/corners"), get(corners))
        .route(&format!("{document}/improve/preview"), post(preview))
        .route(&format!("{document}/improve"), post(improve))
        .route("/spaces/{space}/combine", post(combine))
}

/// What becomes of the colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColourView {
    /// As they are.
    #[default]
    Keep,
    /// Grey.
    Grey,
    /// Black and white.
    BlackWhite,
}

/// What is cut away from each side, as a share of the page (0 to 0.45 each).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CropView {
    /// From the left.
    #[serde(default)]
    pub left: f32,
    /// From the top.
    #[serde(default)]
    pub top: f32,
    /// From the right.
    #[serde(default)]
    pub right: f32,
    /// From the bottom.
    #[serde(default)]
    pub bottom: f32,
}

/// The four corners of a sheet on a page, as shares of its width and height (0 to 1, y from the top): top left, top
/// right, bottom right, bottom left – each `[x, y]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct QuadView {
    /// The corners.
    pub points: [[f32; 2]; 4],
}

/// The corners of the sheet found on a page, if one stands out.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CornersView {
    /// The corners; none if the page fills the picture already or no sheet stands out.
    pub corners: Option<QuadView>,
}

/// How pages are improved – in this order: chosen, the sheet cut out, turned, straightened, cropped, coloured.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ImproveView {
    /// The pages to keep, by their number from 0, in the order they are to be; none: all.
    #[serde(default)]
    pub pages: Option<Vec<usize>>,
    /// The corners of the sheet on each page, by its number from 0, before turning: the sheet is cut out along them
    /// and its perspective corrected; a page not named, or named with none, is left as it is – or its corners are
    /// found, with `find_corners`.
    #[serde(default)]
    pub corners: Vec<Option<QuadView>>,
    /// Find the sheet on each page that `corners` does not name, and cut it out – for photos of paper.
    #[serde(default)]
    pub find_corners: bool,
    /// Quarter turns clockwise for each page, by its number from 0 (0 to 3).
    #[serde(default)]
    pub turns: Vec<u8>,
    /// Straighten each page by the skew of its text.
    #[serde(default)]
    pub deskew: bool,
    /// What is cut away from each page.
    #[serde(default)]
    pub crop: Option<CropView>,
    /// What becomes of the colours.
    #[serde(default)]
    pub colour: ColourView,
}

impl From<ImproveView> for Improve {
    fn from(view: ImproveView) -> Self {
        serde_json::from_value(serde_json::to_value(view).expect("JSON")).expect("the same JSON")
    }
}

/// How many pages a version has.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PagesView {
    /// The number of pages.
    pub pages: usize,
}

/// How wide a picture of a page is to be.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct Width {
    /// At most this many pixels; 800 if not given.
    #[serde(default = "default_width")]
    pub width: u32,
}

/// Which page of the result, and how wide.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct PreviewQuery {
    /// The page of the result, from 0.
    #[serde(default)]
    pub page: usize,
    /// At most this many pixels; 800 if not given.
    #[serde(default = "default_width")]
    pub width: u32,
}

fn default_width() -> u32 {
    800
}

/// Documents to make one of.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Combine {
    /// The documents, in the order of their pages; the first gets the result as its newest version, the others go
    /// into the bin.
    pub documents: Vec<String>,
    /// How the pages are improved, counted across all documents.
    #[serde(default)]
    pub improve: ImproveView,
}

/// Runs picture work off the threads that serve requests.
async fn work<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, aktujo_sdk::AppError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(ApiError::from)
}

fn file_of(shared: &Shared, s: &str, d: &str, number: Option<usize>) -> Result<VersionFile, ApiError> {
    let (space, id) = ids(s, d)?;
    Ok(shared.lock().version_file(&space, &id, number)?)
}

fn png(bytes: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "private, max-age=3600")], bytes).into_response()
}

/// How many pages a version has.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/documents/{document}/versions/{number}/pages",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"),
        ("number" = usize, Path, description = "the version, the first 1")),
    responses((status = 200, body = PagesView), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn pages(
    State(shared): State<Shared>,
    Path((s, d, number)): Path<(String, String, usize)>,
) -> Result<Json<PagesView>, ApiError> {
    let file = file_of(&shared, &s, &d, Some(number))?;
    Ok(Json(PagesView { pages: work(move || aktujo_sdk::page_count(&file)).await? }))
}

/// A page of a version as a PNG – to look at it.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/documents/{document}/versions/{number}/pages/{page}",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"),
        ("number" = usize, Path, description = "the version, the first 1"), ("page" = usize, Path, description = "the page, the first 0"), Width),
    responses((status = 200, content_type = "image/png", body = Vec<u8>), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn page(
    State(shared): State<Shared>,
    Path((s, d, number, page)): Path<(String, String, usize, usize)>,
    Query(width): Query<Width>,
) -> Result<Response, ApiError> {
    let file = file_of(&shared, &s, &d, Some(number))?;
    Ok(png(work(move || aktujo_sdk::page_png(&file, page, width.width)).await?))
}

/// The corners of the sheet of paper on a page of a version – a photo, say – to show and move them.
#[utoipa::path(get, path = "/api/v1/spaces/{space}/documents/{document}/versions/{number}/pages/{page}/corners",
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"),
        ("number" = usize, Path, description = "the version, the first 1"), ("page" = usize, Path, description = "the page, the first 0")),
    responses((status = 200, body = CornersView), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn corners(
    State(shared): State<Shared>,
    Path((s, d, number, page)): Path<(String, String, usize, usize)>,
) -> Result<Json<CornersView>, ApiError> {
    let file = file_of(&shared, &s, &d, Some(number))?;
    let found = work(move || aktujo_sdk::find_corners(&file, page)).await?;
    Ok(Json(CornersView { corners: found.map(|quad| QuadView { points: quad.points }) }))
}

/// A page of what improving the newest version would make, as a PNG – nothing is written.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/documents/{document}/improve/preview", request_body = ImproveView,
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document"), PreviewQuery),
    responses((status = 200, content_type = "image/png", body = Vec<u8>), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn preview(
    State(shared): State<Shared>,
    Path((s, d)): Path<(String, String)>,
    Query(query): Query<PreviewQuery>,
    Json(view): Json<ImproveView>,
) -> Result<Response, ApiError> {
    let file = file_of(&shared, &s, &d, None)?;
    let improve: Improve = view.into();
    Ok(png(work(move || aktujo_sdk::preview_png(&file, &improve, query.page, query.width)).await?))
}

/// Improves the newest version: the result is the document's newest version; the earlier one stays.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/documents/{document}/improve", request_body = ImproveView,
    params(("space" = String, Path, description = "the space"), ("document" = String, Path, description = "the document")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn improve(
    State(shared): State<Shared>,
    Language(language): Language,
    Path((s, d)): Path<(String, String)>,
    Json(view): Json<ImproveView>,
) -> Result<Json<DocumentView>, ApiError> {
    let (space, id) = ids(&s, &d)?;
    let file = file_of(&shared, &s, &d, None)?;
    let name = file.name.clone();
    let improve: Improve = view.into();
    let written = work(move || aktujo_sdk::improved(&file, &improve)).await?;
    let mut workspace = shared.lock();
    workspace.add_improved(&space, &id, &name, &written)?;
    shared.wake();
    document_view(&workspace, &shared, &language, &space, &id)
}

/// Makes one document of several: their pages as a PDF, improved as asked, the newest version of the first; the
/// others go into the bin.
#[utoipa::path(post, path = "/api/v1/spaces/{space}/combine", request_body = Combine,
    params(("space" = String, Path, description = "the space")),
    responses((status = 200, body = DocumentView), (status = 403, body = Problem), (status = 404, body = Problem), (status = 422, body = Problem)))]
async fn combine(
    State(shared): State<Shared>,
    Language(language): Language,
    Path(s): Path<String>,
    Json(request): Json<Combine>,
) -> Result<Json<DocumentView>, ApiError> {
    let space = space(&s)?;
    let documents: Vec<DocumentId> = request
        .documents
        .iter()
        .map(|d| d.parse().map_err(|_| crate::not_found("such document")))
        .collect::<Result<_, _>>()?;
    if documents.len() < 2 {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "two documents or more are combined".into()));
    }
    let files = {
        let workspace = shared.lock();
        documents.iter().map(|id| workspace.version_file(&space, id, None)).collect::<Result<Vec<_>, _>>()?
    };
    let name = files[0].name.clone();
    let improve: Improve = request.improve.into();
    let written = work(move || aktujo_sdk::combined(&files, &improve)).await?;
    let mut workspace = shared.lock();
    workspace.add_combined(&space, &documents, &name, &written)?;
    shared.wake();
    shared.changed();
    document_view(&workspace, &shared, &language, &space, &documents[0])
}
