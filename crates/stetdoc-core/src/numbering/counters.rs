//! The counters: one pass over a story's paragraphs gives every clause number.
//!
//! The model is in the design, section 4.3. A list is keyed by the `w:abstractNum` that its `w:num`
//! names, so two `w:num` over one definition share counters. A `w:startOverride` restarts a level the
//! first time a `w:num` numbers it, and whenever the level restarts. A deleted paragraph mark advances
//! nothing in `Final`, and an inserted one advances nothing in `Original`. In `Redline` a paragraph that
//! only exists in `Original` gets a ghost number and leaves the counters as it found them.

use super::ctx::{Ctx, Entry, Table};
use super::format::{Approx, is_counting, render};
use super::item::{ItemKind, ListId, ListItem, Note};
use super::label::{Piece, strip_trailing_periods};
use crate::api::view::Mode;
use crate::styles::Cascade;
use crate::wordml::{NodeId, NumFact, NumFmt, NumberingDefs};
use std::collections::{HashMap, HashSet};

/// The levels of a list.
const LEVELS: usize = 9;

/// The counters of one list.
#[derive(Clone, Default)]
struct ListState {
    /// The current counter of each level.
    value: [i64; LEVELS],
    /// The level has a value since its last restart.
    entered: [bool; LEVELS],
}

/// The clause numbers of one story in one mode. Immutable, and a pure function of the facts, the
/// definitions and the mode.
#[derive(Debug, Clone, Default)]
pub struct StoryNumbers {
    items: Vec<(NodeId, ListItem)>,
    index: HashMap<NodeId, usize>,
    notes: Vec<Note>,
    placed: HashMap<NodeId, Vec<u32>>,
}

impl StoryNumbers {
    /// Numbers the paragraphs of one story. `facts` are in the story's entry order.
    pub fn build(
        facts: &[NumFact],
        defs: &NumberingDefs,
        cascade: &Cascade,
        mode: Mode,
    ) -> StoryNumbers {
        let mut run = Run {
            ctx: Ctx::new(defs, cascade),
            lists: HashMap::new(),
            started: HashSet::new(),
            out: StoryNumbers::default(),
        };
        for fact in facts {
            run.paragraph(fact, mode);
        }
        let Run { ctx, mut out, .. } = run;
        out.notes = ctx.notes;
        out.placed = ctx.placed;
        out
    }

    /// The list item of a paragraph, if Word numbers or bullets it in this mode.
    pub fn item(&self, paragraph: NodeId) -> Option<&ListItem> {
        self.items.get(*self.index.get(&paragraph)?).map(|(_, i)| i)
    }

    /// The clause number: the label Word shows, such as `1.2` or `(a)`. `None` for a bullet, a level
    /// that shows nothing, and a paragraph that is not numbered.
    pub fn clause_number(&self, paragraph: NodeId) -> Option<&str> {
        match &self.item(paragraph)?.kind {
            ItemKind::Counted { text } => Some(text),
            _ => None,
        }
    }

    /// The label with its ancestors' labels, such as `1.2(a)`, with trailing periods stripped.
    pub fn full_context(&self, paragraph: NodeId) -> Option<&str> {
        let item = self.item(paragraph)?;
        matches!(item.kind, ItemKind::Counted { .. }).then_some(item.full.as_str())
    }

