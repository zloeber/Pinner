use std::path::PathBuf;

use pinner_ecosystem::{Finding, Pin, Rewrite};
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunReport {
    pub pins: Vec<Pin>,
    pub rewrites: Vec<Rewrite>,
    pub findings: Vec<Finding>,
    pub drift: Vec<DriftItem>,
    /// Count of pins applied by `upgrade` (0 for pin/check/audit).
    pub upgraded: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DriftItem {
    pub path: PathBuf,
    pub name: String,
    pub expected: String,
    pub actual: String,
}
