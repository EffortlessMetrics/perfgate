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

#[cfg(test)]
mod tests {
    const INDEX_HTML: &str = include_str!("../../assets/index.html");

    #[test]
    fn dashboard_exposes_audit_events_panel() {
        assert!(INDEX_HTML.contains("<h2>Audit Events</h2>"));
        assert!(INDEX_HTML.contains("id=\"auditBody\""));
        assert!(INDEX_HTML.contains("function loadAuditEvents()"));
        assert!(INDEX_HTML.contains("'/api/v1/audit?limit='"));
        assert!(INDEX_HTML.contains("fetch(url, { headers: authHeaders() })"));
        assert!(INDEX_HTML.contains("id=\"auditAction\""));
        assert!(INDEX_HTML.contains("id=\"auditResourceType\""));
        assert!(INDEX_HTML.contains("id=\"auditActor\""));
        assert!(INDEX_HTML.contains("<option value=\"decision\">Decision</option>"));
    }

    #[test]
    fn dashboard_exposes_decision_ledger_panel() {
        assert!(INDEX_HTML.contains("<h2>Performance Decisions</h2>"));
        assert!(INDEX_HTML.contains("id=\"decisionsBody\""));
        assert!(INDEX_HTML.contains("function loadDecisions()"));
        assert!(INDEX_HTML.contains("'/decisions?' + params.toString()"));
        assert!(INDEX_HTML.contains("id=\"decisionStatus\""));
        assert!(INDEX_HTML.contains("id=\"decisionVerdict\""));
        assert!(INDEX_HTML.contains("id=\"decisionReviewRequired\""));
        assert!(INDEX_HTML.contains("id=\"decisionAccepted\""));
        assert!(INDEX_HTML.contains("id=\"decisionScenario\""));
        assert!(INDEX_HTML.contains("id=\"decisionRule\""));
        assert!(INDEX_HTML.contains("params.set('rule', rule)"));
        assert!(INDEX_HTML.contains("id=\"decisionPagination\""));
        assert!(INDEX_HTML.contains("loadDecisions();"));
    }
}
