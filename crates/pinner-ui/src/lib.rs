//! Compact walkthrough TUI and pretty TTY report summaries.

mod pretty;
mod walkthrough;

pub use pretty::emit_pretty_report;
pub use walkthrough::run_compact_walkthrough;
