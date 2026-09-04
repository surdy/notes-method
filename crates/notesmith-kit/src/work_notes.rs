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
    // Calendar-sync connector (ADR 0025). `include_str!` embeds text only, so
    // `kit apply` writes the script without its executable bit — the docs tell
    // users to `chmod +x` it after installing.
    kit_file!(".notesmith/connectors/calendar-sync.py"),
    kit_file!(".notesmith/connectors/calendar-sync.config.json"),
    // Email-summary connector (ADR 0025 fallback tier). Same story as
    // calendar-sync: `include_str!` embeds text only, so `kit apply` writes the
    // script without its executable bit — the docs tell users to `chmod +x` it.
    kit_file!(".notesmith/connectors/email-summary.py"),
    kit_file!(".notesmith/connectors/email-summary.config.json"),
    // Teams transcript connector (ADR 0025 Decision 4). Same `chmod +x` story
    // as its siblings; it also shells out to `notesmith transcribe --from-vtt`
    // so the transcript body format stays owned by core.
    kit_file!(".notesmith/connectors/transcript-sync.py"),
    kit_file!(".notesmith/connectors/transcript-sync.config.json"),
    // Meeting-prefill pre-render hook (integrations plan, feature 1). The
    // engine invokes hooks as `sh <script>`, so the shim is shell and the
    // logic is Python — neither needs the executable bit `include_str!` drops.
    kit_file!(".notesmith/scripts/meeting-prefill.sh"),
    kit_file!(".notesmith/scripts/meeting-prefill.py"),
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
    "Meetings/Transcripts",
    "Streams",
    "Customers",
    "People",
    "Daily",
    "Weekly",
    "Quarterly",
    "Calendar",
    "Dashboards",
];
