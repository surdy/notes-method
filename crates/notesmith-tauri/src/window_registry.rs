//! The authoritative map from open OS windows to the daemon connection (and
//! vault) they are bound to.
//!
//! Today the desktop has one app-global daemon connection. Per-window
//! connections (ADR 0017) make the daemon a property of *each window*, so this
//! registry is the single source of truth for "which server/vault does this
//! window belong to". It keeps two consistent indexes:
//!
//! - `label -> WindowContext` (authoritative)
//! - `VaultKey -> label` (reuse index, so re-opening a vault focuses the
//!   existing window instead of spawning a duplicate)
//!
//! The type is pure (no Tauri runtime) so the invariants are unit-tested in
//! isolation; `main.rs` wraps it in a `Mutex` managed state.

use std::collections::HashMap;

use crate::vault_window::VaultKey;

/// What an open window is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowContext {
    /// App chrome not bound to any vault (onboarding / main / settings). These
    /// windows act on the default connection.
    Global,
    /// Bound to a server but without a selected vault yet.
    ServerScoped { server_id: String },
    /// A vault window: a specific vault on a specific server.
    VaultScoped { server_id: String, vault: String },
}

impl WindowContext {
    /// Convenience constructor for a vault window.
    pub fn vault(server_id: impl Into<String>, vault: impl Into<String>) -> Self {
        WindowContext::VaultScoped {
            server_id: server_id.into(),
            vault: vault.into(),
        }
    }

    /// The server this window is bound to, if any (`None` for [`Global`]).
    ///
    /// [`Global`]: WindowContext::Global
    pub fn server_id(&self) -> Option<&str> {
        match self {
            WindowContext::Global => None,
            WindowContext::ServerScoped { server_id }
            | WindowContext::VaultScoped { server_id, .. } => Some(server_id),
        }
    }

    /// The server-qualified vault identity, if this is a vault window.
    pub fn vault_key(&self) -> Option<VaultKey> {
        match self {
            WindowContext::VaultScoped { server_id, vault } => {
                Some(VaultKey::new(server_id.clone(), vault.clone()))
            }
            _ => None,
        }
    }

    /// The `(server_id, vault)` pair this window is bound to, but **only** for a
    /// vault window. `Global`/`ServerScoped` windows return `None`, letting
    /// vault-scoped IPC reject them with a clear error instead of dispatching to
    /// the wrong (or no) daemon.
    pub fn vault_binding(&self) -> Option<(&str, &str)> {
        match self {
            WindowContext::VaultScoped { server_id, vault } => Some((server_id, vault)),
            _ => None,
        }
    }
}

/// Bi-directional registry of windows ↔ their connection context.
#[derive(Debug, Default)]
pub struct WindowRegistry {
    by_label: HashMap<String, WindowContext>,
    label_by_key: HashMap<VaultKey, String>,
}

impl WindowRegistry {
    /// Bind `label` to `context`, keeping both indexes consistent.
    ///
    /// If the label previously pointed at a different vault key, that stale
    /// reuse-index entry is dropped first, so a window that is re-bound (or a
    /// label that is reused) never leaves a dangling key mapping.
    pub fn insert(&mut self, label: impl Into<String>, context: WindowContext) {
        let label = label.into();

        // Drop any stale key→label entry this label used to own.
        if let Some(prev) = self.by_label.get(&label)
            && let Some(prev_key) = prev.vault_key()
            && self
                .label_by_key
                .get(&prev_key)
                .is_some_and(|l| l == &label)
        {
            self.label_by_key.remove(&prev_key);
        }

        if let Some(key) = context.vault_key() {
            self.label_by_key.insert(key, label.clone());
        }
        self.by_label.insert(label, context);
    }

    /// The context bound to `label`, if any.
    pub fn context_for_label(&self, label: &str) -> Option<&WindowContext> {
        self.by_label.get(label)
    }

    /// The existing window label for a vault identity, if one is open.
    pub fn label_for_key(&self, key: &VaultKey) -> Option<&str> {
        self.label_by_key.get(key).map(String::as_str)
    }

