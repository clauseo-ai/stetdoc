//! What `replace`, `comment` and `edit` do with a .docx when the agent gives the least: no -o and no --in-place, a text in
//! another case, a comment for every place, and tracked changes that another author made.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use stetdoc_testkit::fixtures::{contract, parts, zip::Zip};

fn stetdoc(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(args)
        .current_dir(cwd)
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        .env("STETDOC_AUTHOR", "Clauseo")
        .env("XDG_CONFIG_HOME", cwd.join("no-config"))
        .output()
        .expect("run stetdoc")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn an_edit_with_no_output_writes_beside_the_input_and_never_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let original = contract::package();
    fs::write(cwd.join("contract.docx"), &original).unwrap();

    let first = stetdoc(cwd, &["replace", "contract.docx", "thirty", "sixty"]);
    let said = text(&first.stdout);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    assert!(
        said.contains("wrote contract.edited.docx\ncontract.docx is unchanged."),
        "{said}"
    );
    assert_eq!(fs::read(cwd.join("contract.docx")).unwrap(), original);

    // The same edit of the original again goes to the next free name, and says that file lacks the first edit.
    let again = stetdoc(
        cwd,
        &[
            "comment",
            "contract.docx",
            "Supplier",
            "Which one?",
            "--all",
        ],
    );
    let said = text(&again.stdout);
    assert!(said.contains("wrote contract.edited-2.docx"), "{said}");
    assert!(
        said.contains("contract.edited.docx already exists"),
        "{said}"
    );

    // An edit of an edited file takes the next number, so the chain reads in order.
    let chained = stetdoc(cwd, &["replace", "contract.edited.docx", "sixty", "ninety"]);
    let said = text(&chained.stdout);
    assert!(said.contains("wrote contract.edited-3.docx"), "{said}");
    assert!(!said.contains("already exists"), "{said}");

    // -o and --dry-run keep their meaning.
    let named = stetdoc(
        cwd,
        &[
            "replace",
            "contract.docx",
            "thirty",
            "sixty",
            "-o",
            "mine.docx",
        ],
    );
    assert!(text(&named.stdout).contains("wrote mine.docx\n"));
    let dry = stetdoc(
        cwd,
        &["replace", "contract.docx", "thirty", "sixty", "--dry-run"],
    );
    assert!(!text(&dry.stdout).contains("wrote"));
    assert!(!cwd.join("contract.edited-4.docx").exists());
}

#[test]
fn a_text_in_another_case_is_named_and_ignore_case_matches_it() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    fs::write(cwd.join("contract.docx"), contract::package()).unwrap();

    let refused = stetdoc(
        cwd,
        &[
            "replace",
            "contract.docx",
            "payment is due",
            "Payment falls due",
        ],
    );
    assert_eq!(refused.status.code(), Some(3));
    let said = text(&refused.stderr);
    assert!(
        said.contains("Ignoring case, it is there once:\n  p4 [Payment is due]"),
        "{said}"
    );
    assert!(
        said.contains("next: Nothing was applied. Run it again with --ignore-case"),
        "{said}"
    );

    let out = stetdoc(
        cwd,
        &[
            "replace",
            "contract.docx",
            "payment is due",
            "Payment falls due",
            "--ignore-case",
            "--dry-run",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("Payment {--is--}"));

    let comment = stetdoc(
        cwd,
        &[
            "comment",
            "contract.docx",
            "THE SUPPLIER",
            "Which one?",
            "--ignore-case",
            "--expect",
            "2",
            "--dry-run",
        ],
    );
    assert_eq!(comment.status.code(), Some(0), "{}", text(&comment.stderr));
}

