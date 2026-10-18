//! The baseline: what each benchmark took and used when it last was accepted, so that a change
//! that makes one slower or heavier shows up before a budget breaks.
//!
//! The file is `bench/baseline.tsv` in this crate, one row per benchmark, input and platform. The
//! time in a row is that of the fastest run, which is the one that load cannot make worse. Time
//! depends on the machine, so a row is for one platform (`macos-aarch64`), and a platform without
//! rows is not checked for time regressions, only against the budgets. The peak memory is a count
//! of the bytes the program asked for, which does not depend on the machine, so a platform without
//! rows is held to the peak of another's until it has rows of its own.
//! `cargo xtask bench --update-baseline` rewrites the rows of the platform it runs on and leaves
//! the others as they are.

use std::path::PathBuf;

/// One row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The benchmark.
    pub bench: String,
    /// The input.
    pub input: String,
    /// The platform: the operating system and the architecture.
    pub platform: String,
    /// The time of the fastest run, in milliseconds.
    pub time_ms: f64,
    /// The peak memory, in bytes.
    pub peak: u64,
}

/// The first line of the file.
pub(crate) const HEADER: &str = "bench\tinput\tplatform\ttime_ms\tpeak_bytes\n";

/// The platform this runs on.
pub(crate) fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Where the file is.
pub(crate) fn path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/bench/baseline.tsv"))
}

/// Reads rows from text. A row with the wrong number of columns, or a number that is not one, is
/// an error that names its line.
pub(crate) fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        let [bench, input, platform, time, peak] = c[..] else {
            return Err(format!(
                "baseline.tsv line {}: expected 5 columns, found {}",
                i + 1,
                c.len()
            ));
        };
        rows.push(Row {
            bench: bench.to_owned(),
            input: input.to_owned(),
            platform: platform.to_owned(),
            time_ms: time
                .parse()
                .map_err(|_| format!("baseline.tsv line {}: bad time", i + 1))?,
            peak: peak
                .parse()
                .map_err(|_| format!("baseline.tsv line {}: bad peak", i + 1))?,
        });
    }
    Ok(rows)
}

/// The text of rows, sorted so that an update changes only the lines that moved.
pub(crate) fn render(rows: &[Row]) -> String {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| (&a.bench, &a.input, &a.platform).cmp(&(&b.bench, &b.input, &b.platform)));
    let mut out = HEADER.to_owned();
    for r in rows {
        out.push_str(&format!(
            "{}\t{}\t{}\t{:.3}\t{}\n",
            r.bench, r.input, r.platform, r.time_ms, r.peak
        ));
    }
    out
}

/// The rows of the file. A missing file is no rows.
pub fn load() -> Result<Vec<Row>, String> {
    match std::fs::read_to_string(path()) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("cannot read {}: {e}", path().display())),
    }
}

/// Writes rows to a file, and the directory it is in if there is none yet.
pub(crate) fn write_to(path: &std::path::Path, rows: &[Row]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    std::fs::write(path, render(rows)).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// `old` with the rows of `new` in place of those for the same benchmark, input and platform.
pub(crate) fn merge(old: Vec<Row>, new: &[Row]) -> Vec<Row> {
    let same =
        |a: &Row, b: &Row| (&a.bench, &a.input, &a.platform) == (&b.bench, &b.input, &b.platform);
    let mut rows: Vec<Row> = old
        .into_iter()
        .filter(|o| !new.iter().any(|n| same(o, n)))
        .collect();
    rows.extend(new.iter().cloned());
    rows
}

/// The row for a benchmark and input on a platform.
pub(crate) fn find<'a>(
    rows: &'a [Row],
    bench: &str,
    input: &str,
    platform: &str,
) -> Option<&'a Row> {
    rows.iter()
        .find(|r| r.bench == bench && r.input == input && r.platform == platform)
}

/// The rows that a measurement is compared with.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Base<'a> {
    /// The row of this platform, for the time.
    pub(crate) time: Option<&'a Row>,
    /// The row of this platform or, where it has none, of another, for the memory.
    pub(crate) peak: Option<&'a Row>,
}

