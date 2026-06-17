//! Pure layout for the multi-server "New Window" / tray "Open" submenu
//! (ADR 0017, Phase B.4).
//!
//! The menu must render instantly from cached data (see [`crate::vault_cache`]),
//! so this module turns a snapshot — the local vault list plus a cached
//! [`ServerGroup`] per remote connection — into a flat list of [`MenuRow`]s. The
//! Tauri menu builder then maps each row to a concrete menu item. Keeping the
//! layout pure lets us unit-test grouping, ordering, status suffixes, and
//! enable/disable policy without a running app.
//!
//! Layout rules:
//! - **Local-only** (no remote servers configured): a flat list of local vault
//!   entries, then "Open Folder…", byte-identical to the pre-B.4 menu.
//! - **Multi-server**: a labeled "Local" group first, then one labeled group per
//!   remote connection. A group whose data is offline / unauthorized renders its
//!   last-known vaults greyed-out with a status suffix in the header; a group
//!   that has never refreshed shows a "Loading…" header.

use crate::vault_cache::VaultListStatus;
use crate::vault_menu::{encode_open_vault_id, encode_open_vault_id_for};

/// How one server group should render, derived from its cache entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupStatus {
    /// Successfully fetched within the freshness window.
    Fresh,
    /// Last-known data, past the freshness TTL — still openable.
    Stale,
    /// The server rejected the request (401/403).
    AuthError,
    /// The server could not be reached.
    Unreachable,
    /// No cache entry yet — the first refresh is still in flight.
    Loading,
}

impl GroupStatus {
    /// Map a cached [`VaultListStatus`] (or its absence) to a group status. A
    /// server with no cache entry yet is [`Loading`](GroupStatus::Loading).
    pub fn from_display(status: Option<VaultListStatus>) -> Self {
        match status {
            None => GroupStatus::Loading,
            Some(VaultListStatus::Fresh) => GroupStatus::Fresh,
            Some(VaultListStatus::Stale) => GroupStatus::Stale,
            Some(VaultListStatus::AuthError) => GroupStatus::AuthError,
            Some(VaultListStatus::Unreachable) => GroupStatus::Unreachable,
        }
    }

    /// Whether vault entries in this group can be opened. Fresh/Stale data is
    /// last-known-good and openable; auth/unreachable/loading groups are inert.
    fn entries_enabled(self) -> bool {
        matches!(self, GroupStatus::Fresh | GroupStatus::Stale)
    }

    /// Short suffix appended to the group header, or `None` for a healthy group.
    fn header_suffix(self) -> Option<&'static str> {
        match self {
            GroupStatus::Fresh | GroupStatus::Stale => None,
            GroupStatus::AuthError => Some("Sign in needed"),
            GroupStatus::Unreachable => Some("Offline"),
            GroupStatus::Loading => Some("Loading\u{2026}"),
        }
    }
}

/// One remote server group fed to the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerGroup {
    /// Stable connection id (used to build server-qualified menu ids).
    pub server_id: String,
    /// Human-readable label shown in the group header.
    pub name: String,
    /// Last-known vault names (may be empty, and may be stale on failure).
    pub vaults: Vec<String>,
    /// Derived render status.
    pub status: GroupStatus,
}

/// Whether a vault row belongs to the local daemon or a remote server, used to
/// pick a source icon (laptop vs. globe) in the rendered menu. `None` (in the
/// local-only flat menu) renders without an icon, preserving the legacy layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultSource {
    /// A vault on the local daemon.
    Local,
    /// A vault on a remote server.
    Remote,
}

/// A single rendered row of the submenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuRow {
    /// A non-interactive section header (rendered as a disabled item).
    Header(String),
    /// A selectable vault entry. `enabled == false` renders greyed-out but the
    /// row is still listed so the user can see what's there.
    Vault {
        /// Menu id encoding `(server_id, vault)` — feed to
        /// [`crate::vault_menu::decode_open_vault_target`].
        id: String,
        /// Display label (the vault name).
        label: String,
        /// Whether the entry can be opened.
        enabled: bool,
        /// Local vs. remote source for the row icon; `None` = no icon (legacy
        /// local-only flat menu).
        source: Option<VaultSource>,
    },
    /// A non-interactive informational row (e.g. "No vaults").
    Info(String),
    /// A visual separator.
    Separator,
    /// The trailing "Open Folder…" action.
    OpenFolder,
}

