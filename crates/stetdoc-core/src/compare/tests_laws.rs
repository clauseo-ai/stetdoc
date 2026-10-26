//! The compare laws on a redline (design `compare.md`, 9.1 and 20): accepting every revision gives the revised document and
//! rejecting every revision gives the original, both read through `tests_dump::dump`, with the blocks that the receipt excludes
//! left out of both sides and everything else compared; N4 (locality), N7 (moves) and the noise of N9.
use super::import::list_definitions;
use super::receipt::CompareReceipt;
use super::tests_dump::{DumpOpts, dump, paired_slots};
use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::revisions::index::ChangeIndex;
use crate::revisions::resolve::resolve;
use crate::revisions::stamp::RevStamp;
use crate::revisions::types::{Action, Selector, Step};
use crate::state::State;
use crate::wordml::{BlockKind, Mode, Paragraph, StoryKind};
use std::collections::{HashMap, HashSet};

/// The stamp the laws use.
pub(super) fn stamp() -> RevStamp {
    RevStamp::new("Clauseo", "2026-10-05T14:03:27Z").expect("a stamp")
}

/// The state with every revision accepted or rejected.
pub(super) fn resolved(state: &State, action: Action) -> Result<State, StetError> {
    let mut tx = state.begin();
    resolve(
        &mut tx,
        &mut ChangeIndex::new(),
        &[Step {
            action,
            selector: Selector::All,
        }],
    )?;
    Ok(tx.commit()?.0)
}

/// The bytes of a subtree as a string: names, attributes and text, nothing left out.
fn bytes_of(n: &crate::xml::NodeRef<'_>, out: &mut String) {
    use crate::xml::NodeKind;
    match n.kind() {
        NodeKind::Element => {
            out.push('<');
            out.push_str(n.namespace());
            out.push('|');
            out.push_str(n.local_name());
            for a in n.attrs() {
                out.push_str(&format!(
                    " {}|{}={}",
                    a.namespace(),
                    a.local_name(),
                    a.value()
                ));
            }
            out.push('>');
            for c in n.children() {
                bytes_of(&c, out);
            }
            out.push_str("</>");
        }
        NodeKind::Text | NodeKind::CData => out.push_str(&n.text()),
        _ => {}
    }
}

/// N4: the paragraphs that no call wrote to are the revised document's, byte for byte.
pub(super) fn locality(
    revised: &State,
    redline: &State,
    touched: &[crate::wordml::BlockKey],
) -> Result<Option<String>, StetError> {
    let touched: std::collections::HashSet<(String, crate::wordml::NodeId)> = touched
        .iter()
        .map(|k| (k.part().to_string(), k.node()))
        .collect();
    let (vr, vd) = (revised.parts(), redline.parts());
    for spec in vr.story_specs()?.0.iter() {
        let (Some(ir), Some(id)) = (vr.id(&spec.part), vd.id(&spec.part)) else {
            continue;
        };
        let (story, tr, td) = (vr.story(ir)?, vr.tree(ir)?, vd.tree(id)?);
        for e in story.blocks() {
            if e.kind != BlockKind::Paragraph || touched.contains(&(spec.part.to_string(), e.node))
            {
                continue;
            }
            let (mut a, mut b) = (String::new(), String::new());
            bytes_of(&tr.node_ref(e.node), &mut a);
            bytes_of(&td.node_ref(e.node), &mut b);
            if a != b {
                return Ok(Some(format!(
                    "N4 locality: a paragraph that no call wrote to changed in {}: {}",
                    spec.part,
                    a.chars().take(200).collect::<String>()
                )));
            }
        }
    }
    Ok(None)
}

/// N7: each move names a deleted block and an inserted block of the redline whose comparison texts are alike (the texts and
/// the bags of words of `text.rs`, which are what the aligner scores).
pub(super) fn moves_are_real(redline: &State, receipt: &CompareReceipt) -> Option<String> {
    use super::similarity::dice;
    use super::text::{bag, key_text};
    let view = redline.parts();
    for m in &receipt.moves {
        // the text of a block, paragraph or table, whatever the revisions around it
        let text = |k: &crate::wordml::BlockKey| -> Option<String> {
            let id = view.id(k.part())?;
            let tree = view.tree(id).ok()?;
            Some(tree.node_ref(k.node()).text_content())
        };
        let (Some(from), Some(to)) = (text(&m.from), text(&m.to)) else {
            return Some("N7 moves: a move names a block that is not in the redline".to_owned());
        };
        let d = dice(&bag(&key_text(&from)), &bag(&key_text(&to)));
        if d < 0.8 {
            return Some(format!(
                "N7 moves: a move whose texts have a Dice score of {d:.2}: {from:?} / {to:?}"
            ));
        }
    }
    None
}

