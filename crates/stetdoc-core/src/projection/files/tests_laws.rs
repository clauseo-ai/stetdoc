//! The laws of the alignment over generated models: edits stick (T2) and nothing is dropped (T3).

use super::testing::*;
use crate::api::files::PlannedOp;
use crate::projection::formats::Format;
use crate::projection::formats::json::JsonFormat;
use crate::projection::formats::stet::StetFormat;
use crate::projection::model::*;
use crate::projection::testing_gen::Rng;
use crate::projection::testing_gen_blocks::generate;

// A text edit yields operations that, applied to a string model of each paragraph, give the edited text.

fn final_texts(p: &crate::projection::model::Projection) -> Vec<(String, Vec<char>)> {
    let mut out = Vec::new();
    let Body::Stories(stories) = &p.body else {
        panic!("a model of files has stories");
    };
    for story in stories {
        for unit in &story.units {
            collect(&unit.blocks, &mut out);
        }
    }
    out
}

fn collect(blocks: &[Block], out: &mut Vec<(String, Vec<char>)>) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                if let Some(r) = p.r#ref {
                    out.push((r.to_string(), final_chars(&p.inlines)));
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        collect(&cell.blocks, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Changes the text of one `Text` inline that is a slot, if there is one.
fn edit_a_slot(p: &mut crate::projection::model::Projection, rng: &mut Rng) -> bool {
    fn editable(inlines: &mut [Inline], rng: &mut Rng) -> bool {
        for inline in inlines.iter_mut() {
            match inline {
                Inline::Text(t) if rng.chance(50) => {
                    let chars: Vec<char> = t.chars().collect();
                    let at = rng.below(chars.len() + 1);
                    let word = *rng.pick(&["new", " x ", "\u{a0}", "{", "word", ""]);
                    let mut next: String = chars.iter().take(at).collect();
                    next.push_str(word);
                    next.extend(chars.iter().skip(at + rng.below(3)));
                    if next != *t {
                        *t = next;
                        return true;
                    }
                }
                Inline::Rev {
                    kind: RevKind::Ins | RevKind::MoveTo,
                    inlines,
                    ..
                }
                | Inline::Link { inlines, .. }
                | Inline::Control {
                    state: ControlState::Editable,
                    inlines,
                    ..
                } => {
                    if editable(inlines, rng) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    for story in stories {
        for unit in &mut story.units {
            for block in &mut unit.blocks {
                if let Block::Paragraph(paragraph) = block
                    && paragraph.r#ref.is_some()
                    && paragraph.flags.readonly.is_none()
                    && editable(&mut paragraph.inlines, rng)
                {
                    return true;
                }
            }
        }
    }
    false
}

#[test]
fn t2_edited_slots_stick_through_every_format() {
    let mut rng = Rng::new(7);
    let mut edited_count = 0;
    for seed in 0..300 {
        let base = generate(seed);
        let mut edited = base.clone();
        if !edit_a_slot(&mut edited, &mut rng) {
            continue;
        }
        let hides = |p: &crate::projection::model::Projection| {
            p.draft(&policy())
                .files
                .iter()
                .any(|f| f.blocks().any(|b| b.facts.trailing > 0))
        };
        if hides(&base) || hides(&edited) {
            continue;
        }
        edited_count += 1;
        for format in [&StetFormat as &dyn Format, &JsonFormat] {
            let baseline = baseline_of(&base, format);
            let files = format.render(&edited).files;
            let draft = format
                .parse(&files, &crate::api::limits::Limits::default())
                .unwrap();
            let plan = super::align(&baseline, &draft, &styles())
                .unwrap_or_else(|e| panic!("{} seed {seed}: {}", format.id(), e.message()));
            let before = final_texts(&base);
            let after = final_texts(&edited);
            for ((r, old), (_, want)) in before.iter().zip(&after) {
                let mut text = old.clone();
                apply_replaces(&mut text, &plan, r);
                let expected = want.clone();
                assert_eq!(text, expected, "{} seed {seed} {r}", format.id());
            }
            assert!(
                plan.ops
                    .iter()
                    .all(|op| matches!(op, PlannedOp::Replace { .. })),
                "{:?}",
                show(&plan)
            );
        }
    }
    assert!(edited_count > 100, "{edited_count}");
}

// T3: nothing is dropped. If the edited file is accepted and says something other than the baseline, the plan
// has an operation, and if the plan has none the edited draft is the baseline's.

#[test]
fn t3_a_mutation_is_refused_or_applied_and_never_dropped() {
    let mut rng = Rng::new(31);
    let (mut accepted, mut refused_n, mut tolerated) = (0, 0, 0);
    for seed in 0..240 {
        let p = generate(seed);
        let format = StetFormat;
        let baseline = baseline_of(&p, &format);
        for (name, text) in format
            .render(&p)
            .files
            .into_iter()
            .flat_map(|f| std::iter::repeat_n(f, 6))
        {
            if text.lines().count() < 6 {
                continue;
            }
            // mutate one character of one body line: not the head block, which is ignored
            let lines: Vec<&str> = text.lines().collect();
            let body_start = lines
                .iter()
                .position(|l| !l.starts_with('{') || l.starts_with("{--") || l.starts_with("{++"))
                .unwrap_or(0);
            if body_start + 1 >= lines.len() {
                continue;
            }
            let pick = body_start + rng.below(lines.len() - body_start);
            let line: Vec<char> = lines[pick].chars().collect();
            if line.is_empty() {
                continue;
            }
            let at = rng.below(line.len());
            let replacement = *rng.pick(&[
                'x', ' ', '{', '}', '\\', '"', '-', '+', 'p', '1', '\n', '\t',
            ]);
            if line[at] == replacement {
                continue;
            }
            let mut changed = line.clone();
            changed[at] = replacement;
            let mutated: Vec<String> = lines
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    if i == pick {
                        changed.iter().collect()
                    } else {
                        (*l).to_string()
                    }
                })
                .collect();
            let files = vec![(name.clone(), format!("{}\n", mutated.join("\n")))];
            let Ok(draft) = format.parse(&files, &crate::api::limits::Limits::default()) else {
                refused_n += 1;
                continue;
            };
            match super::align(&baseline, &draft, &styles()) {
                Err(_) => refused_n += 1,
                Ok(plan) => {
                    accepted += 1;
                    let same = baseline
                        .draft
                        .file(&name)
                        .is_some_and(|b| b.lines == draft.files[0].lines);
                    if plan.ops.is_empty() {
                        tolerated += 1;
                        assert!(
                            same,
                            "seed {seed} {name}: a change was dropped silently\nline {pick}: {:?}",
                            mutated[pick]
                        );
                    } else {
                        assert!(!same, "seed {seed}: a phantom edit");
                    }
                }
            }
        }
    }
    assert!(
        accepted > 20 && refused_n > 20,
        "{accepted} accepted, {refused_n} refused, {tolerated} no-ops"
    );
}

// T5: a hostile edit costs a bounded time. A gap of thousands of removed paragraphs and thousands of new lines
// is paired no further than the caps of `recognize.rs`, and everything else is linear.

#[test]
fn t5_a_huge_gap_and_a_huge_run_are_read_in_bounded_time() {
    let n = 4000;
    let texts: Vec<String> = (0..n)
        .map(|i| format!("Paragraph number {i} has some words in it."))
        .collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let p = plain_doc(&refs);
    // every line but the first and the last removed and its text written again with no ref, and a word added so
    // that it is not the same text (the same text in the same place is refused: it lost its ref and nothing else)
    let rewritten = plan(&p, |t| {
        let lines: Vec<&str> = t.lines().collect();
        let last = lines.len() - 1;
        let out: Vec<String> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                if i < 3 || i == last {
                    (*l).to_string()
                } else {
                    l.split_once(' ')
                        .map_or((*l).to_string(), |(_, r)| format!("{r} again"))
                }
            })
            .collect();
        format!("{}\n", out.join("\n"))
    })
    .unwrap();
    assert_eq!(
        rewritten
            .ops
            .iter()
            .filter(|op| matches!(op, PlannedOp::Delete { .. }))
            .count(),
        n - 2
    );
    assert_eq!(
        rewritten
            .ops
            .iter()
            .filter(|op| matches!(op, PlannedOp::Insert { .. }))
            .count(),
        1
    );
    // a long run of new lines at the end, and a long run of lines that are one split each
    let appended = plan(&p, |t| {
        let mut out = t.to_string();
        for i in 0..n {
            out.push_str(&format!("New line {i}\n"));
        }
        out
    })
    .unwrap();
    assert_eq!(appended.ops.len(), 1);
    let split_all = plan(&p, |t| t.replace(" has some", "\nhas some")).unwrap();
    assert_eq!(
        split_all
            .ops
            .iter()
            .filter(|op| matches!(op, PlannedOp::Split { .. }))
            .count(),
        n
    );
}
