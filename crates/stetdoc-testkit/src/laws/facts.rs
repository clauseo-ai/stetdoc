//! What a document's view says, which is all the random generator of `laws::generate` knows: the `json/1` views of the body,
//! the notes and the headers (refs, Final text, list labels, read-only flags, tables and rows, revision ids, authors, the parts
//! of the stories) and the appendix of the body view (comment ids and authors, note ids).

use serde_json::Value;
use stetdoc_core::{Budget, Document, FormatId, Mode, Scope, StoryName, ViewSpec};

/// A paragraph as the view shows it.
#[derive(Clone, Debug)]
pub struct Para {
    /// Its ref, `p12`.
    pub r: String,
    /// Its Final text, one `char` per offset.
    pub text: Vec<char>,
    /// Whether the view marks it read-only.
    pub readonly: bool,
    /// Whether it has a list label.
    pub list: bool,
    /// The story it is in: `body`, `notes` or `headers`.
    pub story: &'static str,
    /// Whether it is in a table cell.
    pub in_table: bool,
    /// Whether the view says it joins the next paragraph (its mark is deleted, pending).
    pub joins_next: bool,
    /// The top-level block of its story that holds it: its own ref, or the ref of its table.
    pub top: String,
    /// The part that holds its story, such as `word/document.xml` or `word/footer1.xml`.
    pub part: String,
}

/// A table as the view shows it.
#[derive(Clone, Debug)]
pub struct Table {
    /// Its ref, `t3`.
    pub r: String,
    /// Each row's ref and number of cells.
    pub rows: Vec<(String, usize)>,
    /// The top-level block that holds it: its own ref, or the ref of the table it is nested in.
    pub top: String,
}

/// What the view says about a document, which is all the generator knows.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// The paragraphs, in document order, body first.
    pub paras: Vec<Para>,
    /// The tables of the body.
    pub tables: Vec<Table>,
    /// The comments, as `c3`, with the name of the author when the view gives it.
    pub comments: Vec<(String, Option<String>)>,
    /// The notes, as `fn1` or `en2`.
    pub notes: Vec<String>,
    /// The revision ids the view shows after `#`.
    pub revisions: Vec<i64>,
    /// The authors, as code and name.
    pub authors: Vec<(String, String)>,
    /// The style names the view shows on paragraphs.
    pub styles: Vec<String>,
    /// The parts that hold the stories the view shows, such as `word/document.xml`.
    pub parts: Vec<String>,
    /// The part of the body.
    pub body_part: Option<String>,
    /// The refs of the top-level blocks of the body (paragraphs and tables), in order.
    pub top: Vec<String>,
    /// The part of the story being read.
    part: String,
}

fn view_json(doc: &Document, story: StoryName, mode: Mode) -> Result<Vec<Value>, String> {
    let mut spec = ViewSpec {
        scope: Scope::Story(story),
        mode,
        budget: Budget(u32::MAX),
        format: FormatId::new(FormatId::JSON),
        ..ViewSpec::default()
    };
    let mut pages = Vec::new();
    for _ in 0..10_000 {
        let page = doc.view(&spec).map_err(|e| format!("view: {e}"))?;
        pages.push(serde_json::from_str(&page.text).map_err(|e| format!("json/1: {e}"))?);
        match page.next {
            Some(next) => spec.after = Some(next),
            None => return Ok(pages),
        }
    }
    Err("a view did not end".to_string())
}

impl Facts {
    /// The facts of a document, from its Redline views.
    pub fn of(doc: &Document) -> Result<Facts, String> {
        Facts::in_mode(doc, Mode::Redline)
    }

    /// The facts of a document, from its views in a mode. In the Final mode a paragraph's `joins_next` says that its mark is
    /// deleted, pending.
    pub fn in_mode(doc: &Document, mode: Mode) -> Result<Facts, String> {
        let mut facts = Facts::default();
        for (story, name) in [
            (StoryName::Body, "body"),
            (StoryName::Notes, "notes"),
            (StoryName::Headers, "headers"),
        ] {
            let pages = match view_json(doc, story, mode) {
                Ok(pages) => pages,
                Err(e) if story == StoryName::Body => return Err(e),
                Err(_) => continue,
            };
            for page in &pages {
                facts.read_page(page, name);
            }
        }
        facts.revisions.sort_unstable();
        facts.revisions.dedup();
        facts.styles.sort();
        facts.styles.dedup();
        facts.comments.sort();
        facts.comments.dedup();
        facts.notes.sort();
        facts.notes.dedup();
        Ok(facts)
    }