/// N9: how much the redline changes, against the minimal diff of the same texts.
#[derive(Clone, Debug, Default)]
pub(super) struct Noise {
    /// Characters and edits of the minimal diff of the texts, summed over the stories.
    pub min_chars: usize,
    /// As above.
    pub min_spans: usize,
    /// Characters of the redline's inserted and deleted text, a paragraph mark counting one.
    pub red_chars: usize,
    /// The text changes of the redline.
    pub red_spans: usize,
    /// The formatting changes of the redline.
    pub fmt_spans: usize,
}

/// The text of each story by a key that pairs it with the other document's: the body, the footnotes and the endnotes by
/// kind, and each header and footer part by name.
fn story_texts(state: &State, mode: Mode) -> Result<HashMap<String, String>, StetError> {
    let view = state.parts();
    let mut out = HashMap::new();
    for spec in view.story_specs()?.0.iter() {
        let key = match spec.kind {
            StoryKind::Body => "body".to_owned(),
            StoryKind::Footnotes => "footnotes".to_owned(),
            StoryKind::Endnotes => "endnotes".to_owned(),
            StoryKind::Header | StoryKind::Footer => spec.part.to_string(),
            _ => continue,
        };
        let Some(id) = view.id(&spec.part) else {
            continue;
        };
        let (story, tree) = (view.story(id)?, view.tree(id)?);
        let mut lines = Vec::new();
        for e in story.blocks() {
            if e.kind == BlockKind::Paragraph
                && let Some(p) = Paragraph::new(tree, e.node)
            {
                lines.push(p.text(mode));
            }
        }
        out.insert(key, lines.join("\n"));
    }
    Ok(out)
}

/// The noise of a redline. The minimal diff is taken per paired story: the body with the body, the notes with the notes of their
/// kind (a part on one side only against nothing), and a header or footer with the one of its slot (a slot on one side only is not
/// compared, so it is left out).
pub(super) fn noise(
    original: &State,
    revised: &State,
    redline: &State,
) -> Result<Noise, StetError> {
    use crate::revisions::changes::{ChangeKind, changes};
    use crate::revisions::diff::{Cleanup, edits};
    let mut n = Noise::default();
    let (a, b) = (
        story_texts(original, Mode::Final)?,
        story_texts(revised, Mode::Final)?,
    );
    let empty = String::new();
    let mut pairs: Vec<(&String, &String)> = Vec::new();
    for key in ["body", "footnotes", "endnotes"] {
        let (x, y) = (a.get(key), b.get(key));
        if x.is_some() || y.is_some() {
            pairs.push((x.unwrap_or(&empty), y.unwrap_or(&empty)));
        }
    }
    let (da, db) = (
        dump(original, &DumpOpts::default())?,
        dump(revised, &DumpOpts::default())?,
    );
    for (_, pa, pb) in paired_slots(&da, &db) {
        if let (Some(x), Some(y)) = (a.get(&pa), b.get(&pb)) {
            pairs.push((x, y));
        }
    }
    for (x, y) in pairs {
        for e in edits(x, y, Cleanup::Minimal) {
            n.min_chars += e.range.len() + e.with.chars().count();
            n.min_spans += 1;
        }
    }
    let view = redline.parts();
    for spec in view.story_specs()?.0.iter() {
        let Some(id) = view.id(&spec.part) else {
            continue;
        };
        let (story, tree) = (view.story(id)?, view.tree(id)?);
        for c in changes(&story, tree, None) {
            match c.kind {
                ChangeKind::RunFormat
                | ChangeKind::ParagraphFormat
                | ChangeKind::SectionFormat
                | ChangeKind::TableFormat
                | ChangeKind::RowFormat
                | ChangeKind::CellFormat
                | ChangeKind::GridChange
                | ChangeKind::NumberingChange => n.fmt_spans += 1,
                _ => n.red_spans += 1,
            }
        }
        // the characters of every insertion and deletion: text and field code, not what a text box or a fallback repeats
        fn chars_in(n: &crate::xml::NodeRef<'_>) -> usize {
            n.children()
                .map(|c| {
                    if !c.is_element() {
                        return c.text().chars().count();
                    }
                    match c.local_name() {
                        "txbxContent" | "Fallback" | "rPr" => 0,
                        _ => chars_in(&c),
                    }
                })
                .sum()
        }
        let mut stack = vec![tree.root()];
        while let Some(x) = stack.pop() {
            for c in x.child_elements() {
                let l = c.local_name();
                if matches!(l, "ins" | "del") && c.namespace().ends_with("/main") {
                    let parent = c.parent().map(|p| p.local_name());
                    match parent {
                        Some("rPr") => n.red_chars += 1,
                        Some("trPr") => {}
                        _ => n.red_chars += chars_in(&c),
                    }
                } else {
                    stack.push(c);
                }
            }
        }
    }
    Ok(n)
}

