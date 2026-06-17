//! Persistence for the set of Notesmith daemons ("servers") the desktop app
//! can connect to.
//!
//! This is the **system of record** shared by the Settings → Connection UI
//! (#180) and the status-bar switcher (#181). It mirrors `windows_persist.rs`:
//! pure logic plus atomic file I/O, fully unit-testable without a running
//! Tauri runtime.
//!
//! `This Mac` (the local daemon) is **implicit** — it is never stored as an
//! entry. `active_id == None` means the local daemon is active; `Some(id)`
//! selects a stored remote server.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Current schema version. Bump and add a migration when fields change.
pub const SCHEMA_VERSION: u32 = 1;

/// Reserved id that represents the implicit local daemon ("This Mac").
/// Never assigned to a stored entry; selecting it means "go local".
pub const LOCAL_ID: &str = "local";

/// One saved remote server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEntry {
    /// Stable, unique identifier (slug derived from the name).
    pub id: String,
    /// Human-readable label shown in the UI.
    pub name: String,
    /// Daemon base URL, e.g. `http://100.92.14.7:27183`.
    pub url: String,
    /// Optional access token sent as a bearer credential. May be plaintext on
    /// disk (parity with the previous env-var flow); never returned to the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl ServerEntry {
    /// Project to a token-less [`ServerView`] for the UI.
    pub fn view(&self) -> ServerView {
        ServerView {
            id: self.id.clone(),
            name: self.name.clone(),
            url: self.url.clone(),
            has_token: self.token.is_some(),
        }
    }
}

/// On-disk representation of the saved-server set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServersFile {
    pub version: u32,
    /// `None` → the implicit local daemon is active. `Some(id)` → a stored
    /// server is active.
    #[serde(default)]
    pub active_id: Option<String>,
    #[serde(default)]
    pub servers: Vec<ServerEntry>,
}

impl Default for ServersFile {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            active_id: None,
            servers: Vec::new(),
        }
    }
}

/// Fields accepted when adding or updating a server.
#[derive(Debug, Clone, Default)]
pub struct ServerInput {
    pub name: String,
    pub url: String,
    pub token: Option<String>,
}

/// The currently-active connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Active<'a> {
    Local,
    Remote(&'a ServerEntry),
}

/// A fully-resolved daemon connection target for daemon-directed IPC: where to
/// send the request, whether it is remote, and the bearer token (if any) to
/// attach. Unlike [`ServerView`] this *does* carry the token because it is used
/// internally to make authenticated requests, never returned to the frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTarget {
    pub url: String,
    pub remote: bool,
    pub token: Option<String>,
}

/// Token-less projection of a [`ServerEntry`] for the UI. Tokens are never
/// returned to the frontend; `has_token` indicates whether one is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServerView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub has_token: bool,
}

/// Connection list returned to the UI: the active id (`"local"` for the local
/// daemon) plus a token-less view of every stored server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionList {
    pub active_id: String,
    pub servers: Vec<ServerView>,
}

/// Result of probing a candidate daemon URL for reachability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionTestResult {
    pub reachable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ConnectionTestResult {
    /// An unreachable result carrying a human-readable reason.
    pub fn unreachable(error: impl Into<String>) -> Self {
        Self {
            reachable: false,
            latency_ms: None,
            vault_count: None,
            error: Some(error.into()),
        }
    }
}

/// Best-effort vault count from a `/api/app/vaults` response body. The daemon
/// returns a bare JSON array; an object with a `vaults` array is also accepted
/// for forward-compatibility. Any other shape yields `None`.
pub fn parse_vault_count(body: &[u8]) -> Option<u32> {
    match serde_json::from_slice::<serde_json::Value>(body).ok()? {
        serde_json::Value::Array(items) => u32::try_from(items.len()).ok(),
        serde_json::Value::Object(map) => match map.get("vaults") {
            Some(serde_json::Value::Array(items)) => u32::try_from(items.len()).ok(),
            _ => None,
        },
        _ => None,
    }
}

/// Errors from mutating the server set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStoreError {
    EmptyName,
    InvalidUrl,
    NotFound,
    ReservedId,
}