    /// The paragraphs whose number is exactly `text` (trailing periods ignored), in document order:
    /// those whose full-context form matches, or, if none does, those whose label matches.
    pub fn find(&self, text: &str) -> impl Iterator<Item = NodeId> + '_ {
        let query = strip_trailing_periods(text.trim()).to_string();
        let by_full: Vec<NodeId> = self
            .items
            .iter()
            .filter(|(_, i)| matches!(i.kind, ItemKind::Counted { .. }) && i.full == query)
            .map(|(n, _)| *n)
            .collect();
        let found = if by_full.is_empty() {
            self.items
                .iter()
                .filter(|(_, i)| match &i.kind {
                    ItemKind::Counted { text } => strip_trailing_periods(text) == query,
                    _ => false,
                })
                .map(|(n, _)| *n)
                .collect()
        } else {
            by_full
        };
        found.into_iter()
    }

    /// What was approximate, or could not be followed, each note once for the story.
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// The notes that arose while numbering this paragraph: its own label was approximated or its level
    /// was missing. A note that is about the story (`ListNumField`) has no paragraph. The same note may
    /// be at many paragraphs, and `notes` lists it once.
    pub fn notes_at(&self, paragraph: NodeId) -> impl Iterator<Item = &Note> {
        self.placed
            .get(&paragraph)
            .into_iter()
            .flatten()
            .filter_map(|i| self.notes.get(*i as usize))
    }

    /// Records whether the story has `LISTNUM` fields. Word counts them in the lists of the numbered
    /// paragraphs and these counters do not, so a story that has both gets a note. The fields are the
    /// caller's to find, because the facts that `build` reads do not carry them.
    #[must_use]
    pub fn with_list_num_fields(mut self, present: bool) -> StoryNumbers {
        if present && !self.items.is_empty() && !self.notes.contains(&Note::ListNumField) {
            self.notes.push(Note::ListNumField);
        }
        self
    }

    /// Every list item in document order.
    pub fn items(&self) -> impl Iterator<Item = (NodeId, &ListItem)> + '_ {
        self.items.iter().map(|(n, i)| (*n, i))
    }
}

/// The state of one build.
struct Run<'a> {
    ctx: Ctx<'a>,
    lists: HashMap<i64, ListState>,
    /// The (`w:num`, level) pairs that have begun numbering in this story and mode.
    started: HashSet<(i64, u8)>,
    out: StoryNumbers,
}

/// In which modes a paragraph exists: `Final`, `Original`.
fn exists(f: &NumFact) -> (bool, bool) {
    let mark_gone_in_final = f.join_applies && f.mark_revs.gone_in(Mode::Final);
    let mark_gone_in_original = f.join_applies && f.mark_revs.gone_in(Mode::Original);
    use crate::wordml::Presence::{Always, FinalOnly, OriginalOnly};
    (
        matches!(f.presence, Always | FinalOnly) && !mark_gone_in_final,
        matches!(f.presence, Always | OriginalOnly) && !mark_gone_in_original,
    )
}

impl<'a> Run<'a> {
    /// Numbers one paragraph, if it is a list item that exists in the mode.
    fn paragraph(&mut self, f: &NumFact, mode: Mode) {
        let (in_final, in_original) = exists(f);
        let (live, ghost) = match mode {
            Mode::Final => (in_final, false),
            Mode::Original => (in_original, false),
            Mode::Redline => (in_final, !in_final && in_original),
        };
        if !live && !ghost {
            return;
        }
        self.ctx.at = Some(f.node);
        let Some(b) = self.ctx.bind(f, mode == Mode::Original) else {
            return;
        };
        let table = self.ctx.table(b.num_id);
        let Some(own) = self.ctx.entry_or_lower(&table, b.num_id, b.ilvl) else {
            self.ctx.note(Note::UnresolvedNumId { num_id: b.num_id });
            return;
        };
        let key = own.lv.key;
        let snapshot = ghost.then(|| (self.lists.get(&key).cloned(), self.started.clone()));
        let item = self.advance(b.num_id, b.ilvl, &table, own, b.from_style, ghost);
        if let Some((state, started)) = snapshot {
            match state {
                Some(s) => {
                    self.lists.insert(key, s);
                }
                None => {
                    self.lists.remove(&key);
                }
            }
            self.started = started;
        }
        self.out.index.insert(f.node, self.out.items.len());
        self.out.items.push((f.node, item));
    }

