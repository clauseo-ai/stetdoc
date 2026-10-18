//! A seeded generator of paragraphs for the property tests of `docs/design/wordml.md` 10.2.
//!
//! The generator draws nested wrappers (hyperlink, smart tag, custom XML, content control, direction,
//! override, the four revision wrappers), runs with text (astral characters, references, CDATA,
//! whitespace at the edges, line endings that normalize), atoms, marks, complex and simple fields
//! (nested, spanning runs, wrappers and paragraphs, with and without `separate`) and
//! `mc:AlternateContent`. It writes `w:delText` and `w:delInstrText` inside deletions.
//!
//! It also keeps the text the paragraph must have in each mode, worked out from what it wrote and
//! not from the text model: a second, independent statement of the reading rules of sections 4.2
//! to 4.4. A mismatch is a bug in one of the two. Words that must never reach a text (field code,
//! the content of an untaken branch, of a text box, of an unknown element) are the upper-case
//! tokens in `HIDDEN`.

use super::tests_gen_data::{CDATA_PIECES, INSTRS, PIECES, Rng, SHOWN};
use super::*;

#[derive(Clone, Copy)]
struct GField {
    code: bool,
    sep: bool,
    simple: bool,
    shown: bool,
}

/// The state of a generation: what is open, and the three texts written so far.
pub(super) struct Gen {
    pub(super) rng: Rng,
    ins: u32,
    del: u32,
    fields: Vec<GField>,
    width: u32,
    exp: [String; 3],
    id: u32,
    left: usize,
}

/// One generated paragraph and its expected texts, `[Final, Original, Redline]`.
pub(super) struct GenPara {
    pub(super) xml: String,
    pub(super) expect: [String; 3],
}

/// A generated body: the paragraphs in order, and the texts they must have.
pub(super) struct GenBody {
    pub(super) xml: String,
    pub(super) expect: Vec<[String; 3]>,
}

pub(super) fn mode_index(mode: Mode) -> usize {
    match mode {
        Mode::Final => 0,
        Mode::Original => 1,
        Mode::Redline => 2,
    }
}

impl Gen {
    pub(super) fn new(seed: u64) -> Gen {
        Gen {
            rng: Rng::new(seed),
            ins: 0,
            del: 0,
            fields: Vec::new(),
            width: 0,
            exp: Default::default(),
            id: 0,
            left: 0,
        }
    }

    fn id(&mut self) -> u32 {
        self.id += 1;
        self.id
    }

    fn in_code(&self) -> bool {
        self.fields.iter().any(|f| f.code)
    }

    /// Records characters an item contributes, in the modes that hold it. Nothing in field code.
    fn out(&mut self, plain: &str) {
        if self.in_code() || plain.is_empty() {
            return;
        }
        self.width += 1;
        if self.del == 0 {
            self.exp[0].push_str(plain);
        }
        if self.ins == 0 {
            self.exp[1].push_str(plain);
        }
        self.exp[2].push_str(plain);
    }

    fn text_elem(&mut self) -> String {
        let tag = if self.del > 0 { "delText" } else { "t" };
        let attr = if self.rng.chance(70) {
            " xml:space=\"preserve\""
        } else {
            ""
        };
        if self.rng.chance(5) {
            return format!("<w:{tag}{attr}/>");
        }
        let mut body = String::new();
        for _ in 0..1 + self.rng.below(3) {
            let (xml, plain) = if self.rng.chance(20) {
                *self.rng.pick(&CDATA_PIECES)
            } else {
                *self.rng.pick(&PIECES)
            };
            body.push_str(xml);
            self.out(plain);
        }
        format!("<w:{tag}{attr}>{body}</w:{tag}>")
    }