/// What the laws found on one pair.
pub(super) struct Verdict {
    /// The noise of the redline.
    pub noise: Noise,
    /// The first line that differs, with the lines around it, for each law that failed.
    pub failures: Vec<String>,
    /// The receipt of the comparison.
    pub receipt: CompareReceipt,
    /// The redline.
    pub redline: State,
    /// The two prepared inputs.
    pub accepted: (State, State),
}

/// Every difference of two dumps: the lines between the longest common head and tail, with the first of them shown.
fn first_difference(law: &str, got: &[String], want: &[String]) -> Option<String> {
    let head = got.iter().zip(want).take_while(|(g, w)| g == w).count();
    if head == got.len() && head == want.len() {
        return None;
    }
    let tail = got[head..]
        .iter()
        .rev()
        .zip(want[head..].iter().rev())
        .take_while(|(g, w)| g == w)
        .count();
    let (g_end, w_end) = (got.len() - tail, want.len() - tail);
    let at = head;
    let text_of = |l: &str| {
        l.split(" | ")
            .last()
            .unwrap_or("")
            .chars()
            .take(70)
            .collect::<String>()
    };
    // the first character at which the two lines differ, with what surrounds it
    let window = |line: &str, other: &str| {
        let k = line
            .chars()
            .zip(other.chars())
            .position(|(x, y)| x != y)
            .unwrap_or(line.chars().count().min(other.chars().count()));
        let from = k.saturating_sub(100);
        format!(
            "@{k}: {}",
            line.chars().skip(from).take(260).collect::<String>()
        )
    };
    let show = |v: &[String], o: &[String]| {
        let mut out = String::new();
        for k in at.saturating_sub(2)..(at + 2).min(v.len()) {
            let line = v.get(k).map_or("", String::as_str);
            let shown = if k == at {
                window(line, o.get(k).map_or("", String::as_str))
            } else {
                text_of(line)
            };
            out.push_str(&format!("\n   [{k}] {shown}"));
        }
        out
    };
    Some(format!(
        "{law}: lines {at}..{g_end} of {} differ from lines {at}..{w_end} of {}\n  got:{}\n  want:{}",
        got.len(),
        want.len(),
        show(got, want),
        show(want, got)
    ))
}

/// The dumped body of a rejected redline without, for each note of the original that holds no block (`tags`), one note of the
/// same kind that holds exactly one empty paragraph. A reject keeps a note's last paragraph, emptied, since a note must hold a
/// block (`check` W9, revisions.md 3), so it gives `WC063-Footnote-Mod`'s empty note back that way. Nothing else is left out.
fn without_emptied_notes(got: &[String], tags: &[&str]) -> Vec<String> {
    let mut out = got.to_vec();
    let notes = got
        .iter()
        .position(|l| l == "--notes--")
        .map_or(got.len(), |i| i + 1);
    for tag in tags {
        let emptied = |l: &String| {
            l.starts_with(&format!("{tag} P ")) && !l.contains('\n') && l.ends_with(" runs=[] | ")
        };
        if let Some(i) = out.iter().skip(notes).position(emptied) {
            out.remove(notes + i);
        }
    }
    out
}