/// Build the ordered rows for the New Window / Open submenu from a snapshot.
///
/// `local_vaults` are the always-fresh local vault names; `remotes` are the
/// cached groups for each configured remote connection (local first is handled
/// here, callers pass only remotes).
pub fn build_new_window_rows(local_vaults: &[String], remotes: &[ServerGroup]) -> Vec<MenuRow> {
    let mut rows = Vec::new();

    // Local-only: keep the historical flat layout exactly.
    if remotes.is_empty() {
        for vault in local_vaults {
            rows.push(MenuRow::Vault {
                id: encode_open_vault_id(vault),
                label: vault.clone(),
                enabled: true,
                source: None,
            });
        }
        if !local_vaults.is_empty() {
            rows.push(MenuRow::Separator);
        }
        rows.push(MenuRow::OpenFolder);
        return rows;
    }

    // Multi-server: a labeled "Local" group, then one group per remote.
    rows.push(MenuRow::Header("Local".to_string()));
    if local_vaults.is_empty() {
        rows.push(MenuRow::Info("No vaults".to_string()));
    } else {
        for vault in local_vaults {
            rows.push(MenuRow::Vault {
                id: encode_open_vault_id(vault),
                label: vault.clone(),
                enabled: true,
                source: Some(VaultSource::Local),
            });
        }
    }

    for group in remotes {
        rows.push(MenuRow::Separator);
        rows.push(MenuRow::Header(group_header(group)));

        if group.vaults.is_empty() {
            // Only show an explicit "No vaults" row when the server actually
            // responded with an empty list; loading/failed groups already say so
            // in the header suffix.
            if matches!(group.status, GroupStatus::Fresh | GroupStatus::Stale) {
                rows.push(MenuRow::Info("No vaults".to_string()));
            }
            continue;
        }

        let enabled = group.status.entries_enabled();
        for vault in &group.vaults {
            rows.push(MenuRow::Vault {
                id: encode_open_vault_id_for(&group.server_id, vault),
                label: vault.clone(),
                enabled,
                source: Some(VaultSource::Remote),
            });
        }
    }

    rows.push(MenuRow::Separator);
    rows.push(MenuRow::OpenFolder);
    rows
}

