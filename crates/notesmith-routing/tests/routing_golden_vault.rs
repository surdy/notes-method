//! The fixture's `.notesmith/routing.yaml` is the Work Notes kit's routing
//! config (`docs/example-work-notes-kit.md`). These tests drive the real engine
//! over it so the documented kind→folder rules stay true: filing is mechanical,
//! and anything without a recognized `kind` stays in the Inbox for triage.

use notesmith_routing::{RoutingEngine, RoutingError};

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn engine() -> RoutingEngine {
    RoutingEngine::load(&golden_vault()).expect("golden-vault should carry a routing config")
}

#[test]
fn meetings_are_filed_by_date_never_by_customer() {
    let engine = engine();
    let content = "---\nkind: meeting\naudience: external\ndate: 2025-01-14\ncustomers:\n  - \"[[Acme Corp]]\"\n---\n# Check-in\n";

    let route = engine
        .preview("Inbox/2025-01-14 - Acme Corp - Check-in.md", content)
        .unwrap();

    assert_eq!(route.rule_id, "file-meeting");
    assert_eq!(
        route.destination, "Meetings/2025/01/2025-01-14 - Acme Corp - Check-in.md",
        "a meeting is filed by its date — the customer never appears in the path"
    );
}

#[test]
fn a_multi_customer_meeting_routes_the_same_way() {
    let engine = engine();
    let content = "---\nkind: meeting\naudience: internal\ndate: 2025-01-16\ncustomers:\n  - \"[[Acme Corp]]\"\n  - \"[[Globex]]\"\n---\n# Review\n";

    let route = engine
        .preview("Inbox/2025-01-16 - Cross-customer Review.md", content)
        .unwrap();

    assert_eq!(
        route.destination,
        "Meetings/2025/01/2025-01-16 - Cross-customer Review.md"
    );
}

#[test]
fn streams_and_people_route_to_their_flat_folders() {
    let engine = engine();

    let stream = engine
        .preview(
            "Inbox/Acme Corp - Renewal 2026.md",
            "---\nkind: stream\nstatus: active\ncustomers: []\n---\n# Renewal\n",
        )
        .unwrap();
    assert_eq!(stream.rule_id, "file-stream");
    assert_eq!(stream.destination, "Streams/Acme Corp - Renewal 2026.md");

    let person = engine
        .preview(
            "Inbox/Jane Doe.md",
            "---\nkind: person\norg: \"[[Acme Corp]]\"\n---\n# Jane Doe\n",
        )
        .unwrap();
    assert_eq!(person.rule_id, "file-person");
    assert_eq!(person.destination, "People/Jane Doe.md");
}

#[test]
fn a_done_stream_still_routes_to_streams() {
    let engine = engine();

    let route = engine
        .preview(
            "Inbox/Internal - Support Process Redesign.md",
            "---\nkind: stream\nstatus: done\ncustomers: []\n---\n# Redesign\n",
        )
        .unwrap();

    assert_eq!(
        route.destination, "Streams/Internal - Support Process Redesign.md",
        "status is metadata — nothing moves because its state changed"
    );
}

#[test]
fn notes_without_a_recognized_kind_stay_in_the_inbox() {
    let engine = engine();

    for (path, content) in [
        (
            "Inbox/Quick Note.md",
            "---\ntags:\n  - quick\n---\n# Quick\n",
        ),
        (
            "Inbox/Half Filed.md",
            "---\nkind: experiment\n---\n# Unknown kind\n",
        ),
        // A meeting still missing its date cannot be filed by date.
        (
            "Inbox/Undated Meeting.md",
            "---\nkind: meeting\ncustomers: []\n---\n# Undated\n",
        ),
    ] {
        let error = engine.preview(path, content).unwrap_err();
        assert!(
            matches!(error, RoutingError::NoMatch { .. }),
            "{path} should stay in the Inbox for triage, got {error:?}"
        );
    }
}

#[test]
fn notes_outside_the_inbox_are_left_alone() {
    let engine = engine();
    let content = "---\nkind: meeting\naudience: internal\ndate: 2025-01-15\n---\n# Sync\n";

    let error = engine
        .preview("Meetings/2025/01/2025-01-15 - Internal Sync.md", content)
        .unwrap_err();

    assert!(
        matches!(error, RoutingError::NoMatch { .. }),
        "already-filed notes must not be re-routed, got {error:?}"
    );
}

#[test]
fn malformed_frontmatter_degrades_to_no_match_without_panicking() {
    let engine = engine();

    for content in [
        "---\nkind: meeting\ncustomers: [unclosed\ndate: 2025-01-14\n---\n# Broken\n",
        "---\n\tkind:\t\tmeeting\n---\n# Tabs are not valid YAML indentation\n",
        "---\nkind: meeting\ndate:\n  - nested\n  - list\n---\n# Wrong shape\n",
        "no frontmatter at all\n",
    ] {
        let result = engine.preview("Inbox/Broken.md", content);
        assert!(
            result.is_err(),
            "malformed content should not resolve to a destination: {content:?}"
        );
    }
}
