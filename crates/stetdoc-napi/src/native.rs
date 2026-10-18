//! The class and functions the addon exports. Each method reads its JSON arguments, makes one core call and writes the result
//! as JSON, under `marshal::guard`.

use crate::marshal::{CheckPendingRequest, EditRequest, EditorOptions, guard, json, parse};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use stetdoc_core::{
    Batch, CompareOptions, Complement, Document, Edit, ErrorCode, Limits, OpenOptions,
    RevisionFilter, Stamp, StetError, ViewSpec, WorkingCopy,
};

/// One entry of a working copy's complement: its name relative to `.stet/` and its bytes.
#[napi(object)]
pub struct NativeEntry {
    /// The name, relative to `.stet/`.
    pub name: String,
    /// The bytes.
    pub bytes: Buffer,
}

/// A working copy as it crosses the boundary: the readable files as JSON `[name, text]` pairs, and the complement.
#[napi(object)]
pub struct NativeWorkingCopy {
    /// The readable files, as the JSON of `[name, text]` pairs.
    pub files: String,
    /// The complement's entries.
    pub complement: Vec<NativeEntry>,
}

/// What `save` returns: the package and the JSON of its `SaveReceipt`.
#[napi(object)]
pub struct NativeSaved {
    /// The package.
    pub bytes: Buffer,
    /// The JSON of the `SaveReceipt`.
    pub receipt: String,
}

/// What the text editor tool answers: the text of the `tool_result` block and its `is_error` flag.
#[napi(object)]
pub struct NativeToolResult {
    /// The text for the model.
    pub content: String,
    /// Whether the command failed.
    pub is_error: bool,
    /// The JSON of the receipt, when an edit committed.
    pub receipt: Option<String>,
}

/// A `Document`, with the limits it was opened under (which also bound the batches it reads), and the receipt of the
/// comparison that made it, if one did.
#[napi]
pub struct NativeDocument {
    inner: Document,
    limits: Limits,
    compared: Option<String>,
}

fn wrap(inner: Document, limits: Limits) -> NativeDocument {
    NativeDocument {
        inner,
        limits,
        compared: None,
    }
}

#[napi]
impl NativeDocument {
    /// `Document::open` on the bytes of a package, with the JSON of `OpenOptions`.
    #[napi(factory)]
    pub fn open(bytes: Buffer, options: String) -> napi::Result<NativeDocument> {
        guard(|| {
            let options: OpenOptions = parse("open options", &options)?;
            let inner = Document::open(bytes.to_vec(), &options)?;
            Ok(wrap(inner, options.limits))
        })
    }

    /// `Document::resume` on a working copy, with the JSON of `OpenOptions`.
    #[napi(factory)]
    pub fn resume(
        working_copy: NativeWorkingCopy,
        options: String,
    ) -> napi::Result<NativeDocument> {
        guard(|| {
            let options: OpenOptions = parse("open options", &options)?;
            let files: Vec<(String, String)> = parse("working copy's files", &working_copy.files)?;
            let entries = working_copy
                .complement
                .into_iter()
                .map(|entry| (entry.name, entry.bytes.to_vec()))
                .collect();
            let copy = WorkingCopy {
                files,
                complement: Complement { entries },
            };
            let inner = Document::resume(&copy, &options)?;
            Ok(wrap(inner, options.limits))
        })
    }

    /// `compare` of two packages, with the JSON of `CompareOptions`. The receipt is kept for `comparedReceipt`.
    #[napi(factory)]
    pub fn compare(
        original: Buffer,
        revised: Buffer,
        options: String,
    ) -> napi::Result<NativeDocument> {
        guard(|| {
            let options: CompareOptions = parse("compare options", &options)?;
            let compared = stetdoc_core::compare(original.to_vec(), revised.to_vec(), &options)?;
            let receipt = json(&compared.receipt)?;
            let mut document = wrap(compared.document, options.limits);
            document.compared = Some(receipt);
            Ok(document)
        })
    }

    /// The JSON of the receipt of the comparison that made this document, or `null`.
    #[napi]
    pub fn compared_receipt(&self) -> Option<String> {
        self.compared.clone()
    }

    /// The base token.
    #[napi]
    pub fn base(&self) -> String {
        self.inner.base()
    }

    /// `view`, with the JSON of a `ViewSpec`.
    #[napi]
    pub fn view(&self, spec: String) -> napi::Result<String> {
        guard(|| {
            let spec: ViewSpec = parse("view spec", &spec)?;
            json(&self.inner.view(&spec)?)
        })
    }

    /// `edit`, with the JSON of an edit request. Returns the JSON of the `Receipt`.
    #[napi]
    pub fn edit(&mut self, request: String) -> napi::Result<String> {
        guard(|| {
            let request: EditRequest = parse("edit request", &request)?;
            let edit = build_edit(request, &self.limits)?;
            json(&self.inner.edit(edit)?)
        })
    }