    fn read_page(&mut self, page: &Value, story: &'static str) {
        if let Some(authors) = page.pointer("/head/authors").and_then(Value::as_array) {
            for a in authors {
                let code = a.get("code").and_then(Value::as_str).unwrap_or_default();
                let name = a.get("name").and_then(Value::as_str).unwrap_or_default();
                let pair = (code.to_string(), name.to_string());
                if !self.authors.contains(&pair) {
                    self.authors.push(pair);
                }
            }
        }
        for s in page
            .pointer("/body/stories")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(part) = s.get("part").and_then(Value::as_str)
                && !self.parts.iter().any(|p| p == part)
            {
                self.parts.push(part.to_string());
            }
            if story == "body"
                && self.body_part.is_none()
                && let Some(part) = s.get("part").and_then(Value::as_str)
            {
                self.body_part = Some(part.to_string());
            }
            for unit in s
                .get("units")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let part = s.get("part").and_then(Value::as_str).unwrap_or_default();
                self.part = part.to_string();
                self.blocks(unit.get("blocks"), story, None);
            }
        }
        for item in page
            .get("appendix")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(c) = item.get("comment")
                && let Some(id) = c.get("id").and_then(Value::as_i64)
            {
                let author = c
                    .get("author")
                    .and_then(Value::as_u64)
                    .and_then(|i| self.authors.get(i as usize))
                    .map(|(_, n)| n.clone());
                self.comments.push((format!("c{id}"), author));
            }
            if let Some(n) = item.get("note")
                && let Some(ordinal) = n.get("ordinal").and_then(Value::as_u64)
            {
                let kind = n.get("kind").and_then(Value::as_str).unwrap_or_default();
                let prefix = if kind == "endnote" { "en" } else { "fn" };
                self.notes.push(format!("{prefix}{ordinal}"));
            }
        }
        revision_ids(page, &mut self.revisions);
    }

    /// The blocks of a unit (`top` is `None`) or of a cell of the top-level table `top`.
    fn blocks(&mut self, blocks: Option<&Value>, story: &'static str, top: Option<&str>) {
        for b in blocks.and_then(Value::as_array).into_iter().flatten() {
            if let Some(p) = b.get("paragraph") {
                let Some(r) = p.get("ref").and_then(Value::as_str) else {
                    continue;
                };
                if top.is_none() && story == "body" {
                    self.top.push(r.to_string());
                }
                if let Some(style) = p.get("style").and_then(Value::as_str) {
                    self.styles.push(style.to_string());
                }
                self.paras.push(Para {
                    r: r.to_string(),
                    text: p
                        .get("final_text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .chars()
                        .collect(),
                    readonly: p.pointer("/flags/readonly").is_some_and(|v| !v.is_null()),
                    list: p.get("label").is_some_and(|v| !v.is_null()),
                    story,
                    part: self.part.clone(),
                    in_table: top.is_some(),
                    top: top.unwrap_or(r).to_string(),
                    joins_next: p
                        .pointer("/flags/joins_next")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                });
            }
            if let Some(t) = b.get("table") {
                let mut table = Table {
                    r: t.get("ref")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    rows: Vec::new(),
                    top: String::new(),
                };
                let outer = top.unwrap_or(&table.r).to_string();
                table.top = outer.clone();
                if top.is_none() && story == "body" {
                    self.top.push(table.r.clone());
                }
                for row in t
                    .get("rows")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let cells = row.get("cells").and_then(Value::as_array);
                    let r = row.get("ref").and_then(Value::as_str).unwrap_or_default();
                    table
                        .rows
                        .push((r.to_string(), cells.map_or(0, |c| c.len())));
                    for cell in cells.into_iter().flatten() {
                        self.blocks(cell.get("blocks"), story, Some(&outer));
                    }
                }
                if story == "body" && !table.r.is_empty() {
                    self.tables.push(table);
                }
            }
        }
    }
}

/// Every `tag.id` of a revision anywhere in the view.
fn revision_ids(value: &Value, out: &mut Vec<i64>) {
    match value {
        Value::Object(map) => {
            if let Some(id) = map
                .get("tag")
                .and_then(|t| t.get("id"))
                .and_then(Value::as_i64)
            {
                out.push(id);
            }
            map.values().for_each(|v| revision_ids(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| revision_ids(v, out)),
        _ => {}
    }
}
