//! The sweep's own checks: the gaps, the verdict, the shard choice, the deadline, the streaming and the watchdog, with a fake law
//! over a list or over a corpus set in a temporary directory.

use super::*;
use std::sync::Arc;
use std::time::Instant;

fn swept(failures: &[&str]) -> Swept {
    Swept {
        checked: 2,
        checked_ids: vec!["a:one.docx".into(), "b:two.docx".into()],
        failures: failures.iter().map(|f| (*f).to_string()).collect(),
        chosen: 2,
        in_set: 2,
        ..Swept::default()
    }
}

const GAP: Gap = Gap {
    id: "a:one.docx",
    says: "textId is not an attribute",
    why: "a test",
};

#[test]
fn a_known_gap_is_taken_out_of_the_failures_and_nothing_else() {
    let found = swept(&[
        "a:one.docx: {\"op\":\"insert\"}: validation_failed: p1: textId is not an attribute of w:p.",
        "b:two.docx: something else",
    ]);
    let left = found.minus(&[GAP]).unwrap();
    assert_eq!(left.failures, ["b:two.docx: something else"]);
}

#[test]
fn a_gap_that_no_longer_occurs_on_a_document_that_was_checked_is_an_error() {
    let e = swept(&[]).minus(&[GAP]).unwrap_err();
    assert!(
        e.contains("a:one.docx") && e.contains("no longer occurs"),
        "{e}"
    );
    // The same message about another document is not the gap.
    let e = swept(&["b:two.docx: textId is not an attribute"])
        .minus(&[GAP])
        .unwrap_err();
    assert!(e.contains("no longer occurs"), "{e}");
}

#[test]
fn a_gap_of_a_document_that_was_not_checked_is_not_asked_for() {
    let other = Gap {
        id: "c:three.docx",
        ..GAP
    };
    assert!(swept(&[]).minus(&[other]).is_ok());
}

#[test]
fn the_verdict_counts_what_was_checked_skipped_and_failed() {
    let mut found = swept(&[]);
    assert!(found.verdict(2).is_ok());
    assert!(found.verdict(3).is_err());
    found.skipped = vec!["x".into(); 3];
    assert!(found.verdict(2).unwrap_err().contains("skipped"));
    found.skipped.clear();
    found.failures = vec!["b:two.docx: broke".into()];
    assert!(found.verdict(2).unwrap_err().contains("1 failures"));
}

#[test]
fn a_rerun_of_a_few_documents_needs_no_minimum_but_must_check_one() {
    let mut found = swept(&[]);
    found.only = true;
    assert!(found.verdict(200).is_ok());
    found.checked = 0;
    assert!(found.verdict(200).unwrap_err().contains(ENV_ONLY));
}

#[test]
fn a_shard_needs_its_share_of_the_minimum_and_of_the_set() {
    let shard = Shard { index: 1, count: 4 };
    // 200 for the set is 50 for one shard of four.
    let mut found = Swept {
        checked: 50,
        chosen: 50,
        in_set: 200,
        shard: Some(shard),
        ..Swept::default()
    };
    assert!(found.verdict(200).is_ok(), "{:?}", found.verdict(200));
    found.checked = 49;
    assert!(found.verdict(200).unwrap_err().contains("50 expected"));
    // A shard that holds under half of its share of the set (a hash that does not spread the documents) fails.
    let lopsided = Swept {
        checked: 20,
        chosen: 20,
        in_set: 200,
        shard: Some(shard),
        ..Swept::default()
    };
    let e = lopsided.verdict(40).unwrap_err();
    assert!(
        e.contains("under half of its share") && e.contains("1/4"),
        "{e}"
    );
}

#[test]
fn documents_the_deadline_left_unreached_fail_the_verdict() {
    let mut found = swept(&[]);
    found.chosen = 5;
    found.not_reached = 3;
    let e = found.verdict(2).unwrap_err();
    assert!(e.contains("3 of the 5") && e.contains(ENV_DEADLINE), "{e}");
    assert!(found.reach().is_some());
}