impl std::fmt::Display for ServerStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            ServerStoreError::EmptyName => "server name must not be empty",
            ServerStoreError::InvalidUrl => "server URL must be a valid http(s) URL",
            ServerStoreError::NotFound => "no server with that id",
            ServerStoreError::ReservedId => "\"local\" is reserved for the built-in local daemon",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for ServerStoreError {}

/// Return the path to `servers.json` inside the given config directory.
pub fn servers_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join("servers.json")
}

/// Load and parse `servers.json`. Missing, empty, corrupt, or
/// wrong-version files yield the default (local-only) set so the app always
/// starts in a usable state. A warning is logged for corrupt content; this
/// never panics or propagates a parse error.
pub fn load(path: &Path) -> ServersFile {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return ServersFile::default(),
        Err(error) => {
            tracing::warn!(?path, %error, "failed to read servers.json; using local-only default");
            return ServersFile::default();
        }
    };
    if bytes.is_empty() {
        return ServersFile::default();
    }
    match serde_json::from_slice::<ServersFile>(&bytes) {
        Ok(file) if file.version == SCHEMA_VERSION => file.normalized(),
        Ok(_) => {
            // Unknown/older version: no migrations yet — start fresh rather
            // than risk acting on an incompatible shape.
            tracing::warn!(
                ?path,
                "servers.json version mismatch; using local-only default"
            );
            ServersFile::default()
        }
        Err(error) => {
            tracing::warn!(?path, %error, "failed to parse servers.json; using local-only default");
            ServersFile::default()
        }
    }
}

