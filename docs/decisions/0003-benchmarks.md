# 0003. The benchmarks are registered by the core's test code and run by a harness in the testkit

- **Date:** 2026-10-05
- **Status:** accepted on 2026-10-06, when the change that adds it merged, and amended the same
  day by the fixes of its review (known gaps in place of stress inputs, time regressions that warn,
  a spinning sampler)

## Context

The performance budgets in `ARCHITECTURE.md` hold the engine to per-operation time budgets and a peak memory budget on a 300-page
contract. Three things make the benchmarks awkward to place.

- The core reads no clock, and `cargo xtask check` rejects the clock in it, so the timing cannot
  be in the core.
- What there is to time is private. `package`, `xml` and `schema` are not public, `state`,
  `wordml` and `styles` will not be, and when this was decided, the public `Document` did not exist yet.
- The memory budget is a number of bytes, so it has to be measured, and the workspace forbids
  `unsafe`, which is what writing a counting allocator takes.

## Options considered

Where the benchmarks live.

| Option | Strength | Weakness |
|---|---|---|
| 1. A feature-gated public module of the core, as `fuzz` is | A benchmark binary of its own, built by `cargo bench` | A second public module in the interface, a feature that the workspace build would switch on for the testkit and so for every crate, and benchmark code held to the lints of library code |
| 2. Test code in the core registers the benchmarks, and a harness in the testkit runs them | Nothing new is public, the benchmark sees what the module's tests see, and the core's own lints for tests apply | The benchmarks run inside the core's test binary, as one ignored test |
| 3. Timing tests in the core, as `state` had them | No harness | A clock in the core, a line in `core-io-exceptions.txt` for each, and a table and thresholds written again for each |
| 4. Wait for `Document` and benchmark the public interface | Measures what users get | Nothing to measure until then, and no number for `package`, `xml` and `schema` in the meantime |

How peak memory is measured.

| Option | Strength | Weakness |
|---|---|---|
| a. The resident set of the process | What the operating system sees | It includes the allocator's slack and whatever the binary maps, cannot be reset between benchmarks on every platform, and moves with the allocator, so a budget cannot be checked closely and a regression of ten per cent is lost in the noise |
| b. A counting allocator that copies on `realloc` (`peak_alloc`) | A peak that is exact | It adds the old block to the new one while a vector doubles: parsing the densest 300-page contract came out at 126% of its budget where it is 92% |
| c. A counting allocator that passes `realloc` to the system allocator (`stats_alloc`), read by a thread during one run | The count is of what the program holds, the same on every run | The counter only adds and subtracts, so the peak is the highest reading, and a spike shorter than a reading is missed |
| d. An allocator of our own | Exact peak, no thread | `unsafe`, which the workspace forbids outside `stetdoc-napi` |

## Decision

Option 2 and option c.

- **The registry.** `stetdoc-core/src/bench/` exists in test builds only (`#[cfg(test)] mod bench;`).
  Each benchmark is a small file: a `Bench` (its name, what it is held to, what XML it parses) and a
  function that prepares one run on a document. `bench/mod.rs` lists them and has one ignored test,
  `bench::g6`, which hands the list to `stetdoc_testkit::bench::run`. A new benchmark is a new file
  and a line in the list. The module has a row in the Allowed dependencies table that lists what its
  benchmarks use, and the rules for `fuzz` hold for it: no other row lists it and it sits below
  `document`.
- **The harness.** `stetdoc-testkit/src/bench/` reads the clock. It builds the inputs, times each run
  (the median of three to twenty-one, after one that is not counted), measures the peak memory of one
  more, holds both to the budgets and to a baseline, and prints a table. It names no core type, as
  the rest of the testkit: a benchmark passes bytes in and a message out.
- **The memory.** The counting allocator is `stats_alloc`, a crate of a hundred lines with no
  dependencies and a licence the policy allows, installed as the global allocator of the core's test
  binary. A thread spins on the count while a run goes on, and the peak is the highest reading above
  what the program held before `prepare`, so the document the run keeps counts. The count is of bytes
  the program asked for, so it is the same on every run of the same code, and a regression of ten per
  cent is a regression. The thread counts its readings, and a long run that got few of them (a loaded
  machine does not schedule the thread) is a warning, because its peak may be too low.
- **The inputs.** A 300-page contract that is a Harvey LAB document with its body repeated to 300
  pages of text, which is what the budgets are written for. Four more: the densest real document (a
  Council draft of the AI Act, scaled the same way) and the three synthetic worst bodies that `state`
  and `wordml` were measured on (the one with fields has one in every eight paragraphs, about the
  density that `wordml` was measured on, because a field in every paragraph put the refresh of a
  story far outside anything its design considers). A sixth input, a body of 20,000 paragraphs with
  numbering and 561 styles, is for the gates of `numbering` and is run only by the benchmarks that
  name it (`Inputs::Only`), with the memory budget of its own size. Every input is held to every
  budget. The only exemption is a named
  pair of a benchmark and an input in `MEMORY_GAPS`, a known gap, which is a note and not a failure.
  There were four, one cause: on the densest bodies the span tree, the part it was parsed from and
  the part it writes came to 103% of the budget, because the arenas of the tree were vectors that
  hold up to twice what they use. They closed when the arenas became chunks, and the list is empty.
  An entry ends by itself, because a run that measures the pair within its budget fails and says to
  delete the entry, which makes the input a hard gate for that benchmark. The first version
  exempted every benchmark on four whole inputs, with no end.
- **The command.** `cargo xtask bench` fetches the corpus, runs `bench::g6` in a release build, and
  fails when a budget (time or memory), a memory regression or a closed known gap is found. A time
  regression against the baseline is a warning, until there is data on how much the time of a runner
  varies, because the first version failed it and flaked on a loaded machine; `--strict-time` makes
  it a failure again. `--ci` has the margins of a shared runner (twice the time budgets, three times
  a time baseline, a quarter more memory than a baseline; the memory budget is not scaled),
  `--out DIR` writes the table and the measured rows, and `--update-baseline` writes the baseline of
  the platform it runs on.

## Consequences

- **Every benchmark can use what its module's tests use.** None of it is public, and a build without
  `cfg(test)` does not contain any of it.
- **The core's test binary counts allocations,** which costs the other tests of the crate two atomic
  additions an allocation. The timings are taken with the counter in place, so they are a little
  pessimistic.
- **The core's tests depend on the testkit,** which compiles the core a second time. The testkit's API
  is plain data for that reason, and `state` already does the same.
- **The baseline is per platform.** A platform without rows is held to the budgets and not to a time
  baseline until `--update-baseline` writes its rows. The nightly workflow keeps the table and the
  measured rows of each run as an artifact, with a note of the runner, so that the rows of
  linux-x86_64 can be taken from the runs of a fixed runner class once there are some.
- **Revisit** when `Document` exists: the benchmarks that go through it can move to the testkit's own
  benchmark binary, and the registry in the core keeps only what needs the internals.
