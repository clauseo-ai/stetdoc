# 0014. Repair undeclared markup-compatibility prefixes on every write

- **Date:** 2026-10-30
- **Status:** accepted on 2026-10-30

## Context

A Harvey LAB input, the issuer draft indenture of the `long-indenture-markup` task, does not open in Word for Mac 16.114:
Word offers to recover it and then says it cannot open it. The root of its `word/document.xml` is

```xml
<w:document xmlns:mc="…" xmlns:r="…" xmlns:w="…" mc:Ignorable="w14 wp14">
```

and nothing declares `w14` or `wp14`. ECMA-376 Part 3 requires every prefix that `mc:Ignorable`, `mc:ProcessContent`
or `mc:MustUnderstand` names to be declared in scope, and Word refuses a part where one is not. Removing the attribute,
or declaring the two prefixes, makes the file open. The generator (python-docx rewriting a template's body) kept the
attribute and dropped the declarations. The other parts of the package declare both prefixes.

The schema allows the attribute and its value, so stetdoc's validator does not see the defect. The Open XML SDK reports it
(`MC_InvalidIgnorableAttribute`), but a comparison of output against input counts only errors the output adds, so it
called stetdoc's output of this input valid ("new 0") while Word refused it, as it refuses the input. 5 of the 300
documents of the curated corpus have the defect, all of them in the main part, all of them listing `w14` and `wp14`.

stetdoc's locality law (L2) says that an edit changes nothing outside what it touches, apart from the root start tag of a
part it touched, which may gain the declarations that new content needs. It does not let a write change a part the edit
did not touch. Passing the defect through faithfully gave the user a file Word will not open.

## Decision

- **Every write repairs the defect** in every part the check reads (the main part and the parts its relationships name
  with a schema: styles, numbering, settings, notes, headers, footers, comments, people, font table, web settings,
  glossary), whether or not the edit touched the part. Without the repair the file is no use in Word, and a defect that
  stops Word opening the file is not content the author meant to keep. The repair runs after `check::edit` and the scope
  guard, inside the transaction, so a dry run shows it and an edit scoped to one clause still makes it.
- **What the repair changes, and nothing else.** On each element whose `mc:Ignorable`, `mc:ProcessContent` or
  `mc:MustUnderstand` names an undeclared prefix, a prefix Word knows is declared on that element, after its last
  namespace declaration, so the rest of the start tag keeps its bytes. The namespace is the one Word declares under that
  prefix (a short list in `schema/mc_prefixes.rs`, from [MS-DOCX] and the Word-authored roots of the corpus, which wins
  where the schema tables' preferred prefix differs: Word writes `w16` and the tables say `w16cur`, and Word numbers the
  2016 chart namespaces one higher), else the namespace whose preferred prefix in the schema tables it is. A prefix that
  neither knows is taken out of the attribute, which may leave the attribute empty: there is no namespace to declare,
  and an undeclared prefix never meant anything to a consumer.
- **It runs only when the edit changes something.** An edit that changes nothing, and an unedited document, save the
  input bytes exactly, so law L1 is unchanged.
- **The receipt says so** with a new warning code, `repaired`, one warning for each part:
  `warning repaired: declared w14, wp14 in word/document.xml so Word can open the file. ...`. `WarningCode::Repaired` is
  public interface, which is why this is a decision record.
- **`check` reports the defect** on any document with a new finding code, `mc_prefix_undeclared`, a violation, so
  `stetdoc check` of the input says that Word will not open it. Like every problem of the input, it is reported and never
  blocks an edit, and after any edit it is gone.
- **`check` lists it first** (amended on 2026-10-30). A report lists the findings that make Word refuse the file or
  offer to repair it before every other finding, in a quota of their own, so that neither the 200 findings a report keeps
  nor the 20 lines `stetdoc check` prints without `--all` can hide them behind the part's schema findings. The finding
  sat after those and was missed on the Harvey LAB inputs. Its message names the prefixes, and says that stetdoc declares
  the ones Word knows and removes the others when it writes the document. `mc_prefix_undeclared` is the only such code
  today (`check::report::word_refuses`).
- **L2 is amended** for this defect class only: a write may change the start tags of the elements that name undeclared
  prefixes, in any part the check reads, by the declarations and token removals above. The testkit's entry-level check
  of L2 lets the font table and web settings change for this reason; every other part it allows already.

## Consequences

- A file that Word refused before an edit opens after it. The indenture of the bug, edited as the Haiku run edited it,
  opens cleanly in Word for Mac, and the SDK's two `MC_InvalidIgnorableAttribute` errors are gone.
- Byte preservation of untouched parts has one more exception. It is narrow: a start tag gains declarations or loses
  tokens of three attributes, and only where the input was broken in a way Word refuses.
- Each edit walks the elements of every part the check reads once to look for the defect. The result is cached per part
  generation, so a long-lived document walks only the parts an edit changed.
- Other defects that the schema allows and Word refuses may join this class later. Each needs evidence from Word that
  it blocks opening, a repair that changes as little, and a line in this record or a new one.
- A caller that matches `WarningCode` or `FindingCode` exhaustively needs a wildcard arm, as both are `non_exhaustive`.