/// Atomically write `servers.json` via a sibling `.tmp` file and rename.
/// Creates the parent directory if needed.
pub fn save(path: &Path, file: &ServersFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = match path.extension() {
        Some(ext) => {
            let mut new_ext = ext.to_os_string();
            new_ext.push(".tmp");
            path.with_extension(new_ext)
        }
        None => path.with_extension("tmp"),
    };
    let bytes = serde_json::to_vec_pretty(file)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

impl ServersFile {
    /// Drop an `active_id` that no longer refers to a stored server (treat it
    /// as local). Keeps the in-memory state self-consistent after a load.
    fn normalized(mut self) -> Self {
        if let Some(id) = &self.active_id
            && (id == LOCAL_ID || !self.servers.iter().any(|s| &s.id == id))
        {
            self.active_id = None;
        }
        self
    }

    /// Resolve the active connection.
    pub fn active(&self) -> Active<'_> {
        match &self.active_id {
            None => Active::Local,
            Some(id) => self
                .servers
                .iter()
                .find(|s| &s.id == id)
                .map(Active::Remote)
                .unwrap_or(Active::Local),
        }
    }

    /// True when the local daemon is the active connection.
    pub fn is_local_active(&self) -> bool {
        matches!(self.active(), Active::Local)
    }

    /// Resolve the active connection's target daemon URL and whether it is
    /// remote. `local_url` is the URL to use when the local daemon is active.
    ///
    /// This is the pure decision the desktop uses to retarget the webview on a
    /// connection switch or on relaunch: local → (`local_url`, false),
    /// remote → (`entry.url`, true).
    pub fn active_target(&self, local_url: &str) -> (String, bool) {
        match self.active() {
            Active::Local => (local_url.to_string(), false),
            Active::Remote(entry) => (entry.url.clone(), true),
        }
    }

    /// Resolve a specific connection's target by id, independent of which
    /// connection is currently active. [`LOCAL_ID`] (or an id that no longer
    /// refers to a stored server) yields the local target; a stored id yields
    /// that entry's URL flagged remote.
    ///
    /// This is the per-window analogue of [`active_target`](Self::active_target):
    /// each desktop window builds its frontend URL from *its* connection
    /// (ADR 0017), not the single global active one.
    pub fn target_for(&self, id: &str, local_url: &str) -> (String, bool) {
        let target = self.resolve_target(id, local_url);
        (target.url, target.remote)
    }

    /// Like [`target_for`](Self::target_for) but also carries the bearer token
    /// to send with daemon-targeted IPC for that connection. Used to thread a
    /// window's per-server credential onto its requests (ADR 0017).
    pub fn resolve_target(&self, id: &str, local_url: &str) -> ConnectionTarget {
        match self.get(id) {
            Some(entry) if id != LOCAL_ID => ConnectionTarget {
                url: entry.url.clone(),
                remote: true,
                token: entry.token.clone(),
            },
            _ => ConnectionTarget {
                url: local_url.to_string(),
                remote: false,
                token: None,
            },
        }
    }

    pub fn get(&self, id: &str) -> Option<&ServerEntry> {
        self.servers.iter().find(|s| s.id == id)
    }

    /// Token-less connection list for the UI. `active_id` is `"local"` when the
    /// local daemon is active.
    pub fn connection_list(&self) -> ConnectionList {
        ConnectionList {
            active_id: self
                .active_id
                .clone()
                .unwrap_or_else(|| LOCAL_ID.to_string()),
            servers: self.servers.iter().map(ServerEntry::view).collect(),
        }
    }

    /// Add a new server. Validates the input, assigns a unique id, and returns
    /// the new entry's id.
    pub fn add(&mut self, input: ServerInput) -> Result<String, ServerStoreError> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(ServerStoreError::EmptyName);
        }
        let url = normalize_url(&input.url)?;
        let id = self.unique_id(&name);
        self.servers.push(ServerEntry {
            id: id.clone(),
            name,
            url,
            token: clean_token(input.token),
        });
        Ok(id)
    }

    /// Update an existing server in place. Empty optional fields are ignored
    /// (name/url stay unchanged); `token: Some("")` clears the token.
    pub fn update(
        &mut self,
        id: &str,
        name: Option<String>,
        url: Option<String>,
        token: Option<String>,
    ) -> Result<(), ServerStoreError> {
        // Validate before mutating so a bad input can't leave a half update.
        let new_name = match name {
            Some(n) => {
                let n = n.trim().to_string();
                if n.is_empty() {
                    return Err(ServerStoreError::EmptyName);
                }
                Some(n)
            }
            None => None,
        };
        let new_url = match url {
            Some(u) => Some(normalize_url(&u)?),
            None => None,
        };
        let entry = self
            .servers
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(ServerStoreError::NotFound)?;
        if let Some(n) = new_name {
            entry.name = n;
        }
        if let Some(u) = new_url {
            entry.url = u;
        }
        if let Some(t) = token {
            entry.token = clean_token(Some(t));
        }
        Ok(())
    }

    /// Remove a server by id. If it was the active connection, fall back to
    /// local. Returns `NotFound` if no such server exists.
    pub fn remove(&mut self, id: &str) -> Result<(), ServerStoreError> {
        let before = self.servers.len();
        self.servers.retain(|s| s.id != id);
        if self.servers.len() == before {
            return Err(ServerStoreError::NotFound);
        }
        if self.active_id.as_deref() == Some(id) {
            self.active_id = None;
        }
        Ok(())
    }

    /// Select the active connection. `None`, `""`, or `"local"` mean the local
    /// daemon; any other id must match a stored server.
    pub fn set_active(&mut self, id: Option<&str>) -> Result<(), ServerStoreError> {
        match id.map(str::trim) {
            None | Some("") | Some(LOCAL_ID) => {
                self.active_id = None;
                Ok(())
            }
            Some(id) => {
                if self.servers.iter().any(|s| s.id == id) {
                    self.active_id = Some(id.to_string());
                    Ok(())
                } else {
                    Err(ServerStoreError::NotFound)
                }
            }
        }
    }

    /// Generate a stable, unique, non-reserved id from a display name.
    fn unique_id(&self, name: &str) -> String {
        let base = match slugify(name) {
            s if s.is_empty() || s == LOCAL_ID => format!("server-{s}"),
            s => s,
        };
        if !self.id_taken(&base) {
            return base;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}-{n}");
            if !self.id_taken(&candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    fn id_taken(&self, id: &str) -> bool {
        id == LOCAL_ID || self.servers.iter().any(|s| s.id == id)
    }
}

/// Trim and reject empty tokens (treat `""` as "no token").
fn clean_token(token: Option<String>) -> Option<String> {
    token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Validate and canonicalize a daemon URL: must be http/https with a host,
/// returned without a trailing slash.
fn normalize_url(raw: &str) -> Result<String, ServerStoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ServerStoreError::InvalidUrl);
    }
    let url = reqwest::Url::parse(trimmed).map_err(|_| ServerStoreError::InvalidUrl)?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(ServerStoreError::InvalidUrl),
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(ServerStoreError::InvalidUrl);
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

