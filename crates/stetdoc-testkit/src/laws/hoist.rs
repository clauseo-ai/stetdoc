//! The one change L2 by block allows a block the batch does not touch (`laws::blocks`): the paragraph before or after a block
//! the batch touches may gain the range markers and comment reference runs that a removal hoists into it. A comment survives
//! the removal of its text (comments-and-notes.md item 7, revisions.md 3.6.5), so its range markers and its reference run
//! move to the paragraph next to it, and so do bookmarks, permission ranges and move ranges. Each mark gained must be one a
//! touched block, or the markup between blocks next to it, had, or a comment the old body did not have (one the batch made);
//! and markup between blocks may move only from beside a touched block.

use super::blocks::{El, children, contains, is_marker, span};
use std::collections::BTreeSet;

/// A marker or a comment reference, by the family whose ids it shares and its id.
type Mark = (&'static str, String);

/// The family of a marker or a reference: a comment's range and reference share its id, and so do the two ends of a bookmark,
/// a permission range or a move range.
fn family(local: &str) -> Option<&'static str> {
    match local {
        "commentRangeStart" | "commentRangeEnd" | "commentReference" => Some("comment"),
        "bookmarkStart" | "bookmarkEnd" => Some("bookmark"),
        "permStart" | "permEnd" => Some("perm"),
        "moveFromRangeStart" | "moveFromRangeEnd" | "moveToRangeStart" | "moveToRangeEnd" => {
            Some("move")
        }
        _ => None,
    }
}

/// The value of the attribute of an element's start tag whose local name is `local`.
fn attr(xml: &[u8], el: &El, local: &str) -> Option<String> {
    let tag = std::str::from_utf8(xml.get(el.span.0..el.inner.0)?).ok()?;
    let mut rest = tag.split_once(|c: char| c.is_ascii_whitespace())?.1;
    loop {
        let (name, after) = rest.split_once('=')?;
        let after = after.trim_start();
        let quote = after.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let (value, tail) = after.get(1..)?.split_once(quote)?;
        if name.trim().rsplit(':').next() == Some(local) {
            return Some(value.to_string());
        }
        rest = tail;
    }
}

/// The marks of an element and of everything inside it.
fn marks_in(xml: &[u8], el: &El, out: &mut BTreeSet<Mark>) {
    if let Some(f) = family(&el.name) {
        out.insert((f, attr(xml, el, "id").unwrap_or_default()));
    }
    for kid in children(xml, el.inner.0, el.inner.1).unwrap_or_default() {
        marks_in(xml, &kid, out);
    }
}

/// The mark that a child of a paragraph is: a range marker, or a run that holds a comment reference and nothing else but its
/// properties.
fn mark_of(xml: &[u8], el: &El) -> Option<Mark> {
    if is_marker(&el.name) {
        return family(&el.name).map(|f| (f, attr(xml, el, "id").unwrap_or_default()));
    }
    let kids = children(xml, el.inner.0, el.inner.1).filter(|_| el.name == "r")?;
    match kids
        .iter()
        .filter(|k| k.name != "rPr")
        .collect::<Vec<_>>()
        .as_slice()
    {
        [only] if only.name == "commentReference" => {
            Some(("comment", attr(xml, only, "id").unwrap_or_default()))
        }
        _ => None,
    }
}

/// The start tag of an element as written with a start and an end tag, without white space before its `>`: `<w:p a="1"/>` and
/// `<w:p a="1" />` read as `<w:p a="1">`, and so does the `<w:p a="1" >` that opening the second to give it children writes.
fn start_tag(xml: &[u8], el: &El) -> Vec<u8> {
    let tag = xml.get(el.span.0..el.inner.0).unwrap_or_default();
    let body = tag
        .strip_suffix(b"/>")
        .or_else(|| tag.strip_suffix(b">"))
        .unwrap_or(tag);
    let mut open = body.trim_ascii_end().to_vec();
    open.push(b'>');
    open
}

/// What `new` gained, when it is `old` with only range markers and comment reference runs added among the children of a
/// paragraph: the same start tag (an empty paragraph opened to hold them counts as the same), and the children of `old` in
/// order, with nothing between them but those. What the paragraph had of its own stays where it was. A block-level wrapper
/// (`w:sdt` and its `w:sdtContent`, `w:customXml`) is looked through, since the paragraph inside it is the neighbour: its
/// start tag and every child are as they were, but for the paragraphs inside, which may gain what a paragraph may. `None` when
/// `new` is anything else.
fn hoisted_into(old: &[u8], new: &[u8]) -> Option<Vec<Mark>> {
    let first = |xml: &[u8]| children(xml, 0, xml.len()).and_then(|e| e.into_iter().next());
    gained_in(old, &first(old)?, new, &first(new)?)
}

