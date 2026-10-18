//! The anchor of a run of new lines (design 6.4, step 9).
//!
//! A run sits between a previous line and a next line, and the rule needs only the structure of the
//! edited file, which says what each line is a part of. A run has exactly one anchor or is refused:
//! a new line whose place could be read two ways is refused and the message says to use `insert`, which
//! says exactly where the paragraph goes.

use super::style::Previous;
use crate::api::files::Place;
use crate::projection::draft::{DraftBlock, DraftLine};
use crate::projection::keys::Key;
use crate::projection::model::OpaqueBlock;
use crate::refs::BlockRef;

/// Where the paragraphs of a run go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Anchor {
    /// The block they go next to: a paragraph, a table, or the last paragraph of a collapsed field.
    pub block: BlockRef,
    /// Before it or after it.
    pub place: Place,
    /// What the style of the first paragraph is modelled on.
    pub previous: Previous,
}

/// The lines of an edited file, with what each is a part of.
pub(super) struct Lines<'a> {
    lines: &'a [DraftLine],
    common_style: Option<&'a str>,
    /// How many tables are open at each line.
    depth: Vec<u32>,
    /// For each end-of-table line, the table line it ends.
    opens: Vec<Option<usize>>,
}

impl<'a> Lines<'a> {
    pub(super) fn new(lines: &'a [DraftLine], common_style: Option<&'a str>) -> Lines<'a> {
        let mut depth = Vec::with_capacity(lines.len());
        let mut opens = vec![None; lines.len()];
        let mut stack: Vec<usize> = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if let DraftLine::Structure(s) = line {
                match &s.key {
                    Key::Table(_) => stack.push(index),
                    Key::EndTable => {
                        if let Some(open) = stack.pop()
                            && let Some(slot) = opens.get_mut(index)
                        {
                            *slot = Some(open);
                        }
                    }
                    _ => {}
                }
            }
            depth.push(u32::try_from(stack.len()).unwrap_or(u32::MAX));
        }
        Lines {
            lines,
            common_style,
            depth,
            opens,
        }
    }

    /// The name of a paragraph's style: its tag, or the common style of the file.
    pub(super) fn style_of(&self, block: &DraftBlock) -> Option<String> {
        block
            .tags
            .iter()
            .find_map(|key| match key {
                Key::Style(name) => Some(name.to_string()),
                _ => None,
            })
            .or_else(|| self.common_style.map(str::to_string))
    }

    /// How many tables are open at a line.
    pub(super) fn depth_at(&self, index: usize) -> u32 {
        self.depth.get(index).copied().unwrap_or(0)
    }

    fn paragraph_at(&self, index: usize) -> Option<&DraftBlock> {
        match self.lines.get(index)? {
            DraftLine::Block(block) => Some(block),
            DraftLine::Structure(_) => None,
        }
    }

    fn continues_cell(&self, index: usize) -> bool {
        match self.lines.get(index) {
            Some(DraftLine::Block(block)) => block.tags.contains(&Key::CellPlus),
            Some(DraftLine::Structure(s)) => match &s.key {
                Key::Table(table) => table.cell.contains(&Key::CellPlus),
                _ => false,
            },
            None => false,
        }
    }

    /// The anchor of the run of lines `start..end`, or why there is none.
    pub(super) fn anchor(&self, start: usize, end: usize) -> Result<Anchor, String> {
        let next = self.paragraph_at(end).and_then(|b| b.r#ref.map(|r| (r, b)));
        let before_next = |context: &str| -> Result<Anchor, String> {
            match next {
                Some((r, block)) => Ok(Anchor {
                    block: r,
                    place: Place::Before,
                    previous: Previous::Precedes(self.style_of(block)),
                }),
                None => Err(format!(
                    "{context}, and no paragraph follows it to put the new line before"
                )),
            }
        };
        let Some(prev_index) = start.checked_sub(1) else {
            return before_next("it is at the start of the file");
        };
        match self.lines.get(prev_index) {
            Some(DraftLine::Block(block)) => {
                let Some(r) = block.r#ref else {
                    return Err("it follows a line that has no ref".to_string());
                };
                let depth = self.depth.get(prev_index).copied().unwrap_or(0);
                if depth == 0 || self.continues_cell(end) {
                    Ok(Anchor {
                        block: r,
                        place: Place::After,
                        previous: Previous::Follows(self.style_of(block)),
                    })
                } else {
                    Err("it follows the last paragraph of a table cell, and it could be a new paragraph in the cell or a paragraph after the table".to_string())
                }
            }
            Some(DraftLine::Structure(s)) => match &s.key {
                Key::EndTable => {
                    let open = self.opens.get(prev_index).copied().flatten();
                    let table = open.and_then(|i| match self.lines.get(i) {
                        Some(DraftLine::Structure(t)) => match &t.key {
                            Key::Table(table) => table.r#ref,
                            _ => None,
                        },
                        _ => None,
                    });
                    match table {
                        Some(table) => Ok(Anchor {
                            block: table,
                            place: Place::After,
                            previous: Previous::Precedes(next.and_then(|(_, b)| self.style_of(b))),
                        }),
                        None => Err("it follows the end of a table that has no ref".to_string()),
                    }
                }
                Key::Table(_) | Key::Row(_) => {
                    Err("it is inside a table, before its first cell".to_string())
                }
                Key::Section(_) | Key::Unit(_) | Key::Anchored(_) => {
                    if next.is_none() && self.next_is_table(end) {
                        return Err("it is before a table".to_string());
                    }
                    before_next("it is at the start of a section or a unit")
                }
                Key::Opaque(OpaqueBlock::Collapsed {
                    last: Some(last), ..
                }) => Ok(Anchor {
                    block: *last,
                    place: Place::After,
                    previous: Previous::Follows(None),
                }),
                Key::Opaque(_) => Err("it is next to content that has no ref".to_string()),
                _ => Err("it follows a line that is not a paragraph".to_string()),
            },
            None => Err("there is nothing before it".to_string()),
        }
    }

    fn next_is_table(&self, index: usize) -> bool {
        matches!(
            self.lines.get(index),
            Some(DraftLine::Structure(s)) if matches!(s.key, Key::Table(_))
        )
    }
}
