//! The Work Notes kit — the blessed configuration for customer-facing work.
//!
//! Contents are embedded from `kits/work-notes/` at compile time. Those files
//! are byte-identical to `golden-vault/.notesmith/` (see
//! `tests/kit_matches_golden_vault.rs`); edit them there, not here.

use crate::KitFile;

pub(crate) const ID: &str = "work-notes";

pub(crate) const DESCRIPTION: &str = "Meetings, customers, streams, people and tasks. `kind` is the canonical type field and \
     relationships live in frontmatter wikilink lists. See docs/example-work-notes-kit.md.";

macro_rules! kit_file {
    ($relative:literal) => {
        (
            $relative,
            include_str!(concat!("../../../kits/work-notes/", $relative)),
        )
    };
}

pub(crate) const FILES: &[KitFile] = &[
    kit_file!(".notesmith/vault.toml"),
    kit_file!(".notesmith/fields.toml"),
    kit_file!(".notesmith/routing.yaml"),
    kit_file!(".notesmith/skill.md"),
    kit_file!(".notesmith/prompts/daily-note.md"),
    kit_file!(".notesmith/templates/internal-meeting.md"),
    kit_file!(".notesmith/templates/external-meeting.md"),
    kit_file!(".notesmith/templates/stream.md"),
    kit_file!(".notesmith/templates/customer.md"),
    kit_file!(".notesmith/templates/person.md"),
    kit_file!(".notesmith/templates/daily.md"),
    kit_file!(".notesmith/templates/weekly.md"),
    kit_file!(".notesmith/templates/quarterly.md"),
    kit_file!(".notesmith/templates/generic-note.md"),
    kit_file!("Dashboards/Home.md"),
    kit_file!("Dashboards/Tasks - Active.md"),
    kit_file!("Dashboards/Weekly Review.md"),
];

/// Folders exist for humans; metadata is the relationship model. These are
/// created empty so the vault looks right before anything is captured.
pub(crate) const FOLDERS: &[&str] = &[
    "Inbox",
    "Meetings",
    "Streams",
    "Customers",
    "People",
    "Daily",
    "Weekly",
    "Quarterly",
    "Dashboards",
];