/// Lowercase ASCII slug: alphanumerics kept, runs of anything else collapsed
/// to single hyphens, leading/trailing hyphens trimmed.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_hyphen = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_hyphen = false;
        } else if !prev_hyphen {
            out.push('-');
            prev_hyphen = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn tmp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ns-servers-{nanos}-{counter}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn input(name: &str, url: &str) -> ServerInput {
        ServerInput {
            name: name.into(),
            url: url.into(),
            token: None,
        }
    }

    #[test]
    fn default_is_local_only() {
        let file = ServersFile::default();
        assert!(file.servers.is_empty());
        assert!(file.is_local_active());
        assert_eq!(file.active(), Active::Local);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tmp_dir();
        let file = load(&servers_file_path(&dir));
        assert_eq!(file, ServersFile::default());
    }

    #[test]
    fn load_corrupt_file_returns_default_without_panic() {
        let dir = tmp_dir();
        let path = servers_file_path(&dir);
        fs::write(&path, b"{ this is not valid json").unwrap();
        let file = load(&path);
        assert_eq!(file, ServersFile::default());
    }

    #[test]
    fn load_wrong_version_returns_default() {
        let dir = tmp_dir();
        let path = servers_file_path(&dir);
        fs::write(&path, br#"{"version":999,"servers":[]}"#).unwrap();
        assert_eq!(load(&path), ServersFile::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tmp_dir();
        let path = servers_file_path(&dir);
        let mut file = ServersFile::default();
        let id = file
            .add(input("home-server", "http://100.92.14.7:27183"))
            .unwrap();
        file.set_active(Some(&id)).unwrap();
        save(&path, &file).unwrap();

        let loaded = load(&path);
        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(loaded.active(), Active::Remote(loaded.get(&id).unwrap()));
        assert_eq!(loaded.get(&id).unwrap().url, "http://100.92.14.7:27183");
    }

    #[test]
    fn add_trims_name_and_strips_trailing_slash() {
        let mut file = ServersFile::default();
        let id = file.add(input("  Home  ", "http://host:27183/")).unwrap();
        let entry = file.get(&id).unwrap();
        assert_eq!(entry.name, "Home");
        assert_eq!(entry.url, "http://host:27183");
        assert_eq!(entry.id, "home");
    }

    #[test]
    fn add_rejects_empty_name() {
        let mut file = ServersFile::default();
        assert_eq!(
            file.add(input("   ", "http://host:27183")),
            Err(ServerStoreError::EmptyName)
        );
    }

    #[test]
    fn add_rejects_non_http_url() {
        let mut file = ServersFile::default();
        assert_eq!(
            file.add(input("bad", "ftp://host")),
            Err(ServerStoreError::InvalidUrl)
        );
        assert_eq!(
            file.add(input("bad", "not a url")),
            Err(ServerStoreError::InvalidUrl)
        );
        assert_eq!(
            file.add(input("bad", "")),
            Err(ServerStoreError::InvalidUrl)
        );
    }

    #[test]
    fn ids_are_unique_for_duplicate_names() {
        let mut file = ServersFile::default();
        let a = file.add(input("Home", "http://a:27183")).unwrap();
        let b = file.add(input("Home", "http://b:27183")).unwrap();
        let c = file.add(input("Home", "http://c:27183")).unwrap();
        assert_eq!(a, "home");
        assert_eq!(b, "home-2");
        assert_eq!(c, "home-3");
    }

    #[test]
    fn name_colliding_with_reserved_id_is_namespaced() {
        let mut file = ServersFile::default();
        let id = file.add(input("local", "http://host:27183")).unwrap();
        assert_ne!(id, LOCAL_ID);
        assert!(file.set_active(Some(&id)).is_ok());
        assert!(matches!(file.active(), Active::Remote(_)));
    }

    #[test]
    fn update_changes_fields_and_validates() {
        let mut file = ServersFile::default();
        let id = file.add(input("Home", "http://host:27183")).unwrap();
        file.update(
            &id,
            Some("Renamed".into()),
            Some("https://new:8443".into()),
            None,
        )
        .unwrap();
        let entry = file.get(&id).unwrap();
        assert_eq!(entry.name, "Renamed");
        assert_eq!(entry.url, "https://new:8443");

        assert_eq!(
            file.update(&id, None, Some("ftp://nope".into()), None),
            Err(ServerStoreError::InvalidUrl)
        );
        // The bad update did not mutate the entry.
        assert_eq!(file.get(&id).unwrap().url, "https://new:8443");
        assert_eq!(
            file.update("missing", Some("x".into()), None, None),
            Err(ServerStoreError::NotFound)
        );
    }

    #[test]
    fn update_token_set_and_clear() {
        let mut file = ServersFile::default();
        let id = file.add(input("Home", "http://host:27183")).unwrap();
        file.update(&id, None, None, Some("secret".into())).unwrap();
        assert_eq!(file.get(&id).unwrap().token.as_deref(), Some("secret"));
        // Empty string clears it.
        file.update(&id, None, None, Some("  ".into())).unwrap();
        assert_eq!(file.get(&id).unwrap().token, None);
    }

    #[test]
    fn remove_active_falls_back_to_local() {
        let mut file = ServersFile::default();
        let id = file.add(input("Home", "http://host:27183")).unwrap();
        file.set_active(Some(&id)).unwrap();
        assert!(!file.is_local_active());
        file.remove(&id).unwrap();
        assert!(file.is_local_active());
        assert_eq!(file.remove("missing"), Err(ServerStoreError::NotFound));
    }

    #[test]
    fn set_active_local_sentinels() {
        let mut file = ServersFile::default();
        let id = file.add(input("Home", "http://host:27183")).unwrap();
        file.set_active(Some(&id)).unwrap();
        for sentinel in [None, Some(""), Some(LOCAL_ID), Some("  ")] {
            file.set_active(sentinel).unwrap();
            assert!(file.is_local_active());
            file.set_active(Some(&id)).unwrap();
        }
        assert_eq!(
            file.set_active(Some("missing")),
            Err(ServerStoreError::NotFound)
        );
    }

    #[test]
    fn load_drops_dangling_active_id() {
        let dir = tmp_dir();
        let path = servers_file_path(&dir);
        fs::write(&path, br#"{"version":1,"active_id":"ghost","servers":[]}"#).unwrap();
        let file = load(&path);
        assert!(file.is_local_active());
    }

    #[test]
    fn token_is_not_serialized_when_absent() {
        let mut file = ServersFile::default();
        file.add(input("Home", "http://host:27183")).unwrap();
        let json = serde_json::to_string(&file).unwrap();
        assert!(
            !json.contains("token"),
            "absent token must be omitted: {json}"
        );
    }

    #[test]
    fn slugify_handles_punctuation_and_unicode() {
        assert_eq!(slugify("Home Server"), "home-server");
        assert_eq!(slugify("  work / vps  "), "work-vps");
        assert_eq!(slugify("a@@@b"), "a-b");
        assert_eq!(slugify("***"), "");
    }

    #[test]
    fn connection_list_marks_local_and_omits_tokens() {
        let mut file = ServersFile::default();
        let id = file
            .add(ServerInput {
                name: "Home".into(),
                url: "http://host:27183".into(),
                token: Some("secret".into()),
            })
            .unwrap();
        // Local active by default.
        let list = file.connection_list();
        assert_eq!(list.active_id, LOCAL_ID);
        assert_eq!(list.servers.len(), 1);
        assert!(list.servers[0].has_token);
        // The token value never appears in the serialized view.
        let json = serde_json::to_string(&list).unwrap();
        assert!(!json.contains("secret"), "token leaked into view: {json}");

        file.set_active(Some(&id)).unwrap();
        assert_eq!(file.connection_list().active_id, id);
    }

    #[test]
    fn parse_vault_count_handles_array_object_and_garbage() {
        assert_eq!(
            parse_vault_count(br#"[{"name":"a"},{"name":"b"}]"#),
            Some(2)
        );
        assert_eq!(parse_vault_count(br#"[]"#), Some(0));
        assert_eq!(parse_vault_count(br#"{"vaults":[1,2,3]}"#), Some(3));
        assert_eq!(parse_vault_count(br#"{"unexpected":true}"#), None);
        assert_eq!(parse_vault_count(b"not json"), None);
    }

    #[test]
    fn active_target_resolves_local_and_remote() {
        let mut file = ServersFile::default();
        // Local active by default → the supplied local URL, not remote.
        assert_eq!(
            file.active_target("http://127.0.0.1:27183"),
            ("http://127.0.0.1:27183".to_string(), false)
        );

        let id = file
            .add(input("Home", "https://notes.example.com"))
            .unwrap();
        file.set_active(Some(&id)).unwrap();
        // Remote active → the entry URL, flagged remote (local URL ignored).
        assert_eq!(
            file.active_target("http://127.0.0.1:27183"),
            ("https://notes.example.com".to_string(), true)
        );

        // Switching back to local restores the local target.
        file.set_active(Some(LOCAL_ID)).unwrap();
        assert_eq!(
            file.active_target("http://127.0.0.1:27183"),
            ("http://127.0.0.1:27183".to_string(), false)
        );
    }

    #[test]
    fn target_for_resolves_by_id_independent_of_active() {
        let mut file = ServersFile::default();
        let id = file
            .add(input("Home", "https://notes.example.com"))
            .unwrap();
        // Local remains the active connection throughout.
        assert!(file.is_local_active());

        // Explicit local id → local target, regardless of active.
        assert_eq!(
            file.target_for(LOCAL_ID, "http://127.0.0.1:27183"),
            ("http://127.0.0.1:27183".to_string(), false)
        );
        // A stored remote id → that entry's URL, flagged remote — even though
        // the active connection is still local.
        assert_eq!(
            file.target_for(&id, "http://127.0.0.1:27183"),
            ("https://notes.example.com".to_string(), true)
        );
        // An unknown id falls back to local (mirrors `normalized()`).
        assert_eq!(
            file.target_for("ghost", "http://127.0.0.1:27183"),
            ("http://127.0.0.1:27183".to_string(), false)
        );
    }

    #[test]
    fn resolve_target_carries_the_servers_token() {
        let mut file = ServersFile::default();
        let with_token = file
            .add(ServerInput {
                name: "Home".into(),
                url: "https://home.example".into(),
                token: Some("s3cret".into()),
            })
            .unwrap();
        let no_token = file.add(input("Office", "https://office.example")).unwrap();

        // Local target → no token, not remote.
        assert_eq!(
            file.resolve_target(LOCAL_ID, "http://127.0.0.1:27183"),
            ConnectionTarget {
                url: "http://127.0.0.1:27183".into(),
                remote: false,
                token: None,
            }
        );
        // Remote with a token → URL + remote + the bearer token.
        assert_eq!(
            file.resolve_target(&with_token, "http://127.0.0.1:27183"),
            ConnectionTarget {
                url: "https://home.example".into(),
                remote: true,
                token: Some("s3cret".into()),
            }
        );
        // Remote without a token → URL + remote, no token.
        assert_eq!(
            file.resolve_target(&no_token, "http://127.0.0.1:27183"),
            ConnectionTarget {
                url: "https://office.example".into(),
                remote: true,
                token: None,
            }
        );
        // Unknown id → local fallback, no token.
        assert_eq!(
            file.resolve_target("ghost", "http://127.0.0.1:27183").token,
            None
        );
    }
}
