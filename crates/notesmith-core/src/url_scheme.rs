//! Parser for `notesmith://` deep-link URLs.
//!
//! Routes:
//! - `notesmith://app/open/{vault}/{path..}` → open a note
//! - `notesmith://app/daily/{vault}` → today's daily note
//! - `notesmith://app/search/{vault}?q={query}` → full-text search
//! - `notesmith://app/new/{vault}?template={name}&folder={path}` → create from template
//! - `notesmith://app/inbox/{vault}?text={text}` → quick capture
//! - `notesmith://app/task/{vault}/{path..}?line_hash={h}&status={s}` → toggle task
//! - `notesmith://app/command/{name}?args…` → built-in command
//! - `notesmith://user/{action}?params…` → user-defined action

use std::collections::HashMap;

/// A parsed `notesmith://` URL.
#[derive(Debug, Clone, PartialEq)]
pub enum NotesmithUrl {
    /// `notesmith://app/open/{vault}/{path..}`
    Open { vault: String, path: String },
    /// `notesmith://app/daily/{vault}`
    Daily { vault: String },
    /// `notesmith://app/search/{vault}?q={query}`
    Search { vault: String, query: String },
    /// `notesmith://app/new/{vault}?template={name}&folder={path}`
    New {
        vault: String,
        template: Option<String>,
        folder: Option<String>,
    },
    /// `notesmith://app/inbox/{vault}?text={text}`
    Inbox { vault: String, text: String },
    /// `notesmith://app/task/{vault}/{path..}?line_hash={hash}&status={status}`
    Task {
        vault: String,
        path: String,
        line_hash: String,
        status: String,
    },
    /// `notesmith://app/command/{command_name}?args…`
    Command {
        command_name: String,
        args: HashMap<String, String>,
    },
    /// `notesmith://user/{action_name}?params…`
    UserAction {
        action_name: String,
        params: HashMap<String, String>,
    },
}

/// Errors produced when parsing a `notesmith://` URL.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum UrlParseError {
    #[error("invalid scheme: expected notesmith://")]
    InvalidScheme,
    #[error("invalid namespace: expected app/ or user/")]
    InvalidNamespace,
    #[error("unknown app route: {0}")]
    UnknownRoute(String),
    #[error("missing required parameter: {0}")]
    MissingParameter(String),
    #[error("malformed URL: {0}")]
    Malformed(String),
}

/// Parse a `notesmith://` URL string into a [`NotesmithUrl`].
pub fn parse_notesmith_url(url: &str) -> Result<NotesmithUrl, UrlParseError> {
    let rest = url
        .strip_prefix("notesmith://")
        .ok_or(UrlParseError::InvalidScheme)?;

    // Split off the query string
    let (path_part, query_string) = match rest.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (rest, None),
    };

    let params = parse_query_params(query_string.unwrap_or(""));

    // Split the path into segments, filtering out empty ones
    let segments: Vec<&str> = path_part.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        return Err(UrlParseError::InvalidNamespace);
    }

    match segments[0] {
        "app" => parse_app_route(&segments[1..], &params),
        "user" => parse_user_route(&segments[1..], &params),
        _ => Err(UrlParseError::InvalidNamespace),
    }
}

