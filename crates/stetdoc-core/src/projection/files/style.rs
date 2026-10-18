//! The style of a new paragraph is Word's next style (design 6.4, decision 12).
//!
//! A line cannot name a style: a tag is read-only, and a line with no ref has none. What a new paragraph
//! gets is what pressing Enter gives. The previous block of a new paragraph is the paragraph it follows.
//! When the next style of that paragraph's style is another style `T`, the new paragraph takes `T` and no
//! direct paragraph property, which the operation says with `style: Some(T)`. When it is the same style,
//! the copy rule applies: the new paragraph is a copy of the one the compiler models it on, with its
//! list level, indent and spacing, and the operation carries no style. The second new paragraph of a run
//! takes the next style of the first one's style, and so on, as repeated Enters would.

use super::{Outside, StyleRef};

/// What a run of new lines is modelled on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Previous {
    /// The run follows a paragraph, whose style is known by name or not known.
    Follows(Option<String>),
    /// The run follows no paragraph (it follows the end of a table, a section line or the start of a
    /// unit), so its first paragraph is modelled on the paragraph it precedes, whose style is given.
    Precedes(Option<String>),
}

/// The style of each of `count` new paragraphs: `None` is the copy rule, `Some` is a style that is not the
/// previous one.
pub(super) fn styles(
    count: usize,
    previous: &Previous,
    outside: &dyn Outside,
) -> Vec<Option<StyleRef>> {
    let mut out = Vec::with_capacity(count);
    let mut current: Option<String> = match previous {
        Previous::Follows(style) => style.clone(),
        Previous::Precedes(_) => None,
    };
    for index in 0..count {
        if index == 0
            && let Previous::Precedes(model) = previous
        {
            out.push(None);
            current = model.clone();
            continue;
        }
        match current
            .as_deref()
            .and_then(|style| outside.next_style(style))
        {
            Some(next) => {
                current = Some(next.name.clone());
                out.push(Some(next));
            }
            None => out.push(None),
        }
    }
    out
}
