use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct TlsCheckQuery {
    domain: Option<String>,
}

pub async fn tls_check_handler(Query(params): Query<TlsCheckQuery>) -> Response {
    let domain = match params.domain {
        Some(d) if !d.is_empty() => d,
        _ => return (axum::http::StatusCode::FORBIDDEN, "").into_response(),
    };

    let valid = domain.ends_with(".nodns.shop") && domain != "nodns.shop";
    if valid {
        (axum::http::StatusCode::OK, "").into_response()
    } else {
        (axum::http::StatusCode::FORBIDDEN, "").into_response()
    }
}
