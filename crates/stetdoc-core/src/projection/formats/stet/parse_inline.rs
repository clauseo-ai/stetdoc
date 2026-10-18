//! A paragraph's text: slots between markers, and the markers.
//!
//! The text is read once, left to right, with an explicit stack for the markers that open and close
//! around editable text (an insertion, a link, a control) and a depth of at most 32. A raw `{` starts a
//! marker and a raw `}` ends one, so a brace in a text is always escaped and a marker is found without
//! lookahead. The closers `++}` and `--}` are the only markers that have no opening brace of their own:
//! a raw `}` right after two raw plus signs closes an insertion, and after two raw minus signs a deletion.

use super::escape::read_escape;
use super::refs_syntax::read_ref;
use super::tokens::{MAX_PAYLOAD, Tok, marker_end, tokenize};
use super::words::{auto_from, control_from, note_from, object_from, signal_from, state_from};
use crate::projection::draft::Seam;
use crate::projection::keys::{ControlKey, Edge, FieldKey, Key, ObjectKey, RevRef, Span};

/// The deepest nesting of insertions, links and controls in one line.
const MAX_DEPTH: usize = 32;

/// A line cut into slots and seams.
pub(super) struct Cut {
    pub texts: Vec<String>,
    pub seams: Vec<Seam>,
}

/// Why a text could not be read.
#[derive(Debug)]
pub(super) enum ParseError {
    /// The text is not valid `stet/1`.
    Invalid(String),
    /// The text is over a limit.
    Limit(String),
}

impl From<String> for ParseError {
    fn from(message: String) -> ParseError {
        ParseError::Invalid(message)
    }
}

enum Frame {
    Ins { open: usize },
    Link { url: Box<str> },
    Control { control: ControlKey },
}

struct Reader<'a> {
    s: &'a str,
    at: usize,
    texts: Vec<String>,
    seams: Vec<Seam>,
    stack: Vec<Frame>,
    run: (char, usize),
}

/// Reads the text of a paragraph line, which has no trailing spaces.
pub(super) fn parse_text(s: &str) -> Result<Cut, ParseError> {
    let mut reader = Reader {
        s,
        at: 0,
        texts: vec![String::new()],
        seams: Vec::new(),
        stack: Vec::new(),
        run: (' ', 0),
    };
    reader.run_all()?;
    Ok(Cut {
        texts: reader.texts,
        seams: reader.seams,
    })
}

const SPLIT_HINT: &str = "A paragraph cannot be split inside a tracked insertion, an external link or a content control by editing a file. Use the `split` operation.";