#[test]
fn a_shard_is_read_as_i_of_n() {
    assert_eq!(
        Shard::parse("3/16"),
        Ok(Shard {
            index: 3,
            count: 16
        })
    );
    assert_eq!(Shard::parse(" 0 / 1 "), Ok(Shard { index: 0, count: 1 }));
    for bad in ["16/16", "3", "a/b", "1/0", ""] {
        assert!(Shard::parse(bad).unwrap_err().contains(ENV_SHARD), "{bad}");
    }
    assert_eq!(Shard { index: 2, count: 5 }.to_string(), "2/5");
}

/// A SHA-256 as `INDEX.tsv` writes it, made from a number.
fn sha(n: u64) -> String {
    let mut out = String::new();
    let mut h = n;
    for _ in 0..4 {
        h = h.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(1);
        out.push_str(&format!("{h:016x}"));
    }
    out
}

#[test]
fn every_document_is_in_exactly_one_shard_and_the_shards_are_even() {
    let n = 7;
    let mut sizes = [0usize; 7];
    for d in 0..7000 {
        let sha = sha(d);
        let holders: Vec<u64> = (0..n)
            .filter(|i| {
                Shard {
                    index: *i,
                    count: n,
                }
                .holds(&sha)
            })
            .collect();
        assert_eq!(holders.len(), 1, "{sha}");
        sizes[holders[0] as usize] += 1;
    }
    assert!(sizes.iter().all(|s| (800..1200).contains(s)), "{sizes:?}");
    // The shard is the SHA-256's first 16 digits modulo n, the same on every machine.
    assert_eq!(shard_key("0000000000000010ffff"), 16);
    assert!(
        Shard {
            index: 0,
            count: 16
        }
        .holds("0000000000000010ffff")
    );
    assert!(Shard { index: 3, count: 4 }.holds(&format!("{:016x}{}", 7u64, "0".repeat(48))));
    // Text that is not hexadecimal still lands in one shard.
    assert_eq!(shard_key("not-a-sha"), shard_key("not-a-sha"));
}

/// What a sweep said, in order, with the time it said it.
#[derive(Default)]
struct Heard(Mutex<Vec<(Said, String, Instant)>>);

impl Heard {
    fn say(&self, said: Said, line: &str) {
        self.0
            .lock()
            .unwrap()
            .push((said, line.to_string(), Instant::now()));
    }
    fn lines(&self, said: Said) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(s, _, _)| *s == said)
            .map(|(_, l, _)| l.clone())
            .collect()
    }
}

fn one_worker() -> Settings {
    Settings {
        workers: 1,
        ..Settings::default()
    }
}

#[test]
fn a_failure_is_said_the_moment_it_is_found_and_listed_at_the_end() {
    let items = ["a", "b", "c", "d"];
    let heard = Heard::default();
    let say = |s: Said, l: &str| heard.say(s, l);
    let seen_before_c = Mutex::new(Vec::new());
    let swept = sweep(
        "fake",
        &items,
        |s| s,
        &one_worker(),
        &say,
        |item| {
            if *item == "c" {
                // One worker runs the items in order, so b's failure is in the log before c starts.
                *seen_before_c.lock().unwrap() = heard.lines(Said::Failure);
            }
            Outcome::Checked(if *item == "b" {
                Err("seed 7, batch 3: broke".to_string())
            } else {
                Ok(())
            })
        },
    );
    assert_eq!(
        *seen_before_c.lock().unwrap(),
        ["fake: FAILED b: seed 7, batch 3: broke"]
    );
    assert_eq!(swept.failures, ["b: seed 7, batch 3: broke"]);
    assert_eq!((swept.checked, swept.chosen, swept.not_reached), (4, 4, 0));
    let summary = heard.lines(Said::Summary).join("\n");
    assert!(
        summary.contains("4 documents checked and 0 skipped of 4")
            && summary.contains("1 failures")
            && summary.contains("the slowest 4"),
        "{summary}"
    );
}

