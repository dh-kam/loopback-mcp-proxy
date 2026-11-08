use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/dist"]
struct Assets;

pub fn serve(path: &str) -> Response {
    let path = path.trim_start_matches('/');
    if path.starts_with("api/")
        || path == "api"
        || path.starts_with("mcp")
        || path.starts_with("oauth/")
    {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"not found"}"#,
        )
            .into_response();
    }
    if path.is_empty() {
        return file_response("index.html");
    }
    if Assets::get(path).is_some() {
        return file_response(path);
    }
    file_response("index.html")
}

fn file_response(path: &str) -> Response {
    let Some(file) = Assets::get(path) else {
        return (StatusCode::NOT_FOUND, "ui is not built").into_response();
    };
    let mime = mime_of(path);
    ([(header::CONTENT_TYPE, mime)], file.data.into_owned()).into_response()
}

fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}
