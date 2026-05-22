use std::path::Path;

use anyhow::Context;
use arboard::Clipboard;
use clap::Args;
use notesmith_config::{GlobalConfig, detect_vault};
use notesmith_core::{VaultEngine, VaultPath};
use notesmith_vault::NativeVaultEngine;

#[derive(Debug, Args)]
pub struct CopyHtmlCommand {
    /// Note path (relative to vault root)
    path: String,
}

impl CopyHtmlCommand {
    pub fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
    ) -> anyhow::Result<()> {
        let payload = render_clipboard_payload(global_config, explicit_vault, cwd, &self.path)?;
        let mut clipboard =
            Clipboard::new().context("failed to access the system clipboard for copy-html")?;
        clipboard
            .set_html(payload.html, Some(payload.plain_text))
            .context("failed to copy HTML to the system clipboard")?;
        println!("Copied {} as HTML to the clipboard.", self.path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use notesmith_config::{GlobalConfig, VaultConfig};
    use tempfile::TempDir;

    use super::render_clipboard_payload;

    fn create_vault(root: &std::path::Path, name: &str) {
        let config = VaultConfig {
            name: name.to_string(),
            ..Default::default()
        };

        let config_dir = root.join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        config.save_to(&config_dir.join("vault.toml")).unwrap();
    }

    #[test]
    fn render_clipboard_payload_reads_note_from_detected_vault() {
        let temp_dir = TempDir::new().unwrap();
        let vault_root = temp_dir.path().join("work");
        create_vault(&vault_root, "work");
        fs::create_dir_all(vault_root.join("Inbox")).unwrap();
        fs::write(
            vault_root.join("Inbox/Rendered.md"),
            "---\nstatus: draft\n---\n# Heading\n\n[[Target|Alias]]\n",
        )
        .unwrap();

        let payload = render_clipboard_payload(
            &GlobalConfig::default(),
            None,
            &vault_root,
            "Inbox/Rendered.md",
        )
        .unwrap();

        assert!(payload.html.contains("<html"), "html was: {}", payload.html);
        assert!(
            !payload.html.contains("status: draft"),
            "html was: {}",
            payload.html
        );
        assert!(
            payload.html.contains(r#"<a href="Target">Alias</a>"#),
            "html was: {}",
            payload.html
        );
        assert_eq!(payload.plain_text, "# Heading\n\n[[Target|Alias]]\n");
    }
}

struct ClipboardPayload {
    html: String,
    plain_text: String,
}

fn render_clipboard_payload(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    path: &str,
) -> anyhow::Result<ClipboardPayload> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let engine = NativeVaultEngine;
    let content = engine
        .read(&detected.root, &VaultPath::new(path))
        .with_context(|| format!("failed to read note at {path}"))?;

    Ok(ClipboardPayload {
        html: notesmith_html::render_to_html_with_inline_styles(&content),
        plain_text: notesmith_html::strip_frontmatter(&content).to_string(),
    })
}
