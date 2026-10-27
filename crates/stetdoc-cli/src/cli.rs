//! The command tree. Every command and flag is declared here, and its doc comment is its `--help` text, so the help, the
//! skill's command list and the CLI reference come from one source.

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Read and edit Word documents as text, and write the edits back as real tracked changes.
///
/// stetdoc turns a .docx into a working copy of plain text files. Edit them with any tool, check the redline, and pack:
/// everything you did not touch stays byte-identical, and every change is a Word revision with an author and a date.
#[derive(Debug, Parser)]
#[command(
    name = "stetdoc",
    version,
    disable_help_subcommand = true,
    max_term_width = 100
)]
pub struct Cli {
    /// Print the result, or the error, as JSON on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The twelve commands. The last, `mcp`, serves the others as tools.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Write a working copy of a .docx: readable files beside a .stet/ folder that you never touch.
    Unpack(UnpackArgs),
    /// Compile the edits of a working copy into tracked changes, validate them and write the .docx.
    Pack(PackArgs),
    /// Show a document or a working copy: an outline of a large document, or a range, a search, the revisions, the comments.
    View(ViewArgs),
    /// Show the effective formatting of a block and where each value comes from, or the styles, lists or sections.
    Inspect(InspectArgs),
    /// Apply a batch of operations, read from stdin or --ops, to a working copy, or to a .docx with -o or --in-place.
    Edit(EditArgs),
    /// Write a redline of the differences between two documents as tracked changes.
    Compare(CompareArgs),
    /// Validate, and show every revision that pack would write, by author.
    Check(CheckArgs),
    /// Find a defined term, a cross-reference, or what refers to a clause.
    Lookup(LookupArgs),
    /// Print the agent skill embedded in this binary.
    Skills(SkillsArgs),
    /// Install the skill stub for the agents you pick, in your home folder or (with --project) in a project.
    Init(InitArgs),
    /// List the operations, whether each is tracked, the error codes and the format version.
    Capabilities(CapabilitiesArgs),
    /// Serve the commands as tools over the Model Context Protocol, on stdin and stdout.
    Mcp(McpArgs),
}

/// Who is editing, and when.
#[derive(Debug, Args)]
pub struct StampArgs {
    /// The author of the revisions, as Word shows it. Defaults to STETDOC_AUTHOR, then the name saved with stetdoc init.
    /// There is no other default.
    #[arg(long, value_name = "NAME")]
    pub author: Option<String>,
    /// The date of the revisions, YYYY-MM-DDTHH:MM:SS. Defaults to STETDOC_DATE, then the local clock.
    #[arg(long, value_name = "DATE")]
    pub date: Option<String>,
}

/// `stetdoc unpack`.
#[derive(Debug, Args)]
pub struct UnpackArgs {
    /// The .docx to unpack, or with --refresh a working copy to render again.
    pub input: PathBuf,
    /// The working copy to write. Defaults to NAME.stet beside the .docx.
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,
    /// Render every readable file of an existing working copy again, discarding hand edits that were not packed.
    #[arg(long)]
    pub refresh: bool,
}

/// `stetdoc pack`.
#[derive(Debug, Args)]
pub struct PackArgs {
    /// The working copy.
    pub dir: PathBuf,
    /// The .docx to write. Defaults to NAME.edited.docx beside the working copy. An existing file is overwritten only when it
    /// is the document the working copy came from or one it packed.
    #[arg(short, long, value_name = "DOCX")]
    pub output: Option<PathBuf>,
    /// Overwrite the output even when it holds a document this working copy neither came from nor packed.
    #[arg(long)]
    pub force: bool,
    /// Write the edits as plain changes, not tracked ones. Only for documents that are not under review.
    #[arg(long)]
    pub no_track: bool,
    /// Remove the working copy once the .docx is written and validated. When the pack fails, the working copy stays. Files
    /// that stetdoc did not write stay too, and so does the folder that holds them.
    #[arg(long)]
    pub done: bool,
    #[command(flatten)]
    pub stamp: StampArgs,
}

