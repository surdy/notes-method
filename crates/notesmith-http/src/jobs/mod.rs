//! Generic per-vault job runner (ADR 0025 Decision 2, issue #280).
//!
//! `[[jobs]]` entries in `vault.toml` declare scheduled work; the daemon runs
//! one supervised runner task per vault that executes `command`-kind jobs on
//! `every`/`at` schedules with catch-up-on-wake, emitting `job.*` events.
//! Agent-kind jobs and same-day `after` ordering are reserved for #282.

pub mod schedule;
pub mod state;
