//! `stetdoc capabilities`: `capabilities()`, or `json_schema()` with `--schema`.

use crate::cli::CapabilitiesArgs;
use crate::output::{Failure, Outcome, to_json};
use std::fmt::Write;

/// Runs `capabilities`.
pub fn run(args: &CapabilitiesArgs) -> Result<Outcome, Failure> {
    if args.schema {
        let schema = stetdoc_core::json_schema();
        let json = serde_json::from_str(&schema).map_err(|e| {
            Failure::Core(stetdoc_core::StetError::new(
                stetdoc_core::ErrorCode::Internal,
                format!("internal: the JSON Schema is not JSON: {e}"),
            ))
        })?;
        return Ok(Outcome::new(schema, json));
    }
    let caps = stetdoc_core::capabilities();
    let mut text = String::new();
    let formats: Vec<String> = caps
        .formats
        .iter()
        .map(|f| format!("{} (.{})", f.id, f.extension))
        .collect();
    let _ = writeln!(text, "stetdoc {}", caps.version);
    let _ = writeln!(text, "formats: {}", formats.join(", "));
    let _ = writeln!(text, "complement: {}", caps.complement_version);
    let _ = writeln!(text, "\noperations:");
    for op in &caps.ops {
        let tracked = if op.trackable { "tracked" } else { "untracked" };
        let _ = writeln!(text, "  {:<15} {:<10} {}", op.name, tracked, op.summary);
    }
    let _ = writeln!(text, "\nerror codes:");
    for code in &caps.error_codes {
        let _ = writeln!(text, "  {:<20} exit {}", code.code, code.exit);
    }
    Ok(Outcome::new(text, to_json(&caps)?))
}
