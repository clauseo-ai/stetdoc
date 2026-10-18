//! Which inlines are slots and which are seams: the one place that decides (design 3.7).
//!
//! `Text` outside any read-only wrapper is a slot. Inside an insertion (or the destination of a move),
//! an external link and an editable control, text is a slot between an `Open` and a `Close` seam.
//! Everything else is one seam that holds its content in its key: a deletion, a field, hidden text,
//! text that looks hidden, a control that cannot be edited, an atom, a comment mark. A change to a seam
//! is `readonly_region`, so what a seam holds is exactly what an agent may not change.

use super::draft::Seam;
use super::keys::{ControlKey, Edge, FieldKey, Key, ObjectKey, RevRef, Span};
use super::model::{
    Atom, Author, ControlInfo, ControlState, Inline, Mark, RevKind, RevTag, flatten, width_of,
};

/// A paragraph cut into slots and seams: `texts.len() == seams.len() + 1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Regions {
    /// The editable text slots, in order.
    pub texts: Vec<String>,
    /// The seams between them.
    pub seams: Vec<Seam>,
}

/// The author's code for a tag, or `?` for an index the head does not have.
pub(crate) fn author_code(authors: &[Author], index: u32) -> Box<str> {
    usize::try_from(index)
        .ok()
        .and_then(|i| authors.get(i))
        .map_or_else(|| "?".into(), |author| author.code.clone())
}

/// A revision tag as the draft keeps it.
pub(crate) fn rev_ref(tag: &RevTag, authors: &[Author]) -> RevRef {
    RevRef {
        id: tag.id,
        author: author_code(authors, tag.author),
        peer: tag.peer,
    }
}

/// The control's kind, alias and tag as the draft keeps them.
pub(crate) fn control_key(info: &ControlInfo) -> ControlKey {
    ControlKey {
        kind: info.kind,
        alias: info.alias.clone(),
        tag: info.tag.clone(),
    }
}

/// Cuts a paragraph's inlines into slots and seams.
pub(crate) fn regions(inlines: &[Inline], authors: &[Author]) -> Regions {
    let mut walk = Walk {
        authors,
        texts: vec![String::new()],
        seams: Vec::new(),
    };
    walk.run(inlines);
    Regions {
        texts: walk.texts,
        seams: walk.seams,
    }
}

struct Walk<'a> {
    authors: &'a [Author],
    texts: Vec<String>,
    seams: Vec<Seam>,
}

impl Walk<'_> {
    fn seam(&mut self, key: Key, width: u32) {
        self.seams.push(Seam { key, width });
        self.texts.push(String::new());
    }

    fn wrapped(&mut self, span: &Span, inlines: &[Inline]) {
        self.seam(Key::Open(span.clone()), 0);
        self.run(inlines);
        self.seam(Key::Close(span.clone()), 0);
    }

    fn run(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            match inline {
                Inline::Text(text) => {
                    if let Some(slot) = self.texts.last_mut() {
                        slot.push_str(text);
                    }
                }
                Inline::Atom(atom) => self.seam(atom_key(atom), 1),
                Inline::Rev {
                    kind: kind @ (RevKind::Ins | RevKind::MoveTo),
                    tag,
                    inlines,
                } => {
                    let rev = rev_ref(tag, self.authors);
                    let span = if *kind == RevKind::Ins {
                        Span::Ins(rev)
                    } else {
                        Span::MoveIn(rev)
                    };
                    self.wrapped(&span, inlines);
                }
                Inline::Rev {
                    kind: kind @ (RevKind::Del | RevKind::MoveFrom),
                    tag,
                    inlines,
                } => {
                    let key = Key::Deleted {
                        moved: *kind == RevKind::MoveFrom,
                        rev: rev_ref(tag, self.authors),
                        text: flatten(inlines).into(),
                    };
                    self.seam(key, 0);
                }
                Inline::Link { target, inlines } => {
                    let super::model::LinkTarget::External(url) = target;
                    self.wrapped(&Span::Link(url.clone()), inlines);
                }
                Inline::Control {
                    info,
                    state: ControlState::Editable,
                    inlines,
                } => self.wrapped(&Span::Control(control_key(info)), inlines),
                Inline::Control {
                    info,
                    state,
                    inlines,
                } => {
                    let key = Key::Control {
                        info: control_key(info),
                        state: *state,
                        text: flatten(inlines).into(),
                    };
                    self.seam(key, width_of(inlines));
                }
                Inline::Field(field) => {
                    let key = Key::Field(FieldKey {
                        keyword: field.keyword.clone(),
                        text: field.text.clone(),
                        target: field.target,
                        opens: field.opens,
                        closes: field.closes,
                        dirty: field.dirty,
                    });
                    self.seam(key, field.width);
                }
                Inline::Hidden { inlines, .. } => {
                    self.seam(Key::Hidden(flatten(inlines).into()), width_of(inlines));
                }
                Inline::LooksHidden { signal, inlines } => {
                    let key = Key::LooksHidden {
                        signal: *signal,
                        text: flatten(inlines).into(),
                    };
                    self.seam(key, width_of(inlines));
                }
                Inline::Mark(Mark::CommentStart(id)) => self.seam(
                    Key::Comment {
                        id: *id,
                        edge: Edge::Start,
                    },
                    0,
                ),
                Inline::Mark(Mark::CommentEnd(id)) => self.seam(
                    Key::Comment {
                        id: *id,
                        edge: Edge::End,
                    },
                    0,
                ),
            }
        }
    }
}

fn atom_key(atom: &Atom) -> Key {
    match atom {
        Atom::PageBreak => Key::PageBreak,
        Atom::ColumnBreak => Key::ColumnBreak,
        Atom::NoteRef { kind, ordinal } => Key::NoteRef {
            kind: *kind,
            ordinal: *ordinal,
        },
        Atom::NoteMark { kind } => Key::NoteMark(*kind),
        Atom::Object(object) => Key::Object(ObjectKey {
            kind: object.kind,
            alt: object.alt.clone(),
            text_box: object.text_box.clone(),
        }),
        Atom::Math(text) => Key::Math(text.clone()),
        Atom::Auto(kind) => Key::Auto(*kind),
    }
}