fn parse_app_route(
    segments: &[&str],
    params: &HashMap<String, String>,
) -> Result<NotesmithUrl, UrlParseError> {
    if segments.is_empty() {
        return Err(UrlParseError::Malformed(
            "missing route after app/".to_string(),
        ));
    }

    match segments[0] {
        "open" => {
            if segments.len() < 3 {
                return Err(UrlParseError::MissingParameter(
                    "vault and path required for open".to_string(),
                ));
            }
            let vault = percent_decode(segments[1]);
            let path = segments[2..]
                .iter()
                .map(|s| percent_decode(s))
                .collect::<Vec<_>>()
                .join("/");
            Ok(NotesmithUrl::Open { vault, path })
        }
        "daily" => {
            if segments.len() < 2 {
                return Err(UrlParseError::MissingParameter(
                    "vault required for daily".to_string(),
                ));
            }
            let vault = percent_decode(segments[1]);
            Ok(NotesmithUrl::Daily { vault })
        }
        "search" => {
            if segments.len() < 2 {
                return Err(UrlParseError::MissingParameter(
                    "vault required for search".to_string(),
                ));
            }
            let vault = percent_decode(segments[1]);
            let query = params
                .get("q")
                .cloned()
                .ok_or_else(|| UrlParseError::MissingParameter("q".to_string()))?;
            Ok(NotesmithUrl::Search { vault, query })
        }
        "new" => {
            if segments.len() < 2 {
                return Err(UrlParseError::MissingParameter(
                    "vault required for new".to_string(),
                ));
            }
            let vault = percent_decode(segments[1]);
            Ok(NotesmithUrl::New {
                vault,
                template: params.get("template").cloned(),
                folder: params.get("folder").cloned(),
            })
        }
        "inbox" => {
            if segments.len() < 2 {
                return Err(UrlParseError::MissingParameter(
                    "vault required for inbox".to_string(),
                ));
            }
            let vault = percent_decode(segments[1]);
            let text = params
                .get("text")
                .cloned()
                .ok_or_else(|| UrlParseError::MissingParameter("text".to_string()))?;
            Ok(NotesmithUrl::Inbox { vault, text })
        }
        "task" => {
            if segments.len() < 3 {
                return Err(UrlParseError::MissingParameter(
                    "vault and path required for task".to_string(),
                ));
            }
            let vault = percent_decode(segments[1]);
            let path = segments[2..]
                .iter()
                .map(|s| percent_decode(s))
                .collect::<Vec<_>>()
                .join("/");
            let line_hash = params
                .get("line_hash")
                .cloned()
                .ok_or_else(|| UrlParseError::MissingParameter("line_hash".to_string()))?;
            let status = params
                .get("status")
                .cloned()
                .ok_or_else(|| UrlParseError::MissingParameter("status".to_string()))?;
            Ok(NotesmithUrl::Task {
                vault,
                path,
                line_hash,
                status,
            })
        }
        "command" => {
            if segments.len() < 2 {
                return Err(UrlParseError::MissingParameter(
                    "command name required".to_string(),
                ));
            }
            let command_name = percent_decode(segments[1]);
            Ok(NotesmithUrl::Command {
                command_name,
                args: params.clone(),
            })
        }
        other => Err(UrlParseError::UnknownRoute(other.to_string())),
    }
}

fn parse_user_route(
    segments: &[&str],
    params: &HashMap<String, String>,
) -> Result<NotesmithUrl, UrlParseError> {
    if segments.is_empty() {
        return Err(UrlParseError::MissingParameter(
            "action name required for user/".to_string(),
        ));
    }

    let action_name = percent_decode(segments[0]);
    Ok(NotesmithUrl::UserAction {
        action_name,
        params: params.clone(),
    })
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if query.is_empty() {
        return map;
    }
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            map.insert(percent_decode(key), percent_decode(value));
        }
    }
    map
}

