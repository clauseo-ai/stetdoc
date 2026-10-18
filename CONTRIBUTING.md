# Contributing to stetdoc

Thank you for helping. stetdoc is under development toward 1.0.0, and nothing is
released yet. [`AGENTS.md`](AGENTS.md) is the short version of this file, written for
agents, and applies to people too. Everyone taking part follows the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Before you start

1. Read [`README.md`](README.md), then [`ARCHITECTURE.md`](ARCHITECTURE.md) and
   [`GLOSSARY.md`](GLOSSARY.md). Use the glossary's words in code, docs and messages.
2. Install Rust through [rustup](https://rustup.rs). The toolchain is pinned in
   `rust-toolchain.toml` and installs itself.
3. Install the tools the check needs: `cargo install cargo-deny --locked --version 0.20.2`,
   the version CI uses, and `cargo install cargo-insta --locked` to review snapshots.

## The loop

```
cargo xtask check --quick   while you work: format, static checks, clippy and tests for changed crates
cargo xtask check           before you push: CI's check job
```

If `cargo xtask check` passes locally, CI's check job passes. CI also runs `cargo xtask fuzz
<target> --time 60` for each fuzz target, which needs the pinned nightly and cargo-fuzz (see
Fuzzing below). Every failure names the file and line and says how to fix it.

## Making a change

- Work on a branch. Keep one concern per pull request.
- Commit messages are lowercase and imperative, for example `add the xml span tree`.
- Put tests at the interface: laws, property tests and snapshots go through
  `Document`. Unit tests are for modules with their own invariants.
- Document every public item. Keep each module's `//!` contract true.
- Do not add `unsafe` outside `stetdoc-napi`, and do not add `unwrap`, `expect` or
  `panic!` to library code. The core never panics on input,
  and its extra lints are listed in `AGENTS.md`.
- Add a dependency only when the change needs it. `cargo deny` checks its licence.
- Update the docs in the same pull request. The checklist is in `AGENTS.md`.

### The TypeScript library

The package `@stetdoc/node` lives in `crates/stetdoc-napi/npm` and needs Node 22.18 or later, or Bun 1.3.13 or later, and no
npm install:

```
node scripts/build.mjs        build the addon and copy it next to index.js (--release for an optimized one)
node test/smoke.mts           the smoke test under Node; `bun test/smoke.mts` runs the same file under Bun
cargo xtask codegen           regenerate types.d.ts after a change to a public data type
cargo xtask release check     the packages and the release workflow agree with the workspace (the tests run it too)
```

CI's `napi` job runs the smoke test on Bun 1.3.13, Bun 1.3.14 and Node 24, and type-checks it. Releases are built by
`.github/workflows/release.yml` on a `v*` tag, into a draft GitHub release; nothing is published from CI.

### Snapshots

Snapshots change only when the change intends to change output. Review them with
`cargo insta test -p <crate> --review` and explain them in the pull request. A failing law is never fixed
by accepting a snapshot.

### Changes to the public interface

The public interface is what `stetdoc-core/src/lib.rs` re-exports. Change it only with a
new decision record in `docs/decisions/`, numbered and dated like
[0001](docs/decisions/0001-core-interface.md).

### Test documents

Prefer building one with the testkit fixture builder (`stetdoc_testkit::fixtures`). To add a
real document, list it in `corpus/manifest/` with its pinned source, SHA-256, tier, licence
and expected outcome, and record its source in [`corpus/SOURCES.md`](corpus/SOURCES.md). A
document stetdoc must refuse is in the hostile tier and says at which stage it is refused
(`open`, `read`, `xml` or `document`) and why, using a name from `corpus/reasons.tsv`: the
refusals are the exemption list of law L1, so each one needs a reason. A refusal at `read` or
`xml` also names the part or parts that carry it, in the `part` column (several are separated
by `;`). A layer that never reads those parts does not meet the refusal, so for that layer the
document must open and law L1 must hold. `cargo xtask corpus check` runs the manifest check
alone.

The compare pairs are in `corpus/pairs.tsv`. Each names its original and revised
document by hash, and the documents of the `curated` pairs are fetched with the curated corpus, in
a set of their own (`curatedpairs`) that only the pair test reads. The rest run
nightly, or by hand after `cargo xtask corpus fetch --pairs` with `STETDOC_CORPUS_SET=pairs`
(`cargo test --release -p stetdoc-core --lib compare::tests_pairs`). A pair whose redline noise is
over the threshold must have its reason in the last column, and the threshold's numbers are in
`crates/stetdoc-testkit/bench/noise_baseline.tsv`.

No document is committed: `cargo xtask corpus fetch` downloads and verifies them into a cache
that every checkout on the machine shares (`STETDOC_CORPUS_DIR`, else
`$XDG_CACHE_HOME/stetdoc/corpora`, else `~/.cache/stetdoc/corpora`). Fetching is safe to run
from several checkouts at once. The first `cargo xtask check` on a machine needs the network
to fill the cache, which takes about a minute, and later runs need none. A set in the cache
never changes after it is built, not even to repair a damaged document, so a test run that
started earlier is not disturbed. The cache links sets with symbolic links, so on Windows it
needs Developer Mode or an elevated shell. Word oracle tests are skipped locally when Word is
absent.

### Random edit batches

The edit laws run random edit batches on every document. `laws::generate` in the testkit makes them from a
document's view only (refs, Final text and offsets, comments, notes, tables, revision ids), with every
operation, valid and edge payloads, tracked and untracked, one to four operations, and some that must
be refused. `laws::random` checks L2 to L5 (and H14 and validity) on each, and `laws::random_files` does
the same for random edits of `document.md`. A run picks a seed and prints it, and a failure names the
document, the seed and the batch. The random runs need a release build: `cargo xtask check` runs
them as a step of their own (`random laws (release)`), and a debug `cargo test` skips them and keeps
the fixed smoke set. To run the same batches again:

```
STETDOC_SEED=<seed> cargo test --release -p stetdoc-testkit --test laws_random
```

`STETDOC_BATCHES` sets the batches per document (50 by default, the minimum a release needs), `STETDOC_FILE_EDITS`
the random edits of files (10), and `STETDOC_SHARD=i/n` runs one shard of the documents, as the nightly
`laws` workflow does on the full corpora. The fixed batches of `laws::edits` stay as a smoke set.

### The LibreOffice oracle

[Decision 0005](docs/decisions/0005-word-oracle.md) checks a sample of stetdoc's outputs in a second reader. Besides a
hand check in Word for the web, LibreOffice is an independent reader that runs by script:

```
cargo xtask oracle libreoffice                        500 outputs, seed 20261009
cargo xtask oracle libreoffice --sample 50 --seed 7   a smaller sample, another seed
```

It needs LibreOffice (`brew install --cask libreoffice` on macOS, or `STETDOC_SOFFICE` naming its `soffice`); the tests skip
without it, as on CI. A headless soffice runs a Python macro (`crates/stetdoc-testkit/src/oracle/libreoffice.py`) that loads
each output, converts it to PDF, accepts all and rejects all its tracked changes, and the test holds the texts against
stetdoc's Final and Original views and triages each mismatch as a LibreOffice deviation or a stetdoc suspect. The report,
one diff per mismatch and the sampled outputs are written to `target/oracle/libreoffice/`, which git ignores; never commit
an output, since each is made from a corpus document. `STETDOC_LO_ONLY=3,17` reads only those outputs of the sample again.

### Fuzzing

The container (`package`) and the span tree (`xml`) have fuzz targets. A release needs
at least 24 CPU-hours of fuzzing per target without a crash, and CI runs each target for 60
seconds on every pull request.

```
cargo xtask fuzz xml                                    60 seconds
cargo xtask fuzz package --time 600                     ten minutes
cargo xtask fuzz xml --repro fuzz/artifacts/xml/crash-…  run one input again
cargo xtask fuzz xml --time 600 -- -fork=2              libFuzzer arguments follow `--`
```

Fuzzing needs two tools that the rest of the workspace does not: a nightly toolchain that is
pinned for fuzzing alone (`rustup toolchain install nightly-2026-09-28 --profile minimal`) and
`cargo install cargo-fuzz --locked --version 0.13.2`. The command checks for both, says how to
install what is missing, and never skips. It then fetches the curated corpus, seeds
`fuzz/corpus/<target>` with the fixtures and the small documents and XML parts of the corpus, and
runs libFuzzer. A failure leaves its input in `fuzz/artifacts/<target>`, and the command names the
file and how to reproduce it. Git ignores both directories.

The fuzz crate is outside the root workspace, so it declares its one dependency, `libfuzzer-sys`,
itself and not in `[workspace.dependencies]`, and it has its own `Cargo.lock`. `cargo xtask check`
runs cargo-deny on it with `fuzz/deny.toml`, which is the root policy and one more licence: NCSA,
the licence of the libFuzzer sources that `libfuzzer-sys` carries. The fuzz binaries are test tools
and are never shipped, and a test checks that the two policies differ in nothing else.

A target is a function `fn(&[u8])` in `crates/stetdoc-core/src/fuzz`, which exists only with the
off-by-default `fuzzing` feature, and a file in `fuzz/fuzz_targets`. The first four bytes of an
input choose what the function does, such as which part to replace or which edit to make, and the
rest is the document. A panic, a hang or a broken law is a failure. Refusing hostile input is not.
[Decision 0002](docs/decisions/0002-fuzz-entry-points.md) says why the entry points are there. If
a target stops on a crash in code you do not own, open an issue with its input and do not change
the target to avoid it.

### Benchmarks

The benchmarks hold the engine to a time budget for each operation on a 300-page contract (open 300 ms,
view 200 ms, a 50-operation edit 1 s, save and export 1 s each) and to a peak memory of 4 MiB plus
the file plus eight times the XML that is parsed. `cargo xtask bench` times what exists, in a
release build, measures its peak memory, and fails when a budget or the memory baseline is broken.
What exists is the container (`package_*`), the span tree (`xml_*`), schema validation
(`schema_*`), the committed state (`state_*`: open, a batch of fifty writes with a read of the story
after each, the commit of it, save, export, resume), `wordml` (`wordml_*`: the scan, the field pass,
the text pass, a refresh, and fifty text edits, field edits and paragraph insertions, each followed
by a refresh and a read), and the style cascade and clause numbers (`numbering_*`).

```
cargo xtask bench                          every benchmark on every input, the budgets as written
cargo xtask bench --ci                     the margins of a shared runner: twice the time budgets, three
                                           times the time baseline, a quarter more memory than the baseline
cargo xtask bench --strict-time            a time slower than its baseline fails, where it only warns
cargo xtask bench --filter xml             the benchmarks whose name contains `xml` (repeat for more)
cargo xtask bench --input dense            the inputs whose name contains `dense` (repeat for more)
cargo xtask bench --out DIR                also write the table and the measured rows into a directory
cargo xtask bench --list                   the benchmarks and the inputs, and nothing is run
cargo xtask bench --update-baseline        write the baseline of this platform, to be committed
```

Run it on a quiet machine: the times of a loaded one move by a factor of two. It fetches the curated
corpus first, because the 300-page contract is a Harvey LAB document with its body repeated to 300
pages of text (3,000 characters a page). The other inputs are the densest real document (a Council
draft of the AI Act, scaled the same way) and three synthetic bodies that `state` and `wordml` were
measured on, all made in memory and none committed: 12 MB in 18,273 blocks, 18 MB of short runs, and
13 MB with a field in one paragraph in eight. One more input is for the gates of `numbering` alone:
a body of 20,000 paragraphs, 8,000 of them numbered, with 561 styles in chains of ten, which only
the benchmarks that name it run on (`Inputs::Only`), because the gate is written for it: the
cascade, the facts of the paragraphs and the numbers must take 25 ms together, and fifty writes
interleaved with reads of the story and then one read of the numbers must fit the edit budget.
Every input is held to every budget. The one exception is a list of named pairs
of a benchmark and an input that are known to be over the memory budget, `MEMORY_GAPS` in
`crates/stetdoc-testkit/src/bench/budget.rs`, and it is empty: the four pairs that were on it
closed when the span tree began to store its nodes in chunks. A known gap is a note, and it ends by
itself: when the pair is measured within its budget the run fails and says to delete the entry,
which makes the input a hard gate for that benchmark.

Each line of the table is one benchmark on one input:

```
benchmark      input          ms  limit ms  peak MiB  limit MiB  time/base  mem/base
xml_parse      contract-300p  7.87      300      11.6       30.8       1.00      1.00  ok
```

`ms` is the median of several runs and `limit ms` the budget of the operation the benchmark is
part of, which a part may not exceed. `peak MiB` is the most the program had allocated during a run,
above what it held before the run was prepared, so the document the run keeps counts, and `limit MiB`
is the memory budget. `time/base` and `mem/base` are the ratios to the baseline in
`crates/stetdoc-testkit/bench/baseline.tsv` (`-` where there is none): of the fastest run, because
load only makes a run slower, and of the peak. The memory is a count of the bytes the program asked
its allocator for, so it does not depend on the speed or the load of the machine, and the time does.

The last column says what the line is, and a line under the table gives each reason:

- `FAIL` fails the run. A time over its budget (twice it with `--ci`) does, and so does memory over
  its budget, memory more than 1.1 times its baseline (1.25 times with `--ci`), and a known gap that
  has closed.
- `warn` does not fail the run. A time is a warning when the fastest run is more than 1.5 times its
  baseline (3 times with `--ci`) and has grown by more than 2 ms. A time depends on the load of the
  machine and nobody has measured yet how much a runner varies, so until there is data a regression
  of time is a warning (`--strict-time` makes it a failure on a machine that is quiet and has a
  baseline of its own). A run is also a warning when the sampler read the heap too few times in a
  long run, so that its peak may be too low: run it again on a quieter machine. On GitHub Actions
  each failure and warning is also an annotation on the run.
- `over` is a known gap that is still open.
- `ok` is the rest.

The baseline is per platform, because time is. A platform with no rows is held to the budgets and,
for memory, to the rows of another platform, with a note saying so; its time is not held to a
baseline until `cargo xtask bench --update-baseline` has written its rows. After a change that makes
something faster or heavier on purpose, run that command on a quiet machine and commit the rows it
wrote. `--out DIR` writes the table (`bench-table.txt`) and the rows that were measured
(`bench-rows.tsv`, in the format of the baseline), and the nightly workflow keeps both as an
artifact, so that the rows of a platform can be recorded from the runs of a runner that does not
change.

A benchmark is a small file in `crates/stetdoc-core/src/bench`, which exists in test builds only,
and a line in the list in `bench/mod.rs`; ARCHITECTURE.md says which rows to touch. The core reads
no clock, so the clock, the inputs, the budgets and the table are in `stetdoc_testkit::bench`.
[Decision 0003](docs/decisions/0003-benchmarks.md) says why it is built this way.

## Review

Every merge is reviewed before it lands. For work done by agents, the reviewer is a
separate agent on a different model.

## Licence of contributions

Unless you state otherwise, any contribution you submit for inclusion in stetdoc is
licensed as `MIT OR Apache-2.0`, as in the [licence files](LICENSE-MIT), without
additional terms or conditions.

## Security

Do not report vulnerabilities in public issues. See [`SECURITY.md`](SECURITY.md).
