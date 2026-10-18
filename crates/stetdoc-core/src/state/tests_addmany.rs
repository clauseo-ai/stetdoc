//! Random transactions that call `add_part` several times, with reads and edits in between: after
//! every step every cached value equals a rebuild from scratch, and what the commit makes is a state
//! that exports and resumes. One edit adds four parts, and the first version of
//! `add_part` was wrong for the second call.

use super::tests::*;
use super::tests_coherence::incoherent;
use super::*;
use crate::package::NewPart;
use crate::refs::Claim;

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn endnotes_with_paragraph() -> NewPart<'static> {
    NewPart {
        name: "word/endnotes.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
        bytes: format!(
            "<w:endnotes xmlns:w=\"{W}\"><w:endnote w:id=\"1\"><w:p><w:r><w:t>end</w:t></w:r></w:p></w:endnote></w:endnotes>"
        )
        .into_bytes(),
        related_from: Some((MAIN, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes")),
    }
}

/// A part that is no story, related from the main part, like `people.xml`.
fn people() -> NewPart<'static> {
    NewPart {
        name: "word/people.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.people+xml",
        bytes: b"<w15:people xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\"/>"
            .to_vec(),
        related_from: Some((
            MAIN,
            "http://schemas.microsoft.com/office/2011/relationships/people",
        )),
    }
}

/// A part with no relationship from anywhere: the content types change and no relationships part does.
fn custom_xml() -> NewPart<'static> {
    NewPart {
        name: "customXml/item1.xml",
        content_type: "application/xml",
        bytes: b"<root/>".to_vec(),
        related_from: None,
    }
}

fn catalogue() -> Vec<NewPart<'static>> {
    vec![
        footnotes_with_paragraph(),
        comments_with_paragraph(),
        endnotes_with_paragraph(),
        people(),
        custom_xml(),
    ]
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Reads everything that `add_part` can make stale, so that a cache holds it.
fn read_everything(tx: &Tx<'_>) {
    let view = tx.parts();
    let main = view.id(MAIN).unwrap();
    let _ = view.rels(main).unwrap();
    for spec in &view.story_specs().unwrap().0 {
        let _ = view.story(view.id(&spec.part).unwrap()).unwrap();
    }
    let _ = view.ids(crate::wordml::IdKind::ParaId).unwrap();
}

fn run(seed: u64) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let original = package_bytes(&paragraphs(&["one", "two", "three"]));
    let s = state_from(original.clone(), &Limits::default());
    let mut parts = catalogue();
    let mut tx = s.begin();
    let mut added = 0;
    for step in 0..(2 + rng.below(8)) {
        match rng.below(6) {
            0 | 1 if !parts.is_empty() => {
                let part = parts.remove(rng.below(parts.len()));
                tx.add_part(part).unwrap();
                added += 1;
            }
            2 => read_everything(&tx),
            3 => edit_first_text(&mut tx, &format!("edit {step}")),
            4 => {
                // A borrow that changes nothing.
                let _ = tx.part_mut(MAIN).unwrap();
            }
            _ => {
                // A refused second add of a part that is there: it changes nothing and does not count.
                if let Some(first) = catalogue()
                    .into_iter()
                    .find(|p| tx.parts().contains(p.name))
                {
                    let calls = tx.added.as_ref().map(|a| a.calls);
                    assert!(tx.add_part(first).is_err(), "seed {seed}");
                    assert_eq!(tx.added.as_ref().map(|a| a.calls), calls, "seed {seed}");
                }
            }
        }
        let bad = incoherent(&tx.parts());
        assert!(bad.is_empty(), "seed {seed} step {step}: {bad:?}");
    }
    read_everything(&tx);
    let (next, committed) = tx.commit().unwrap();
    let bad = incoherent(&next.parts());
    assert!(bad.is_empty(), "seed {seed} after the commit: {bad:?}");
    // Every block of every story has a ref, and the refs minted are the new stories' blocks.
    let view = next.parts();
    let mut blocks = 0;
    for spec in &view.story_specs().unwrap().0 {
        let part = view.id(&spec.part).unwrap();
        let story = view.story(part).unwrap();
        for entry in story.blocks() {
            let key = story.key(entry);
            let r = next.refs.ref_of(&key);
            assert!(
                r.is_some(),
                "seed {seed}: a block of {} has no ref",
                spec.part
            );
            let r = r.unwrap().to_string();
            assert_eq!(
                next.resolve(Claim::Implied, &r).unwrap(),
                key,
                "seed {seed}: {r}"
            );
            blocks += 1;
        }
    }
    let stories_added = (0..3)
        .filter(|i| {
            next.parts().contains(
                [
                    "word/footnotes.xml",
                    "word/comments.xml",
                    "word/endnotes.xml",
                ][*i],
            )
        })
        .count();
    assert_eq!(
        committed.minted.len(),
        stories_added,
        "seed {seed}: one paragraph in each new story"
    );
    assert_eq!(blocks, 3 + stories_added, "seed {seed}");
    // The result saves, exports and resumes to the same refs.
    let e = next.export().unwrap();
    let rec = Recorded {
        base: e.base,
        head_sha256: e.head_sha256,
        refs_sha256: e.refs_sha256,
    };
    let back = State::resume(e.head, &e.refs, &rec, Some(&original), &Limits::default())
        .unwrap_or_else(|e| panic!("seed {seed} ({added} parts added): resume failed: {e}"));
    assert_eq!(all_refs(&back), all_refs(&next), "seed {seed}");
    let main = back.parts().id(MAIN).unwrap();
    let rels = back.parts().rels(main).unwrap();
    let want = ["footnotes", "comments", "endnotes"]
        .iter()
        .filter(|k| next.parts().contains(&format!("word/{k}.xml")))
        .count();
    assert_eq!(
        rels.iter()
            .filter(|r| ["footnotes", "comments", "endnotes"]
                .iter()
                .any(|k| r.rel_type == format!("{REL}/{k}")))
            .count(),
        want,
        "seed {seed}"
    );
}

#[test]
fn random_transactions_with_several_add_parts_stay_coherent_and_resume() {
    for seed in 1..=300 {
        run(seed);
    }
}
