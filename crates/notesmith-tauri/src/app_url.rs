//! URL helpers for loading the SvelteKit frontend in Tauri windows.

use std::path::Path;

/// Custom protocol used when the desktop shell serves bundled frontend assets.
pub const APP_PROTOCOL: &str = "notesmith-app";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendMode {
    /// Load `/app/` from the daemon itself.
    Daemon,
    /// Load bundled frontend assets and point API calls at the daemon.
    Embedded,
}

pub fn app_window_url(daemon_base: &str, vault: Option<&str>, mode: FrontendMode) -> String {
    app_route_window_url(daemon_base, "/", vault, mode)
}

pub fn app_route_window_url(
    daemon_base: &str,
    route: &str,
    vault: Option<&str>,
    mode: FrontendMode,
) -> String {
    let daemon_base = daemon_base.trim_end_matches('/');
    let route = route.trim_start_matches('/');
    let route_path = if route.is_empty() {
        String::new()
    } else {
        format!("{route}/").trim_end_matches('/').to_string()
    };
    match mode {
        FrontendMode::Daemon => {
            let base = format!("{daemon_base}/app/{route_path}");
            append_query(base, vault, None)
        }
        FrontendMode::Embedded => {
            let base = format!("{APP_PROTOCOL}://localhost/app/{route_path}");
            append_query(base, vault, Some(daemon_base))
        }
    }
}

pub fn app_asset_path(request_path: &str) -> Option<String> {
    if request_path != "/app" && !request_path.starts_with("/app/") {
        return None;
    }
    let trimmed = request_path.strip_prefix("/app")?;
    let asset_path = match trimmed {
        "" | "/" => "index.html",
        rest => rest.strip_prefix('/').unwrap_or(rest),
    };
    if asset_path.is_empty() || asset_path.contains("..") || asset_path.starts_with('/') {
        return None;
    }
    Some(asset_path.to_string())
}

pub fn should_fallback_to_index(request_path: &str) -> bool {
    let Some(asset_path) = app_asset_path(request_path) else {
        return false;
    };
    asset_path == "index.html" || Path::new(&asset_path).extension().is_none()
}

fn append_query(base: String, vault: Option<&str>, api_base: Option<&str>) -> String {
    let mut params = Vec::new();
    if let Some(api_base) = api_base {
        params.push(("apiBase", api_base));
    }
    if let Some(vault) = vault {
        params.push(("vault", vault));
    }
    if params.is_empty() {
        return base;
    }
    let query = params
        .into_iter()
        .map(|(key, value)| format!("{key}={}", encode_query_value(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{query}")
}

/// Minimal URL-query-component encoder (percent-encodes characters outside
/// the unreserved set). Avoids a dependency on `urlencoding` for a few call sites.
pub fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let is_unreserved = byte.is_ascii_alphanumeric()
            || byte == b'-'
            || byte == b'_'
            || byte == b'.'
            || byte == b'~';
        if is_unreserved {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_mode_loads_frontend_from_daemon() {
        assert_eq!(
            app_window_url("http://127.0.0.1:27183", None, FrontendMode::Daemon),
            "http://127.0.0.1:27183/app/"
        );
        assert_eq!(
            app_window_url("http://127.0.0.1:27183/", Some("work"), FrontendMode::Daemon),
            "http://127.0.0.1:27183/app/?vault=work"
        );
    }

    #[test]
    fn embedded_mode_loads_local_app_and_carries_remote_api_base() {
        assert_eq!(
            app_window_url("http://100.64.0.10:27183", None, FrontendMode::Embedded),
            "notesmith-app://localhost/app/?apiBase=http%3A%2F%2F100.64.0.10%3A27183"
        );
        assert_eq!(
            app_window_url("http://100.64.0.10:27183/", Some("Work Vault"), FrontendMode::Embedded),
            "notesmith-app://localhost/app/?apiBase=http%3A%2F%2F100.64.0.10%3A27183&vault=Work%20Vault"
        );
    }

    #[test]
    fn route_urls_load_spa_routes() {
        assert_eq!(
            app_route_window_url(
                "http://127.0.0.1:27183",
                "/settings",
                None,
                FrontendMode::Daemon
            ),
            "http://127.0.0.1:27183/app/settings"
        );
        assert_eq!(
            app_route_window_url(
                "https://notesmith.example",
                "settings",
                Some("work"),
                FrontendMode::Embedded
            ),
            "notesmith-app://localhost/app/settings?apiBase=https%3A%2F%2Fnotesmith.example&vault=work"
        );
    }

    #[test]
    fn app_asset_path_maps_app_prefix_to_bundled_assets() {
        assert_eq!(app_asset_path("/app"), Some("index.html".to_string()));
        assert_eq!(app_asset_path("/app/"), Some("index.html".to_string()));
        assert_eq!(
            app_asset_path("/app/_app/immutable/entry/start.js"),
            Some("_app/immutable/entry/start.js".to_string())
        );
        assert_eq!(app_asset_path("/application"), None);
        assert_eq!(app_asset_path("/splash"), None);
    }

    #[test]
    fn only_spa_routes_fallback_to_index() {
        assert!(should_fallback_to_index("/app/settings"));
        assert!(should_fallback_to_index("/app/notes/today"));
        assert!(!should_fallback_to_index("/app/_app/immutable/chunk.js"));
        assert!(!should_fallback_to_index("/splash"));
    }
}
