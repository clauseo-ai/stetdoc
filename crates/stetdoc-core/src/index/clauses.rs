//! Clauses: the labelled paragraphs of a story, with the label a lawyer would cite.
//!
//! A clause is a paragraph that has a label, computed by Word's numbering (`StoryNumbers` in `Final`) or
//! typed by the author (`labels.rs`). `full_context` stops at the list, and in contracts the `(a)` items
//! are often a separate list from the headings they sit under, so "4.2(a)" is no list's `full`. The label is
//! composed here with a **stack of lists** over the labelled paragraphs in story order (design 7.2).

use super::labels::{Class, Noun, after_label, normalize, typed_label};
use crate::numbering::ItemKind;
use crate::wordml::{BlockKey, Story, StoryKind};

/// Where a clause's label came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// Word's numbering computed it.
    Computed,
    /// The author typed it.
    Typed,
}

/// A labelled paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Clause {
    /// The paragraph.
    pub block: BlockKey,
    /// The story it is in.
    pub story: StoryKind,
    /// Its entry in the story's blocks, so that the blocks under it can be found.
    pub entry: usize,
    /// The label as it shows: `4.2`, `(a)`, `III`.
    pub label: String,
    /// The label with the labels of the clauses it sits under: `4.2(a)`.
    pub full: String,
    /// How many clauses it sits inside, itself included.
    pub depth: u8,
    /// Where the label came from.
    pub source: Source,
    /// The full label was built across lists, which is a heuristic, and not taken from Word.
    pub composed: bool,
    /// The noun in front of the label, such as `Article`.
    pub noun: Option<&'static str>,
    /// The first 80 characters of its text.
    pub start: String,
    /// The entry of the division (`Article 6`, `Schedule 2`) it is in, in its own story: the part of the document
    /// that starts over, so that `paragraph 2` means the one in the same Article as the citing paragraph.
    pub division: Option<usize>,
    /// Another way to cite it: `6(2)` for paragraph 2 of Article 6, as EU instruments cite, when the Article is
    /// numbered and the label is a plain number.
    pub alias: Option<String>,
}

impl Clause {
    /// The clause as a reader recognizes it: its noun and full label, and the start of what it says, cut at 50
    /// characters, such as `Section 4.2 Payment` or `4.2(a) the Buyer pays`.
    pub(crate) fn heading(&self) -> String {
        let rest = match self.source {
            Source::Typed => after_label(&self.start),
            Source::Computed => self.start.as_str(),
        };
        let label = match self.noun {
            Some(noun) => format!("{noun} {}", self.full),
            None => self.full.clone(),
        };
        let full = if rest.is_empty() {
            label
        } else {
            format!("{label} {rest}")
        };
        if full.chars().count() > 50 {
            let cut: String = full.chars().take(49).collect();
            format!("{}…", cut.trim_end())
        } else {
            full
        }
    }

    /// The entry of the division the clause belongs to: the one it is in, or itself when it is one.
    pub(crate) fn division_entry(&self) -> Option<usize> {
        self.division.or_else(|| {
            (self.depth == 1 && self.noun.is_some_and(is_division) && !self.label.starts_with('('))
                .then_some(self.entry)
        })
    }
}

/// The list a label counts in: a list of Word's, or a kind of typed numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListKey {
    Computed(i64),
    Typed(Class),
}

/// One entry of the stack: a clause and where it stands in its list.
struct Entry {
    list: ListKey,
    level: u32,
    full: String,
    noun: Option<&'static str>,
    /// The label is a sub-item, `(a)`, and not a heading.
    sub: bool,
    /// The entry of the story the clause is, and its label.
    at: usize,
    label: String,
}

/// The nouns of the parts that a document is divided into, each of which starts over.
pub(super) fn is_division(noun: &str) -> bool {
    matches!(
        noun,
        "Article" | "Part" | "Schedule" | "Exhibit" | "Annex" | "Appendix"
    )
}

/// What the builder is given for a paragraph: the story, the paragraph's entry in it, and its text. Nothing is
/// copied until the paragraph turns out to be a clause, because most are not.
pub(super) struct Labelled<'a> {
    pub story: &'a Story,
    pub entry: usize,
    pub text: &'a str,
}

