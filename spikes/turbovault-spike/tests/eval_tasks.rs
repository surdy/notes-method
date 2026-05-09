//! Test TurboVault task parsing with all 7 custom statuses and emoji metadata.

#[test]
fn parses_todo_task() {
    // - [ ] Review PR for API changes
    todo!()
}

#[test]
fn parses_done_task() {
    // - [x] Send weekly report
    todo!()
}

#[test]
fn parses_in_progress_task() {
    // - [/] Draft Q1 planning doc
    todo!()
}

#[test]
fn parses_cancelled_task() {
    // - [-] Cancelled standup
    todo!()
}

#[test]
fn parses_blocked_task() {
    // - [b] Blocked on infra team
    // EXPECTED TO FAIL — TurboVault only supports [ ], [x], [/], [-].
    todo!()
}

#[test]
fn parses_waiting_task() {
    // - [w] Waiting for customer response
    // EXPECTED TO FAIL — not a standard TurboVault status.
    todo!()
}

#[test]
fn parses_on_hold_task() {
    // - [h] On hold pending legal review
    // EXPECTED TO FAIL — not a standard TurboVault status.
    todo!()
}

#[test]
fn parses_task_due_date_emoji() {
    // 📅 2025-01-16
    // EXPECTED TO FAIL — TurboVault has due_date as TODO in parser.
    todo!()
}

#[test]
fn parses_task_priority_emoji() {
    // 🔼, ⏫, 🔽
    // EXPECTED TO FAIL.
    todo!()
}

#[test]
fn parses_task_scheduled_emoji() {
    // ⏳ 2025-01-20
    // EXPECTED TO FAIL.
    todo!()
}

#[test]
fn parses_task_start_emoji() {
    // 🛫 2025-02-15
    // EXPECTED TO FAIL.
    todo!()
}

#[test]
fn parses_task_done_emoji() {
    // ✅ 2025-01-15
    // EXPECTED TO FAIL.
    todo!()
}
