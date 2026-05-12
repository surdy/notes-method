use axum::{extract::FromRequestParts, http::request::Parts};

/// Allowed origins for config write operations.
/// In desktop mode, Tauri webview sends `tauri://localhost`.
/// In dev/web mode, localhost origins are allowed.
const ALLOWED_WRITE_ORIGINS: &[&str] =
    &["tauri://localhost", "http://localhost", "http://127.0.0.1"];

#[derive(Debug)]
pub struct WriteGuard;

impl<S: Send + Sync> FromRequestParts<S> for WriteGuard {
    type Rejection = (axum::http::StatusCode, axum::Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // If no Origin header, allow (same-origin requests / curl / CLI)
        let Some(origin) = parts.headers.get("origin").and_then(|v| v.to_str().ok()) else {
            return Ok(WriteGuard);
        };

        if ALLOWED_WRITE_ORIGINS
            .iter()
            .any(|allowed| origin.starts_with(allowed))
        {
            return Ok(WriteGuard);
        }

        Err((
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "origin_not_allowed",
                "message": format!("Origin '{origin}' is not allowed for write operations")
            })),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    async fn extract_guard(origin: Option<&str>) -> Result<WriteGuard, axum::http::StatusCode> {
        let mut builder = Request::builder().uri("/test");
        if let Some(o) = origin {
            builder = builder.header("origin", o);
        }
        let request = builder.body(()).unwrap();
        let (mut parts, _) = request.into_parts();
        WriteGuard::from_request_parts(&mut parts, &())
            .await
            .map_err(|(status, _)| status)
    }

    #[tokio::test]
    async fn allows_no_origin() {
        assert!(extract_guard(None).await.is_ok());
    }

    #[tokio::test]
    async fn allows_tauri_origin() {
        assert!(extract_guard(Some("tauri://localhost")).await.is_ok());
    }

    #[tokio::test]
    async fn allows_localhost_origin() {
        assert!(extract_guard(Some("http://localhost:5173")).await.is_ok());
    }

    #[tokio::test]
    async fn allows_127_origin() {
        assert!(extract_guard(Some("http://127.0.0.1:27183")).await.is_ok());
    }

    #[tokio::test]
    async fn rejects_foreign_origin() {
        let result = extract_guard(Some("https://evil.example.com")).await;
        assert_eq!(result.unwrap_err(), axum::http::StatusCode::FORBIDDEN);
    }
}
