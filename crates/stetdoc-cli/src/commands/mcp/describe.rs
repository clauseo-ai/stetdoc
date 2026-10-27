//! The rules each tool's description carries after the command's own line: the few a model needs to call the tool right
//! the first time, for clients that never pass the server's instructions on. The full guide stays in `skills_get`.

/// The rules of a tool, by its name, or nothing.
pub fn rules(tool: &str) -> &'static str {
    match tool {
        "unpack" => {
            "Only to edit by changing readable files: view, replace, comment and edit work on a .docx directly. Unpack \
             once per task and keep the folder; pack writes the .docx."
        }
        "pack" => {
            "Writes NAME.edited.docx, or output, and keeps the working copy up to date. Pass done on the last pack of \
             the task. On stale_target, unpack that file again rather than force."
        }
        "view" => {
            "Works on a .docx or a working copy. A page is about 3000 tokens: when the result has `next`, call again \
             with after set to it. A long body shows its outline: then view a range or clause. find lists one short \
             line per matching paragraph (ref, clause, the text around the match), the cheap way to locate text."
        }
        "edit" => {
            "Give the operations in ops (typed objects, the default) or a batch in batch (block text or JSON), not \
             both; all apply or none. Refs (p42, t3, r17, c3) come from a view. base is the base token of that view or \
             of the last receipt: required for a working copy, optional for a .docx, whose refs are then read from the \
             file as it is now. A .docx is written to NAME.edited.docx beside it, unless output names another file or \
             in_place writes it back (only for a file the user asked to update or this task wrote); dry_run writes \
             nothing. manual or manual_color marks the changes with strikethrough and underline, not tracked changes: \
             only when the user asks for that. To change or comment on text, replace and comment are simpler."
        }
        "replace" => {
            "Each old must be in the document exactly once, as the final view reads it, or nothing is written and the \
             error lists each place: use a longer old, narrow with clause or range, or pass all or expect. An empty new \
             deletes. A .docx is written to NAME.edited.docx beside it, unless output names another file or in_place \
             writes it back (only for a file the user asked to update or this task wrote). manual or manual_color \
             marks the changes with strikethrough and underline, not tracked changes: only when the user asks for that."
        }
        "comment" => {
            "anchor must be in the document exactly once, as for replace: use a longer one, or narrow with clause or \
             range. A .docx is written to NAME.edited.docx beside it, unless output names another file or in_place \
             writes it back. To reply to or resolve a comment, use edit."
        }
        "check" => "Run it on a working copy before pack to see what pack will write.",
        "skills_get" => {
            "core has the whole workflow and its rules, operations every field of each operation, errors what each \
             error code means and what to do."
        }
        _ => "",
    }
}