#[test]
fn the_slowest_documents_are_kept_slowest_first_and_at_most_twenty() {
    let items: Vec<u64> = (0..30).collect();
    let ids: Vec<String> = items.iter().map(|i| format!("d{i}")).collect();
    let swept = sweep(
        "fake",
        &ids,
        |s| s.as_str(),
        &Settings::default(),
        &|_, _| {},
        |id| {
            // d29 is the slowest.
            let n: u64 = id[1..].parse().unwrap();
            std::thread::sleep(Duration::from_millis(if n == 29 { 80 } else { 1 }));
            Outcome::Checked(Ok(()))
        },
    );
    assert_eq!(swept.slowest.len(), SLOWEST);
    assert_eq!(swept.slowest[0].1, "d29");
    assert!(swept.slowest.windows(2).all(|w| w[0].0 >= w[1].0));
}

#[test]
fn after_the_deadline_no_document_is_started_and_the_rest_are_counted() {
    let items: Vec<String> = (0..10).map(|i| format!("d{i}")).collect();
    let heard = Heard::default();
    let say = |s: Said, l: &str| heard.say(s, l);
    // A deadline that has passed: nothing runs.
    let past = Settings {
        deadline: Some(UNIX_EPOCH),
        ..one_worker()
    };
    let ran = AtomicUsize::new(0);
    let swept = sweep(
        "fake",
        &items,
        |s| s.as_str(),
        &past,
        &say,
        |_| {
            ran.fetch_add(1, Ordering::Relaxed);
            Outcome::Checked(Ok(()))
        },
    );
    assert_eq!(ran.load(Ordering::Relaxed), 0);
    assert_eq!((swept.checked, swept.not_reached), (0, 10));
    assert!(swept.verdict(0).unwrap_err().contains("10 of the 10"));
    assert_eq!(heard.lines(Said::Deadline).len(), 1);
    // A deadline that passes during the run: the document that is running finishes, and no other starts.
    let soon = Settings {
        deadline: Some(SystemTime::now() + Duration::from_millis(300)),
        ..one_worker()
    };
    let swept = sweep(
        "fake",
        &items,
        |s| s.as_str(),
        &soon,
        &|_, _| {},
        |_| {
            std::thread::sleep(Duration::from_millis(100));
            Outcome::Checked(Ok(()))
        },
    );
    assert!(
        (1..=4).contains(&swept.checked),
        "{} checked",
        swept.checked
    );
    assert_eq!(swept.checked + swept.not_reached, 10);
    assert!(swept.reach().unwrap().contains("of the 10 documents"));
}

#[test]
fn the_watchdog_names_a_slow_document_and_the_limit_fails_it_once() {
    let items = ["quick", "slow"];
    let heard = Heard::default();
    let say = |s: Said, l: &str| heard.say(s, l);
    let settings = Settings {
        watch: Duration::from_millis(100),
        limit: Duration::from_millis(400),
        progress: Duration::from_millis(150),
        ..one_worker()
    };
    let named_while_running = Mutex::new(Vec::new());
    let swept = sweep(
        "fake",
        &items,
        |s| s,
        &settings,
        &say,
        |item| {
            if *item == "slow" {
                std::thread::sleep(Duration::from_millis(900));
                // The watchdog said it while the document ran, so a job killed now would still have it in its log.
                *named_while_running.lock().unwrap() = heard.lines(Said::Failure);
                assert_eq!(heard.lines(Said::Watch).len(), 1);
            }
            Outcome::Checked(Ok(()))
        },
    );
    let watch = heard.lines(Said::Watch);
    assert!(
        watch.len() == 1 && watch[0].contains("slow is still running"),
        "{watch:?}"
    );
    let running = named_while_running.lock().unwrap().clone();
    assert!(
        running.len() == 1 && running[0].contains("slow: still running after"),
        "{running:?}"
    );
    // It counts once, with the time it took, and fails the verdict.
    assert!(swept.failures.is_empty(), "{:?}", swept.failures);
    assert_eq!(swept.over_limit.len(), 1, "{:?}", swept.over_limit);
    assert!(
        swept.over_limit[0].starts_with("slow: took")
            && swept.over_limit[0].contains("over the limit"),
        "{:?}",
        swept.over_limit
    );
    assert!(
        swept
            .verdict(0)
            .unwrap_err()
            .contains("over the time limit")
    );
    assert_eq!(heard.lines(Said::Failure).len(), 1);
    let progress = heard.lines(Said::Progress);
    assert!(
        progress
            .iter()
            .any(|l| l.contains("1 of 2 documents done") && l.contains("1 running")),
        "{progress:?}"
    );
}

