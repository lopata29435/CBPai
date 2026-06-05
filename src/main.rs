use axum::http::{header, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "static".to_string());
    let index_html = tokio::fs::read_to_string(format!("{static_dir}/index.html"))
        .await
        .unwrap_or_else(|_| "<!doctype html><title>kitchen-app</title>".to_string());

    let base = static_dir.clone();
    let index = index_html.clone();
    let app = Router::new().route("/api/health", get(health)).fallback(
        move |uri: Uri| {
            let base = base.clone();
            let index = index.clone();
            async move { static_or_index(uri, base, index).await }
        },
    );

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("kitchen-app listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Отдаём реальный файл из static_dir; если его нет — index.html с кодом 200 (SPA-роутинг).
async fn static_or_index(uri: Uri, base: String, index: String) -> Response {
    let rel = uri.path().trim_start_matches('/');
    if !rel.is_empty() && !rel.contains("..") {
        let full = format!("{base}/{rel}");
        if let Ok(bytes) = tokio::fs::read(&full).await {
            return ([(header::CONTENT_TYPE, content_type(rel))], bytes).into_response();
        }
    }
    Html(index).into_response()
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "webmanifest" => "application/manifest+json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "kitchen-app",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