/// `hoisted_into` for one element of each text.
fn gained_in(old: &[u8], o: &El, new: &[u8], n: &El) -> Option<Vec<Mark>> {
    if o.name != n.name || start_tag(old, o) != start_tag(new, n) {
        return None;
    }
    let old_kids = children(old, o.inner.0, o.inner.1)?;
    let new_kids = children(new, n.inner.0, n.inner.1)?;
    let mut gained = Vec::new();
    match o.name.as_str() {
        "p" => {
            let mut next = old_kids.iter().peekable();
            for kid in &new_kids {
                if next.peek().is_some_and(|o| span(old, o) == span(new, kid)) {
                    next.next();
                } else {
                    gained.push(mark_of(new, kid)?);
                }
            }
            next.peek().is_none().then_some(gained)
        }
        "sdt" | "sdtContent" | "customXml" if old_kids.len() == new_kids.len() => {
            for (ok, nk) in old_kids.iter().zip(&new_kids) {
                if span(old, ok) != span(new, nk) {
                    gained.extend(gained_in(old, ok, new, nk)?);
                }
            }
            Some(gained)
        }
        _ => None,
    }
}

/// The body before a batch, cut into its top-level elements: how many view blocks each holds, their refs, and whether the
/// batch touches it.
pub(super) struct Before<'a> {
    pub(super) xml: &'a [u8],
    pub(super) els: &'a [El],
    pub(super) counts: &'a [usize],
    pub(super) refs: &'a [Vec<String>],
    pub(super) touched: &'a [bool],
}

impl Before<'_> {
    fn hit(&self, i: usize) -> bool {
        self.touched.get(i).copied().unwrap_or(false)
    }

    /// Whether element `i` is next to one the batch touches: the first block on either side of it, or markup between blocks on
    /// the way to it, is touched.
    fn near_touched(&self, i: usize) -> bool {
        let block = |j: usize| self.counts.get(j).copied().unwrap_or(0) > 0;
        let left = (0..i).rev();
        let right = i + 1..self.els.len();
        [left.collect::<Vec<_>>(), right.collect()]
            .iter()
            .any(|side| {
                for &j in side {
                    if self.hit(j) {
                        return true;
                    }
                    if block(j) {
                        return false;
                    }
                }
                false
            })
    }

    /// The ids of the comments whose markers or references the body had, worked out the first time they are asked for: only
    /// a neighbour that gained a comment no touched block had needs them.
    fn comments_once(&self) -> Lazy<'_> {
        Lazy {
            before: self,
            ids: std::cell::OnceCell::new(),
        }
    }
}

/// The comment ids of a body, worked out once when first asked for.
struct Lazy<'a> {
    before: &'a Before<'a>,
    ids: std::cell::OnceCell<BTreeSet<String>>,
}

impl Lazy<'_> {
    fn get(&self) -> &BTreeSet<String> {
        self.ids.get_or_init(|| {
            let mut marks = BTreeSet::new();
            for el in self.before.els {
                marks_in(self.before.xml, el, &mut marks);
            }
            marks
                .into_iter()
                .filter(|(f, _)| *f == "comment")
                .map(|(_, id)| id)
                .collect()
        })
    }
}

