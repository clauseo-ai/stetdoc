# 0015. Receipts warn when an edit moves a typed cross-reference or removes a definition in use

- **Date:** 2026-11-02
- **Status:** accepted on 2026-11-02

## Context

A bug report against 0.9.1 showed the most common way a redline of nested clauses goes wrong. Clause numbers such as
`16.2(b)` come from Word's numbering, so inserting or deleting a limb renumbers every limb after it. A reference to a limb
is typed text (`has the meaning given in Clause 16.2(b)`), so it does not move. In the master supply agreement of the
Harvey LAB evals, inserting a limb after p196 left the definition of "Cure Period" pointing at the new limb, and deleting
p197 left it pointing at the next one and removed the only real definition of a term that p198 and p199 still use. The
receipt said `validated 1 unit: no problems`, and `check` said `no violations`.

The index already resolved these mentions for `view` and `lookup`. Nothing compared where a mention resolved before an
edit with where it resolves after. The fix adds public types and warning codes, so this records it.

## Options considered

What a mention is compared with, after the edit.

| Option | Strength | Weakness |
|---|---|---|
| 1. The Final view of the starting state | It is what the reader of the document sees now, and what the agent read before it edited | Undoing a pending change (rejecting a deletion, deleting one's own insertion) moves the references back to where they were, and that reads as drift |
| 2. The Original view of the starting state | An undo puts a reference back, so it says nothing | Every reference that a pending change already moved is reported again on every later edit, even one in another clause |
| 3. Option 1, with accept and reject exempt | Undo by `reject` says nothing | The comparison is by mention and not by operation, so it cannot tell which operation moved a mention. A batch that rejects one change and inserts a limb would hide the insertion's drift, and deleting one's own insertion is not a `reject` |
| 4. Option 1, and a mention that the Original view of the starting state resolves to the same block is not drift | An undo says nothing, and a pending change is reported once, by the edit that makes it | A second index of the starting state, in the Original view, when there is a candidate |

## Decision

Option 4.

- **After every edit batch the receipt compares the cross-references.** That covers operations, an edit of files, a dry
  run, `pack`, and `check` of a working copy with pending hand edits, which dry-runs them. `list` operations renumber, and
  they go through the same check. The check (`document/drift.rs`) takes the resolved prose mentions of the starting state
  and resolves each again in the result, both over Final text and numbers. A mention is the ref of the citing block and
  the mention as written, such as `p32` and `Clause 16.2(b)`.
- **A target is a block, never a label.** Two targets are the same when they are the same block by ref, since the label
  is what drifts. A block the batch adds has no ref in a dry run, so every message calls it "a block this batch adds",
  in the dry run and in the real edit alike (law H14).
- **What the agent edited is not drift.** A mention whose citing block or text the batch changed is skipped.
- **An undo is not drift.** A mention that now names what it named in the Original view of the starting state, or is
  unresolved now and was unresolved there, is not reported.
- **Three warning codes.** `ref_retargeted`: the mention names another block now. `ref_unresolved`: it resolves to
  nothing now, or to several blocks. `definition_removed`: a term lost its last definition, and the Final text still
  uses it. A definition paragraph whose verb points elsewhere (`has the meaning given in Clause X`, `is defined in`) is a
  pointer: it does not stand in for a real definition, so a term whose real definition went while only pointers remain
  is reported too. The message names the blocks that still use the term and the blocks that cite the block that held
  the definition.
- **Warnings, never refusals.** A drafter may mean a reference to follow the new limb. The agent decides, and the
  warning's next step says to update the reference in the same batch or confirm that it is meant to move.
- **The head line counts them,** as in `3 cross-reference warnings below (2 ref_retargeted, 1 definition_removed).`,
  so a receipt cut by `Receipt::text_within` still says they are there. The text shows at most 8 and then says how many
  more there are. The tail of a receipt is never cut, so a receipt with 8 such warnings can be longer than the 2 KB of
  the MCP tools' receipts. The JSON is complete: `Receipt.warnings` holds every warning.
- **New public items.** `RefDrift` (`source`, `text`, `before`, `after`) and `RefTarget` (`ref`, `label`) in
  `api/receipt.rs`, re-exported from `lib.rs`, and `Receipt.ref_drift`, one entry for each `ref_retargeted` and
  `ref_unresolved` warning, in the same order. `after` is null when the mention resolves to nothing or to several blocks,
  and `ref` is null for a block the batch adds. The field is left out of the JSON when it is empty. `WarningCode` gains
  `RefRetargeted`, `RefUnresolved` and `DefinitionRemoved`. They are receipt types and a warning code, which decision
  0004 lists as public by kind, so its list needs no change.
- **The names come from the bug report.** `ref_retargeted`, `ref_unresolved` and `ref_drift` say "ref" for a
  cross-reference, while the glossary keeps "ref" for a block reference. A rename before 1.0 is open.

## Consequences

- An edit that renumbers a list warns about each typed reference that now names another limb, and about a definition it
  took out from under a term still in use. The bug report's two repros give exactly the warnings it asked for.
- A receipt without such a warning is byte for byte as it was, and the JSON gains no field.
- The cost, for a batch that changed something: an index and edge pass over the starting state and one over the result.
  The index and the edges of the Original view are built only when a candidate exists, and the walk for the uses of a
  term only when a definition went. The check stops after the first pass when the starting state has no resolved
  mention and no defined term.
- Out of scope: field `REF` cross-references, which follow their bookmark and whose cached result stetdoc does not
  update, and mentions that were relative or ambiguous before the edit. `compare` receipts leave `ref_drift` empty.
- A caller that matches `WarningCode` exhaustively needs a wildcard arm, as it is `non_exhaustive`.
