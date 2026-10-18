# AGENTS.md

stetdoc projects a `.docx` into a working copy an agent can edit, and compiles the edits
back into the same package with real Word tracked changes. This file is the entry point
for agents and is just as useful to people. `CLAUDE.md` is a symlink to it.

Read in this order: [`README.md`](README.md), this file, [`ARCHITECTURE.md`](ARCHITECTURE.md),
[`GLOSSARY.md`](GLOSSARY.md), [`docs/decisions/`](docs/decisions/0001-core-interface.md), then
[`CONTRIBUTING.md`](CONTRIBUTING.md) and [`SECURITY.md`](SECURITY.md). Use the glossary's words
verbatim in code, docs and messages.

## Commands

```
cargo xtask check --quick    inner loop: format, static checks, clippy and tests for changed crates
cargo xtask check            what CI's check job runs, not its fuzz or bench jobs; run it before you report or push
cargo xtask docs [--check]   regenerate generated docs (references, the CLI reference, the skill stub); --check fails if stale
cargo xtask codegen [--check]  regenerate the schema tables from the pinned input; --check fails if stale
cargo xtask corpus fetch     fetch, verify by SHA-256 and cache the curated corpus (--full: all of it; --pairs: every compare pair)
cargo xtask fuzz <target>    fuzz `package` or `xml` (--time S; CI: 60); needs cargo-fuzz and a pinned nightly
cargo xtask bench            time the engine and measure peak memory against the budgets in ARCHITECTURE.md (--ci, --update-baseline)
cargo xtask release check    the npm packages and .github/workflows/release.yml agree with the workspace
cargo insta test -p <crate> --review    run a crate's tests and review snapshot changes
```

The toolchain is pinned in `rust-toolchain.toml`, and rustup installs it on first use. The
check needs `cargo-deny` (`cargo install cargo-deny --locked --version 0.20.2`), and review
needs `cargo install cargo-insta --locked`. A missing tool fails the check, it never skips.
Every failure names the file and line and says how to fix it. The check builds `xtask`
without the core, so a broken core still gets every static check, plus the compiler's errors.

## Map

```
crates/stetdoc-core/     the engine, behind every surface. Modules are listed in ARCHITECTURE.md
crates/stetdoc-cli/      the `stetdoc` binary (thin)
crates/stetdoc-napi/     Node-API bindings (thin; the only crate that may use unsafe)
crates/stetdoc-editor/   Claude's text editor tool; depends only on the core's public interface
crates/stetdoc-testkit/  corpus loaders, fixture builder, generators, conformance suites, oracles
xtask/                   check, docs, codegen, corpus fetch
corpus/                  the corpus manifest (pinned source, SHA-256, tier, expected outcome, licence); no documents
docs/                    user guides, design/, decisions/, reference/ (generated)
```

[`ARCHITECTURE.md`](ARCHITECTURE.md) has the code map, the allowed dependencies between
modules, the cross-cutting rules and a "where do I change X" table. Each core module's
`mod.rs` opens with its contract: what it owns, its interface, its invariants, what it
may depend on and where to start reading.

## Do not touch

- **`generated/` directories.** Never edit them by hand. Change the input and run
  `cargo xtask codegen`.
- **Released format adapters** (`stet/1` and later). Old working copies must still open.
- **The meaning of an existing error code or its exit class.** Codes are frozen from
  1.0. Adding a code is fine (see "Where do I change X").
- **A corpus source without a `corpus/SOURCES.md` entry.** Add the entry in the same change.
- **The public interface without a decision record.** A change to what `lib.rs`
  re-exports needs a new file in `docs/decisions/`.
- **The Allowed dependencies table without the code.** Change them together, and say why.

## Rules the checks enforce

- Modules depend on each other only as the Allowed dependencies table says. Do not alias
  or glob the crate root. `lib.rs` holds only docs, `mod x;`, `pub use` and, for fuzzing, `pub mod fuzz;`.
- No file over 600 lines, unless `xtask/file-size-exceptions.txt` lists it with a reason.
- Every relative link and anchor in a tracked `.md` file resolves, and rustdoc is clean.
- The Errors table in `ARCHITECTURE.md` matches `ErrorCode`, and generated docs are fresh.
- `unsafe` is forbidden outside `stetdoc-napi`. Library code never uses `unwrap`, `expect`
  or `panic!`. Tests may, and an integration test file opens with the allow in
  `crates/stetdoc-cli/tests/binary.rs`. The testkit is test support and allows them too.
- The core also denies indexing and slicing that can panic, `unreachable!`, `todo!`,
  `unimplemented!`, `dbg!`, printing and `#[allow]`. The only way around a lint is a local
  `#[expect(clippy::name, reason = "a real reason")]`.
