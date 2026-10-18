//! Time bounds on the work that grows with the number of children of one parent. The other
//! tests count results; this one is allowed the clock for that reason (`core-io-exceptions`).

use super::*;
// The I/O check reads one file at a time and takes test code to be what carries a test attribute
// there, so the items that name the clock say so.
#[cfg(test)]
use std::time::{Duration, Instant};

/// Far more than any of these takes. A rewrite that scales with the square or the cube of the
/// children (5,000 of them in reverse order, here) would not finish in a minute.
#[cfg(test)]
const BOUND: Duration = Duration::from_secs(5);

/// The children of a `w:numbering`, in reverse order: `num`, then `abstractNum`, then
/// `numPicBullet`, as many of each as given. `w:numbering` is a property container at the root
/// of its part.
#[cfg(test)]
fn reversed_numbering(nums: usize, abstracts: usize, bullets: usize) -> Vec<QName<'static>> {
    std::iter::repeat_n(w("num"), nums)
        .chain(std::iter::repeat_n(w("abstractNum"), abstracts))
        .chain(std::iter::repeat_n(w("numPicBullet"), bullets))
        .collect()
}

#[cfg(test)]
fn numbering() -> Option<Ctx> {
    Ctx::from_path(PartKind::Numbering, &[])
}

#[test]
fn rewriting_thousands_of_children_in_reverse_order_is_fast() {
    let children = reversed_numbering(2000, 2000, 1000);
    let tree = ListTree::new(&children);
    let start = Instant::now();
    let order = reorder(numbering(), &tree, 0, None).expect("w:numbering is a property container");
    let took = start.elapsed();
    assert!(
        took < BOUND,
        "reorder of {} children took {took:?}",
        children.len()
    );
    // bullets first, then the abstract numberings, then the numberings, each group in the order
    // it was in
    let expected: Vec<Option<usize>> = (4000..5000)
        .chain(2000..4000)
        .chain(0..2000)
        .map(Some)
        .collect();
    assert_eq!(order, expected);
}

#[test]
fn inserting_among_thousands_of_children_in_reverse_order_is_fast() {
    let children = reversed_numbering(2000, 2000, 1000);
    let tree = ListTree::new(&children);
    let start = Instant::now();
    let got = insert_in_order(numbering(), &tree, 0, w("abstractNum"), Prefer::Last);
    let took = start.elapsed();
    assert!(
        took < BOUND,
        "insert_in_order among {} children took {took:?}",
        children.len()
    );
    // The container is out of order, so the answer is the rewrite, with the new element after
    // the last abstractNum: 1000 bullets and 2000 abstract numberings come before it.
    let Fit::Reorder(order) = got.fit else {
        panic!("expected a rewrite, got {:?}", got.fit);
    };
    assert_eq!(order.len(), children.len() + 1);
    assert_eq!(order.iter().filter(|s| s.is_none()).count(), 1);
    assert_eq!(order.get(3000), Some(&None));
}

#[test]
fn validating_thousands_of_misplaced_children_is_fast() {
    // Each misplaced child is reported, and naming what blocked it must not scan the children
    // before it again.
    let mut xml = String::from(&format!("<w:numbering {NS_DECLS}>"));
    for i in 0..3000 {
        xml.push_str(&format!(
            r#"<w:num w:numId="{i}"><w:abstractNumId w:val="0"/></w:num>"#
        ));
    }
    for i in 0..3000 {
        xml.push_str(&format!(
            r#"<w:abstractNum w:abstractNumId="{i}"><w:lvl w:ilvl="0"/></w:abstractNum>"#
        ));
    }
    xml.push_str("</w:numbering>");
    let tree = parse(&xml);
    let start = Instant::now();
    let report = validate_part(PartKind::Numbering, &tree);
    let took = start.elapsed();
    assert!(took < BOUND, "validating took {took:?}");
    assert!(report.set.total() >= 3000, "{}", report.set.total());
}
