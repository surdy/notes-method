use axum::{
    extract::FromRequestParts,
    http::{Uri, request::Parts},
};

/// Allowed origins for config write operations.
/// In desktop mode, Tauri webview sends `tauri://localhost`.
/// In embedded-frontend remote mode, the Tauri app protocol sends `notesmith-app://localhost`.
/// On Windows/Android, Tauri custom protocols use `http://notesmith-app.localhost`.
/// In dev/web mode, localhost origins are allowed.
const EMBEDDED_APP_HTTP_HOST: &str = "notesmith-app.localhost";

#[derive(Debug)]
pub struct WriteGuard;

impl<S: Send + Sync> FromRequestParts<S> for WriteGuard {
    type Rejection = (axum::http::StatusCode, axum::Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // If no Origin header, allow (same-origin requests / curl / CLI)
        let Some(origin) = parts.headers.get("origin").and_then(|v| v.to_str().ok()) else {
            return Ok(WriteGuard);
        };

        if is_allowed_write_origin(origin) {
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

fn is_allowed_write_origin(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    let Some(scheme) = uri.scheme_str() else {
        return false;
    };
    let Some(host) = uri.host() else {
        return false;
    };

    match scheme {
        "tauri" | "notesmith-app" => host.eq_ignore_ascii_case("localhost"),
        "http" => {
            host.eq_ignore_ascii_case("localhost")
                || host == "127.0.0.1"
                || host.eq_ignore_ascii_case(EMBEDDED_APP_HTTP_HOST)
        }
        _ => false,
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
    async fn allows_embedded_app_origin() {
        assert!(
            extract_guard(Some("notesmith-app://localhost"))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn allows_embedded_app_http_origin() {
        assert!(
            extract_guard(Some("http://notesmith-app.localhost"))
                .await
                .is_ok()
        );
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

    #[tokio::test]
    async fn rejects_localhost_prefix_spoofing() {
        let result = extract_guard(Some("http://localhost.evil.example")).await;
        assert_eq!(result.unwrap_err(), axum::http::StatusCode::FORBIDDEN);
    }
}
