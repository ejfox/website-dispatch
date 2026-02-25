use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Html,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use tower_http::cors::CorsLayer;

const PORT: u16 = 6420;

static AUTH_PIN: OnceLock<String> = OnceLock::new();

pub struct AppState {
    pub pin: String,
}

pub fn get_pin() -> String {
    AUTH_PIN
        .get_or_init(|| {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            format!("{:06}", rng.gen_range(0..1_000_000))
        })
        .clone()
}

pub fn get_local_url() -> Option<String> {
    local_ip_address::local_ip()
        .ok()
        .map(|ip| format!("http://{}:{}", ip, PORT))
}

fn check_auth(headers: &HeaderMap, pin: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.strip_prefix("Bearer ").unwrap_or("") == pin)
        .unwrap_or(false)
}

pub async fn start_server() {
    let pin = get_pin();
    let state = Arc::new(AppState { pin });

    let app = Router::new()
        .route("/", get(serve_ui))
        .route("/api/auth", post(api_auth))
        .route("/api/files", get(api_list_files))
        .route("/api/files/{slug}/publish", post(api_publish))
        .route("/api/files/{slug}/schedule", post(api_schedule))
        .route("/api/files/{slug}/schedule", delete(api_cancel_schedule))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", PORT)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Companion server failed to bind port {}: {}", PORT, e);
            return;
        }
    };

    eprintln!("Companion UI running on port {}", PORT);
    if let Some(url) = get_local_url() {
        eprintln!("Companion URL: {}", url);
    }
    eprintln!("PIN: {}", get_pin());

    let _ = axum::serve(listener, app).await;
}

async fn serve_ui() -> Html<&'static str> {
    Html(include_str!("../companion-ui.html"))
}

#[derive(Deserialize)]
struct AuthRequest {
    pin: String,
}

#[derive(Serialize)]
struct AuthResponse {
    token: String,
}

async fn api_auth(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    if body.pin == state.pin {
        Ok(Json(AuthResponse {
            token: state.pin.clone(),
        }))
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[derive(Serialize)]
struct CompanionFile {
    path: String,
    filename: String,
    slug: String,
    title: Option<String>,
    word_count: usize,
    is_safe: bool,
    warnings: Vec<String>,
    published_url: Option<String>,
    published_date: Option<u64>,
    publish_at: Option<String>,
    unlisted: bool,
    has_password: bool,
    modified: u64,
    created: u64,
}

async fn api_list_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<CompanionFile>>, StatusCode> {
    if !check_auth(&headers, &state.pin) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let files =
        crate::vault::get_recent_files(200).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let companion_files: Vec<CompanionFile> = files
        .into_iter()
        .map(|f| CompanionFile {
            slug: f.filename.trim_end_matches(".md").to_string(),
            path: f.path,
            filename: f.filename,
            title: f.title,
            word_count: f.word_count,
            is_safe: f.is_safe,
            warnings: f.warnings,
            published_url: f.published_url,
            published_date: f.published_date,
            publish_at: f.publish_at,
            unlisted: f.unlisted,
            has_password: f.password.is_some(),
            modified: f.modified,
            created: f.created,
        })
        .collect();

    Ok(Json(companion_files))
}

async fn api_publish(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !check_auth(&headers, &state.pin) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Find the file by slug
    let files =
        crate::vault::get_recent_files(500).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file = files
        .iter()
        .find(|f| f.filename.trim_end_matches(".md") == slug)
        .ok_or(StatusCode::NOT_FOUND)?;

    match crate::publish::publish_file(&file.path, &slug, None) {
        Ok(url) => Ok(Json(serde_json::json!({ "url": url }))),
        Err(e) => Ok(Json(serde_json::json!({ "error": e }))),
    }
}

#[derive(Deserialize)]
struct ScheduleRequest {
    publish_at: String,
}

async fn api_schedule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<ScheduleRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !check_auth(&headers, &state.pin) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let files =
        crate::vault::get_recent_files(500).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file = files
        .iter()
        .find(|f| f.filename.trim_end_matches(".md") == slug)
        .ok_or(StatusCode::NOT_FOUND)?;

    crate::vault::set_frontmatter_field(&file.path, "publish_at", &body.publish_at)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn api_cancel_schedule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !check_auth(&headers, &state.pin) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let files =
        crate::vault::get_recent_files(500).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file = files
        .iter()
        .find(|f| f.filename.trim_end_matches(".md") == slug)
        .ok_or(StatusCode::NOT_FOUND)?;

    crate::vault::remove_frontmatter_field(&file.path, "publish_at")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_pin_format() {
        let pin = get_pin();
        assert_eq!(pin.len(), 6, "PIN should be 6 digits: {}", pin);
        assert!(
            pin.chars().all(|c| c.is_ascii_digit()),
            "PIN should be all digits: {}",
            pin
        );
    }

    #[test]
    fn test_get_pin_stable() {
        let pin1 = get_pin();
        let pin2 = get_pin();
        assert_eq!(pin1, pin2, "PIN should be stable across calls");
    }

    #[test]
    fn test_get_local_url() {
        let url = get_local_url();
        // May be None in CI or environments without network
        if let Some(ref u) = url {
            assert!(u.starts_with("http://"), "URL should be http: {}", u);
            assert!(u.contains(":6420"), "URL should contain port: {}", u);
        }
    }

    #[test]
    fn test_check_auth() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer 123456".parse().unwrap());
        assert!(check_auth(&headers, "123456"));
        assert!(!check_auth(&headers, "654321"));

        let empty_headers = HeaderMap::new();
        assert!(!check_auth(&empty_headers, "123456"));
    }

    #[test]
    fn test_companion_file_serialization() {
        let file = CompanionFile {
            path: "/test/path.md".into(),
            filename: "path.md".into(),
            slug: "path".into(),
            title: Some("Test Title".into()),
            word_count: 100,
            is_safe: true,
            warnings: vec![],
            published_url: None,
            published_date: None,
            publish_at: Some("2026-03-01T09:00:00Z".into()),
            unlisted: false,
            has_password: false,
            modified: 1000000,
            created: 900000,
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("\"slug\":\"path\""));
        assert!(json.contains("\"publish_at\":\"2026-03-01T09:00:00Z\""));
    }
}
