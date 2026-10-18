//! `stetdoc skills get`: the skill embedded in the binary.

use crate::cli::{SkillsAction, SkillsArgs};
use crate::output::{Failure, Outcome};
use crate::skill;
use stetdoc_core::{ErrorCode, StetError};

/// Runs `skills`.
pub fn run(args: &SkillsArgs) -> Result<Outcome, Failure> {
    let SkillsAction::Get { name, full } = &args.action;
    let names: Vec<&str> = match (name.as_deref(), full) {
        (None, true) => skill::NAMES.to_vec(),
        (Some(_), true) => {
            return Err(Failure::usage(
                "--full prints every part of the skill, so it takes no name.",
                "Run stetdoc skills get --full, or stetdoc skills get NAME.",
            ));
        }
        (Some(name), false) => vec![name],
        (None, false) => vec!["core"],
    };
    let mut parts = Vec::new();
    for name in &names {
        let text = skill::get(name).ok_or_else(|| {
            StetError::new(
                ErrorCode::NotFound,
                format!("There is no skill named {name}."),
            )
            .with_next(format!("Name one of: {}.", skill::NAMES.join(", ")))
        })?;
        parts.push((*name, text));
    }
    let text = parts
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let json = if let [(name, text)] = parts.as_slice() {
        serde_json::json!({ "name": name, "text": text })
    } else {
        serde_json::json!({ "parts": parts.iter().map(|(n, t)| serde_json::json!({ "name": n, "text": t })).collect::<Vec<_>>() })
    };
    Ok(Outcome::new(text, json))
}