impl Reader<'_> {
    fn push_raw(&mut self, c: char) {
        if let Some(slot) = self.texts.last_mut() {
            slot.push(c);
        }
        self.run = if c == '+' || c == '-' {
            if self.run.0 == c {
                (c, self.run.1 + 1)
            } else {
                (c, 1)
            }
        } else {
            (' ', 0)
        };
    }

    fn push_decoded(&mut self, c: char) {
        if let Some(slot) = self.texts.last_mut() {
            slot.push(c);
        }
        self.run = (' ', 0);
    }

    fn seam(&mut self, key: Key) {
        self.seams.push(Seam { key, width: 0 });
        self.texts.push(String::new());
        self.run = (' ', 0);
    }

    fn run_all(&mut self) -> Result<(), ParseError> {
        while let Some(rest) = self.s.get(self.at..) {
            let Some(c) = rest.chars().next() else {
                break;
            };
            match c {
                '\\' => {
                    let (decoded, next) =
                        read_escape(self.s, self.at, false).map_err(|e| e.message())?;
                    self.push_decoded(decoded);
                    self.at = next;
                }
                '{' => self.marker()?,
                '}' => self.closer()?,
                other => {
                    self.push_raw(other);
                    self.at += other.len_utf8();
                }
            }
        }
        match self.stack.last() {
            None => Ok(()),
            Some(Frame::Ins { .. }) => Err(ParseError::Invalid(format!(
                "the `{{++` is not closed on this line. {SPLIT_HINT}"
            ))),
            Some(Frame::Link { .. }) => Err(ParseError::Invalid(format!(
                "the `{{link ..}}` is not closed with `{{/link}}` on this line. {SPLIT_HINT}"
            ))),
            Some(Frame::Control { .. }) => Err(ParseError::Invalid(format!(
                "the `{{ctl ..}}` is not closed with `{{/ctl}}` on this line. {SPLIT_HINT}"
            ))),
        }
    }

    fn push_frame(&mut self, frame: Frame) -> Result<(), ParseError> {
        if self.stack.len() >= MAX_DEPTH {
            return Err(ParseError::Limit(format!(
                "markers are nested more than {MAX_DEPTH} deep"
            )));
        }
        self.stack.push(frame);
        Ok(())
    }

    /// A raw `}` that is not the end of a marker: only the end of `++}`.
    fn closer(&mut self) -> Result<(), ParseError> {
        let closes_insertion = matches!(self.stack.last(), Some(Frame::Ins { .. }))
            && self.run.0 == '+'
            && self.run.1 >= 2;
        if !closes_insertion {
            return Err(ParseError::Invalid(
                "a } that does not end a marker. Write \\} for a literal brace".to_string(),
            ));
        }
        if let Some(slot) = self.texts.last_mut() {
            slot.truncate(slot.len().saturating_sub(2));
        }
        let Some(Frame::Ins { open }) = self.stack.pop() else {
            return Ok(());
        };
        self.at += 1;
        let (rev, direction, next) = rev_tag(self.s, self.at)?;
        self.at = next;
        let span = match direction {
            Some(Direction::From) => Span::MoveIn(rev),
            Some(Direction::To) => {
                return Err(ParseError::Invalid(
                    "an insertion is moved from another place: write `move from`, not `move to`"
                        .to_string(),
                ));
            }
            None => Span::Ins(rev),
        };
        if let Some(seam) = self.seams.get_mut(open) {
            seam.key = Key::Open(span.clone());
        }
        self.seam(Key::Close(span));
        Ok(())
    }

    fn marker(&mut self) -> Result<(), ParseError> {
        let rest = self.s.get(self.at..).unwrap_or("");
        if rest.starts_with("{++") {
            let open = self.seams.len();
            let placeholder = Span::Ins(RevRef {
                id: 0,
                author: "".into(),
                peer: None,
            });
            self.seam(Key::Open(placeholder));
            self.push_frame(Frame::Ins { open })?;
            self.at += 3;
            return Ok(());
        }
        if rest.starts_with("{--") {
            return self.deleted();
        }
        let end = marker_end(self.s, self.at)?;
        let body = self.s.get(self.at + 1..end).unwrap_or("");
        self.at = end + 1;
        match classify(body)? {
            Marker::Seam(key) => self.seam(key),
            Marker::OpenLink(url) => {
                let key = Key::Open(Span::Link(url.clone()));
                self.seam(key);
                self.push_frame(Frame::Link { url })?;
            }
            Marker::OpenControl(control) => {
                self.seam(Key::Open(Span::Control(control.clone())));
                self.push_frame(Frame::Control { control })?;
            }
            Marker::CloseLink => match self.stack.pop() {
                Some(Frame::Link { url }) => self.seam(Key::Close(Span::Link(url))),
                _ => {
                    return Err("a {/link} with no {link ..} open before it"
                        .to_string()
                        .into());
                }
            },
            Marker::CloseControl => match self.stack.pop() {
                Some(Frame::Control { control }) => self.seam(Key::Close(Span::Control(control))),
                _ => {
                    return Err("a {/ctl} with no {ctl ..} open before it"
                        .to_string()
                        .into());
                }
            },
        }
        Ok(())
    }

    /// `{--text--}{#id AUTH}`: the text is escaped, and ends at a raw `}` right after two raw minus signs.
    fn deleted(&mut self) -> Result<(), ParseError> {
        let mut at = self.at + 3;
        let mut text = String::new();
        let mut run = 0usize;
        loop {
            let Some(rest) = self.s.get(at..) else {
                break;
            };
            let Some(c) = rest.chars().next() else {
                return Err(ParseError::Invalid(
                    "the `{--` is not closed with `--}` on this line".to_string(),
                ));
            };
            match c {
                '\\' => {
                    let (decoded, next) =
                        read_escape(self.s, at, false).map_err(|e| e.message())?;
                    text.push(decoded);
                    run = 0;
                    at = next;
                }
                '{' => {
                    return Err(ParseError::Invalid(
                        "a { inside deleted text. Deleted text is read-only: write \\{ for a literal brace".to_string(),
                    ));
                }
                '}' => {
                    if run < 2 {
                        return Err(ParseError::Invalid(
                            "a } inside deleted text that does not end it with `--}`".to_string(),
                        ));
                    }
                    text.truncate(text.len().saturating_sub(2));
                    at += 1;
                    break;
                }
                other => {
                    text.push(other);
                    run = if other == '-' { run + 1 } else { 0 };
                    at += other.len_utf8();
                }
            }
            if text.chars().count() > MAX_PAYLOAD + 2 {
                return Err(ParseError::Limit(format!(
                    "deleted text over {MAX_PAYLOAD} characters"
                )));
            }
        }
        let (rev, direction, next) = rev_tag(self.s, at)?;
        let moved = match direction {
            Some(Direction::To) => true,
            Some(Direction::From) => {
                return Err(ParseError::Invalid(
                    "deleted text is moved to another place: write `move to`, not `move from`"
                        .to_string(),
                ));
            }
            None => false,
        };
        self.at = next;
        self.seam(Key::Deleted {
            moved,
            rev,
            text: text.into(),
        });
        Ok(())
    }
}