/// `stetdoc view`.
#[derive(Debug, Args)]
#[command(group(ArgGroup::new("scope").args(["outline", "range", "clause", "revisions", "comments", "story"])))]
pub struct ViewArgs {
    /// A .docx or a working copy. A working copy shows its last export, not edits that were not packed.
    pub input: PathBuf,
    /// The headings only.
    #[arg(long)]
    pub outline: bool,
    /// With --outline, the deepest heading level shown.
    #[arg(long, value_name = "N", requires = "outline")]
    pub depth: Option<u8>,
    /// The blocks from one ref to another, such as p40..p60, or from a ref to the end of its story.
    #[arg(long, value_name = "REFS")]
    pub range: Option<String>,
    /// The block a clause number resolves to, with the blocks under it, such as 12.3.
    #[arg(long, value_name = "NUMBER")]
    pub clause: Option<String>,
    /// The blocks that contain this text, with their neighbours, paged like a range. Case-insensitive unless it has a
    /// capital letter. With --range, only in that range; with --revisions, only the changes whose text contains it.
    #[arg(long, value_name = "TEXT", conflicts_with_all = ["outline", "clause", "comments", "story"])]
    pub find: Option<String>,
    /// With --find, how many blocks to show on each side of a match.
    #[arg(long, value_name = "N", requires = "find", default_value_t = 1)]
    pub context: u8,
    /// The tracked changes, one line each: their ids, kind, author, day, block and text, cut to 80 characters.
    #[arg(long)]
    pub revisions: bool,
    /// With --revisions, the whole inserted and deleted text of each change (up to 1,000 characters), not 80.
    #[arg(long, requires = "revisions")]
    pub full: bool,
    /// The comment threads.
    #[arg(long)]
    pub comments: bool,
    /// With --revisions or --comments, only those by this author (a name or the code the view shows).
    #[arg(long, value_name = "NAME")]
    pub author: Option<String>,
    /// With --revisions, only this kind: insert, delete, move or format.
    #[arg(long, value_name = "KIND", requires = "revisions")]
    pub kind: Option<String>,
    /// With --comments, only resolved threads.
    #[arg(long, requires = "comments", conflicts_with = "open")]
    pub resolved: bool,
    /// With --comments, only open threads.
    #[arg(long, requires = "comments")]
    pub open: bool,
    /// One story whole.
    #[arg(long, value_enum, value_name = "STORY")]
    pub story: Option<StoryArg>,
    /// How tracked changes appear.
    #[arg(long, value_enum, default_value_t = ModeArg::Redline)]
    pub mode: ModeArg,
    /// The size of a page, in estimated tokens (about 4 characters each). 0 shows everything.
    #[arg(long, value_name = "TOKENS", default_value_t = stetdoc_core::Budget::DEFAULT)]
    pub budget: u32,
    /// Everything on one page, however long: the same as --budget 0, and it wins over --budget.
    #[arg(long)]
    pub all: bool,
    /// Continue after this cursor, from the last line of the previous page.
    #[arg(long, value_name = "CURSOR")]
    pub after: Option<String>,
    /// Leave out the margin notes and the appendix of the notes and comments the page cites.
    #[arg(long)]
    pub no_notes: bool,
}

/// A story, by the name an agent uses.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum StoryArg {
    /// The body.
    Body,
    /// Footnotes and endnotes.
    Notes,
    /// Headers and footers.
    Headers,
    /// Comments.
    Comments,
}

/// How tracked changes appear in a view.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ModeArg {
    /// Insertions in, deletions out.
    Final,
    /// The document before any tracked change.
    Original,
    /// Both, marked.
    Redline,
}

/// `stetdoc inspect`.
#[derive(Debug, Args)]
pub struct InspectArgs {
    /// A .docx or a working copy.
    pub input: PathBuf,
    /// A ref (p42, t3, r17), or styles, numbering or sections.
    pub target: String,
}

/// `stetdoc edit`.
#[derive(Debug, Args)]
pub struct EditArgs {
    /// A working copy, or a .docx with -o or --in-place.
    pub input: PathBuf,
    /// The batch, as block text or JSON. Defaults to stdin; - is stdin too.
    #[arg(long, value_name = "FILE")]
    pub ops: Option<PathBuf>,
    /// With a .docx, the .docx to write. It must not be the input: --in-place writes back to it.
    #[arg(short, long, value_name = "DOCX")]
    pub output: Option<PathBuf>,
    /// With a .docx, write the result back to it, atomically, and only if no one changed it during the edit. A link is
    /// written through, a hard link is broken, the permissions are kept and the owner is not; a read-only file is refused.
    /// A working copy needs no flag: an edit always writes it.
    #[arg(long)]
    pub in_place: bool,
    /// Write plain changes, not tracked ones. Only for documents that are not under review.
    #[arg(long)]
    pub no_track: bool,
    /// Validate and show the receipt, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Refuse the whole batch if it changes anything outside these refs and ranges, such as p12,p40..p60.
    #[arg(long, value_name = "REFS", value_delimiter = ',')]
    pub scope: Vec<String>,
    /// Follow the paragraph's quotes (match), or keep them as typed.
    #[arg(long, value_enum, default_value_t = QuotesArg::Match)]
    pub quotes: QuotesArg,
    /// What offsets count: Unicode scalar values, or UTF-16 units as JavaScript does.
    #[arg(long, value_enum, default_value_t = OffsetsArg::Scalar)]
    pub offsets: OffsetsArg,
    #[command(flatten)]
    pub stamp: StampArgs,
    /// The batch's text when it comes from somewhere other than stdin or --ops: the MCP server's `batch` argument.
    #[arg(skip)]
    pub batch: Option<String>,
}

