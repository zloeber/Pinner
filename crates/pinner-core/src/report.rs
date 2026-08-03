use std::path::PathBuf;

use pinner_ecosystem::{Finding, Pin, Rewrite};

#[derive(Debug, Clone, Default)]
pub struct RunReport {
    pub pins: Vec<Pin>,
    pub rewrites: Vec<Rewrite>,
    pub findings: Vec<Finding>,
    pub drift: Vec<DriftItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftItem {
    pub path: PathBuf,
    pub name: String,
    pub expected: String,
    pub actual: String,
}
