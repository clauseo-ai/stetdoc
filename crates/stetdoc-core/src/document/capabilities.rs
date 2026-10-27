//! `capabilities()` and `json_schema()`: what this release can do, and the shape of every public type, as data.

use crate::api::batch::Batch;
use crate::api::capabilities::{
    Capabilities, ErrorInfo, FieldInfo, FindingInfo, FormatInfo, OpInfo,
};
use crate::api::check::{CheckReport, FindingCode};
use crate::api::error::{ErrorCode, StetError};
use crate::api::files::ReadableFiles;
use crate::api::inspect::Inspection;
use crate::api::limits::Limits;
use crate::api::lookup::Lookup;
use crate::api::occurrence::Occurrences;
use crate::api::op::Op;
use crate::api::op_spec::OPS;
use crate::api::options::{CompareOptions, OpenOptions};
use crate::api::receipt::{CheckResult, Receipt};
use crate::api::revision::RevisionInfo;
use crate::api::stamp::Stamp;
use crate::api::view::{FormatId, View, ViewSpec, WarningCode};
use crate::api::working_copy::SaveReceipt;
use schemars::JsonSchema;

/// The version of the complement this release writes. It reads the versions up to its own.
pub(crate) const COMPLEMENT_VERSION: &str = "1.0";

/// What this release can do: the operations with their fields, the formats, the codes and the default limits.
pub fn capabilities() -> Capabilities {
    Capabilities {
        version: env!("CARGO_PKG_VERSION").to_string(),
        complement_version: COMPLEMENT_VERSION.to_string(),
        formats: vec![FormatInfo {
            id: FormatId::STET.to_string(),
            extension: "md".to_string(),
        }],
        ops: OPS
            .iter()
            .map(|spec| OpInfo {
                name: spec.name.to_string(),
                summary: spec.summary.to_string(),
                trackable: spec.trackable,
                reversible: spec.reversible,
                one_of: spec.one_of.iter().map(|s| (*s).to_string()).collect(),
                example: spec.example.to_string(),
                fields: spec
                    .fields
                    .iter()
                    .map(|f| FieldInfo {
                        name: f.name.to_string(),
                        kind: f.kind.as_str().to_string(),
                        required: f.required,
                        doc: f.doc.to_string(),
                        choices: f.choices.iter().map(|c| (*c).to_string()).collect(),
                    })
                    .collect(),
            })
            .collect(),
        error_codes: ErrorCode::ALL
            .iter()
            .map(|code| ErrorInfo {
                code: code.as_str().to_string(),
                exit: code.exit_class().number(),
                description: code.description().to_string(),
                next: code.next_hint().to_string(),
            })
            .collect(),
        warning_codes: WarningCode::ALL
            .iter()
            .map(|c| c.as_str().to_string())
            .collect(),
        finding_codes: FindingCode::ALL
            .iter()
            .map(|code| FindingInfo {
                code: code.as_str().to_string(),
                class: match code.class() {
                    crate::api::check::Class::Violation => "violation",
                    crate::api::check::Class::Note => "note",
                }
                .to_string(),
                description: code.description().to_string(),
            })
            .collect(),
        limits: Limits::default(),
    }
}

/// Every public data type, so that one schema describes them all.
#[derive(JsonSchema)]
#[expect(
    dead_code,
    reason = "the fields exist so that the schema describes their types"
)]
struct Everything {
    batch: Batch,
    op: Op,
    receipt: Receipt,
    check_result: CheckResult,
    view_spec: ViewSpec,
    view: View,
    lookup: Lookup,
    occurrences: Occurrences,
    inspection: Inspection,
    check_report: CheckReport,
    revisions: Vec<RevisionInfo>,
    readable_files: ReadableFiles,
    save_receipt: SaveReceipt,
    capabilities: Capabilities,
    open_options: OpenOptions,
    compare_options: CompareOptions,
    limits: Limits,
    stamp: Stamp,
    error: StetError,
}

/// One JSON Schema document for every public data type, under `$defs`. The generated TypeScript types and the CLI reference
/// read it from the binary.
pub fn json_schema() -> String {
    let schema = schemars::schema_for!(Everything);
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}