- The core uses no `std::fs`, `net`, `env`, `process`, `SystemTime` or `Instant`. A test that needs
  a file reads it through a helper outside the core, or is listed in
  `xtask/core-io-exceptions.txt` with a reason.
- `missing_docs` is denied: document every public item. Warnings are errors in CI.

## Snapshots

Update a snapshot only when the change intends to change output. Review it with
`cargo insta test -p <crate> --review`, and explain it in the pull request. `cargo xtask check`
never writes snapshots. A failing law (L1 to L6) is never fixed by accepting a snapshot.

## Adding a test document

Build a synthetic one with `stetdoc_testkit::fixtures` (`zip::Zip` and `parts::Doc` build a
package; `edge()`, `word()`, `hostile()` list the set). Each says what must happen: open, or an
error code at a stage (`open`, `read`, `xml`, `document`) with a reason from `corpus/reasons.tsv`.
At `read` and `xml` it names the part too: a layer that never reads it sees `Ok`. Add a real
document to `corpus/manifest/<source>.tsv` (SHA-256, tier, licence, outcome, stage, part, reason;
pin the commit in `corpus/sources.tsv`) and its source to `corpus/SOURCES.md`. No document is
committed: `cargo xtask corpus fetch` downloads them (network on the first run) and `cargo xtask
check` runs it first. All checkouts share the cache: `STETDOC_CORPUS_DIR`, else
`$XDG_CACHE_HOME/stetdoc/corpora`, else `~/.cache/stetdoc/corpora`. Tests read
`sets/curated/<tier>/` and `INDEX.tsv` there (`STETDOC_CORPUS_SET` picks another set; a set never
changes). Oracle tests (Word, LibreOffice) skip locally without the tool.

## Where the laws live

The laws run through `Document`: each is a function in the testkit's `laws` module over the core's public interface, run by
`crates/stetdoc-testkit/tests/laws_*.rs`. The core's own tests dev-depend on the testkit for the corpus and the fixtures,
which compiles the core twice, so `corpus`, `expect` and `fixtures` are plain data, never a core type, and `laws` is a module
only the testkit's tests use. The harnesses of `package`, `xml` and `state` stay in the core (they read the fetched corpus,
see `xtask/core-io-exceptions.txt`), and so do the write-protocol tests of `document`.

## How to contribute a change

These steps apply to people and to the AI agents working for them.

1. **Open an issue first** for anything bigger than a small fix, so we can agree on the approach before you
   write it. Bugs and doc fixes can go straight to a pull request.
2. **Work on a branch** of your fork, one topic per branch. Read the contract of every module you touch or use.
3. **Test at the interface.** Laws, properties and snapshots go through `Document`.
   Unit tests are for modules with their own invariants: `xml`, `package`, `numbering`,
   `schema`, `wordml`'s story scan and text model, `refs` and `state`.
4. **Add dependencies** to `[workspace.dependencies]` and write `name.workspace = true` in the crate. Say why
   in the pull request. Run `cargo check --workspace` once and commit `Cargo.lock`. Keep `--locked` in the alias.
5. **Run `cargo xtask check`** before you open the pull request. CI runs it too, and review starts once it passes.
6. **Commit** with lowercase imperative messages, such as `add the xml span tree`. Never commit secrets.
7. **Describe the pull request.** Say what changed, why, what you checked and what you did not, and whether an
   AI agent wrote it. You answer for the diff either way, so read all of it first.
8. **Expect review.** A maintainer reviews every pull request, looking at the tests, the module contracts, the
   docs checklist and any snapshot change. Answer with new commits, not force-pushes.

## Definition of done

- `cargo xtask check` passes, and the working tree is clean and committed.
- New behaviour has tests at the interface, including a failing case.
- Public items are documented, and the module's `//!` contract still tells the truth.
- Docs checklist, updated in the same change:
  - [ ] `ARCHITECTURE.md`: code map, module contracts, tables, "where do I change X"
  - [ ] `GLOSSARY.md`: any new domain term
  - [ ] `docs/decisions/`: a record for any change to the public interface
  - [ ] `cargo xtask docs` run, so generated references are current
  - [ ] `CHANGELOG.md`: a line under Unreleased for any user-visible change
  - [ ] `README.md`, `CONTRIBUTING.md`: if the workflow or usage changed

## Where do I change X?

The table is in [`ARCHITECTURE.md`](ARCHITECTURE.md#where-do-i-change-x). To add an error code,
edit the `error_codes!` list in `api/error.rs`, add the code to the Errors table in
`ARCHITECTURE.md`, run `cargo xtask docs`, and raise it where it belongs.

## Rules for writing

Write docs and messages in plain whole sentences, without em dashes. Match the style of
the existing docs. Do not use "prototype" or "v0.1" framing: stetdoc is under development
toward 1.0.0.