/// Which end of a move a change is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    /// `move from #n`: the change is the destination, and `n` is the source.
    From,
    /// `move to #n`: the change is the source, and `n` is the destination.
    To,
}

/// A change's id, author and move, from a list of tokens that follow a `#12`: `[#12, JD, move, from, #13]`.
pub(super) fn rev_tokens(toks: &[Tok]) -> Result<(RevRef, Option<Direction>), String> {
    let mut it = toks.iter();
    let Some(Tok::Word(id)) = it.next() else {
        return Err("a change must start with its id, such as #12".to_string());
    };
    let id = id
        .strip_prefix('#')
        .and_then(|n| n.parse::<i64>().ok())
        .ok_or_else(|| format!("{id:?} is not a change id such as #12"))?;
    let Some(Tok::Word(author)) = it.next() else {
        return Err("a change must name its author's code after its id".to_string());
    };
    let mut peer = None;
    let mut direction = None;
    if let Some(Tok::Word(m)) = it.next() {
        if m != "move" {
            return Err(format!("{m:?} is not expected after a change's author"));
        }
        direction = match it.next() {
            Some(Tok::Word(d)) if d == "from" => Some(Direction::From),
            Some(Tok::Word(d)) if d == "to" => Some(Direction::To),
            _ => return Err("`move` must be followed by `from` or `to`".to_string()),
        };
        peer = match it.next() {
            Some(Tok::Word(p)) if p == "-" => None,
            Some(Tok::Word(p)) => Some(
                p.strip_prefix('#')
                    .and_then(|n| n.parse::<i64>().ok())
                    .ok_or_else(|| format!("{p:?} is not a change id such as #13"))?,
            ),
            _ => return Err("`move from` and `move to` need the other end's id".to_string()),
        };
    }
    if it.next().is_some() {
        return Err("something after the end of a change tag".to_string());
    }
    Ok((
        RevRef {
            id,
            author: author.as_str().into(),
            peer,
        },
        direction,
    ))
}