/// The rows for a benchmark and input, seen from `platform`.
pub(crate) fn base<'a>(rows: &'a [Row], bench: &str, input: &str, platform: &str) -> Base<'a> {
    let own = find(rows, bench, input, platform);
    let any = own.or_else(|| rows.iter().find(|r| r.bench == bench && r.input == input));
    Base {
        time: own,
        peak: any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(bench: &str, input: &str, platform: &str, time_ms: f64, peak: u64) -> Row {
        Row {
            bench: bench.into(),
            input: input.into(),
            platform: platform.into(),
            time_ms,
            peak,
        }
    }

    #[test]
    fn rows_survive_a_round_trip_and_come_out_sorted() {
        let rows = vec![
            row("b", "x", "linux-x86_64", 12.5, 2048),
            row("a", "y", "macos-aarch64", 0.25, 7),
        ];
        let text = render(&rows);
        assert!(text.starts_with(HEADER));
        let back = parse(&text).unwrap();
        assert_eq!(back[0].bench, "a");
        assert_eq!(back[1], rows[0]);
    }

    #[test]
    fn a_time_of_microseconds_keeps_its_resolution() {
        let back = parse(&render(&[row("a", "x", "p", 0.0042, 5)])).unwrap();
        assert_eq!(
            back[0].time_ms, 0.004,
            "a package open takes about that, and 0.00 would divide by nothing"
        );
    }

    #[test]
    fn a_bad_line_is_refused_with_its_number() {
        assert!(
            parse("h\nonly\ttwo\n")
                .unwrap_err()
                .contains("line 2: expected 5 columns")
        );
        assert!(
            parse("h\na\tb\tc\tfast\t1\n")
                .unwrap_err()
                .contains("bad time")
        );
        assert!(
            parse("h\na\tb\tc\t1.0\tmany\n")
                .unwrap_err()
                .contains("bad peak")
        );
    }

    #[test]
    fn an_update_replaces_the_rows_of_its_platform_and_keeps_the_others() {
        let old = vec![
            row("a", "x", "linux-x86_64", 1.0, 1),
            row("a", "x", "macos-aarch64", 2.0, 2),
        ];
        let merged = merge(
            old,
            &[
                row("a", "x", "macos-aarch64", 3.0, 3),
                row("b", "x", "macos-aarch64", 4.0, 4),
            ],
        );
        assert_eq!(merged.len(), 3);
        assert_eq!(
            find(&merged, "a", "x", "linux-x86_64").unwrap().time_ms,
            1.0
        );
        assert_eq!(find(&merged, "a", "x", "macos-aarch64").unwrap().peak, 3);
        assert!(find(&merged, "a", "y", "macos-aarch64").is_none());
    }

    #[test]
    fn rows_are_written_where_there_is_no_directory_yet_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("bench").join("baseline.tsv");
        let rows = vec![row("b", "y", "p", 2.5, 20), row("a", "x", "p", 1.0, 10)];
        write_to(&file, &rows).unwrap();
        let back = parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(
            back[0].bench, "a",
            "sorted, so that an update changes the lines that moved"
        );
        assert_eq!(back[1].peak, 20);
    }

    #[test]
    fn the_time_is_compared_on_one_platform_and_the_memory_on_any() {
        let rows = vec![row("a", "x", "macos-aarch64", 2.0, 20)];
        let own = base(&rows, "a", "x", "macos-aarch64");
        assert!(own.time.is_some() && own.peak.is_some());
        let other = base(&rows, "a", "x", "linux-x86_64");
        assert!(
            other.time.is_none(),
            "a time on another machine says nothing"
        );
        assert_eq!(
            other.peak.map(|r| r.peak),
            Some(20),
            "the count does not depend on the machine"
        );
        let none = base(&rows, "a", "y", "macos-aarch64");
        assert!(none.time.is_none() && none.peak.is_none());
    }

    #[test]
    fn the_platform_names_the_system_and_the_architecture() {
        let p = platform();
        assert!(
            p.contains('-') && p.starts_with(std::env::consts::OS),
            "{p}"
        );
    }
}