    fn atom_elem(&mut self) -> String {
        let (xml, plain) = match self.rng.below(15) {
            0 => ("<w:tab/>".to_string(), "\t"),
            1 => ("<w:br w:type=\"textWrapping\"/>".to_string(), "\n"),
            2 => ("<w:br w:type=\"page\"/>".to_string(), "\u{c}"),
            3 => ("<w:br w:type=\"column\"/>".to_string(), "\u{e}"),
            4 => ("<w:noBreakHyphen/>".to_string(), "\u{2011}"),
            5 => ("<w:softHyphen/>".to_string(), "\u{ad}"),
            6 => (
                "<w:sym w:font=\"Wingdings\" w:char=\"F0FC\"/>".to_string(),
                "\u{f0fc}",
            ),
            7 => (
                "<w:drawing><wp:inline xmlns:wp=\"urn:wp\"><w:txbxContent><w:p><w:r><w:t>HIDDEN</w:t></w:r></w:p></w:txbxContent></wp:inline></w:drawing>".to_string(),
                "\u{1}",
            ),
            8 => {
                let id = self.id();
                (format!("<w:footnoteReference w:id=\"{id}\"/>"), "\u{2}")
            }
            9 => ("<w:pgNum/>".to_string(), "\u{5}"),
            10 => ("<w:cr/>".to_string(), "\n"),
            11 => ("<w:ptab w:alignment=\"left\"/>".to_string(), "\t"),
            12 => ("<w:lastRenderedPageBreak/>".to_string(), ""),
            13 => {
                let id = self.id();
                (format!("<w:commentReference w:id=\"{id}\"/>"), "")
            }
            _ => ("<w:sym w:char=\"zz\"/>".to_string(), "\u{fffd}"),
        };
        self.out(plain);
        xml
    }

    fn run(&mut self) -> String {
        let mut s = String::from("<w:r>");
        if self.rng.chance(30) {
            s.push_str("<w:rPr><w:b/></w:rPr>");
        }
        let plain = self.rng.chance(35);
        for _ in 0..if plain { 1 } else { 1 + self.rng.below(3) } {
            if plain {
                s.push_str(&self.text_elem());
                continue;
            }
            let piece = match self.rng.below(12) {
                0..=6 => self.text_elem(),
                7..=9 => self.atom_elem(),
                10 => self.alternate_in_run(),
                _ => self.ruby(),
            };
            s.push_str(&piece);
        }
        s.push_str("</w:r>");
        s
    }

    /// An `mc:AlternateContent` inside a run: one object, whichever branch Word takes, or none.
    fn alternate_in_run(&mut self) -> String {
        match self.rng.below(3) {
            0 => {
                self.out("\u{1}");
                "<mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing/></mc:Choice><mc:Fallback><w:pict/></mc:Fallback></mc:AlternateContent>".to_string()
            }
            1 => {
                self.out("\u{1}");
                "<mc:AlternateContent><mc:Choice Requires=\"zz\" xmlns:zz=\"urn:nope\"><w:t>UNTAKEN</w:t></mc:Choice><mc:Fallback><w:drawing/></mc:Fallback></mc:AlternateContent>".to_string()
            }
            _ => "<mc:AlternateContent><mc:Choice Requires=\"zz\" xmlns:zz=\"urn:nope\"><w:drawing/></mc:Choice></mc:AlternateContent>".to_string(),
        }
    }

    /// Ruby: the base is read, the annotation is not.
    fn ruby(&mut self) -> String {
        let annotation = "<w:rt><w:r><w:t>HIDDEN</w:t></w:r></w:rt>";
        let base = format!("<w:r>{}</w:r>", self.text_elem());
        format!("<w:ruby>{annotation}<w:rubyBase>{base}</w:rubyBase></w:ruby>")
    }

    fn mark(&mut self) -> String {
        let id = self.id();
        match self.rng.below(10) {
            0 => format!("<w:bookmarkStart w:id=\"{id}\" w:name=\"b{id}\"/>"),
            1 => format!(
                "<w:bookmarkEnd w:id=\"{}\"/>",
                self.rng.below(id as usize + 1)
            ),
            2 => format!("<w:commentRangeStart w:id=\"{id}\"/>"),
            3 => format!("<w:commentRangeEnd w:id=\"{id}\"/>"),
            4 => "<w:proofErr w:type=\"spellStart\"/>".to_string(),
            5 => format!("<w:permStart w:id=\"{id}\" w:edGrp=\"everyone\"/>"),
            6 => format!("<w:permEnd w:id=\"{id}\"/>"),
            7 => format!("<w:moveFromRangeStart w:id=\"{id}\" w:name=\"m{id}\"/>"),
            8 => format!("<w:moveToRangeEnd w:id=\"{id}\"/>"),
            _ => "<w:unknownThing><w:r><w:t>SKIPPED</w:t></w:r></w:unknownThing>".to_string(),
        }
    }

