//! What the MCP tools say about themselves: each tool's description and each property's, in the few words a model needs to
//! call it right the first time, for clients that never pass the server's instructions on. Every word here is sent on
//! every turn of every conversation that lists the tools, so the detail stays in `skills_get` and in the refusals, which
//! say what to do next. The CLI's `--help` keeps its longer texts; a property this file does not name falls back to them.
//!
//! The descriptions steer an agent to the direct path, which is also the cheapest: one `replace`, `comment` or `edit` call
//! on the .docx itself, all or nothing, which validates what it writes. No unpack, pack, check or dry run is needed.

/// The description of a tool, by its name, or `None` for the command's own line.
pub fn tool(name: &str) -> Option<&'static str> {
    Some(match name {
        "unpack" => {
            "Write a working copy of a .docx. Not needed to edit: replace, comment and edit change a \
             .docx in one call. Unpack only to edit the files with your own tools, then pack."
        }
        "pack" => {
            "Write a working copy's edits into the .docx as tracked changes, validated. On \
             stale_target, unpack again rather than force."
        }
        "view" => {
            "Read a .docx: pages of about 3000 tokens (pass after: the page's next), the outline of a long body, a \
             range, clauses, revisions, comments, or find: a line per matching paragraph with its ref, the cheap way \
             to locate text. Use one of outline, range, clause, revisions, comments, story; find also goes with range, \
             story or revisions."
        }
        "inspect" => {
            "Show a block's effective formatting and where it comes from, or the styles, numbering or sections."
        }
        "edit" => {
            "Change a .docx in one call: all its operations and comments together, applied all or none. A refused \
             call changes nothing and says why, so never dry-run first. Refs (p42, t3, c3) come from view. A .docx \
             needs no base, unpack, pack or check: it goes to NAME.edited.docx unless output or in_place says where, \
             and it is validated as written. Op fields: skills_get name operations. For text changes or comments, \
             replace and comment need no refs."
        }
        "replace" => {
            "Replace text in a .docx as tracked changes in one call, no refs needed: put every change in pairs. Each old \
             must be in the document once, or nothing is written and the error lists each place: retry with all: true \
             for every one, a range, or a longer old. Empty new deletes. Goes to NAME.edited.docx unless output or \
             in_place; it is validated as written, so no check or dry run is needed."
        }
        "comment" => {
            "Comment on text in a .docx in one call, no refs needed: anchor and text for one, comments for several, all \
             or none. Each anchor must be there once, as for replace (all: true for every one). Goes to \
             NAME.edited.docx unless output or in_place. To reply or resolve, use edit."
        }
        "compare" => "Write a redline of two documents as tracked changes.",
        "check" => {
            "Only for a .docx stetdoc did not write, or hand edits of a working copy: stetdoc validates what it writes."
        }
        "lookup" => "Find a defined term, a cross-reference, or what refers to a clause.",
        "skills_get" => {
            "The guide: core (workflow, rules), operations (op fields), errors (what each code means)."
        }
        _ => return None,
    })
}

/// Whether a flag is left out of a tool: `offsets` serves programs that count in UTF-16, as JavaScript does, and an
/// agent counts characters. The CLI keeps it.
pub fn hidden(tool: &str, name: &str) -> bool {
    matches!((tool, name), ("edit", "offsets"))
}

/// The description of a property of a tool, or `None` for the flag's help. A text here is used as it is, without the
/// `Cannot be used with` the schema otherwise adds, so a tool whose properties go in groups says so in its description.
pub fn property(tool: &str, name: &str) -> Option<&'static str> {
    let own = match (tool, name) {
        ("unpack", "input") => Some("The .docx, or with refresh a working copy."),
        ("unpack", "output") => {
            Some("The working copy to write, instead of NAME.stet beside the .docx.")
        }
        ("unpack", "refresh") => Some("Render a working copy again, dropping unpacked hand edits."),
        ("pack", "dir") => Some("The working copy."),
        ("pack", "done") => Some("Remove the working copy once the .docx is written."),
        ("pack", "force") => Some("Overwrite an output this working copy did not write."),
        ("pack" | "edit" | "replace" | "comment", "output") => {
            Some("The .docx to write, instead of NAME.edited.docx.")
        }
        ("compare", "original") => Some("The original document."),
        ("compare", "revised") => Some("The revised document."),
        ("compare", "output") => Some("The redline to write."),
        ("check", "all") => Some("List up to 200 findings, not 20."),
        ("check", "strict") => Some("Fail on any violation, old ones too."),
        ("inspect", "target") => Some("A ref (p42, t3, r17), or styles, numbering or sections."),
        ("lookup", "query") => Some("A ref, a clause number or a defined term."),
        ("skills_get", "name") => Some("core (the default), operations or errors."),
        ("skills_get", "full") => Some("Every part of the skill."),
        ("view", "input") => Some("The .docx, or a working copy."),
        ("view", "after") => Some("The next of the previous page."),
        ("view", "all") => Some("Everything on one page."),
        ("view", "author") => Some("With revisions or comments: only this author."),
        ("view", "budget") => Some("The page size in tokens; 0 is everything."),
        ("view", "clause") => Some("The blocks of these clauses, such as 12.3."),
        ("view", "comments") => Some("The comment threads."),
        ("view", "context") => Some("With find or revisions: whole blocks, with N on each side."),
        ("view", "depth") => Some("With outline: the deepest level."),
        ("view", "find") => {
            Some("Paragraphs holding any of these texts, one line each with its ref.")
        }
        ("view", "full") => Some("With revisions: the whole changed text."),
        ("view", "kind") => Some("With revisions: insert, delete, move or format."),
        ("view", "mode") => Some("How tracked changes show."),
        ("view", "no_notes") => Some("Leave out the margin notes."),
        ("view", "open") => Some("With comments: only open threads."),
        ("view", "resolved") => Some("With comments: only resolved threads."),
        ("view", "outline") => Some("The headings only."),
        ("view", "range") => Some("The blocks from one ref to another, such as p40..p60."),
        ("view", "revisions") => Some("The tracked changes, one line each."),
        ("view", "story") => Some("One story whole, or with find, only its matches."),
        ("edit", "ops_file") => Some("A file holding the batch, instead of ops."),
        ("edit", "scope") => {
            Some("Refuse the batch if it changes anything outside these refs, such as p40..p60.")
        }
        ("replace", "all") => Some("Replace every occurrence of each old."),
        ("comment", "all") => Some("Comment on every occurrence of each anchor."),
        ("replace" | "comment", "expect") => Some("Every occurrence, only if there are exactly N."),
        _ => None,
    };
    own.or(match name {
        "input" => Some("The .docx, or a working copy."),
        "author" => Some("The author, if not the server's."),
        "date" => Some("YYYY-MM-DDTHH:MM:SS."),
        "in_place" => Some(
            "Write back to the input: only a file the user asked to update or this task wrote.",
        ),
        "dry_run" => Some("Write nothing. Rarely needed: a refusal writes nothing."),
        "quotes" => Some("Quotes."),
        "manual" => Some("Strike and underline, not tracked: only if the user asks."),
        "manual_color" => Some("As manual, in red and blue."),
        "no_track" => Some("Untracked: only for a document not under review."),
        "clause" => Some("Look only in these clauses, such as 11.1."),
        "range" => Some("Look only in these blocks, such as p40..p60."),
        "ignore_case" => Some("Match whatever the case."),
        _ => None,
    })
}
