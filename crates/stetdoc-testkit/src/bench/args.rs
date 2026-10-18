//! The arguments of a run. `cargo xtask bench` passes them in `STETDOC_BENCH_ARGS`, one string of
//! words, because the benchmarks run inside a test binary that has no command line of its own. The
//! words are separated by the ASCII unit separator (`'\u{1f}'`), so that a directory for `--out`
//! may have spaces in its name; a string without one is split at white space, for a person who
//! sets the variable by hand.

use super::budget::Mode;
use std::path::PathBuf;

/// The variable that holds the arguments.
pub(crate) const ENV: &str = "STETDOC_BENCH_ARGS";

/// What a run is asked to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Args {
    /// How strictly to judge: `--ci` for the generous margins of a shared runner.
    pub mode: Mode,
    /// `--strict-time`: a time regression against the baseline fails the run. Without it, it is a
    /// warning, because a time depends on the load of the machine and nobody has measured yet how
    /// much a runner varies.
    pub strict_time: bool,
    /// `--update-baseline`: write what was measured as the baseline of this platform.
    pub update_baseline: bool,
    /// `--list`: print the benchmarks and the inputs, and run nothing.
    pub list: bool,
    /// `--out DIR`: also write the table and the rows that were measured into a directory.
    pub out: Option<PathBuf>,
    /// `--filter WORD`: only the benchmarks whose name contains a word. May be repeated.
    pub filter: Vec<String>,
    /// `--input WORD`: only the inputs whose name contains a word. May be repeated.
    pub input: Vec<String>,
}

impl Args {
    /// Reads the words. An unknown word, or a flag without its value, is an error that says so.
    pub(crate) fn parse(words: &[&str]) -> Result<Args, String> {
        let mut args = Args::default();
        let mut words = words.iter().copied();
        while let Some(word) = words.next() {
            match word {
                "--ci" => args.mode = Mode::Ci,
                "--strict-time" => args.strict_time = true,
                "--update-baseline" => args.update_baseline = true,
                "--list" => args.list = true,
                "--out" | "--filter" | "--input" => {
                    let value = words
                        .next()
                        .ok_or_else(|| format!("{word} needs a value"))?;
                    match word {
                        "--out" => args.out = Some(PathBuf::from(value)),
                        "--filter" => args.filter.push(value.to_owned()),
                        _ => args.input.push(value.to_owned()),
                    }
                }
                other => return Err(format!("unknown benchmark argument `{other}`")),
            }
        }
        Ok(args)
    }

    /// Reads `STETDOC_BENCH_ARGS`; unset means no arguments.
    pub(crate) fn from_env() -> Result<Args, String> {
        let text = std::env::var(ENV).unwrap_or_default();
        Args::from_text(&text)
    }

    /// Reads a string of words: separated by the unit separator if it has one, else by white space.
    fn from_text(text: &str) -> Result<Args, String> {
        let words: Vec<&str> = if text.contains('\u{1f}') {
            text.split('\u{1f}').filter(|w| !w.is_empty()).collect()
        } else {
            text.split_whitespace().collect()
        };
        Args::parse(&words)
    }

    /// Whether the benchmark called `name` is asked for.
    pub(crate) fn wants_bench(&self, name: &str) -> bool {
        self.filter.is_empty() || self.filter.iter().any(|w| name.contains(w.as_str()))
    }

    /// Whether the input called `name` is asked for.
    pub(crate) fn wants_input(&self, name: &str) -> bool {
        self.input.is_empty() || self.input.iter().any(|w| name.contains(w.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_words_mean_a_strict_run_of_everything() {
        let args = Args::parse(&[]).unwrap();
        assert_eq!(args, Args::default());
        assert!(args.wants_bench("anything") && args.wants_input("anything"));
        assert_eq!(args.mode, Mode::Nightly);
    }

    #[test]
    fn flags_and_filters_are_read() {
        let args = Args::parse(&[
            "--ci", "--filter", "xml", "--filter", "package", "--input", "dense", "--list",
        ])
        .unwrap();
        assert_eq!(args.mode, Mode::Ci);
        assert!(args.list && !args.update_baseline && !args.strict_time);
        assert!(args.wants_bench("xml_parse") && args.wants_bench("package_open"));
        assert!(!args.wants_bench("schema_validate"));
        assert!(args.wants_input("node-dense-18mb") && !args.wants_input("contract-300p"));
        assert!(Args::parse(&["--update-baseline"]).unwrap().update_baseline);
    }

    #[test]
    fn a_strict_time_run_and_an_output_directory_are_read() {
        let args = Args::parse(&["--strict-time", "--out", "/tmp/results"]).unwrap();
        assert!(args.strict_time);
        assert_eq!(args.out, Some(PathBuf::from("/tmp/results")));
        assert_eq!(Args::parse(&[]).unwrap().out, None);
        assert!(
            Args::parse(&["--out"])
                .unwrap_err()
                .contains("needs a value")
        );
    }

    #[test]
    fn words_are_separated_by_the_unit_separator_so_a_directory_may_have_spaces() {
        let args = Args::from_text("--out\u{1f}/tmp/my results\u{1f}--ci").unwrap();
        assert_eq!(args.out, Some(PathBuf::from("/tmp/my results")));
        assert_eq!(args.mode, Mode::Ci);
        let by_hand = Args::from_text("--ci  --filter xml").unwrap();
        assert_eq!(
            (by_hand.mode, by_hand.filter),
            (Mode::Ci, vec!["xml".to_owned()])
        );
    }

    #[test]
    fn a_bad_word_is_refused_by_name() {
        assert!(Args::parse(&["--fast"]).unwrap_err().contains("--fast"));
        assert!(
            Args::parse(&["--filter"])
                .unwrap_err()
                .contains("needs a value")
        );
    }
}
