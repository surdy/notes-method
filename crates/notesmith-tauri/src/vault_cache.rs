//! Pure per-server vault cache for the multi-server "New Window" menu.
//!
//! The menu must render **instantly** from cached data — it can never block on
//! a per-server network round-trip (ADR 0017, Phase B). This module is the
//! pure state: for each connection (`server_id`) it remembers the last-known
//! vault list, when it was last successfully fetched, and the outcome of the
//! most recent refresh. The async refresher (Phase B.3) feeds it
//! [`RefreshOutcome`]s; the menu builder (Phase B.4) reads snapshots and asks
//! [`ServerVaults::display_status`] how to render each group.
//!
//! Crucially, a *failed* refresh never discards the previously-known vaults —
//! an offline or unauthorized server still shows its last-known list (greyed
//! out by the menu), rather than collapsing to an empty group.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// How a server's cached vault list should be treated when rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultListStatus {
    /// Successfully fetched within the freshness window.
    Fresh,
    /// Previously fetched, but the data is older than the TTL (or the most
    /// recent refresh failed) — show the cached list, visually de-emphasised.
    Stale,
    /// The server rejected the request (401/403) — credentials need attention.
    AuthError,
    /// The server could not be reached (timeout / connection error).
    Unreachable,
}

/// The outcome of a single attempt to refresh one server's vault list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// The server returned its vault list.
    Loaded(Vec<String>),
    /// The server rejected the request (401/403).
    AuthError,
    /// The server could not be reached.
    Unreachable,
}

/// One server's cached vault list plus the freshness metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVaults {
    /// Last-known vault names. Preserved across failed refreshes.
    pub vaults: Vec<String>,
    /// When the vault list was last *successfully* fetched, if ever.
    pub last_seen: Option<SystemTime>,
    /// The result of the most recent refresh attempt.
    pub status: VaultListStatus,
}

impl ServerVaults {
    fn empty(status: VaultListStatus) -> Self {
        Self {
            vaults: Vec::new(),
            last_seen: None,
            status,
        }
    }

    /// How this entry should be rendered *now*, given a freshness `ttl`.
    ///
    /// A successfully-fetched entry decays from [`Fresh`] to [`Stale`] once it
    /// is older than `ttl`. A fetch that failed keeps its sticky
    /// [`AuthError`]/[`Unreachable`] status until the next success.
    ///
    /// [`Fresh`]: VaultListStatus::Fresh
    /// [`Stale`]: VaultListStatus::Stale
    /// [`AuthError`]: VaultListStatus::AuthError
    /// [`Unreachable`]: VaultListStatus::Unreachable
    pub fn display_status(&self, now: SystemTime, ttl: Duration) -> VaultListStatus {
        match self.status {
            VaultListStatus::AuthError => VaultListStatus::AuthError,
            VaultListStatus::Unreachable => VaultListStatus::Unreachable,
            VaultListStatus::Fresh | VaultListStatus::Stale => match self.last_seen {
                Some(last_seen) => match now.duration_since(last_seen) {
                    Ok(age) if age <= ttl => VaultListStatus::Fresh,
                    _ => VaultListStatus::Stale,
                },
                None => VaultListStatus::Stale,
            },
        }
    }
}

/// Per-server vault lists, keyed by `server_id`.
#[derive(Debug, Default)]
pub struct VaultCache {
    by_server: HashMap<String, ServerVaults>,
}

impl VaultCache {
    /// A clone of the cached entry for `server_id`, if one exists.
    pub fn get(&self, server_id: &str) -> Option<ServerVaults> {
        self.by_server.get(server_id).cloned()
    }

    /// Number of servers with a cache entry.
    pub fn len(&self) -> usize {
        self.by_server.len()
    }

    /// True when no server has a cache entry.
    pub fn is_empty(&self) -> bool {
        self.by_server.is_empty()
    }

    /// Fold a refresh outcome for `server_id` into the cache at time `now`.
    ///
    /// - [`RefreshOutcome::Loaded`] replaces the vault list, stamps `last_seen`,
    ///   and marks the entry [`Fresh`].
    /// - [`RefreshOutcome::AuthError`]/[`Unreachable`] **preserve** the existing
    ///   vault list and `last_seen`, only flipping the status — so the menu can
    ///   still show the last-known vaults greyed out.
    ///
    /// [`Fresh`]: VaultListStatus::Fresh
    /// [`Unreachable`]: RefreshOutcome::Unreachable
    pub fn merge_refresh(&mut self, server_id: &str, outcome: RefreshOutcome, now: SystemTime) {
        match outcome {
            RefreshOutcome::Loaded(vaults) => {
                let entry = self
                    .by_server
                    .entry(server_id.to_string())
                    .or_insert_with(|| ServerVaults::empty(VaultListStatus::Fresh));
                entry.vaults = vaults;
                entry.last_seen = Some(now);
                entry.status = VaultListStatus::Fresh;
            }
            RefreshOutcome::AuthError => {
                self.set_failed_status(server_id, VaultListStatus::AuthError);
            }
            RefreshOutcome::Unreachable => {
                self.set_failed_status(server_id, VaultListStatus::Unreachable);
            }
        }
    }