/// Minimal percent-decoding for common URL-encoded characters.
fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(hi), Some(lo)) = (hi, lo) {
                if let (Some(h), Some(l)) = (hex_val(hi), hex_val(lo)) {
                    result.push((h << 4 | l) as char);
                    continue;
                }
            }
            // Malformed percent encoding — pass through
            result.push('%');
        } else if byte == b'+' {
            result.push(' ');
        } else {
            result.push(byte as char);
        }
    }
    result
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── app/open ────────────────────────────────────────────────────

    #[test]
    fn parse_open_simple() {
        let url = "notesmith://app/open/main/Inbox/hello.md";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::Open {
                vault: "main".into(),
                path: "Inbox/hello.md".into(),
            }
        );
    }

    #[test]
    fn parse_open_deep_path() {
        let url = "notesmith://app/open/work/Projects/2026/Q2/notes.md";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::Open {
                vault: "work".into(),
                path: "Projects/2026/Q2/notes.md".into(),
            }
        );
    }

    #[test]
    fn parse_open_percent_encoded_path() {
        let url = "notesmith://app/open/main/My%20Notes/hello%20world.md";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::Open {
                vault: "main".into(),
                path: "My Notes/hello world.md".into(),
            }
        );
    }

    #[test]
    fn parse_open_missing_path() {
        let url = "notesmith://app/open/main";
        assert_eq!(
            parse_notesmith_url(url).unwrap_err(),
            UrlParseError::MissingParameter("vault and path required for open".into()),
        );
    }

    // ── app/daily ───────────────────────────────────────────────────

    #[test]
    fn parse_daily() {
        let url = "notesmith://app/daily/main";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::Daily {
                vault: "main".into(),
            }
        );
    }

    #[test]
    fn parse_daily_missing_vault() {
        let url = "notesmith://app/daily";
        assert_eq!(
            parse_notesmith_url(url).unwrap_err(),
            UrlParseError::MissingParameter("vault required for daily".into()),
        );
    }

    // ── app/search ──────────────────────────────────────────────────

    #[test]
    fn parse_search() {
        let url = "notesmith://app/search/main?q=meeting+notes";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::Search {
                vault: "main".into(),
                query: "meeting notes".into(),
            }
        );
    }

    #[test]
    fn parse_search_missing_q() {
        let url = "notesmith://app/search/main";
        assert_eq!(
            parse_notesmith_url(url).unwrap_err(),
            UrlParseError::MissingParameter("q".into()),
        );
    }

    // ── app/new ─────────────────────────────────────────────────────

    #[test]
    fn parse_new_with_all_params() {
        let url = "notesmith://app/new/main?template=meeting&folder=Inbox";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::New {
                vault: "main".into(),
                template: Some("meeting".into()),
                folder: Some("Inbox".into()),
            }
        );
    }

    #[test]
    fn parse_new_without_optional_params() {
        let url = "notesmith://app/new/main";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::New {
                vault: "main".into(),
                template: None,
                folder: None,
            }
        );
    }

    // ── app/inbox ───────────────────────────────────────────────────

    #[test]
    fn parse_inbox() {
        let url = "notesmith://app/inbox/main?text=Remember+to+buy+milk";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::Inbox {
                vault: "main".into(),
                text: "Remember to buy milk".into(),
            }
        );
    }

    #[test]
    fn parse_inbox_missing_text() {
        let url = "notesmith://app/inbox/main";
        assert_eq!(
            parse_notesmith_url(url).unwrap_err(),
            UrlParseError::MissingParameter("text".into()),
        );
    }

    // ── app/task ────────────────────────────────────────────────────

    #[test]
    fn parse_task() {
        let url = "notesmith://app/task/main/Projects/todo.md?line_hash=abc123&status=done";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::Task {
                vault: "main".into(),
                path: "Projects/todo.md".into(),
                line_hash: "abc123".into(),
                status: "done".into(),
            }
        );
    }

    #[test]
    fn parse_task_missing_status() {
        let url = "notesmith://app/task/main/todo.md?line_hash=abc";
        assert_eq!(
            parse_notesmith_url(url).unwrap_err(),
            UrlParseError::MissingParameter("status".into()),
        );
    }

    // ── app/command ─────────────────────────────────────────────────

    #[test]
    fn parse_command() {
        let url = "notesmith://app/command/theme-toggle?mode=dark";
        let parsed = parse_notesmith_url(url).unwrap();
        match parsed {
            NotesmithUrl::Command { command_name, args } => {
                assert_eq!(command_name, "theme-toggle");
                assert_eq!(args.get("mode").unwrap(), "dark");
            }
            other => panic!("expected Command, got {other:?}"),
        }
    }

    // ── user/ ───────────────────────────────────────────────────────

    #[test]
    fn parse_user_action() {
        let url = "notesmith://user/standup?date=2026-05-10";
        let parsed = parse_notesmith_url(url).unwrap();
        match parsed {
            NotesmithUrl::UserAction {
                action_name,
                params,
            } => {
                assert_eq!(action_name, "standup");
                assert_eq!(params.get("date").unwrap(), "2026-05-10");
            }
            other => panic!("expected UserAction, got {other:?}"),
        }
    }

    #[test]
    fn parse_user_action_no_params() {
        let url = "notesmith://user/weekly-review";
        assert_eq!(
            parse_notesmith_url(url).unwrap(),
            NotesmithUrl::UserAction {
                action_name: "weekly-review".into(),
                params: HashMap::new(),
            }
        );
    }

    // ── error cases ─────────────────────────────────────────────────

    #[test]
    fn reject_wrong_scheme() {
        assert_eq!(
            parse_notesmith_url("https://example.com").unwrap_err(),
            UrlParseError::InvalidScheme,
        );
    }

    #[test]
    fn reject_invalid_namespace() {
        assert_eq!(
            parse_notesmith_url("notesmith://other/something").unwrap_err(),
            UrlParseError::InvalidNamespace,
        );
    }

    #[test]
    fn reject_unknown_app_route() {
        assert_eq!(
            parse_notesmith_url("notesmith://app/foobar/main").unwrap_err(),
            UrlParseError::UnknownRoute("foobar".into()),
        );
    }

    #[test]
    fn reject_empty_path() {
        assert_eq!(
            parse_notesmith_url("notesmith://").unwrap_err(),
            UrlParseError::InvalidNamespace,
        );
    }
}
