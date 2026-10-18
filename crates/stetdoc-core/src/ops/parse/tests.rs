//! Batches: the two syntaxes, the table that describes the operations, and what they refuse.

use crate::api::batch::Batch;
use crate::api::error::ErrorCode;
use crate::api::limits::Limits;
use crate::api::op::Op;
use crate::api::op_spec::OPS;
use std::collections::BTreeSet;

const BASE: &str = "3f9a0c11d2b4e577:7:51ac09f2e8b34d11";

fn limits() -> Limits {
    Limits::default()
}

fn json_batch(ops: &str) -> String {
    format!("{{\"base\":\"{BASE}\",\"ops\":[{ops}]}}")
}

fn code(text: &str) -> ErrorCode {
    Batch::parse(text, &limits())
        .err()
        .map(|e| e.code())
        .unwrap_or(ErrorCode::Internal)
}

#[test]
fn every_entry_of_the_table_reads_as_its_operation() {
    // H1, the first half: the example of each entry is an operation of that name, whose keys the entry lists.
    assert_eq!(OPS.len(), 19);
    for spec in OPS {
        let op: Op =
            serde_json::from_str(spec.example).unwrap_or_else(|e| panic!("{}: {e}", spec.name));
        assert_eq!(op.name(), spec.name);
        assert_eq!(op.spec().name, spec.name);
        let value: serde_json::Value = serde_json::from_str(spec.example).unwrap();
        for key in value.as_object().unwrap().keys().filter(|k| *k != "op") {
            assert!(
                spec.fields.iter().any(|f| f.name == key),
                "{} lists no field {key}",
                spec.name
            );
        }
        // And it is a batch that validates.
        let batch = json_batch(spec.example);
        Batch::parse(&batch, &limits()).unwrap_or_else(|e| panic!("{}: {e}", spec.name));
    }
}