/// The comparison of L2 by block, on the cut body before the batch and the body after it. The blocks a batch leaves alone keep
/// their order, so each is looked for from the place after the last one found: a paragraph that gained hoisted markers is
/// taken where it stands, not mistaken for an equal paragraph further down (empty ones often are equal), which would leave
/// that one to look moved.
pub(super) fn kept_or_hoisted(
    before: &Before<'_>,
    new: &[u8],
    new_els: &[El],
) -> Result<(), String> {
    let found: Vec<&[u8]> = new_els.iter().map(|e| span(new, e)).collect();
    let mut claimed = vec![false; found.len()];
    let mut cursor = 0;
    // What a removal may hoist into a neighbour: the marks of the blocks the batch touches, and of the markers between blocks
    // next to them.
    let mut allowed: BTreeSet<Mark> = BTreeSet::new();
    for (i, el) in before.els.iter().enumerate() {
        let gap = before.counts.get(i) == Some(&0) && is_marker(&el.name) && before.near_touched(i);
        if before.hit(i) || gap {
            marks_in(before.xml, el, &mut allowed);
        }
    }
    let comments = before.comments_once();
    for (i, el) in before.els.iter().enumerate() {
        if before.hit(i) {
            continue;
        }
        let bytes = span(before.xml, el);
        let near = before.near_touched(i);
        let paragraph = before.counts.get(i).copied().unwrap_or(0) > 0;
        // A comment survives the removal of its text (comments-and-notes.md item 7, revisions.md 3.6.5), so its range markers
        // and its reference run move to the paragraph next to it, and so do bookmarks: each mark must be one a touched block
        // had, or a comment the old body did not have at all (one the batch made).
        let fits =
            |m: &Mark| allowed.contains(m) || (m.0 == "comment" && !comments.get().contains(&m.1));
        let (mut far, mut foreign) = (false, None);
        let mut at = None;
        for j in (cursor..found.len()).chain(0..cursor) {
            if claimed.get(j) != Some(&false) {
                continue;
            }
            let there = found.get(j).copied().unwrap_or_default();
            if there == bytes {
                at = Some(j);
                break;
            }
            let Some(gained) = hoisted_into(bytes, there).filter(|_| paragraph) else {
                continue;
            };
            match gained.iter().find(|m| !fits(m)) {
                _ if !near => far = true,
                None => {
                    at = Some(j);
                    break;
                }
                Some(m) => {
                    foreign.get_or_insert_with(|| m.clone());
                }
            }
        }
        if let Some(j) = at {
            if let Some(c) = claimed.get_mut(j) {
                *c = true;
            }
            cursor = cursor.max(j + 1);
            continue;
        }
        // A marker between blocks next to a touched one may move: a removal hoists it into a neighbour.
        if !paragraph && is_marker(&el.name) && near && contains(new, bytes) {
            continue;
        }
        let refs = before.refs.get(i).cloned().unwrap_or_default();
        let what = if refs.is_empty() {
            format!("a w:{} at the top of the body", el.name)
        } else {
            refs.join(", ")
        };
        let head: String = String::from_utf8_lossy(bytes).chars().take(160).collect();
        return Err(match (foreign, far) {
            (Some((family, id)), _) => format!(
                "L2: {what} gained a {family} mark with the id {id:?}, which no block the batch touches had"
            ),
            (None, true) => format!(
                "L2: {what}, which is not next to a block the batch touches, gained range markers or a comment reference"
            ),
            (None, false) => format!(
                "L2: {what}, which the batch does not name, changed its XML (it was {head:?})"
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::laws::blocks::{blocks_in, body};

    fn gained(old: &[u8], new: &[u8]) -> Option<Vec<String>> {
        hoisted_into(old, new).map(|g| g.into_iter().map(|(f, id)| format!("{f} {id}")).collect())
    }

    #[test]
    fn an_empty_paragraph_with_a_space_before_its_slash_counts_as_opened() {
        // tdf112450_vml_polyline, seed 134: `<w:p a="1" />` is opened as `<w:p a="1" >` to hold a hoisted comment.
        let old = br#"<w:p w:rsidR="1" />"#;
        let mark = r#"<w:commentRangeStart w:id="1"/><w:commentRangeEnd w:id="1"/></w:p>"#;
        let new = format!(r#"<w:p w:rsidR="1" >{mark}"#);
        assert_eq!(
            gained(old, new.as_bytes()),
            Some(vec!["comment 1".into(), "comment 1".into()])
        );
        // Another attribute is still a change of the start tag.
        let other = format!(r#"<w:p w:rsidR="2" >{mark}"#);
        assert_eq!(gained(old, other.as_bytes()), None);
    }

    #[test]
    fn a_neighbour_that_gained_only_hoisted_markers_says_what_it_gained() {
        let old = br#"<w:p w:rsidR="1"/>"#;
        let new = br#"<w:p w:rsidR="1"><w:commentRangeEnd w:id="2"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="2"/></w:r></w:p>"#;
        assert_eq!(
            gained(old, new),
            Some(vec!["comment 2".into(), "comment 2".into()])
        );
        let old = br#"<w:p><w:pPr/><w:r><w:t>Kept.</w:t></w:r></w:p>"#;
        let new = br#"<w:p><w:pPr/><w:bookmarkStart w:id="0" w:name="x"/><w:commentRangeStart w:id="3"/><w:r><w:t>Kept.</w:t></w:r></w:p>"#;
        assert_eq!(
            gained(old, new),
            Some(vec!["bookmark 0".into(), "comment 3".into()])
        );
    }

    #[test]
    fn a_neighbour_keeps_its_own_markers_where_they_were() {
        // The neighbour has a comment of its own; the hoisted markers come first. Its own markers are not taken for hoisted
        // ones, and moving one of them is a change.
        let old = br#"<w:p w:x="1"><w:r><w:t>a</w:t></w:r><w:commentRangeStart w:id="9"/><w:r><w:t>b</w:t></w:r><w:commentRangeEnd w:id="9"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="9"/></w:r></w:p>"#;
        let new = br#"<w:p w:x="1"><w:commentRangeStart w:id="0"/><w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/></w:r><w:r><w:t>a</w:t></w:r><w:commentRangeStart w:id="9"/><w:r><w:t>b</w:t></w:r><w:commentRangeEnd w:id="9"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="9"/></w:r></w:p>"#;
        assert_eq!(gained(old, new).map(|g| g.len()), Some(3));
        let moved = br#"<w:p w:x="1"><w:commentRangeStart w:id="9"/><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r><w:commentRangeEnd w:id="9"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="9"/></w:r></w:p>"#;
        assert_eq!(gained(old, moved), None);
        let other_tag = br#"<w:p w:x="2"><w:r><w:t>a</w:t></w:r><w:commentRangeStart w:id="9"/><w:r><w:t>b</w:t></w:r><w:commentRangeEnd w:id="9"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="9"/></w:r></w:p>"#;
        assert_eq!(gained(old, other_tag), None);
    }

    #[test]
    fn a_neighbour_whose_text_or_properties_changed_is_not() {
        let old = br#"<w:p><w:r><w:t>Kept.</w:t></w:r></w:p>"#;
        let changed = br#"<w:p><w:commentRangeEnd w:id="2"/><w:r><w:t>Changed.</w:t></w:r></w:p>"#;
        assert_eq!(gained(old, changed), None);
        // A run with text besides the reference is not a reference run.
        let more = br#"<w:p><w:r><w:t>Kept.</w:t></w:r><w:r><w:t>x</w:t><w:commentReference w:id="2"/></w:r></w:p>"#;
        assert_eq!(gained(old, more), None);
        let props =
            br#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Kept.</w:t></w:r></w:p>"#;
        assert_eq!(gained(old, props), None);
    }

    /// L2 by block on two bodies, with the batch touching the elements of `touched`.
    fn compare(old: &str, new: &str, touched: &[bool]) -> Result<(), String> {
        let wrap = |b: &str| format!("<w:document><w:body>{b}</w:body></w:document>").into_bytes();
        let (old, new) = (wrap(old), wrap(new));
        let els = body(&old).unwrap_or_default();
        let counts: Vec<usize> = els
            .iter()
            .map(|e| blocks_in(&old, e).unwrap_or(0))
            .collect();
        let refs: Vec<Vec<String>> = (1..=els.len()).map(|i| vec![format!("e{i}")]).collect();
        assert_eq!(touched.len(), els.len());
        let before = Before {
            xml: &old,
            els: &els,
            counts: &counts,
            refs: &refs,
            touched,
        };
        kept_or_hoisted(&before, &new, &body(&new).unwrap_or_default())
    }

    const A: &str = r#"<w:p><w:r><w:t>a</w:t></w:r></w:p>"#;
    const GONE: &str = r#"<w:p><w:commentRangeStart w:id="5"/><w:r><w:t>b</w:t></w:r><w:commentRangeEnd w:id="5"/><w:r><w:commentReference w:id="5"/></w:r></w:p>"#;
    const C: &str = r#"<w:p><w:r><w:t>c</w:t></w:r></w:p>"#;
    const D: &str = r#"<w:p><w:commentRangeStart w:id="7"/><w:r><w:t>d</w:t></w:r><w:commentRangeEnd w:id="7"/></w:p>"#;

    /// A paragraph with the markers and the reference of comment `id` before its own children `own`.
    fn with(id: &str, own: &str) -> String {
        format!(
            r#"<w:p><w:commentRangeStart w:id="{id}"/><w:commentRangeEnd w:id="{id}"/><w:r><w:commentReference w:id="{id}"/></w:r>{own}</w:p>"#
        )
    }

    #[test]
    fn only_a_neighbour_of_a_touched_block_gains_marks_and_only_its_own() {
        let old = format!("{A}{GONE}{C}{D}");
        let removed = [false, true, false, false];
        let c = r#"<w:r><w:t>c</w:t></w:r>"#;
        // The removed paragraph's comment moves into the paragraph after it, or the one before.
        assert_eq!(
            compare(&old, &format!("{A}{}{D}", with("5", c)), &removed),
            Ok(())
        );
        let a = r#"<w:r><w:t>a</w:t></w:r>"#;
        assert_eq!(
            compare(&old, &format!("{}{C}{D}", with("5", a)), &removed),
            Ok(())
        );
        // So does a comment the batch made, which the old body did not have.
        assert_eq!(
            compare(&old, &format!("{A}{}{D}", with("9", c)), &removed),
            Ok(())
        );
        // Not a comment of a block the batch leaves alone.
        let e = compare(&old, &format!("{A}{}{D}", with("7", c)), &removed).unwrap_err();
        assert!(e.contains("no block the batch touches had"), "{e}");
        // And nothing goes into a paragraph that is not next to the removed one.
        let d = r#"<w:commentRangeStart w:id="7"/><w:r><w:t>d</w:t></w:r><w:commentRangeEnd w:id="7"/>"#;
        let e = compare(&old, &format!("{A}{C}{}", with("5", d)), &removed).unwrap_err();
        assert!(e.contains("not next to a block the batch touches"), "{e}");
    }

    #[test]
    fn markup_between_blocks_moves_only_from_beside_a_touched_block() {
        let mark = r#"<w:bookmarkStart w:id="3" w:name="x"/>"#;
        let c = r#"<w:r><w:t>c</w:t></w:r>"#;
        let c_with_mark = format!("<w:p>{mark}{c}</w:p>");
        // A bookmark between a paragraph and the removed one moves into the paragraph after the removed one.
        let old = format!("{A}{mark}{GONE}{C}");
        assert_eq!(
            compare(
                &old,
                &format!("{A}{c_with_mark}"),
                &[false, false, true, false]
            ),
            Ok(())
        );
        // One that is not beside a touched block stays where it was.
        let old = format!("{mark}{A}{C}{GONE}");
        let e = compare(
            &old,
            &format!("{A}{c_with_mark}"),
            &[false, false, false, true],
        )
        .unwrap_err();
        assert!(e.contains("w:bookmarkStart"), "{e}");
    }

    #[test]
    fn equal_paragraphs_are_matched_where_they_stand() {
        // fdo64256: three equal paragraphs after the removed one, and the first gained its comment. The last is not taken for
        // the one that gained it.
        let s = r#"<w:p><w:r><w:t>s</w:t></w:r></w:p>"#;
        let own = r#"<w:r><w:t>s</w:t></w:r>"#;
        let old = format!("{GONE}{s}{s}{s}");
        let new = format!("{}{s}{s}", with("5", own));
        assert_eq!(compare(&old, &new, &[true, false, false, false]), Ok(()));
        // fdo76098: the comment of a paragraph inside a removed table goes into the paragraph before the table, and an equal
        // paragraph further down stays as it was.
        let table = r#"<w:tbl><w:tr><w:tc><w:p><w:commentRangeStart w:id="5"/><w:r><w:t>in</w:t></w:r><w:commentRangeEnd w:id="5"/><w:r><w:commentReference w:id="5"/></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let old = format!("{A}{s}{table}{C}{s}");
        let new = format!("{A}{}{C}{s}", with("5", own));
        assert_eq!(
            compare(&old, &new, &[false, false, true, false, false]),
            Ok(())
        );
        // Still, the paragraph far from the table may not gain it.
        let new = format!("{A}{s}{C}{}", with("5", own));
        assert!(compare(&old, &new, &[false, false, true, false, false]).is_err());
    }

    #[test]
    fn a_paragraph_inside_a_content_control_is_looked_through() {
        // adeu 08_agent_nested_revisions, seed 7: the comment of a removed table goes into the paragraph inside the content
        // control before it.
        let control = |inner: &str| {
            format!(
                r#"<w:sdt><w:sdtPr><w:tag w:val="ref"/></w:sdtPr><w:sdtContent>{inner}</w:sdtContent></w:sdt>"#
            )
        };
        let own = r#"<w:r><w:t>ref</w:t></w:r>"#;
        let table = r#"<w:tbl><w:tr><w:tc><w:p><w:commentRangeStart w:id="5"/><w:r><w:t>in</w:t></w:r><w:commentRangeEnd w:id="5"/><w:r><w:commentReference w:id="5"/></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let old = format!("{A}{}{table}{C}", control(&format!("<w:p>{own}</w:p>")));
        let new = format!("{A}{}{C}", control(&with("5", own)));
        assert_eq!(compare(&old, &new, &[false, false, true, false]), Ok(()));
        // The wrapper itself stays as it was.
        let other = format!(
            r#"<w:sdt><w:sdtPr><w:tag w:val="other"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#,
            with("5", own)
        );
        assert!(
            compare(
                &old,
                &format!("{A}{other}{C}"),
                &[false, false, true, false]
            )
            .is_err()
        );
    }
}
