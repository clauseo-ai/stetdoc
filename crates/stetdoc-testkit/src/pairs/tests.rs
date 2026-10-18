use super::*;
use std::path::Path;

fn committed() -> Vec<Pair> {
    parse(MANIFEST).expect("corpus/pairs.tsv parses")
}

fn count(pairs: &[Pair], class: &str, curated: bool) -> usize {
    pairs
        .iter()
        .filter(|p| p.class == class && (!curated || p.curated))
        .count()
}

#[test]
fn the_manifest_holds_the_pairs_of_the_four_classes() {
    let pairs = committed();
    let all: Vec<usize> = ["lab", "oxpt", "dox", "dcb"]
        .iter()
        .map(|c| count(&pairs, c, false))
        .collect();
    assert_eq!(all, [354, 184, 200, 41]);
    let curated: Vec<usize> = ["lab", "oxpt", "dox", "dcb"]
        .iter()
        .map(|c| count(&pairs, c, true))
        .collect();
    assert_eq!(curated, [40, 181, 0, 20]);
}

#[test]
fn every_class_of_the_manifest_has_a_gate_and_its_rows_match_the_counts() {
    let pairs = committed();
    let baseline = Baseline::committed();
    for class in ["lab", "oxpt", "dox", "dcb"] {
        assert!(baseline.gate(class).is_some(), "{class} has no gate");
        for scope in [Scope::Curated, Scope::Full] {
            let n = pairs
                .iter()
                .filter(|p| p.class == class && scope.takes(p))
                .count();
            match baseline.row(scope, class) {
                Some(row) => assert_eq!(row.pairs, n, "{} {class}", scope.name()),
                None => assert_eq!(n, 0, "{} {class} has pairs and no row", scope.name()),
            }
        }
    }
}

#[test]
fn the_scope_follows_the_set() {
    assert_eq!(Scope::of_set("curated-0123456789ab"), Scope::Curated);
    assert_eq!(Scope::of_set("curated"), Scope::Curated);
    assert_eq!(Scope::of_set("pairs-0123456789ab"), Scope::Full);
    assert_eq!(Scope::of_set("full"), Scope::Full);
    assert_eq!(
        pair_set("curated-0123456789ab"),
        "curatedpairs-0123456789ab"
    );
    assert_eq!(pair_set("curated"), "curatedpairs");
    assert_eq!(pair_set("pairs-0123456789ab"), "pairs-0123456789ab");
    assert_eq!(pair_set("full"), "full");
}

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn pair(curated: bool, outlier: &str) -> String {
    let set = if curated { "curated" } else { "full" };
    format!("# header\nlab\t{set}\t{A}\t{B}\tlab:x\t{outlier}\n")
}

#[test]
fn a_pair_resolves_against_a_set_by_hash_and_a_missing_document_is_an_error() {
    let index = format!(
        "h\na:1\tlab\tok\t{A}\t1\tlab/a.docx\tp\tn\tMIT\t-\t-\t-\nb:1\tlab\tok\t{B}\t1\tlab/b.docx\tp\tn\tMIT\t-\t-\t-\n"
    );
    let corpus = Corpus {
        entries: crate::corpus::parse_index(&index, Path::new("/set")).expect("index"),
    };
    let pairs = parse(&pair(true, "-")).expect("parse");
    let r = resolve(&pairs, &corpus, Scope::Curated).expect("resolves");
    assert_eq!(r[0].revised.path, Path::new("/set/lab/b.docx"));
    let half = Corpus {
        entries: corpus.entries[..1].to_vec(),
    };
    let err = resolve(&pairs, &half, Scope::Curated).expect_err("b is missing");
    assert!(err.contains("pairs.tsv:2 lab:x"), "{err}");
    let full_only = parse(&pair(false, "-")).expect("parse");
    assert!(
        resolve(&full_only, &half, Scope::Curated)
            .expect("skipped")
            .is_empty()
    );
    assert!(resolve(&full_only, &half, Scope::Full).is_err());
    assert!(parse("lab\tsome\ta\tb\tl\t-\n").is_err());
    assert!(parse("lab\tfull\ta\tb\n").is_err());
}

#[test]
fn an_unrecorded_outlier_and_a_recorded_one_under_the_gate_fail() {
    let gate = Gate { c: 1.5 };
    let within = Figures {
        min_chars: 100,
        min_spans: 10,
        red_chars: 190,
        red_spans: 12,
        fmt_spans: 0,
    };
    let chars = Figures {
        red_chars: 191,
        ..within
    };
    let spans = Figures {
        red_spans: 13,
        ..within
    };
    let plain = &parse(&pair(true, "-")).expect("parse")[0];
    let recorded = &parse(&pair(true, "a short pair")).expect("parse")[0];
    assert_eq!(gate.verdict(plain, &within), None);
    assert_eq!(gate.verdict(recorded, &chars), None);
    assert_eq!(gate.verdict(recorded, &spans), None);
    let e = gate.verdict(plain, &chars).expect("unrecorded");
    assert!(e.contains("not recorded") && e.contains("chars 191"), "{e}");
    let e = gate.verdict(plain, &spans).expect("unrecorded");
    assert!(e.contains("spans 13 > 10 + 2"), "{e}");
    let e = gate.verdict(recorded, &within).expect("stale");
    assert!(
        e.contains("within the gate") && e.contains("a short pair"),
        "{e}"
    );
}

#[test]
fn the_totals_of_a_class_may_not_grow_past_the_baseline() {
    let row = Row {
        pairs: 2,
        min_chars: 1000,
        red_chars: 1200,
        fmt_spans: 100,
        excluded: 1,
        c: 1.7,
    };
    let mut t = Totals::default();
    t.add(
        &Figures {
            min_chars: 1000,
            red_chars: 1260,
            fmt_spans: 107,
            ..Figures::default()
        },
        true,
    );
    assert!(
        t.against("lab", &row).is_empty(),
        "{:?}",
        t.against("lab", &row)
    );
    t.add(
        &Figures {
            min_chars: 0,
            red_chars: 1,
            fmt_spans: 1,
            ..Figures::default()
        },
        true,
    );
    let e = t.against("lab", &row);
    assert_eq!(e.len(), 3, "{e:?}");
    assert_eq!(
        t.row_text(Scope::Full, "lab", 1.561),
        "full\tlab\t2\t1000\t1261\t108\t2\t1.561\t1.72"
    );
}

#[test]
fn the_p99_leaves_out_the_pairs_the_slack_decides() {
    let f = |min_chars, red_chars| Figures {
        min_chars,
        red_chars,
        ..Figures::default()
    };
    assert_eq!(p99(&[f(0, 160), f(39, 500)]), 1.0);
    let many: Vec<Figures> = (0..100).map(|i| f(100, 100 + i)).collect();
    assert_eq!(p99(&many), 1.99);
}
