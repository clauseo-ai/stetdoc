# 0005. Word for the oracle tests: Word for the web by hand, and LibreOffice by script

- **Date:** 2026-10-09
- **Status:** accepted on 2026-10-09

## Context

The release checks asked for an oracle suite run against desktop Word (Word for Mac driven through AppleScript) and for 500 stetdoc
outputs opened in Word without a repair prompt. The development Mac has no Microsoft Office, and no Microsoft 365
trial was started. The free options were:

- **Word for the web**, free with a Microsoft account. It is real Word, but it cannot be driven by AppleScript and its
  repair behaviour differs from desktop Word, so it suits a hand check, not 500 automated opens.
- **LibreOffice**, free and scriptable headless. It is an independent reader of the format, but it is more lenient
  than Word and has its own gaps with tracked changes, so it cannot stand in for Word alone.

## Decision

The Word check has two parts:

1. **A hand check in Word for the web.** An ignored test (`crates/stetdoc-testkit/tests/word_pack/`) writes a pack of
   redlines built on fixture documents, one per case of the Word checklists in the design docs, with `CHECKLIST.md`
   giving the text that Accept All and Reject All must produce. A maintainer opens each in Word for the web and records
   whether it opens without an error or repair message, whether the changes show with their author, and whether
   Accept All and Reject All give the expected text. Run before each release.
2. **LibreOffice by script.** `cargo xtask oracle libreoffice --sample 500` builds a seeded, stratified sample of 500
   outputs from the curated corpus and reads each in headless LibreOffice. Every output must load and convert to PDF,
   and its accept-all and reject-all text must match stetdoc's Final and Original views, unless the mismatch is a
   documented LibreOffice deviation with a minimal reproduction.

The oracle suite of exact revision shapes (Word's own XML after the same edit) is not run. It returns if desktop Word
becomes available (a Microsoft 365 licence, a self-hosted Mac runner or a Windows machine).

## Consequences

- The Word check passes when every pack file passes the hand check and the LibreOffice run has no unexplained mismatch.
- What desktop Word alone would catch, such as a repair prompt that Word for the web does not show, or a revision shape
  that both Word for the web and LibreOffice accept but desktop Word rewrites, is a residual risk. `FIDELITY.md` and
  the release notes say so.
- The pack and the LibreOffice sample are rebuilt from code, so they never hold corpus documents in the repository.