    fn wrapper(&mut self, depth: u32) -> String {
        let id = self.id();
        let rev = format!(" w:id=\"{id}\" w:author=\"a\" w:date=\"2020-01-01T00:00:00Z\"");
        let (open, close, ins, del) = match self.rng.below(11) {
            0 => (
                "<w:hyperlink w:anchor=\"a\">".to_string(),
                "</w:hyperlink>",
                0,
                0,
            ),
            1 => (
                "<w:smartTag w:uri=\"u\" w:element=\"e\"><w:smartTagPr/>".to_string(),
                "</w:smartTag>",
                0,
                0,
            ),
            2 => (
                "<w:customXml w:uri=\"x\" w:element=\"y\">".to_string(),
                "</w:customXml>",
                0,
                0,
            ),
            3 => (
                "<w:sdt><w:sdtPr><w:alias w:val=\"a\"/></w:sdtPr><w:sdtContent>".to_string(),
                "</w:sdtContent></w:sdt>",
                0,
                0,
            ),
            4 => ("<w:dir w:val=\"rtl\">".to_string(), "</w:dir>", 0, 0),
            5 => ("<w:bdo w:val=\"rtl\">".to_string(), "</w:bdo>", 0, 0),
            6 | 10 => (format!("<w:ins{rev}>"), "</w:ins>", 1, 0),
            7 => (format!("<w:del{rev}>"), "</w:del>", 0, 1),
            8 => (format!("<w:moveFrom{rev}>"), "</w:moveFrom>", 0, 1),
            _ => (format!("<w:moveTo{rev}>"), "</w:moveTo>", 1, 0),
        };
        self.ins += ins;
        self.del += del;
        let inner = self.content(depth + 1);
        self.ins -= ins;
        self.del -= del;
        format!("{open}{inner}{close}")
    }

    fn simple_field(&mut self, depth: u32) -> String {
        let instr = *self.rng.pick(&INSTRS);
        let shown = shown(instr);
        self.fields.push(GField {
            code: false,
            sep: true,
            simple: true,
            shown,
        });
        let depth_here = self.fields.len();
        let before = self.width;
        let mut inner = if self.rng.chance(70) {
            self.content(depth + 1)
        } else {
            String::new()
        };
        if self.rng.chance(90) {
            // close the complex fields begun inside, so the simple field is the one that closes
            while self.fields.len() > depth_here && !self.fields.last().is_some_and(|f| f.simple) {
                inner.push_str(&self.end_field());
            }
        }
        if self.width == before && shown {
            self.out("\u{6}");
        }
        self.fields.pop();
        let attr = instr.replace('"', "&quot;");
        format!("<w:fldSimple w:instr=\"{attr}\">{inner}</w:fldSimple>")
    }

    /// The runs that close the innermost complex field: a separate sometimes, then the end.
    fn end_field(&mut self) -> String {
        let mut s = String::new();
        if self.fields.last().is_some_and(|f| f.code && !f.simple) && self.rng.chance(50) {
            s.push_str(&self.fld_char("separate"));
        }
        s.push_str(&self.fld_char("end"));
        s
    }