    /// Every registered vault window as `(label, server_id, vault)`.
    ///
    /// Order is unspecified. `Global`/`ServerScoped` windows are skipped — only
    /// vault-bound windows are returned, so callers can snapshot, enumerate
    /// open vaults, or look a label up by vault name without consulting a
    /// separate name-keyed map.
    pub fn vault_windows(&self) -> Vec<(String, String, String)> {
        self.by_label
            .iter()
            .filter_map(|(label, context)| match context {
                WindowContext::VaultScoped { server_id, vault } => {
                    Some((label.clone(), server_id.clone(), vault.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Remove a window by label, returning its former context. Cleans up the
    /// reuse index when the removed window owned its key mapping.
    pub fn remove_label(&mut self, label: &str) -> Option<WindowContext> {
        let context = self.by_label.remove(label)?;
        if let Some(key) = context.vault_key()
            && self.label_by_key.get(&key).is_some_and(|l| l == label)
        {
            self.label_by_key.remove(&key);
        }
        Some(context)
    }

    /// Number of registered windows (test/diagnostic helper).
    pub fn len(&self) -> usize {
        self.by_label.len()
    }

    /// True when no windows are registered.
    pub fn is_empty(&self) -> bool {
        self.by_label.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(server: &str, vault: &str) -> VaultKey {
        VaultKey::new(server, vault)
    }

    #[test]
    fn context_helpers_expose_server_and_key() {
        assert_eq!(WindowContext::Global.server_id(), None);
        assert_eq!(WindowContext::Global.vault_key(), None);

        let server = WindowContext::ServerScoped {
            server_id: "remote".into(),
        };
        assert_eq!(server.server_id(), Some("remote"));
        assert_eq!(server.vault_key(), None);

        let vault = WindowContext::vault("remote", "personal");
        assert_eq!(vault.server_id(), Some("remote"));
        assert_eq!(vault.vault_key(), Some(key("remote", "personal")));
    }

    #[test]
    fn vault_binding_only_resolves_for_vault_windows() {
        // A vault window exposes its (server, vault) pair...
        assert_eq!(
            WindowContext::vault("remote", "personal").vault_binding(),
            Some(("remote", "personal"))
        );
        // ...but a Global or merely server-scoped window does not: a vault-scoped
        // command invoked from one must be rejected, not sent to a wrong daemon.
        assert_eq!(WindowContext::Global.vault_binding(), None);
        assert_eq!(
            WindowContext::ServerScoped {
                server_id: "remote".into(),
            }
            .vault_binding(),
            None
        );
    }

    #[test]
    fn insert_then_resolve_by_label_and_key() {
        let mut reg = WindowRegistry::default();
        reg.insert(
            "main:personal-abc",
            WindowContext::vault("local", "personal"),
        );

        assert_eq!(
            reg.context_for_label("main:personal-abc"),
            Some(&WindowContext::vault("local", "personal"))
        );
        assert_eq!(
            reg.label_for_key(&key("local", "personal")),
            Some("main:personal-abc")
        );
    }

    #[test]
    fn same_vault_distinct_servers_are_separate_entries() {
        let mut reg = WindowRegistry::default();
        reg.insert("label-local", WindowContext::vault("local", "personal"));
        reg.insert("label-remote", WindowContext::vault("remote", "personal"));

        assert_eq!(reg.len(), 2);
        assert_eq!(
            reg.label_for_key(&key("local", "personal")),
            Some("label-local")
        );
        assert_eq!(
            reg.label_for_key(&key("remote", "personal")),
            Some("label-remote")
        );
    }

    #[test]
    fn rebinding_a_label_drops_the_stale_key_mapping() {
        let mut reg = WindowRegistry::default();
        reg.insert("L", WindowContext::vault("local", "old"));
        // The same window navigates to a different vault.
        reg.insert("L", WindowContext::vault("local", "new"));

        assert_eq!(reg.label_for_key(&key("local", "old")), None);
        assert_eq!(reg.label_for_key(&key("local", "new")), Some("L"));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn global_and_server_scoped_have_no_reuse_index() {
        let mut reg = WindowRegistry::default();
        reg.insert("settings", WindowContext::Global);
        reg.insert(
            "server-window",
            WindowContext::ServerScoped {
                server_id: "remote".into(),
            },
        );

        assert_eq!(
            reg.context_for_label("settings"),
            Some(&WindowContext::Global)
        );
        assert_eq!(reg.len(), 2);
        // No vault keys were registered.
        assert_eq!(reg.label_for_key(&key("remote", "anything")), None);
    }

    #[test]
    fn remove_label_cleans_both_indexes() {
        let mut reg = WindowRegistry::default();
        reg.insert("L", WindowContext::vault("local", "personal"));

        let removed = reg.remove_label("L");
        assert_eq!(removed, Some(WindowContext::vault("local", "personal")));
        assert!(reg.is_empty());
        assert_eq!(reg.label_for_key(&key("local", "personal")), None);
        assert_eq!(reg.context_for_label("L"), None);
        assert_eq!(reg.remove_label("L"), None);
    }

    #[test]
    fn removing_a_label_does_not_clobber_another_labels_key() {
        // Two labels transiently claim the same key (e.g. a race); removing the
        // one that does NOT currently own the reuse-index entry must not delete
        // the live mapping.
        let mut reg = WindowRegistry::default();
        reg.insert("first", WindowContext::vault("local", "p"));
        reg.insert("second", WindowContext::vault("local", "p")); // now owns the key

        reg.remove_label("first");
        assert_eq!(reg.label_for_key(&key("local", "p")), Some("second"));
    }

    #[test]
    fn vault_windows_lists_only_vault_bound_windows() {
        let mut reg = WindowRegistry::default();
        reg.insert("label-local", WindowContext::vault("local", "personal"));
        reg.insert("label-remote", WindowContext::vault("remote", "personal"));
        reg.insert("settings", WindowContext::Global);
        reg.insert(
            "server-window",
            WindowContext::ServerScoped {
                server_id: "remote".into(),
            },
        );

        let mut got = reg.vault_windows();
        got.sort();
        assert_eq!(
            got,
            vec![
                (
                    "label-local".to_string(),
                    "local".to_string(),
                    "personal".to_string()
                ),
                (
                    "label-remote".to_string(),
                    "remote".to_string(),
                    "personal".to_string()
                ),
            ]
        );
    }
}