#[test]
fn comment_takes_all_and_expect_as_replace_does() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    fs::write(cwd.join("contract.docx"), contract::package()).unwrap();

    let all = stetdoc(
        cwd,
        &[
            "comment",
            "contract.docx",
            "Supplier",
            "Which one?",
            "--all",
            "--dry-run",
        ],
    );
    let said = text(&all.stdout);
    assert!(
        said.contains("comment on \"Supplier\": 2 times, in 2 blocks: p1, p2"),
        "{said}"
    );
    assert!(
        said.contains("{c0}Supplier{/c0}") && said.contains("{c1}Supplier{/c1}"),
        "{said}"
    );

    let wrong = stetdoc(
        cwd,
        &[
            "comment",
            "contract.docx",
            "Supplier",
            "Which one?",
            "--expect",
            "3",
        ],
    );
    assert_eq!(wrong.status.code(), Some(3));
    assert!(text(&wrong.stderr).starts_with("error[count_mismatch]"));
}

#[test]
fn every_receipt_and_the_view_name_the_changes_another_author_made() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let body = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">Rated at least </w:t></w:r>"#,
        r#"<w:ins w:id="1" w:author="Dana Chen" w:date="2026-01-02T10:00:00Z"><w:r><w:t>A-</w:t></w:r></w:ins></w:p>"#,
        r#"<w:p><w:r><w:t>Payment is due within thirty days.</w:t></w:r></w:p>"#,
    );
    let doc = parts::Doc::new().body(body).bytes();
    fs::write(
        cwd.join("rated.docx"),
        Zip::new(parts::entries_doc(&doc)).build(),
    )
    .unwrap();
    let line = "1 tracked change by Dana Chen you did not make: #0 insert p1 \"A-\".\nTell the user about it.\n";

    let view = text(&stetdoc(cwd, &["view", "rated.docx"]).stdout);
    assert!(
        view.lines()
            .nth(1)
            .is_some_and(|l| l.starts_with("1 tracked change by Dana Chen")),
        "{view}"
    );
    let replaced = text(
        &stetdoc(
            cwd,
            &["replace", "rated.docx", "thirty", "sixty", "--dry-run"],
        )
        .stdout,
    );
    assert!(replaced.ends_with(line), "{replaced}");
    let commented = text(
        &stetdoc(
            cwd,
            &["comment", "rated.docx", "thirty", "Why?", "--dry-run"],
        )
        .stdout,
    );
    assert!(commented.ends_with(line), "{commented}");

    // The author's own changes are not named, and the ledger, which lists every change, has no line.
    let own = stetdoc(
        cwd,
        &[
            "replace",
            "rated.docx",
            "thirty",
            "sixty",
            "--author",
            "Dana Chen",
            "--dry-run",
        ],
    );
    assert!(!text(&own.stdout).contains("did not make"));
    let ledger = text(&stetdoc(cwd, &["view", "rated.docx", "--revisions"]).stdout);
    assert!(!ledger.contains("did not make"), "{ledger}");
}

#[test]
fn a_replace_of_three_hundred_places_prints_a_receipt_an_agent_can_read_whole() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let sentence = "The Issuer shall pay the Issuer's costs, and the Issuer shall keep its books where the Trustee can see them.";
    let body: String = (1..=100)
        .map(|i| format!("<w:p><w:r><w:t>{i}. {sentence}</w:t></w:r></w:p>"))
        .collect();
    let doc = parts::Doc::new().body(&body).bytes();
    fs::write(
        cwd.join("long.docx"),
        Zip::new(parts::entries_doc(&doc)).build(),
    )
    .unwrap();

    let out = stetdoc(
        cwd,
        &[
            "replace",
            "long.docx",
            "the Issuer",
            "the Company",
            "--all",
            "--ignore-case",
            "-o",
            "out.docx",
        ],
    );
    let said = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    // Under the room an agent's tool output has, and still saying what happened and where the rest is.
    assert!(said.len() < 12_500, "{} bytes", said.len());
    assert!(
        said.contains("applied 300 operations, tracked as Clauseo"),
        "{said}"
    );
    assert!(said.contains(" more operations, not listed here"), "{said}");
    assert!(said.contains(" more blocks not shown here"), "{said}");
    assert!(
        said.contains("`stetdoc view FILE --revisions --context 1`"),
        "{said}"
    );
    assert!(said.contains("\nwrote out.docx\n"), "{said}");
}
