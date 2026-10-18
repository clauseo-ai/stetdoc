//! The bare words of `stet/1`: the spellings of the closed vocabularies, in both directions.
//!
//! A bare word is only ever one of these, so a reader never has to guess where a word ends. Everything
//! that is text (a name, a URL, an alt text) is a quoted string instead.

use crate::projection::model::{
    AutoKind, CellChange, ChangeKind, ControlState, LooksHiddenSignal, MarkKind, NoteKind,
    ObjectKind, RowChange, Slot,
};
use crate::wordml::{ControlKind, SectionBreak};

macro_rules! words {
    ($to:ident, $from:ident, $ty:ty, { $($variant:path => $word:literal),+ $(,)? }) => {
        pub(super) fn $to(value: $ty) -> &'static str {
            match value {
                $($variant => $word),+
            }
        }

        pub(super) fn $from(word: &str) -> Option<$ty> {
            match word {
                $($word => Some($variant),)+
                _ => None,
            }
        }
    };
}

words!(control_word, control_from, ControlKind, {
    ControlKind::PlainText => "text",
    ControlKind::RichText => "rich",
    ControlKind::Date => "date",
    ControlKind::DropDown => "dropdown",
    ControlKind::ComboBox => "combo",
    ControlKind::Checkbox => "checkbox",
    ControlKind::Picture => "picture",
    ControlKind::Citation => "citation",
    ControlKind::Bibliography => "bibliography",
    ControlKind::BuildingBlock => "block",
    ControlKind::Group => "group",
    ControlKind::RepeatingSection => "repeating",
    ControlKind::Equation => "equation",
    ControlKind::Unknown => "unknown",
});

words!(object_word, object_from, ObjectKind, {
    ObjectKind::Image => "img",
    ObjectKind::Shape => "shape",
    ObjectKind::Chart => "chart",
    ObjectKind::Object => "object",
    ObjectKind::TextBox => "textbox",
});

words!(break_word, break_from, SectionBreak, {
    SectionBreak::NextPage => "next-page",
    SectionBreak::Continuous => "continuous",
    SectionBreak::EvenPage => "even-page",
    SectionBreak::OddPage => "odd-page",
    SectionBreak::NextColumn => "next-column",
});

words!(change_word, change_from, ChangeKind, {
    ChangeKind::Run => "run",
    ChangeKind::Paragraph => "para",
    ChangeKind::Section => "section",
    ChangeKind::Table => "table",
    ChangeKind::Grid => "grid",
    ChangeKind::Row => "row",
    ChangeKind::Cell => "cell",
    ChangeKind::Numbering => "numbering",
});

words!(mark_word, mark_from, MarkKind, {
    MarkKind::Inserted => "inserted",
    MarkKind::Deleted => "deleted",
    MarkKind::MovedTo => "moved-to",
    MarkKind::MovedFrom => "moved-from",
});

words!(row_word, row_from, RowChange, {
    RowChange::Inserted => "inserted",
    RowChange::Deleted => "deleted",
});

words!(cell_word, cell_from, CellChange, {
    CellChange::Inserted => "inserted",
    CellChange::Deleted => "deleted",
    CellChange::Merged => "merged",
});

words!(auto_word, auto_from, AutoKind, {
    AutoKind::Page => "page",
    AutoKind::Date => "date",
    AutoKind::Value => "value",
});

words!(signal_word, signal_from, LooksHiddenSignal, {
    LooksHiddenSignal::White => "white",
    LooksHiddenSignal::Tiny => "tiny",
});

words!(slot_word, slot_from, Slot, {
    Slot::Default => "default",
    Slot::First => "first",
    Slot::Even => "even",
});

/// The two letters in front of a note's number: `fn` for a footnote and `en` for an endnote.
pub(super) fn note_letters(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::Footnote => "fn",
        NoteKind::Endnote => "en",
    }
}

/// The note kind a pair of letters names.
pub(super) fn note_from(letters: &str) -> Option<NoteKind> {
    match letters {
        "fn" => Some(NoteKind::Footnote),
        "en" => Some(NoteKind::Endnote),
        _ => None,
    }
}

/// The word of a control state that cannot be edited. An editable control has none.
pub(super) fn state_word(state: ControlState) -> Option<&'static str> {
    match state {
        ControlState::Editable => None,
        ControlState::Locked => Some("locked"),
        ControlState::DataBound => Some("bound"),
        ControlState::Placeholder => Some("placeholder"),
    }
}

/// The control state a word names.
pub(super) fn state_from(word: &str) -> Option<ControlState> {
    match word {
        "locked" => Some(ControlState::Locked),
        "bound" => Some(ControlState::DataBound),
        "placeholder" => Some(ControlState::Placeholder),
        _ => None,
    }
}