#[test]
fn a_survey_says_a_miss_as_a_miss_and_lists_it_for_the_test_to_count() {
    let items = ["a", "b"];
    let heard = Heard::default();
    let say = |s: Said, l: &str| heard.say(s, l);
    let settings = Settings {
        misses: true,
        ..one_worker()
    };
    let swept = sweep(
        "survey",
        &items,
        |s| s,
        &settings,
        &say,
        |item| {
            Outcome::Checked(if *item == "b" {
                Err("does not line up".to_string())
            } else {
                Ok(())
            })
        },
    );
    assert!(heard.lines(Said::Failure).is_empty());
    assert_eq!(
        heard.lines(Said::Miss),
        ["survey: missed b: does not line up"]
    );
    assert_eq!(swept.failures, ["b: does not line up"]);
    assert!(heard.lines(Said::Summary)[0].contains("1 misses"));
}

/// A corpus set in a temporary directory: `n` documents of which only those of `keep` are on disk, so that reading any other
/// fails.
fn corpus_with(n: u64, keep: impl Fn(&str) -> bool) -> (tempfile::TempDir, Corpus) {
    let dir = tempfile::tempdir().unwrap();
    let set = dir.path().join("sets/curated");
    std::fs::create_dir_all(set.join("lab")).unwrap();
    let mut index = String::from(
        "id\ttier\texpect\tsha256\tsize\tpath\tproducer\tword_fp\tspdx\tstage\treason\tpart\n",
    );
    for d in 0..n {
        let sha = sha(d);
        let path = format!("lab/{d}.docx");
        index.push_str(&format!(
            "s:{d}.docx\tlab\tok\t{sha}\t1\t{path}\tx\tn\tMIT\t-\t-\t-\n"
        ));
        if keep(&sha) {
            std::fs::write(set.join(&path), crate::fixtures::contract::package()).unwrap();
        }
    }
    std::fs::write(set.join("INDEX.tsv"), index).unwrap();
    std::fs::write(set.join("COMPLETE"), "hash\n").unwrap();
    let corpus = Corpus::open(dir.path(), "curated").unwrap();
    (dir, corpus)
}

#[test]
fn a_shard_reads_and_counts_only_its_own_documents() {
    let shard = Shard { index: 2, count: 5 };
    // The documents of other shards are not on disk: reading one would fail the sweep.
    let (_dir, corpus) = corpus_with(40, |sha| shard.holds(sha));
    let mine = corpus
        .entries
        .iter()
        .filter(|e| shard.holds(&e.sha256))
        .count();
    assert!(mine > 0 && mine < 40);
    let settings = Settings {
        shard: Some(shard),
        ..Settings::default()
    };
    let laws = Arc::new(AtomicUsize::new(0));
    let swept = over(&corpus, "fake", &settings, &|_, _| {}, |_| {
        laws.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    assert!(swept.failures.is_empty(), "{:?}", swept.failures);
    assert_eq!(
        (swept.checked, swept.chosen, swept.in_set),
        (mine, mine, 40)
    );
    assert_eq!(laws.load(Ordering::Relaxed), mine);
    assert_eq!(swept.shard, Some(shard));
    // Its share of a minimum of 40 for the set is 8.
    assert_eq!(swept.verdict(40).is_ok(), mine >= 8);
}
