//! Dashboard handlers for serving the Web UI.

use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

/// Handler for serving the dashboard index page.
pub async fn dashboard_index() -> Response {
    serve_asset("index.html")
}

/// Handler for serving static assets by path.
pub async fn static_asset(Path(path): Path<String>) -> Response {
    serve_asset(&path)
}

/// Helper for serving static assets.
pub fn serve_asset(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, mime.as_ref().parse().unwrap());
            (StatusCode::OK, headers, content.data.into_owned()).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}