/// The `{#12 JD}` that must follow `++}` and `--}`, immediately.
fn rev_tag(s: &str, at: usize) -> Result<(RevRef, Option<Direction>, usize), ParseError> {
    if s.get(at..).is_none_or(|r| !r.starts_with("{#")) {
        return Err(ParseError::Invalid(
            "a tracked change must be followed by its tag, such as {#12 JD}. Changes are the document's and are not written by hand"
                .to_string(),
        ));
    }
    let end = marker_end(s, at)?;
    let body = s.get(at + 1..end).unwrap_or("");
    let toks = tokenize(body)?;
    let (rev, direction) = rev_tokens(&toks)?;
    Ok((rev, direction, end + 1))
}

enum Marker {
    Seam(Key),
    OpenLink(Box<str>),
    OpenControl(ControlKey),
    CloseLink,
    CloseControl,
}

fn quoted(toks: &[Tok], at: usize, what: &str) -> Result<Box<str>, String> {
    match toks.get(at) {
        Some(Tok::Quoted(text)) => Ok(text.as_str().into()),
        _ => Err(format!("{what} needs a quoted text")),
    }
}

fn end_at(toks: &[Tok], at: usize, what: &str) -> Result<(), String> {
    if toks.len() == at {
        Ok(())
    } else {
        Err(format!("something unexpected after the end of {what}"))
    }
}