/// Compares two packages and checks N1, N2, N4 and N7 on the redline. The blocks that the receipt excludes are left out of both
/// dumps of each law, and everything else is compared: the body, the notes, and the headers and footers (all of them for N1,
/// which reads the revised package against itself, and the paired slots for N2).
pub(super) fn check_pair(original: Vec<u8>, revised: Vec<u8>) -> Result<Verdict, StetError> {
    let limits = Limits::default();
    let prepared = super::prepare::prepare(original.clone(), revised.clone(), &limits, &stamp())?;
    let (redline, receipt) =
        super::compare_states(original.clone(), revised.clone(), &limits, &stamp())?;
    let shared: HashSet<String> = {
        let a: HashSet<String> = list_definitions(&prepared.original)?
            .into_values()
            .collect();
        list_definitions(&prepared.revised)?
            .into_values()
            .filter(|d| a.contains(d))
            .collect()
    };
    let mut on_redline = DumpOpts {
        drop: HashSet::new(),
        shared_lists: shared.clone(),
    };
    let mut on_original = DumpOpts {
        drop: HashSet::new(),
        shared_lists: shared.clone(),
    };
    let mut on_rejected = DumpOpts {
        drop: HashSet::new(),
        shared_lists: shared,
    };
    for e in &receipt.exclusions {
        for k in &e.redline {
            on_redline.drop.insert((k.part().to_string(), k.node()));
            on_rejected.drop.insert((k.part().to_string(), k.node()));
        }
        for k in &e.copies {
            on_rejected.drop.insert((k.part().to_string(), k.node()));
        }
        for k in &e.original {
            on_original.drop.insert((k.part().to_string(), k.node()));
        }
    }
    let want_orig = dump(&prepared.original, &on_original)?;
    let want_rev = dump(&prepared.revised, &on_redline)?;
    let mut failures = Vec::new();
    if let Some(f) = locality(&prepared.revised, &redline, &receipt.declared.touched)? {
        failures.push(f);
    }
    if let Some(f) = moves_are_real(&redline, &receipt) {
        failures.push(f);
    }
    let noise = noise(&prepared.original, &prepared.revised, &redline)?;
    let accepted = dump(&resolved(&redline, Action::Accept)?, &on_redline)?;
    failures.extend(first_difference(
        "N1 accept",
        &accepted.body,
        &want_rev.body,
    ));
    let mut parts: Vec<&String> = want_rev.sides.keys().collect();
    parts.sort();
    let none: Vec<String> = Vec::new();
    for part in parts {
        let got = accepted.sides.get(part).unwrap_or(&none);
        let want = want_rev.sides.get(part).unwrap_or(&none);
        failures.extend(first_difference(&format!("N1 accept {part}"), got, want));
    }
    let rejected = dump(&resolved(&redline, Action::Reject)?, &on_rejected)?;
    failures.extend(first_difference(
        "N2 reject",
        &without_emptied_notes(&rejected.body, &want_orig.empty_notes),
        &want_orig.body,
    ));
    for (slot, pa, pb) in paired_slots(&want_orig, &want_rev) {
        let got = rejected.sides.get(&pb).unwrap_or(&none);
        let want = want_orig.sides.get(&pa).unwrap_or(&none);
        failures.extend(first_difference(
            &format!("N2 reject {slot:?} {pb} against {pa}"),
            got,
            want,
        ));
    }
    Ok(Verdict {
        noise,
        failures,
        receipt,
        redline,
        accepted: (prepared.original, prepared.revised),
    })
}

#[test]
fn n2_takes_one_emptied_paragraph_for_an_empty_note_of_the_original_and_nothing_else() {
    let line = |s: &str| s.to_owned();
    let empty = line("F P c= s=footnote text l= n= p= m= runs=[] | ");
    let text = line("F P c= s=footnote text l= n= p= m= runs=[\"|\"] | the note");
    let two = format!("{empty}\n{}", empty.trim_start_matches("F "));
    let body = |notes: &[&String]| {
        let mut b = vec![line("P | Ten"), line("--notes--")];
        b.extend(notes.iter().map(|n| (*n).clone()));
        b
    };
    // One emptied paragraph stands for the original's empty note, and only for one.
    assert_eq!(without_emptied_notes(&body(&[&empty]), &["F"]), body(&[]));
    assert_eq!(
        without_emptied_notes(&body(&[&empty, &empty]), &["F"]),
        body(&[&empty])
    );
    // Not without an empty note in the original, not for an endnote, not with text, not with two paragraphs.
    assert_eq!(
        without_emptied_notes(&body(&[&empty]), &[]),
        body(&[&empty])
    );
    assert_eq!(
        without_emptied_notes(&body(&[&empty]), &["E"]),
        body(&[&empty])
    );
    assert_eq!(
        without_emptied_notes(&body(&[&text]), &["F"]),
        body(&[&text])
    );
    assert_eq!(without_emptied_notes(&body(&[&two]), &["F"]), body(&[&two]));
}