    /// `save`: the package and the JSON of its receipt.
    #[napi]
    pub fn save(&self) -> napi::Result<NativeSaved> {
        guard(|| {
            let saved = self.inner.save()?;
            Ok(NativeSaved {
                receipt: json(&saved.receipt)?,
                bytes: saved.bytes.into(),
            })
        })
    }

    /// `inspect` of a ref or of `styles`, `numbering` or `sections`.
    #[napi]
    pub fn inspect(&self, target: String) -> napi::Result<String> {
        guard(|| json(&self.inner.inspect(&target)?))
    }

    /// `revisions`, with the JSON of a `RevisionFilter`.
    #[napi]
    pub fn revisions(&self, filter: String) -> napi::Result<String> {
        guard(|| {
            let filter: RevisionFilter = parse("revision filter", &filter)?;
            json(&self.inner.revisions(&filter)?)
        })
    }

    /// `lookup` of a defined term, a clause number or a cross-reference.
    #[napi]
    pub fn lookup(&self, query: String) -> napi::Result<String> {
        guard(|| json(&self.inner.lookup(&query)?))
    }

    /// `check` of the document as it is.
    #[napi]
    pub fn check(&self) -> napi::Result<String> {
        guard(|| json(&self.inner.check()))
    }

    /// `check_pending`, with the JSON of a check request.
    #[napi]
    pub fn check_pending(&self, request: String) -> napi::Result<String> {
        guard(|| {
            let request: CheckPendingRequest = parse("check request", &request)?;
            let stamp = match (request.author, request.date) {
                (Some(author), Some(date)) => Some(Stamp::new(author, date)),
                (None, None) => None,
                _ => {
                    return Err(StetError::new(
                        ErrorCode::InvalidPayload,
                        "A check request names an author and a date together, or neither.",
                    ));
                }
            };
            json(
                &self
                    .inner
                    .check_pending(request.pending, stamp, request.strict)?,
            )
        })
    }

    /// `files`: the readable files, as the JSON of `ReadableFiles`.
    #[napi]
    pub fn files(&self) -> napi::Result<String> {
        guard(|| json(&self.inner.files()?))
    }

    /// One command of Claude's text editor tool, with its `input` as the model sent it (JSON) and the JSON of the editor's
    /// options: the `Stamp` fields and an optional `path_prefix`. A `StetError` is the result's text, for the model, and not a
    /// thrown error; options that do not parse are thrown.
    #[napi]
    pub fn text_editor(
        &mut self,
        input: String,
        options: String,
    ) -> napi::Result<NativeToolResult> {
        guard(|| {
            let options: EditorOptions = parse("text editor options", &options)?;
            let stamp = Stamp::new(options.author, options.date);
            let response =
                stetdoc_editor::respond(&mut self.inner, &input, &stamp, &options.path_prefix);
            Ok(NativeToolResult {
                content: response.text,
                is_error: response.is_error,
                receipt: response.receipt.as_ref().map(json).transpose()?,
            })
        })
    }

    /// `export`: the working copy.
    #[napi]
    pub fn export(&self) -> napi::Result<NativeWorkingCopy> {
        guard(|| {
            let copy = self.inner.export()?;
            Ok(NativeWorkingCopy {
                files: json(&copy.files)?,
                complement: copy
                    .complement
                    .entries
                    .into_iter()
                    .map(|(name, bytes)| NativeEntry {
                        name,
                        bytes: bytes.into(),
                    })
                    .collect(),
            })
        })
    }
}

/// Turns an edit request into an `Edit`: exactly one source, then the builder's options.
fn build_edit(request: EditRequest, limits: &Limits) -> Result<Edit, StetError> {
    let stamp = Stamp::new(request.author, request.date);
    let mut edit = match (request.ops, request.batch, request.files) {
        (Some(ops), None, None) => {
            let Some(base) = request.base else {
                return Err(StetError::new(
                    ErrorCode::InvalidPayload,
                    "An edit of operations names its base token: pass base, such as doc.base().",
                ));
            };
            let text = json(&serde_json::json!({ "base": base, "ops": ops }))?;
            Edit::ops(Batch::parse(&text, limits)?, stamp)
        }
        (None, Some(text), None) => Edit::ops(Batch::parse(&text, limits)?, stamp),
        (None, None, Some(files)) => Edit::files(files, stamp),
        _ => {
            return Err(StetError::new(
                ErrorCode::InvalidPayload,
                "An edit takes exactly one of ops, batch and files.",
            ));
        }
    };
    if request.untracked {
        edit = edit.untracked();
    }
    if !request.scope.is_empty() {
        edit = edit.scope(request.scope);
    }
    if request.dry_run {
        edit = edit.dry_run();
    }
    Ok(edit.quotes(request.quotes).offsets(request.offsets))
}

/// `capabilities`, as JSON.
#[napi]
pub fn capabilities() -> napi::Result<String> {
    guard(|| json(&stetdoc_core::capabilities()))
}

/// The JSON Schema of every public type.
#[napi]
pub fn json_schema() -> napi::Result<String> {
    guard(|| Ok(stetdoc_core::json_schema()))
}