fn digits(rest: &str) -> Option<i64> {
    let body = rest.strip_prefix('-').unwrap_or(rest);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

fn classify(body: &str) -> Result<Marker, String> {
    let toks = tokenize(body)?;
    let Some(Tok::Word(first)) = toks.first() else {
        return Err("an empty marker `{}`. Write \\{ \\} for literal braces".to_string());
    };
    let first = first.as_str();
    let simple = |key: Key| end_at(&toks, 1, first).map(|()| Marker::Seam(key));
    match first {
        "link" => {
            let url = quoted(&toks, 1, "{link}")?;
            end_at(&toks, 2, "{link}")?;
            Ok(Marker::OpenLink(url))
        }
        "/link" => end_at(&toks, 1, "{/link}").map(|()| Marker::CloseLink),
        "/ctl" => end_at(&toks, 1, "{/ctl}").map(|()| Marker::CloseControl),
        "ctl" => control(&toks),
        "field" => field(&toks).map(Marker::Seam),
        "hidden" => {
            let text = quoted(&toks, 1, "{hidden}")?;
            end_at(&toks, 2, "{hidden}")?;
            Ok(Marker::Seam(Key::Hidden(text)))
        }
        "looks-hidden" => {
            let Some(Tok::Word(signal)) = toks.get(1) else {
                return Err("{looks-hidden} needs `white` or `tiny`".to_string());
            };
            let signal = signal_from(signal)
                .ok_or_else(|| format!("{signal:?} is not `white` or `tiny`"))?;
            let text = quoted(&toks, 2, "{looks-hidden}")?;
            end_at(&toks, 3, "{looks-hidden}")?;
            Ok(Marker::Seam(Key::LooksHidden { signal, text }))
        }
        "math" => {
            let text = quoted(&toks, 1, "{math}")?;
            end_at(&toks, 2, "{math}")?;
            Ok(Marker::Seam(Key::Math(text)))
        }
        "pagebreak" => simple(Key::PageBreak),
        "colbreak" => simple(Key::ColumnBreak),
        word => {
            if let Some(kind) = auto_from(word) {
                return simple(Key::Auto(kind));
            }
            if let Some(kind) = object_from(word) {
                return object(&toks, kind).map(Marker::Seam);
            }
            if let Some(key) = comment_or_note(word) {
                return simple(key);
            }
            Err(format!(
                "{{{word} ..}} is not a marker. A new line holds text only: write a literal brace as \\{{ and \\}}"
            ))
        }
    }
}

fn comment_or_note(word: &str) -> Option<Key> {
    if let Some(id) = word.strip_prefix("/c").and_then(digits) {
        return Some(Key::Comment {
            id,
            edge: Edge::End,
        });
    }
    if let Some(id) = word.strip_prefix('c').and_then(digits) {
        return Some(Key::Comment {
            id,
            edge: Edge::Start,
        });
    }
    let letters = word.get(..2)?;
    let kind = note_from(letters)?;
    let rest = word.get(2..)?;
    if rest == "." {
        return Some(Key::NoteMark(kind));
    }
    let ordinal = rest
        .parse::<u32>()
        .ok()
        .filter(|_| rest.bytes().all(|b| b.is_ascii_digit()))?;
    Some(Key::NoteRef { kind, ordinal })
}

fn control(toks: &[Tok]) -> Result<Marker, String> {
    let Some(Tok::Word(kind)) = toks.get(1) else {
        return Err("{ctl} needs the kind of control".to_string());
    };
    let kind = control_from(kind).ok_or_else(|| format!("{kind:?} is not a kind of control"))?;
    let mut at = 2;
    let mut alias = None;
    if let Some(Tok::Quoted(text)) = toks.get(at) {
        alias = Some(text.as_str().into());
        at += 1;
    }
    let mut tag = None;
    if let Some(Tok::Named(name, value)) = toks.get(at) {
        match (name.as_str(), &**value) {
            ("tag", Tok::Quoted(text)) => tag = Some(text.as_str().into()),
            _ => return Err(format!("{name}= is not expected in {{ctl}}")),
        }
        at += 1;
    }
    let control = ControlKey { kind, alias, tag };
    let Some(Tok::Word(state)) = toks.get(at) else {
        end_at(toks, at, "{ctl}")?;
        return Ok(Marker::OpenControl(control));
    };
    let state = state_from(state)
        .ok_or_else(|| format!("{state:?} is not `locked`, `bound` or `placeholder`"))?;
    let text = quoted(toks, at + 1, "a read-only {ctl}")?;
    end_at(toks, at + 2, "{ctl}")?;
    Ok(Marker::Seam(Key::Control {
        info: control,
        state,
        text,
    }))
}

fn field(toks: &[Tok]) -> Result<Key, String> {
    let keyword: Box<str> = match toks.get(1) {
        Some(Tok::Word(w) | Tok::Quoted(w)) => w.as_str().into(),
        _ => return Err("{field} needs a keyword".to_string()),
    };
    let mut at = 2;
    let (mut opens, mut closes) = (true, true);
    if let Some(Tok::Word(part)) = toks.get(at) {
        match part.as_str() {
            "start" => closes = false,
            "more" => (opens, closes) = (false, false),
            "end" => opens = false,
            other => return Err(format!("{other:?} is not `start`, `more` or `end`")),
        }
        at += 1;
    }
    let text = quoted(toks, at, "{field}")?;
    at += 1;
    let mut target = None;
    let mut dirty = false;
    for tok in toks.iter().skip(at) {
        match tok {
            Tok::Word(w) if w == "dirty" => dirty = true,
            Tok::Word(w) if w.starts_with("->") => {
                let token = w.get(2..).unwrap_or("");
                target = Some(read_ref(token).map_err(|e| e.message().to_string())?);
            }
            _ => return Err("something unexpected after the result of {field}".to_string()),
        }
    }
    Ok(Key::Field(FieldKey {
        keyword,
        text,
        target,
        opens,
        closes,
        dirty,
    }))
}

fn object(toks: &[Tok], kind: crate::projection::model::ObjectKind) -> Result<Key, String> {
    let mut at = 1;
    let mut alt = None;
    if let Some(Tok::Quoted(text)) = toks.get(at) {
        alt = Some(text.as_str().into());
        at += 1;
    }
    let mut text_box = None;
    if matches!(toks.get(at), Some(Tok::Word(w)) if w == "box") {
        text_box = Some(quoted(toks, at + 1, "box")?);
        at += 2;
    }
    end_at(toks, at, "an object marker")?;
    Ok(Key::Object(ObjectKey {
        kind,
        alt,
        text_box,
    }))
}
