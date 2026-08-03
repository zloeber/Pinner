use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use pinner_ecosystem::EcosystemKind;
use serde::Deserialize;

use crate::error::CoreError;

/// Default pin styles (enforced by ecosystem crates in v1):
/// - mise: exact tool versions
/// - node/python: exact semver (respecting `pin_exact_ranges` for ^/~)
/// - docker: digest pins
/// - actions: commit SHA pins
#[derive(Debug, Clone)]
pub struct Policy {
    pub enabled: Vec<EcosystemKind>,
    pub ignore_globs: Vec<String>,
    pub offline_default: bool,
    pub toolchain_install: bool,
    pub pin_exact_ranges: bool,
    pub allow_floating: Vec<AllowFloating>,
    ignore_matcher: GlobSet,
}

#[derive(Debug, Clone)]
pub struct AllowFloating {
    pub ecosystem: EcosystemKind,
    pub name: String,
    pub path_glob: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PolicyFile {
    ecosystems: Option<EcosystemsSection>,
    ignore: Option<Vec<String>>,
    toolchain: Option<ToolchainSection>,
    pinning: Option<PinningSection>,
}

#[derive(Debug, Default, Deserialize)]
struct EcosystemsSection {
    mise: Option<bool>,
    node: Option<bool>,
    python: Option<bool>,
    docker: Option<bool>,
    actions: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct ToolchainSection {
    install: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct PinningSection {
    exact_ranges: Option<bool>,
}

impl Policy {
    pub fn default_policy() -> Self {
        Self::from_parts(
            vec![
                EcosystemKind::Mise,
                EcosystemKind::Node,
                EcosystemKind::Python,
                EcosystemKind::Docker,
                EcosystemKind::Actions,
            ],
            vec![
                "**/node_modules/**".to_string(),
                "**/.git/**".to_string(),
                "**/vendor/**".to_string(),
            ],
            false,
            true,
            true,
            Vec::new(),
        )
        .expect("default ignore globs are valid")
    }

    pub fn load(path: Option<&Path>) -> Result<Self, CoreError> {
        let mut policy = Self::default_policy();
        let Some(path) = path else {
            return Ok(policy);
        };

        let contents = std::fs::read_to_string(path)?;
        let file: PolicyFile = toml::from_str(&contents)?;
        policy.merge_file(file)?;
        Ok(policy)
    }

    pub fn is_enabled(&self, kind: EcosystemKind) -> bool {
        self.enabled.contains(&kind)
    }

    pub fn is_ignored(&self, path: &Path) -> bool {
        let normalized = path.to_string_lossy().replace('\\', "/");
        self.ignore_matcher.is_match(normalized)
    }

    fn merge_file(&mut self, file: PolicyFile) -> Result<(), CoreError> {
        if let Some(ecosystems) = file.ecosystems {
            apply_ecosystem(&mut self.enabled, EcosystemKind::Mise, ecosystems.mise);
            apply_ecosystem(&mut self.enabled, EcosystemKind::Node, ecosystems.node);
            apply_ecosystem(&mut self.enabled, EcosystemKind::Python, ecosystems.python);
            apply_ecosystem(&mut self.enabled, EcosystemKind::Docker, ecosystems.docker);
            apply_ecosystem(&mut self.enabled, EcosystemKind::Actions, ecosystems.actions);
        }

        if let Some(ignore) = file.ignore {
            self.ignore_globs = ignore;
            self.ignore_matcher = build_matcher(&self.ignore_globs)?;
        }

        if let Some(toolchain) = file.toolchain {
            if let Some(install) = toolchain.install {
                self.toolchain_install = install;
            }
        }

        if let Some(pinning) = file.pinning {
            if let Some(exact_ranges) = pinning.exact_ranges {
                self.pin_exact_ranges = exact_ranges;
            }
        }

        Ok(())
    }

    fn from_parts(
        enabled: Vec<EcosystemKind>,
        ignore_globs: Vec<String>,
        offline_default: bool,
        toolchain_install: bool,
        pin_exact_ranges: bool,
        allow_floating: Vec<AllowFloating>,
    ) -> Result<Self, CoreError> {
        let ignore_matcher = build_matcher(&ignore_globs)?;
        Ok(Self {
            enabled,
            ignore_globs,
            offline_default,
            toolchain_install,
            pin_exact_ranges,
            allow_floating,
            ignore_matcher,
        })
    }
}

fn apply_ecosystem(enabled: &mut Vec<EcosystemKind>, kind: EcosystemKind, value: Option<bool>) {
    let Some(enabled_flag) = value else {
        return;
    };
    if enabled_flag {
        if !enabled.contains(&kind) {
            enabled.push(kind);
        }
    } else {
        enabled.retain(|k| *k != kind);
    }
}

fn build_matcher(globs: &[String]) -> Result<GlobSet, CoreError> {
    let mut builder = GlobSetBuilder::new();
    for glob in globs {
        builder.add(Glob::new(glob)?);
    }
    Ok(builder.build()?)
}