#[test]
fn the_fields_of_each_entry_are_the_properties_of_its_payload() {
    // H1, the second half: no field is accepted that the table does not list, and none is listed that is not accepted.
    let schema = serde_json::to_value(schemars::schema_for!(Op)).unwrap();
    let variants = schema["oneOf"].as_array().expect("Op is a oneOf");
    assert_eq!(variants.len(), OPS.len());
    for spec in OPS {
        let variant = variants
            .iter()
            .find(|v| v["properties"]["op"]["const"] == spec.name)
            .unwrap_or_else(|| panic!("no schema for {}", spec.name));
        let properties: BTreeSet<&str> = variant["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .filter(|k| *k != "op")
            .collect();
        let listed: BTreeSet<&str> = spec.fields.iter().map(|f| f.name).collect();
        assert_eq!(properties, listed, "{}", spec.name);
        let required: BTreeSet<&str> = variant["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r.as_str())
            .filter(|k| *k != "op")
            .collect();
        let needed: BTreeSet<&str> = spec
            .fields
            .iter()
            .filter(|f| f.required)
            .map(|f| f.name)
            .collect();
        assert_eq!(required, needed, "required fields of {}", spec.name);
    }
}

#[test]
fn json_refuses_what_the_table_does_not_list() {
    let table = [
        (
            r#"{"op":"replace","ref":"p1","with":"x","colour":"red"}"#,
            "unknown field",
        ),
        (r#"{"op":"frobnicate","ref":"p1"}"#, "unknown variant"),
        (r#"{"op":"replace","ref":"p1"}"#, "missing field"),
        (r#"{"op":"replace","ref":"p1","with":5}"#, "invalid type"),
    ];
    for (op, why) in table {
        let e = Batch::parse(&json_batch(op), &limits())
            .err()
            .unwrap_or_else(|| panic!("{op} was accepted"));
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{op}");
        assert!(e.message().contains(why), "{op}: {}", e.message());
    }
}

#[test]
fn a_bare_array_an_empty_batch_and_a_missing_base_are_refused() {
    assert_eq!(code("[]"), ErrorCode::InvalidPayload);
    assert_eq!(code(""), ErrorCode::InvalidPayload);
    assert_eq!(code("   \n"), ErrorCode::InvalidPayload);
    assert_eq!(code(&json_batch("")), ErrorCode::InvalidPayload);
    assert_eq!(
        code(r#"{"ops":[{"op":"merge","ref":"p1"}]}"#),
        ErrorCode::InvalidPayload
    );
    assert_eq!(code("---\n@merge\nref: p1\n"), ErrorCode::InvalidPayload);
    let e = Batch::parse(
        &format!(
            "{{\"base\":\"p1\",\"ops\":[{}]}}",
            r#"{"op":"merge","ref":"p1"}"#
        ),
        &limits(),
    )
    .err()
    .unwrap();
    assert!(e.message().contains("base token"), "{}", e.message());
}

#[test]
fn the_fields_of_each_operation_are_checked_before_anything_is_staged() {
    let table = [
        (
            r#"{"op":"replace","ref":"p1","find":"a","start":1,"end":2,"with":"x"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"replace","ref":"p1","start":3,"end":2,"with":"x"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"replace","ref":"p1","start":1,"with":"x"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"replace","ref":"p1","occurrence":1,"with":"x"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"replace","ref":"p1","find":"","with":"x"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"replace","ref":"p1","with":""}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"replace","ref":"x1","with":"y"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"replace","ref":"p0","with":"y"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"replace","ref":"p1","with":"a\u0001b"}"#,
            ErrorCode::InvalidPayload,
        ),
        (r#"{"op":"split","ref":"p1"}"#, ErrorCode::InvalidPayload),
        (
            r#"{"op":"split","ref":"p1","at":3,"before":"x"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"list","ref":"p1","level":1,"remove":true}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"list","ref":"p1","level":9}"#,
            ErrorCode::InvalidPayload,
        ),
        (r#"{"op":"accept"}"#, ErrorCode::InvalidPayload),
        (
            r#"{"op":"accept","all":true,"author":"JD"}"#,
            ErrorCode::InvalidPayload,
        ),
        (r#"{"op":"reject","ids":[]}"#, ErrorCode::InvalidPayload),
        (
            r#"{"op":"reply","comment":"p3","text":"x"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"reply","comment":"c","text":"x"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"delete-note","note":"x3"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"paragraph","ref":"p1","align":"middle"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"row","ref":"r1","action":"duplicate"}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"insert","ref":"p1","place":"after","paragraphs":[]}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"set","props":[{"name":"jc"}]}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"set","ref":"p1","props":[]}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"set","ref":"p1","props":[{"name":"two words"}]}"#,
            ErrorCode::InvalidPayload,
        ),
    ];
    for (op, expected) in table {
        let e = Batch::parse(&json_batch(op), &limits())
            .err()
            .unwrap_or_else(|| panic!("{op} was accepted"));
        assert_eq!(e.code(), expected, "{op}: {}", e.message());
        assert_eq!(e.op_index(), Some(0), "{op}");
    }
    // The index is the operation's.
    let two = json_batch(&format!(
        "{},{}",
        r#"{"op":"merge","ref":"p1"}"#, r#"{"op":"merge","ref":"zz"}"#
    ));
    assert_eq!(
        Batch::parse(&two, &limits()).err().unwrap().op_index(),
        Some(1)
    );
}

#[test]
fn bare_strings_and_objects_both_make_paragraphs() {
    let batch = Batch::parse(
        &json_batch(r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["one",{"text":"two","style":"Heading 2"}]}"#),
        &limits(),
    )
    .unwrap();
    let Op::Insert(insert) = &batch.ops[0] else {
        panic!("not an insert")
    };
    assert_eq!(insert.paragraphs[0].text, "one");
    assert_eq!(insert.paragraphs[0].style, None);
    assert_eq!(insert.paragraphs[1].style.as_deref(), Some("Heading 2"));
}

#[test]
fn block_text_reads_the_example_in_its_own_documentation() {
    let text = format!(
        "base: {BASE}\n---\n@replace\nref: p42\nfind: the Supplier shall\nwith: the Supplier must\n---\n@insert\nref: p42\nplace: after\nparagraphs: <<END\nA first new paragraph.\nA second one.\nEND\n"
    );
    let batch = Batch::parse(&text, &limits()).unwrap();
    assert_eq!(batch.base, BASE);
    assert_eq!(batch.ops.len(), 2);
    let Op::Replace(r) = &batch.ops[0] else {
        panic!("not a replace")
    };
    assert_eq!(
        (r.r#ref.as_str(), r.find.as_deref(), r.with.as_str()),
        ("p42", Some("the Supplier shall"), "the Supplier must")
    );
    let Op::Insert(i) = &batch.ops[1] else {
        panic!("not an insert")
    };
    assert_eq!(i.paragraphs.len(), 2);
    assert_eq!(i.paragraphs[1].text, "A second one.");
}

#[test]
fn block_text_keeps_every_character_of_a_heredoc() {
    let text = format!(
        "base: {BASE}\n@replace\nref: p1\nwith: <<EOT\n  braces {{}} and \\ and <<END and \"quotes\"\n\ttabbed\n\nEND\nEOT\n"
    );
    let batch = Batch::parse(&text, &limits()).unwrap();
    let Op::Replace(r) = &batch.ops[0] else {
        panic!("not a replace")
    };
    assert_eq!(
        r.with,
        "  braces {} and \\ and <<END and \"quotes\"\n\ttabbed\n\nEND"
    );
}

#[test]
fn block_text_refuses_what_it_cannot_read() {
    let head = format!("base: {BASE}\n");
    let table = [
        (
            format!("{head}@frobnicate\nref: p1\n"),
            "is not an operation",
        ),
        (
            format!("{head}@merge\nref: p1\ncolour: red\n"),
            "is not a field of",
        ),
        (format!("{head}@merge\nref: p1\nref: p2\n"), "given twice"),
        (
            format!("{head}@split\nref: p1\nat: seven\n"),
            "whole number",
        ),
        (
            format!("{head}@resolve\ncomment: c1\nreopen: maybe\n"),
            "true",
        ),
        (
            format!("{head}@replace\nref: p1\nwith: <<END\nunfinished\n"),
            "closing line",
        ),
        (
            format!("{head}@replace\nref: <<END\np1\nEND\nwith: x\n"),
            "heredoc",
        ),
        (format!("{head}ref: p1\n"), "before the first"),
        (format!("{head}@merge\nref: p1\r\n"), "carriage return"),
        (format!("{head}@merge\njust some words\n"), "expected"),
        (format!("{head}@replace\nref: p1\n"), "not complete"),
        (format!("{head}base: {BASE}\n@merge\nref: p1\n"), "twice"),
    ];
    for (text, why) in table {
        let e = Batch::parse(&text, &limits())
            .err()
            .unwrap_or_else(|| panic!("accepted: {text:?}"));
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{text:?}");
        assert!(e.message().contains(why), "{text:?}: {}", e.message());
    }
}

#[test]
fn a_list_may_repeat_its_key_and_a_property_line_has_a_name_a_value_and_pairs() {
    let text = format!(
        "base: {BASE}\n@set\nref: p1\nprops: <<END\njc center\nspacing before=6pt after=0\nEND\nclear: keepNext\nclear: keepLines\n"
    );
    let batch = Batch::parse(&text, &limits()).unwrap();
    let Op::Set(set) = &batch.ops[0] else {
        panic!("not a set")
    };
    assert_eq!(set.props[0].name, "jc");
    assert_eq!(set.props[0].val.as_deref(), Some("center"));
    assert_eq!(
        set.props[1].attrs.get("before").map(String::as_str),
        Some("6pt")
    );
    assert_eq!(
        set.clear.as_deref(),
        Some(&["keepNext".to_string(), "keepLines".to_string()][..])
    );
}

/// A small generator: the same seed gives the same batch.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n.max(1)
    }

    fn text(&mut self, lines: bool) -> String {
        const PIECES: [&str; 14] = [
            "the Supplier",
            "shall",
            "\"quoted\"",
            "it's",
            "{braces}",
            "back\\slash",
            "<<END",
            "END",
            " leading",
            "trailing ",
            "tab\there",
            "\u{201C}curly\u{201D}",
            "emoji \u{1F600}",
            "caf\u{e9}",
        ];
        let mut s = String::new();
        for i in 0..1 + self.next(4) {
            if i > 0 {
                s.push_str(if lines && self.next(3) == 0 {
                    "\n"
                } else {
                    " "
                });
            }
            s.push_str(PIECES[self.next(PIECES.len())]);
        }
        s
    }

    fn op(&mut self) -> Op {
        let r = |l: &mut Lcg| format!("p{}", 1 + l.next(900));
        let json = match self.next(9) {
            0 => {
                serde_json::json!({"op":"replace","ref":r(self),"find":self.text(false),"with":self.text(true)})
            }
            1 => {
                serde_json::json!({"op":"replace","ref":r(self),"start":self.next(10),"end":10 + self.next(10),"with":self.text(true)})
            }
            2 => {
                serde_json::json!({"op":"insert","ref":r(self),"place":"before","paragraphs":[self.text(false), self.text(false)]})
            }
            3 => serde_json::json!({"op":"delete","ref":r(self),"to":r(self)}),
            4 => serde_json::json!({"op":"split","ref":r(self),"at":self.next(100)}),
            5 => {
                serde_json::json!({"op":"comment","ref":r(self),"find":self.text(false),"text":self.text(true),"initials":"JD"})
            }
            6 => serde_json::json!({"op":"accept","ids":[self.next(50), 60 + self.next(50)]}),
            7 => {
                serde_json::json!({"op":"format","ref":r(self),"bold":true,"size":"12pt","clear":[self.text(false)]})
            }
            _ => serde_json::json!({"op":"reject","author":self.text(false)}),
        };
        serde_json::from_value(json).unwrap()
    }
}

#[test]
fn the_two_syntaxes_agree_on_every_batch_that_block_text_can_say() {
    // H2: parse(to_json(b)) == b and parse(to_block(b)) == b, on generated batches.
    let mut lcg = Lcg(7);
    for round in 0..300 {
        let ops: Vec<Op> = (0..1 + lcg.next(4)).map(|_| lcg.op()).collect();
        let batch = Batch::new(BASE, ops);
        assert_eq!(
            Batch::parse(&batch.to_json(), &limits()).unwrap(),
            batch,
            "json, round {round}"
        );
        let block = batch.to_block().unwrap();
        assert_eq!(
            Batch::parse(&block, &limits()).unwrap(),
            batch,
            "block, round {round}:\n{block}"
        );
    }
}

#[test]
fn what_block_text_cannot_say_is_refused_by_the_writer() {
    let styled = Batch::new(
        BASE,
        vec![serde_json::from_value(serde_json::json!({"op":"insert","ref":"p1","place":"after","paragraphs":[{"text":"x","style":"Heading 1"}]})).unwrap()],
    );
    assert!(styled.to_block().is_err());
    let newline = Batch::new(
        BASE,
        vec![serde_json::from_value(
            serde_json::json!({"op":"insert","ref":"p1","place":"after","paragraphs":["one\ntwo"]}),
        )
        .unwrap()],
    );
    assert!(newline.to_block().is_err());
    let children = Batch::new(
        BASE,
        vec![serde_json::from_value(serde_json::json!({"op":"set","ref":"p1","props":[{"name":"pBdr","children":[{"name":"top","val":"single"}]}]})).unwrap()],
    );
    assert!(children.to_block().is_err());
    // JSON says all of it.
    for batch in [styled, newline, children] {
        assert_eq!(Batch::parse(&batch.to_json(), &limits()).unwrap(), batch);
    }
}

#[test]
fn the_limits_are_met_before_the_batch_is_read() {
    // H9, the parse side.
    let one = r#"{"op":"merge","ref":"p1"}"#;
    let few = Limits {
        max_batch_ops: 2,
        ..Limits::default()
    };
    let three = json_batch(&format!("{one},{one},{one}"));
    assert_eq!(
        Batch::parse(&three, &few).err().unwrap().code(),
        ErrorCode::LimitExceeded
    );
    let small = Limits {
        max_batch_bytes: 40,
        ..Limits::default()
    };
    assert_eq!(
        Batch::parse(&three, &small).err().unwrap().code(),
        ErrorCode::LimitExceeded
    );
    let short = Limits {
        max_op_text_bytes: 4,
        ..Limits::default()
    };
    let long = json_batch(r#"{"op":"replace","ref":"p1","with":"more than four"}"#);
    assert_eq!(
        Batch::parse(&long, &short).err().unwrap().code(),
        ErrorCode::LimitExceeded
    );
    // The bytes of all the text, not only of one field.
    let total = Limits {
        max_batch_bytes: 120,
        ..Limits::default()
    };
    let many = json_batch(&format!("{one},{one}"));
    assert!(Batch::parse(&many, &total).is_ok());
    let text_op =
        r#"{"op":"replace","ref":"p1","with":"0123456789012345678901234567890123456789"}"#;
    assert_eq!(
        Batch::parse(&json_batch(&format!("{text_op},{text_op}")), &total)
            .err()
            .unwrap()
            .code(),
        ErrorCode::LimitExceeded
    );
}