/// Builds the clauses of a story one paragraph at a time, in story order.
#[derive(Default)]
pub(super) struct Builder {
    stack: Vec<Entry>,
    out: Vec<Clause>,
    previous_label: Option<String>,
}

/// The first 80 characters of a text, for a clause's `start`.
fn start_of(text: &str) -> String {
    text.chars().take(80).collect()
}

/// The level of a typed label in its list: the number of dots in a dotted number, and 0 for the rest.
fn typed_level(label: &str, class: Class) -> u32 {
    match class {
        Class::Dotted | Class::Named(_) => {
            u32::try_from(label.trim_end_matches('.').matches('.').count()).unwrap_or(0)
        }
        _ => 0,
    }
}

impl Builder {
    /// A paragraph that Word numbers, with the label E computed for it.
    pub(super) fn computed(
        &mut self,
        at: &Labelled<'_>,
        kind: &ItemKind,
        list: i64,
        level: u8,
        own_full: &str,
    ) {
        let ItemKind::Counted { text } = kind else {
            return;
        };
        // Word's text may carry the noun (`Article I`, `Section 1.01`), in the label and in the full label. The
        // noun is kept apart, so that every label is of the same shape as a typed one.
        let (label_noun, bare) = split_noun(text.trim());
        let (full_noun, full_bare) = split_noun(own_full.trim());
        self.place(
            at,
            ListKey::Computed(list),
            u32::from(level),
            bare.to_string(),
            full_bare.to_string(),
            full_noun.or(label_noun),
            Source::Computed,
        );
    }

    /// A paragraph whose text starts with a typed label, if it does.
    pub(super) fn typed(&mut self, at: &Labelled<'_>) {
        let Some(found) = typed_label(at.text, self.previous_label.as_deref()) else {
            return;
        };
        let level = typed_level(&found.label, found.class);
        let full = found.label.clone();
        self.place(
            at,
            ListKey::Typed(found.class),
            level,
            found.label,
            full,
            found.noun.map(Noun::name),
            Source::Typed,
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the pieces of one clause, which a struct would only rename"
    )]
    fn place(
        &mut self,
        at: &Labelled<'_>,
        list: ListKey,
        level: u32,
        label: String,
        own_full: String,
        noun: Option<&'static str>,
        source: Source,
    ) {
        // A division (`Article`, `Schedule`, `Exhibit`) starts the document over: nothing above it stays. A
        // heading of any other kind (`4.2`, `Section 4.2`) ends the sub-items `(a)` and `(i)` of the one before.
        // Without this, a list of recitals at the head of a contract would stay on the stack, and the first
        // `(a)` of every later section would be taken for a repeat of the recital's.
        if label.starts_with('(') {
            // A sub-item is placed by its list, below.
        } else if noun.is_some_and(is_division) {
            self.stack.clear();
        } else {
            while self.stack.last().is_some_and(|e| e.sub) {
                self.stack.pop();
            }
        }
        // The first entry of this list at this level or deeper is replaced; failing that, this is a child of
        // the last entry of the list at a shallower level; failing that, a child of the top of the stack.
        let same = self
            .stack
            .iter()
            .position(|e| e.list == list && e.level >= level);
        let keep = match same {
            Some(k) => k,
            None => self
                .stack
                .iter()
                .rposition(|e| e.list == list)
                .map_or(self.stack.len(), |k| k + 1),
        };
        self.stack.truncate(keep);
        // A label that starts with `(` is a sub-item, and under a clause of another list it is composed here.
        // Under a clause of its own list the full label is the list's, which Word's numbering already made.
        let across = self
            .stack
            .last()
            .filter(|e| e.list != list && label.starts_with('('))
            .map(|e| (e.full.clone(), e.noun));
        let (full, composed, noun) = match across {
            Some((parent, parent_noun)) => (format!("{parent}{label}"), true, parent_noun.or(noun)),
            None => (own_full, false, noun),
        };
        // The division that holds it, and the way a citation of that division and this number would write it.
        let first = self
            .stack
            .first()
            .filter(|e| e.noun.is_some_and(is_division));
        let division = first.map(|e| e.at);
        let alias = first
            .filter(|e| {
                e.noun == Some("Article")
                    && !e.label.is_empty()
                    && e.label.chars().all(|c| c.is_ascii_digit())
            })
            .and_then(|e| {
                let digits: String = full.chars().take_while(char::is_ascii_digit).collect();
                let rest = full.get(digits.len()..).unwrap_or("");
                let plain = !digits.is_empty() && (rest.is_empty() || rest.starts_with('('));
                plain.then(|| format!("{}({digits}){rest}", e.label))
            });
        self.stack.push(Entry {
            list,
            level,
            full: full.clone(),
            noun,
            sub: label.starts_with('('),
            at: at.entry,
            label: label.clone(),
        });
        self.previous_label = Some(label.clone());
        let Some(block) = at.story.blocks().get(at.entry).map(|e| at.story.key(e)) else {
            return;
        };
        self.out.push(Clause {
            block,
            story: at.story.kind(),
            entry: at.entry,
            label,
            full,
            depth: u8::try_from(self.stack.len()).unwrap_or(u8::MAX),
            source,
            composed,
            noun,
            start: start_of(at.text),
            division,
            alias,
        });
    }