    /// One `fldChar` run, with its effect on the generator's field stack, which mirrors the reading
    /// rules of 4.4 for the cases it writes.
    fn fld_char(&mut self, kind: &str) -> String {
        match kind {
            "begin" => {
                let instr = *self.rng.pick(&INSTRS);
                self.fields.push(GField {
                    code: true,
                    sep: false,
                    simple: false,
                    shown: shown(instr),
                });
                let dirty = if self.rng.chance(10) {
                    " w:dirty=\"true\""
                } else {
                    ""
                };
                let mut s = format!("<w:r><w:fldChar w:fldCharType=\"begin\"{dirty}/></w:r>");
                let tag = if self.del > 0 {
                    "delInstrText"
                } else {
                    "instrText"
                };
                let mut rest = instr;
                while !rest.is_empty() {
                    // the instructions are ASCII, so any byte is a place to cut
                    let cut = if rest.len() > 2 && self.rng.chance(50) {
                        1 + self.rng.below(rest.len() - 1)
                    } else {
                        rest.len()
                    };
                    let (piece, tail) = rest.split_at(cut);
                    s.push_str(&format!(
                        "<w:r><w:{tag} xml:space=\"preserve\">{piece}</w:{tag}></w:r>"
                    ));
                    rest = tail;
                }
                s
            }
            "separate" => {
                if let Some(f) = self.fields.last_mut().filter(|f| f.code) {
                    f.code = false;
                    f.sep = true;
                }
                "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>".to_string()
            }
            _ => {
                if let Some(top) = self.fields.last().copied() {
                    if !top.sep && !top.simple && top.shown {
                        if let Some(f) = self.fields.last_mut() {
                            f.code = false;
                        }
                        self.out("\u{6}");
                    }
                    self.fields.pop();
                }
                "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>".to_string()
            }
        }
    }

    fn field_step(&mut self) -> String {
        let r = self.rng.below(100);
        let deep = self.fields.len() >= 4;
        match self.fields.last().copied() {
            Some(f) if !f.simple && f.code => match r {
                0..=24 if !deep => self.fld_char("begin"),
                25..=59 => self.fld_char("separate"),
                60..=79 => self.fld_char("end"),
                _ => {
                    let tag = if self.del > 0 { "delText" } else { "t" };
                    format!("<w:r><w:{tag}>CODETEXT</w:{tag}></w:r>")
                }
            },
            Some(f) if !f.simple => match r {
                0..=44 => self.fld_char("end"),
                45..=64 if !deep => self.fld_char("begin"),
                _ => self.run(),
            },
            _ if !deep => self.fld_char("begin"),
            _ => self.run(),
        }
    }

    fn content(&mut self, depth: u32) -> String {
        let n = if depth == 0 {
            1 + self.rng.below(5)
        } else {
            self.rng.below(4)
        };
        let mut s = String::new();
        for _ in 0..n {
            s.push_str(&self.child(depth));
        }
        s
    }

    fn child(&mut self, depth: u32) -> String {
        if self.left == 0 {
            return self.run();
        }
        self.left -= 1;
        let r = self.rng.below(100);
        if r < 5 || (r < 20 && !self.fields.is_empty()) {
            return self.field_step();
        }
        match r {
            20..=55 => self.run(),
            56..=72 if depth < 4 => self.wrapper(depth),
            73..=82 => self.mark(),
            83..=88 if depth < 4 => self.simple_field(depth),
            89..=92 => self.alternate(depth),
            93..=95 => {
                self.out("\u{4}");
                "<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>".to_string()
            }
            _ => self.run(),
        }
    }

    /// An `mc:AlternateContent` among the content: the branch Word takes is read, the other is not.
    fn alternate(&mut self, depth: u32) -> String {
        let hidden = "<w:r><w:t>UNTAKEN</w:t></w:r>";
        match self.rng.below(3) {
            0 => {
                let inner = self.content(depth + 1);
                format!(
                    "<mc:AlternateContent><mc:Choice Requires=\"wps\">{inner}</mc:Choice><mc:Fallback>{hidden}</mc:Fallback></mc:AlternateContent>"
                )
            }
            1 => {
                let inner = self.content(depth + 1);
                format!(
                    "<mc:AlternateContent><mc:Choice Requires=\"zz\" xmlns:zz=\"urn:nope\">{hidden}</mc:Choice><mc:Fallback>{inner}</mc:Fallback></mc:AlternateContent>"
                )
            }
            _ => format!(
                "<mc:AlternateContent><mc:Choice Requires=\"zz\" xmlns:zz=\"urn:nope\">{hidden}</mc:Choice></mc:AlternateContent>"
            ),
        }
    }

