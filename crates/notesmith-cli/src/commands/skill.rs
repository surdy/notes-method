use std::{fs, path::Path};

use clap::Subcommand;
use notesmith_config::{GlobalConfig, detect_vault};

use crate::commands::vault::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Print the vault's skill file to stdout
    Print,
}

impl SkillCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        match self {
            SkillCommand::Print => cmd_print(global_config, explicit_vault, cwd, format),
        }
    }
}

fn cmd_print(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let detected = detect_vault(cwd, explicit_vault, global_config)?;
    let skill_path = detected.root.join(".notesmith").join("skill.md");

    match fs::read_to_string(&skill_path) {
        Ok(content) => match format {
            OutputFormat::Text => print!("{content}"),
            OutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "path": skill_path,
                    "exists": true,
                    "content": content,
                }))?
            ),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let message = format!(
                "No skill file found at {}. Create .notesmith/skill.md to teach agents how to operate this vault.",
                skill_path.display()
            );
            match format {
                OutputFormat::Text => println!("{message}"),
                OutputFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "path": skill_path,
                        "exists": false,
                        "message": message,
                    }))?
                ),
            }
        }
        Err(error) => return Err(error.into()),
    }

    Ok(())
}
