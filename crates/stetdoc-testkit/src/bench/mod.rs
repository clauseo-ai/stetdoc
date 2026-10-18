//! The benchmark harness for the speed and memory budgets: the time and the peak memory of the engine's operations,
//! held to the budgets of `ARCHITECTURE.md` and to a baseline.
//!
//! The core may not read a clock, so the clock is here, and so are the inputs, the budgets, the
//! baseline and the table. A benchmark is a [`Bench`], a few lines of code in a small file of
//! `stetdoc-core/src/bench/`, which `cfg(test)` code of the core lists (see `bench/mod.rs` in the
//! core): the harness gives it a document, times the run it prepares, and reports.
//! `cargo xtask bench` runs them all, in a release build, as one ignored test of the core.
//!
//! Like the rest of this crate it names no core type, so a benchmark passes bytes in and a
//! message out. The binary that runs it has to install the counting allocator, which is
//! `stats_alloc::INSTRUMENTED_SYSTEM` (see [`measure()`]).
//!
//! **Interface.** What a benchmark file and the core's test need, and nothing else: the types of a
//! benchmark ([`Bench`], [`Input`], [`Limit`] and the rest of `spec`), [`run()`], the names and
//! the builder of the inputs (`inputs::NAMES`, `inputs::from_body`), the baseline rows
//! (`baseline::load`, [`baseline::Row`]), and [`measure()`], which `tests/bench_peak.rs` calls. The
//! budgets, the judge, the table and the arguments are private to the crate.

mod args;
pub mod baseline;
mod budget;
pub mod inputs;
mod judge;
mod measure;
mod numbered;
mod run;
mod spec;

pub use measure::{Measured, Watched, measure};
pub use run::run;
pub use spec::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

/// The counting allocator for the unit tests of this crate, which call `measure`.
#[cfg(test)]
mod counting {
    #[global_allocator]
    static COUNTING: &stats_alloc::StatsAlloc<std::alloc::System> =
        &stats_alloc::INSTRUMENTED_SYSTEM;
}