    /// One paragraph. Fields still open from the paragraphs before it carry in.
    pub(super) fn paragraph(&mut self) -> GenPara {
        self.exp = Default::default();
        self.left = 8 + self.rng.below(30);
        let mut xml = String::from("<w:p>");
        if self.rng.chance(25) {
            xml.push_str("<w:pPr><w:pStyle w:val=\"Normal\"/><w:rPr><w:b/></w:rPr></w:pPr>");
        }
        xml.push_str(&self.content(0));
        if self.rng.chance(85) {
            while self.fields.last().is_some_and(|f| !f.simple) {
                xml.push_str(&self.end_field());
            }
        }
        // a field still in its code phase is closed at the end of its paragraph (4.4)
        self.fields.retain(|f| !f.code);
        xml.push_str("</w:p>");
        GenPara {
            xml,
            expect: std::mem::take(&mut self.exp),
        }
    }

    /// `count` paragraphs, some inside the block-level wrappers and the revised rows and cells that
    /// put a paragraph in a revision it does not name itself (4.2).
    pub(super) fn body(&mut self, count: usize) -> GenBody {
        let mut xml = String::new();
        let mut expect = Vec::new();
        for _ in 0..count {
            let id = self.id();
            let rev = format!(" w:id=\"{id}\" w:author=\"a\"");
            let (open, close, ins, del) = match self.rng.below(16) {
                0 => (
                    format!("<w:moveFrom{rev}>"),
                    "</w:moveFrom>".to_string(),
                    0,
                    1,
                ),
                1 => (format!("<w:ins{rev}>"), "</w:ins>".to_string(), 1, 0),
                2 => (format!("<w:del{rev}>"), "</w:del>".to_string(), 0, 1),
                3 => (format!("<w:moveTo{rev}>"), "</w:moveTo>".to_string(), 1, 0),
                4 => (
                    "<w:sdt><w:sdtPr/><w:sdtContent>".to_string(),
                    "</w:sdtContent></w:sdt>".to_string(),
                    0,
                    0,
                ),
                5 => (
                    format!("<w:tbl><w:tr><w:trPr><w:del{rev}/></w:trPr><w:tc><w:tcPr/>"),
                    "</w:tc></w:tr></w:tbl>".to_string(),
                    0,
                    1,
                ),
                6 => (
                    format!("<w:tbl><w:tr><w:trPr><w:ins{rev}/></w:trPr><w:tc>"),
                    "</w:tc></w:tr></w:tbl>".to_string(),
                    1,
                    0,
                ),
                7 => (
                    format!("<w:tbl><w:tr><w:tc><w:tcPr><w:cellDel{rev}/></w:tcPr>"),
                    "</w:tc></w:tr></w:tbl>".to_string(),
                    0,
                    1,
                ),
                8 => (
                    format!("<w:tbl><w:tr><w:tc><w:tcPr><w:cellIns{rev}/></w:tcPr>"),
                    "</w:tc></w:tr></w:tbl>".to_string(),
                    1,
                    0,
                ),
                // a row or a cell marked as both inserted and deleted is in neither mode
                9 => (
                    format!(
                        "<w:tbl><w:tr><w:trPr><w:del{rev}/><w:ins w:id=\"{}\"/></w:trPr><w:tc>",
                        id + 5000
                    ),
                    "</w:tc></w:tr></w:tbl>".to_string(),
                    1,
                    1,
                ),
                10 => (
                    format!(
                        "<w:tbl><w:tr><w:tc><w:tcPr><w:cellIns{rev}/><w:cellDel w:id=\"{}\"/></w:tcPr>",
                        id + 5000
                    ),
                    "</w:tc></w:tr></w:tbl>".to_string(),
                    1,
                    1,
                ),
                _ => (String::new(), String::new(), 0, 0),
            };
            self.ins += ins;
            self.del += del;
            let p = self.paragraph();
            self.ins -= ins;
            self.del -= del;
            xml.push_str(&format!("{open}{}{close}", p.xml));
            expect.push(p.expect);
        }
        GenBody { xml, expect }
    }
}

/// True if the instruction names a field Word shows with no stored result.
fn shown(instr: &str) -> bool {
    let word = instr
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches('"');
    SHOWN.contains(&word.to_ascii_uppercase().as_str())
}