/// `"<name>"` for a healthy group, `"<name> — <status>"` otherwise.
fn group_header(group: &ServerGroup) -> String {
    match group.status.header_suffix() {
        Some(suffix) => format!("{} \u{2014} {}", group.name, suffix),
        None => group.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(server_id: &str, name: &str, vaults: &[&str], status: GroupStatus) -> ServerGroup {
        ServerGroup {
            server_id: server_id.to_string(),
            name: name.to_string(),
            vaults: vaults.iter().map(|v| v.to_string()).collect(),
            status,
        }
    }

    #[test]
    fn local_only_layout_matches_legacy_flat_menu() {
        let rows = build_new_window_rows(&["personal".into(), "work".into()], &[]);
        assert_eq!(
            rows,
            vec![
                MenuRow::Vault {
                    id: encode_open_vault_id("personal"),
                    label: "personal".into(),
                    enabled: true,
                    source: None,
                },
                MenuRow::Vault {
                    id: encode_open_vault_id("work"),
                    label: "work".into(),
                    enabled: true,
                    source: None,
                },
                MenuRow::Separator,
                MenuRow::OpenFolder,
            ]
        );
    }

    #[test]
    fn local_only_with_no_vaults_is_just_open_folder() {
        let rows = build_new_window_rows(&[], &[]);
        assert_eq!(rows, vec![MenuRow::OpenFolder]);
    }

    #[test]
    fn multi_server_groups_local_first_with_qualified_remote_ids() {
        let remotes = vec![group(
            "memory",
            "memory",
            &["people", "tech"],
            GroupStatus::Fresh,
        )];
        let rows = build_new_window_rows(&["personal".into()], &remotes);
        assert_eq!(
            rows,
            vec![
                MenuRow::Header("Local".into()),
                MenuRow::Vault {
                    id: encode_open_vault_id("personal"),
                    label: "personal".into(),
                    enabled: true,
                    source: Some(VaultSource::Local),
                },
                MenuRow::Separator,
                MenuRow::Header("memory".into()),
                MenuRow::Vault {
                    id: encode_open_vault_id_for("memory", "people"),
                    label: "people".into(),
                    enabled: true,
                    source: Some(VaultSource::Remote),
                },
                MenuRow::Vault {
                    id: encode_open_vault_id_for("memory", "tech"),
                    label: "tech".into(),
                    enabled: true,
                    source: Some(VaultSource::Remote),
                },
                MenuRow::Separator,
                MenuRow::OpenFolder,
            ]
        );
    }

    #[test]
    fn unreachable_remote_lists_last_known_vaults_disabled() {
        let remotes = vec![group(
            "memory",
            "memory",
            &["people"],
            GroupStatus::Unreachable,
        )];
        let rows = build_new_window_rows(&[], &remotes);
        assert!(rows.contains(&MenuRow::Header("memory \u{2014} Offline".into())));
        assert!(rows.contains(&MenuRow::Vault {
            id: encode_open_vault_id_for("memory", "people"),
            label: "people".into(),
            enabled: false,
            source: Some(VaultSource::Remote),
        }));
    }

    #[test]
    fn auth_error_remote_is_flagged_and_disabled() {
        let remotes = vec![group(
            "memory",
            "memory",
            &["people"],
            GroupStatus::AuthError,
        )];
        let rows = build_new_window_rows(&[], &remotes);
        assert!(rows.contains(&MenuRow::Header("memory \u{2014} Sign in needed".into())));
        assert!(rows.contains(&MenuRow::Vault {
            id: encode_open_vault_id_for("memory", "people"),
            label: "people".into(),
            enabled: false,
            source: Some(VaultSource::Remote),
        }));
    }

    #[test]
    fn loading_remote_shows_only_a_loading_header() {
        let remotes = vec![group("memory", "memory", &[], GroupStatus::Loading)];
        let rows = build_new_window_rows(&["personal".into()], &remotes);
        // Header with the Loading suffix, but no vault/info rows for the group.
        assert!(rows.contains(&MenuRow::Header("memory \u{2014} Loading\u{2026}".into())));
        // The only rows after the remote header are the trailing separator +
        // Open Folder — the loading group contributes no Info/Vault rows.
        assert!(!rows.iter().any(|row| matches!(row, MenuRow::Info(_))));
    }

    #[test]
    fn fresh_remote_with_empty_list_shows_no_vaults_info() {
        let remotes = vec![group("memory", "memory", &[], GroupStatus::Fresh)];
        let rows = build_new_window_rows(&[], &remotes);
        assert!(rows.contains(&MenuRow::Header("memory".into())));
        assert!(rows.contains(&MenuRow::Info("No vaults".into())));
    }

    #[test]
    fn vault_rows_carry_source_local_then_remote() {
        // Local-only flat menu: no source (legacy, icon-less).
        let flat = build_new_window_rows(&["personal".into()], &[]);
        assert!(matches!(
            flat.first(),
            Some(MenuRow::Vault { source: None, .. })
        ));

        // Multi-server: local group rows are Local, remote group rows are Remote.
        let remotes = vec![group("memory", "memory", &["people"], GroupStatus::Fresh)];
        let rows = build_new_window_rows(&["personal".into()], &remotes);
        let sources: Vec<Option<VaultSource>> = rows
            .iter()
            .filter_map(|row| match row {
                MenuRow::Vault { source, .. } => Some(*source),
                _ => None,
            })
            .collect();
        assert_eq!(
            sources,
            vec![Some(VaultSource::Local), Some(VaultSource::Remote)]
        );
    }

    #[test]
    fn from_display_maps_statuses_and_absence() {
        assert_eq!(GroupStatus::from_display(None), GroupStatus::Loading);
        assert_eq!(
            GroupStatus::from_display(Some(VaultListStatus::Fresh)),
            GroupStatus::Fresh
        );
        assert_eq!(
            GroupStatus::from_display(Some(VaultListStatus::Stale)),
            GroupStatus::Stale
        );
        assert_eq!(
            GroupStatus::from_display(Some(VaultListStatus::AuthError)),
            GroupStatus::AuthError
        );
        assert_eq!(
            GroupStatus::from_display(Some(VaultListStatus::Unreachable)),
            GroupStatus::Unreachable
        );
    }
}