    /// The clauses, in order.
    pub(super) fn finish(self) -> Vec<Clause> {
        self.out
    }
}

/// `Article I` as a noun and a label, or the label alone.
fn split_noun(label: &str) -> (Option<&'static str>, &str) {
    if let Some((word, rest)) = label.split_once(' ')
        && let Some(noun) = Noun::of(word).filter(|n| n.starts_labels())
    {
        return (Some(noun.name()), rest.trim());
    }
    (None, label)
}

/// How a clause was found for a query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The query's path is the clause's full label.
    FullLabel,
    /// The query's noun and path are the clause's noun and label.
    NounAndLabel,
}

impl Reason {
    /// The reason in words, for a lookup.
    pub(crate) fn words(self) -> &'static str {
        match self {
            Reason::FullLabel => "the full label",
            Reason::NounAndLabel => "the noun and the label",
        }
    }
}

/// A query split into the noun it opens with and the path after it.
pub(crate) struct Query {
    pub noun: Option<Noun>,
    pub path: String,
}

/// `clause 4.2(a)` as `Clause` and `4.2(a)`; `Article III` as `Article` and `III`; `4.2` as no noun. `None` for
/// text that is not a reference: a sentence, or a noun with nothing after it.
pub(crate) fn parse_query(text: &str) -> Option<Query> {
    let text = text.trim().trim_end_matches(['.', ',', ';', ':']);
    let words: Vec<&str> = text.split_whitespace().collect();
    let (first, rest) = words.split_first()?;
    if words.len() > 4 {
        return None;
    }
    let (noun, pieces): (Option<Noun>, &[&str]) = match Noun::of(first) {
        Some(noun) => (Some(noun), rest),
        None => (None, words.as_slice()),
    };
    let path: String = pieces.concat();
    let shaped = !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_alphanumeric() || ".()-".contains(c));
    let has_digit = path.chars().any(|c| c.is_ascii_digit());
    let named_numeral = noun.is_some()
        && (path.chars().count() == 1 && path.chars().all(char::is_alphabetic)
            || path.chars().all(|c| "IVXLCDMivxlcdm".contains(c)));
    (shaped && (has_digit || path.starts_with('(') || named_numeral)).then(|| Query {
        noun,
        path: normalize(&path),
    })
}

/// A clause that matches a query, with the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    /// The clause's position in the table.
    pub at: usize,
    /// Why it matched.
    pub reason: Reason,
}

/// Every clause of the table that the query names, with the ones that match the noun first.
pub(crate) fn resolve(clauses: &[&Clause], query: &Query) -> Vec<Match> {
    let mut by_noun = Vec::new();
    let mut by_path = Vec::new();
    for (at, clause) in clauses.iter().enumerate() {
        let named = std::iter::once(&clause.full).chain(clause.alias.as_ref());
        if !named.into_iter().any(|f| normalize(f) == query.path) {
            continue;
        }
        let noun_matches = match (query.noun, clause.noun) {
            (Some(q), Some(c)) => q.name() == c,
            _ => false,
        };
        if noun_matches {
            by_noun.push(Match {
                at,
                reason: Reason::NounAndLabel,
            });
        } else {
            by_path.push(Match {
                at,
                reason: Reason::FullLabel,
            });
        }
    }
    by_noun.extend(by_path);
    by_noun
}