    /// Drop cache entries for servers no longer in `live_ids` (e.g. a server was
    /// removed). The local entry should always be passed in `live_ids`.
    pub fn retain_servers(&mut self, live_ids: &[String]) {
        self.by_server
            .retain(|id, _| live_ids.iter().any(|live| live == id));
    }

    fn set_failed_status(&mut self, server_id: &str, status: VaultListStatus) {
        let entry = self
            .by_server
            .entry(server_id.to_string())
            .or_insert_with(|| ServerVaults::empty(status));
        entry.status = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn loaded_refresh_replaces_vaults_and_marks_fresh() {
        let mut cache = VaultCache::default();
        cache.merge_refresh(
            "home",
            RefreshOutcome::Loaded(vec!["personal".into(), "work".into()]),
            t(100),
        );

        let entry = cache.get("home").expect("entry exists");
        assert_eq!(
            entry.vaults,
            vec!["personal".to_string(), "work".to_string()]
        );
        assert_eq!(entry.last_seen, Some(t(100)));
        assert_eq!(entry.status, VaultListStatus::Fresh);
    }

    #[test]
    fn failed_refresh_preserves_last_known_vaults_and_timestamp() {
        let mut cache = VaultCache::default();
        cache.merge_refresh(
            "home",
            RefreshOutcome::Loaded(vec!["personal".into()]),
            t(100),
        );

        // Server goes offline: keep the last-known list + last_seen, flip status.
        cache.merge_refresh("home", RefreshOutcome::Unreachable, t(200));
        let entry = cache.get("home").expect("entry exists");
        assert_eq!(entry.vaults, vec!["personal".to_string()]);
        assert_eq!(entry.last_seen, Some(t(100)));
        assert_eq!(entry.status, VaultListStatus::Unreachable);

        // Auth fails next: same preservation, AuthError status.
        cache.merge_refresh("home", RefreshOutcome::AuthError, t(300));
        let entry = cache.get("home").expect("entry exists");
        assert_eq!(entry.vaults, vec!["personal".to_string()]);
        assert_eq!(entry.last_seen, Some(t(100)));
        assert_eq!(entry.status, VaultListStatus::AuthError);
    }

    #[test]
    fn failure_with_no_prior_entry_creates_an_empty_failed_entry() {
        let mut cache = VaultCache::default();
        cache.merge_refresh("home", RefreshOutcome::Unreachable, t(100));
        let entry = cache.get("home").expect("entry exists");
        assert!(entry.vaults.is_empty());
        assert_eq!(entry.last_seen, None);
        assert_eq!(entry.status, VaultListStatus::Unreachable);
    }

    #[test]
    fn display_status_decays_from_fresh_to_stale_past_ttl() {
        let mut cache = VaultCache::default();
        cache.merge_refresh(
            "home",
            RefreshOutcome::Loaded(vec!["personal".into()]),
            t(100),
        );
        let entry = cache.get("home").unwrap();
        let ttl = Duration::from_secs(60);

        // Within the TTL → Fresh.
        assert_eq!(entry.display_status(t(150), ttl), VaultListStatus::Fresh);
        // Past the TTL → Stale (but vaults are still there).
        assert_eq!(entry.display_status(t(200), ttl), VaultListStatus::Stale);
    }

    #[test]
    fn display_status_keeps_sticky_failure_states() {
        let entry = ServerVaults {
            vaults: vec!["personal".into()],
            last_seen: Some(t(100)),
            status: VaultListStatus::AuthError,
        };
        let ttl = Duration::from_secs(60);
        assert_eq!(
            entry.display_status(t(120), ttl),
            VaultListStatus::AuthError
        );
        assert_eq!(
            entry.display_status(t(10_000), ttl),
            VaultListStatus::AuthError
        );
    }

    #[test]
    fn retain_servers_drops_removed_entries() {
        let mut cache = VaultCache::default();
        cache.merge_refresh("home", RefreshOutcome::Loaded(vec!["a".into()]), t(100));
        cache.merge_refresh("work", RefreshOutcome::Loaded(vec!["b".into()]), t(100));
        assert_eq!(cache.len(), 2);

        cache.retain_servers(&["home".to_string()]);
        assert_eq!(cache.len(), 1);
        assert!(cache.get("home").is_some());
        assert!(cache.get("work").is_none());
    }
}
