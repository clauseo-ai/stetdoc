# 0002. The fuzz targets reach the core through one feature-gated module

- **Date:** 2026-10-05
- **Status:** proposed on 2026-10-05, accepted when the change that adds it merges

## Context

Before a release, each target must run for at least 24 CPU-hours without a crash, and
every pull request runs a 60-second smoke of each target. The fuzz targets are `cargo-fuzz`
binaries: separate crates that link libFuzzer and are built with a nightly compiler and sanitizer
flags. They can call only what `stetdoc-core` makes public.

The first two targets are the ZIP container and the XML span tree, which are `package` and `xml`.
Both are private modules of the core, and decision 0001 keeps them private: the public interface is
the `Document` handle and the error vocabulary, and `lib.rs` re-exports exactly that. A target
cannot call `Package::open` or `SpanTree::parse` as things stand.

## Options considered

| Option | Strength | Weakness |
|---|---|---|
| 1. Make `package` and `xml` public | Targets call the real types | Their layout becomes interface that decision 0001 says is not, and the authors of those modules would have to keep it stable |
| 2. A `#[doc(hidden)]` public re-export, always compiled | Small | A symbol in every build that no caller may use, and a hole in the rule that `lib.rs` re-exports only the interface |
| 3. A module behind an off-by-default feature, with a few `fn(&[u8])` entry points | Nothing new is public in a normal build. The targets see two functions, not two modules | One module that is not private when the feature is on |
| 4. Fuzz inside the core's tests, with a generator | No feature | No coverage guidance, which is what makes libFuzzer find things, and it is not `cargo-fuzz`, the tool the test plan named |

## Decision

Option 3.

- **The feature.** `stetdoc-core` has a feature `fuzzing`, off by default, that adds no
  dependency. With it, `lib.rs` declares `#[cfg(feature = "fuzzing")] pub mod fuzz;`. Without it
  the crate is the crate it was.
- **The module.** `fuzz` holds `package(&[u8])` and `xml(&[u8])`. Each takes the bytes a fuzzer
  made, drives its module through the calls an edit makes, and panics when a law of the module
  breaks, because a panic is how libFuzzer sees one. The first four bytes of an input choose what
  the harness does, and the rest is the document, so the fuzzer changes the choice and the
  document apart.
  - `package`: open; write back the input by the shortcut and by the rebuild that an edit uses;
    read every part twice; replace one part and write; the output must open, hold the same parts,
    give the replacement back, and leave every other entry as it was, byte for byte.
  - `xml`: parse; serialize clean and with every start tag dirty, both to the input; make one
    edit; the part it writes must parse again as the tree that wrote it, serialize to itself, and
    hold every clean node as it was.
  - Refusing hostile input is never a failure. A panic, a hang, a runaway allocation or a broken
    law is.
- **The rules.** The Allowed dependencies table gets a row, `fuzz`, that lists `api`, `package`
  and `xml`. No row lists `fuzz`, and it sits below `document`, whose "any of the above" now means
  the rows above it. The check lets `lib.rs` make exactly one module public, `fuzz`, behind exactly
  that feature. When this was decided, `document` did not exist yet. A target that goes through it
  adds `document` to the row, as `batch` and `files` now do.
- **The hooks.** The harness uses three functions that the `package` and `xml` modules wrote for their own laws and
  gated on `cfg(test)`: `Package::write_rebuilt`, `Package::raw_region` and
  `SpanTree::mark_all_tags_dirty`. They are now gated on `any(test, feature = "fuzzing")`.
- **The targets.** They are in `fuzz/`, a cargo workspace of its own, because `cargo-fuzz` needs
  one and because libFuzzer is compiled from C++, which `cargo xtask check` should not do. They are
  built with a nightly toolchain that is pinned for fuzzing only. `cargo xtask fuzz <target>`
  checks for the toolchain and for `cargo-fuzz`, seeds the corpus from the fixtures and the curated
  corpus, and runs the target.
- **The check.** `cargo xtask check` runs clippy twice, with the default features and with all of
  them, and the tests and rustdoc with all features, so the module is linted, tested and documented.
  The run with the default features is the one that builds the core as the binaries link it, and it
  is what stops shipping code from calling a function that only the feature adds. A test checks
  that nothing that ships enables the feature.
- **The dependency.** `libfuzzer-sys` is the one dependency that is not declared in
  `[workspace.dependencies]`, because the fuzz crate is outside the workspace. Its `Cargo.lock` is
  committed, and `cargo xtask check` runs cargo-deny on the crate with `fuzz/deny.toml`. That is
  the root policy and one more licence, NCSA, which the libFuzzer sources in `libfuzzer-sys` are
  under. The root policy does not allow it, because it names what can be shipped, and the fuzz
  binaries never are. A test checks that the two policies differ in nothing else.

## Consequences

- **Two functions are the whole surface.** A change to `package` or `xml` that keeps their
  signatures does not touch the targets. A change to the hooks, or to what `Package` and
  `SpanTree` promise, shows up as a compile error or a failing law in the harness.
- **The harness holds the modules to their documented laws on input nobody wrote.** A document
  that the container opens but cannot write back is a finding, and so is an edit that writes a part
  that does not parse.
- **The core's no-panic rule has one deliberate exception**, in `fuzz` and only with the feature,
  and the contract of the module says so.
- **Revisit** now that `Document` exists and the `batch` and `files` targets drive it: the entry
  points for `package` and `xml` may be worth dropping if the targets through `Document` reach the
  same code.