/// What an edit does with quotes.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum QuotesArg {
    /// Follow the paragraph's convention, and say so in the receipt.
    Match,
    /// Keep every character as typed.
    AsTyped,
}

/// The unit of offsets.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OffsetsArg {
    /// Unicode scalar values.
    Scalar,
    /// UTF-16 code units.
    Utf16,
}

/// `stetdoc compare`.
#[derive(Debug, Args)]
pub struct CompareArgs {
    /// The original document.
    pub original: PathBuf,
    /// The revised document.
    pub revised: PathBuf,
    /// The redline to write.
    #[arg(short, long, value_name = "DOCX")]
    pub output: PathBuf,
    #[command(flatten)]
    pub stamp: StampArgs,
}

/// `stetdoc check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// A working copy or a .docx.
    pub input: PathBuf,
    /// Fail (exit 5) on any violation, including ones already in the input.
    #[arg(long)]
    pub strict: bool,
    /// List every finding the report keeps (the first 200), not only the first 20.
    #[arg(long)]
    pub all: bool,
    #[command(flatten)]
    pub stamp: StampArgs,
}

/// `stetdoc lookup`.
#[derive(Debug, Args)]
pub struct LookupArgs {
    /// A .docx or a working copy.
    pub input: PathBuf,
    /// A ref, a clause number or a defined term.
    pub query: String,
}

/// `stetdoc skills`.
#[derive(Debug, Args)]
pub struct SkillsArgs {
    /// What to do with the skill.
    #[command(subcommand)]
    pub action: SkillsAction,
}

/// `stetdoc skills get`.
#[derive(Debug, Subcommand)]
pub enum SkillsAction {
    /// Print a skill: core (the default), operations or errors.
    Get {
        /// The skill's name.
        name: Option<String>,
        /// Print every part of the skill.
        #[arg(long)]
        full: bool,
    },
}

/// `stetdoc init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Install into a project instead of your home folder, and write the AGENTS.md block there.
    #[arg(long)]
    pub project: bool,
    /// With --project, the project's folder. Defaults to the current directory.
    #[arg(long, value_name = "DIR", requires = "project")]
    pub dir: Option<PathBuf>,
    /// The agents to install for, such as claude-code,codex, or all. Without it, init asks in a terminal and otherwise
    /// installs for the agents it detects.
    #[arg(long, value_name = "AGENT", value_delimiter = ',')]
    pub agent: Vec<String>,
    /// List the agents, whether each is detected, its skill folder, and the stetdoc version installed there.
    #[arg(long, conflicts_with_all = ["remove", "mcp"])]
    pub list: bool,
    /// Remove the skill folders stetdoc wrote, and with --project and no --agent the AGENTS.md block. Anything else is left
    /// alone.
    #[arg(long, conflicts_with = "mcp")]
    pub remove: bool,
    /// Also print, for each agent, the command or config snippet that registers `stetdoc mcp`. Nothing is edited.
    #[arg(long)]
    pub mcp: bool,
    /// Save NAME as the author of your revisions in the config file. Without it, init asks in a terminal when no name is
    /// saved, suggesting your git user.name.
    #[arg(long, value_name = "NAME", conflicts_with_all = ["list", "remove"])]
    pub author: Option<String>,
    /// List what would be written or removed, and change nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Replace a SKILL.md that is not an unchanged stetdoc stub, or is a link. Without it, init refuses and changes nothing.
    #[arg(long)]
    pub force: bool,
}

/// `stetdoc capabilities`.
#[derive(Debug, Args)]
pub struct CapabilitiesArgs {
    /// Print the JSON Schema of every public type instead.
    #[arg(long)]
    pub schema: bool,
}

/// `stetdoc mcp`.
#[derive(Debug, Args)]
pub struct McpArgs {
    /// Confine every path argument to this folder, and resolve relative paths against it. Without it, relative paths
    /// resolve against the current directory and any path is allowed.
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,
    /// The author of the revisions of every call that writes them and names none, ahead of STETDOC_AUTHOR and the name
    /// saved with stetdoc init.
    #[arg(long, value_name = "NAME")]
    pub author: Option<String>,
}