    /// Moves the counters to this paragraph and builds its list item.
    fn advance(
        &mut self,
        num_id: i64,
        ilvl: u8,
        table: &Table<'a>,
        own: &Entry<'a>,
        from_style: bool,
        ghost: bool,
    ) -> ListItem {
        let l = usize::from(ilvl);
        let lv = own.lv;
        let mut st = self.lists.remove(&lv.key).unwrap_or_default();
        // Deeper levels restart, unless their lvlRestart says otherwise.
        for d in (l + 1)..LEVELS {
            let restart = table
                .get(d)
                .and_then(|e| e.as_ref())
                .and_then(|e| e.lv.level.restart);
            let restarts = match restart {
                None => true,
                Some(0) => false,
                Some(n) if usize::from(n) > d => true,
                Some(n) => l < usize::from(n),
            };
            if restarts && let Some(e) = st.entered.get_mut(d) {
                *e = false;
            }
        }
        // Shallower levels that never had a value show their start, and count as entered.
        for j in 0..l {
            if st.entered.get(j).copied().unwrap_or(true) {
                continue;
            }
            let Some(Some(ej)) = table.get(j) else {
                continue;
            };
            set(&mut st.value, j, ej.lv.start);
            set(&mut st.entered, j, true);
            self.started.insert((num_id, j as u8));
        }
        // The level itself.
        let first_use = own.has_override && !self.started.contains(&(num_id, ilvl));
        let entered = st.entered.get(l).copied().unwrap_or(false);
        let value = if !entered || first_use {
            lv.start
        } else {
            st.value.get(l).copied().unwrap_or(0).saturating_add(1)
        };
        set(&mut st.value, l, value);
        set(&mut st.entered, l, true);
        self.started.insert((num_id, ilvl));

        let text = self.label(table, own, ilvl, &st.value);
        let kind = match lv.level.fmt {
            NumFmt::Bullet => ItemKind::Bullet {
                glyph: lv.level.text.to_string(),
            },
            _ if !is_counting(&lv.level.fmt) || text.is_empty() => ItemKind::Plain,
            _ => ItemKind::Counted { text: text.clone() },
        };
        let counted = matches!(kind, ItemKind::Counted { .. });
        let full = if counted {
            self.full_context(table, ilvl, &st, &text)
        } else {
            String::new()
        };
        self.lists.insert(lv.key, st);
        ListItem {
            level: ilvl,
            num_id,
            list: ListId(lv.key),
            kind,
            suffix: lv.level.suff,
            ghost,
            from_style,
            label_hidden: lv.level.run.on_off("vanish") == Some(true),
            full,
        }
    }

    /// The label with its ancestors, as `REF \w` shows it: while a level's text does not carry its
    /// ancestors, the label of the next shallower level, rendered from the counters as they are now, is
    /// put in front. (A level that never restarts, `lvlRestart` 0, keeps its old value under a new
    /// parent, and its label must show that value with the new parent's number.)
    fn full_context(&mut self, table: &Table<'a>, ilvl: u8, st: &ListState, text: &str) -> String {
        let mut parts = vec![strip_trailing_periods(text).to_string()];
        let mut j = ilvl;
        while j > 0 {
            let carries = table
                .get(usize::from(j))
                .and_then(|e| e.as_ref())
                .is_some_and(|e| e.carries_ancestors);
            if carries {
                break;
            }
            j -= 1;
            let Some(Some(above)) = table.get(usize::from(j)) else {
                continue;
            };
            if !is_counting(&above.lv.level.fmt)
                || !st.entered.get(usize::from(j)).copied().unwrap_or(false)
            {
                continue;
            }
            let label = self.label(table, above, j, &st.value);
            if !label.is_empty() {
                parts.insert(0, strip_trailing_periods(&label).to_string());
            }
        }
        parts.concat()
    }

    /// The level's text with the counters filled in. Placeholders for deeper levels render nothing.
    fn label(
        &mut self,
        table: &Table<'a>,
        entry: &Entry<'a>,
        ilvl: u8,
        values: &[i64; LEVELS],
    ) -> String {
        let legal = entry.lv.level.legal;
        let mut out = String::new();
        for piece in &entry.pieces {
            match piece {
                Piece::Literal(l) => out.push_str(l),
                Piece::Level(j) if *j > ilvl => {}
                Piece::Level(j) => {
                    let Some(Some(named)) = table.get(usize::from(*j)) else {
                        continue;
                    };
                    let fmt = if legal && is_counting(&named.lv.level.fmt) {
                        NumFmt::Decimal
                    } else {
                        named.lv.level.fmt.clone()
                    };
                    let rendered = render(&fmt, values.get(usize::from(*j)).copied().unwrap_or(0));
                    if let Some(a) = rendered.approx {
                        self.ctx.note(match a {
                            Approx::Unsupported(fmt) => Note::UnsupportedFormat { fmt },
                            Approx::OutOfRange(fmt) => Note::OutOfRange { fmt },
                        });
                    }
                    out.push_str(&rendered.text);
                }
            }
        }
        out.trim().to_string()
    }
}

/// Writes `v` at index `i` if there is one.
fn set<T>(slice: &mut [T], i: usize, v: T) {
    if let Some(slot) = slice.get_mut(i) {
        *slot = v;
    }
}
