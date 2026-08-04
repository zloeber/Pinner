use pinner_ecosystem::Pin;
use serde_json::Value;

use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinDecision {
    Accept,
    Skip,
    Edit { pinned: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkthroughOutcome {
    Continue { pins: Vec<Pin> },
    Aborted,
}

/// Apply per-pin decisions in order. `decisions.len()` must equal `pins.len()`.
///
/// Skip → omit; Edit → clone pin with new pinned + metadata `user_override=true`; Accept → keep.
pub fn apply_walkthrough_decisions(
    pins: &[Pin],
    decisions: &[PinDecision],
) -> Result<WalkthroughOutcome, CoreError> {
    if pins.len() != decisions.len() {
        return Err(CoreError::WalkthroughLengthMismatch {
            pins: pins.len(),
            decisions: decisions.len(),
        });
    }

    let mut out = Vec::with_capacity(pins.len());
    for (pin, decision) in pins.iter().zip(decisions.iter()) {
        match decision {
            PinDecision::Accept => out.push(pin.clone()),
            PinDecision::Skip => {}
            PinDecision::Edit { pinned } => {
                let mut edited = pin.clone();
                edited.pinned = pinned.clone();
                edited
                    .metadata
                    .insert("user_override".into(), Value::Bool(true));
                out.push(edited);
            }
        }
    }
    Ok(WalkthroughOutcome::Continue { pins: out })
}
