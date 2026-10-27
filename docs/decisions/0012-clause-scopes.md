# 0012. A clause scope names one clause or several

- **Date:** 2026-10-27
- **Status:** accepted on 2026-10-27

## Context

An agent reading a credit agreement wanted two neighbouring sections side by side and ran
`stetdoc view docs/credit-agreement.docx --clause 4.01 --clause 4.02`. clap refused it ("the argument '--clause
<NUMBER>' cannot be used multiple times"), so the agent had to make two calls or guess a range of refs. `replace` and
`comment` take `--clause` too, with the same limit. `Scope::Clause` held one string, and `lib.rs` re-exports `Scope`, so
taking several clauses changes the public interface, and this records it.

## Decision

- **`Scope::Clause(Vec<String>)`** replaces `Scope::Clause(String)`. Each string is a clause as before (`clause 4.2(a)`,
  `4.2(a)`, `Article III`). JSON reads one string, `{"clause": "11.1"}`, or a list, `{"clause": ["11.1", "12.3"]}`, as
  `Scope::Find` reads its needles, and writes a list of one as a string, so a scope written before this change reads the
  same and writes back the same. The schema, and with it the Node types, is `string | string[]`.
- **One clause is unchanged.** It resolves, selects, labels and pages exactly as it did, through the same function,
  with no extra work: the only difference is that its focus is held in a list of one.
- **Several clauses are a union.** Each clause resolves as it would alone; one that is unknown, ambiguous or not a
  clause fails the whole view with the error it would give alone, which names that clause. The blocks of all of them
  are put in the order of the stories and of the document, and each block is kept once, so a clause and a clause inside
  it (4.01 and 4.01(a)) show the outer one's blocks, and the same clause named twice shows once. An empty list is
  `invalid_payload`.
- **The label names the clauses** in document order: `clause 4.01` for one, `clauses 4.01, 4.02` for several, each
  once. The position line reads `{position: p216..p222 of clauses 4.01, 4.02 (7 of 76 blocks) ...}`.
- **Every clause named is a focus.** The focus of a clause view is the block under which the view writes what cites the
  clause (`{~ cited by p139}`). With several clauses the caller asked about each of them, so each clause's block gets its
  own "cited by" note, a nested one included. Picking only the first would hide what cites the others, and none would
  lose what one clause shows today. Internally `Selected::focus` became a list, empty for the other scopes.
- **No gap marker.** The blocks between two clauses are left out silently, as in a range: the refs on each line show
  the jump, and the page stays compact. A find, whose hits are scattered, keeps its `{~ N blocks not shown}` lines.
- **Paging** works as for one clause: a cursor names the last block shown, and `--after` finds it in the union.
- The CLI's `--clause` on `view`, `replace` and `comment` may be given more than once. The MCP tools derive their schema
  from the flags, so `clause` there is an array of strings, and a lone string still works. `Document::occurrences`
  takes the same scope, so `replace` and `comment` look in the union and say `in clauses 4.01, 4.02` in their messages.

## Consequences

- A Rust caller that built `Scope::Clause("11.1".into())` now writes `Scope::Clause(vec!["11.1".into()])`, and one that
  matched on it gets a list. JSON and Node callers need nothing.
- A view of several clauses in different stories puts the stories in the order of the story list, as the other scopes
  do.
