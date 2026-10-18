//! The payloads of each of the nineteen operations, for [`super::generate`]: valid ones, edge ones, and a few that must be
//! refused, each marked with the codes it may be refused with.

use super::generate::{Gen, MISSING, PAYLOAD, kept_note_marks, plain, q, text, word};

/// Fills the fields of the operation `name`.
pub fn fill(g: &mut Gen<'_>, name: &str) {
    match name {
        "replace" => replace(g),
        "insert" => insert(g),
        "delete" => delete(g),
        "split" => split(g),
        "merge" => merge(g),
        "format" => format(g),
        "paragraph" => paragraph(g),
        "style" => style(g),
        "list" => list(g),
        "set" => set(g),
        "row" => row(g),
        "comment" => comment(g),
        "reply" | "edit-comment" => {
            comment_id(g);
            {
                let v = q(&text(g.rng));
                g.field("text", v)
            };
        }
        "resolve" => {
            comment_id(g);
            if g.rng.chance(30) {
                g.field("reopen", "true");
            }
        }
        "delete-comment" => comment_id(g),
        "delete-note" => note(g),
        _ => review(g),
    }
}

fn replace(g: &mut Gen<'_>) {
    let p = g.para_ref();
    let ranged = p.as_ref().is_some_and(|p| g.within(p));
    if g.rng.chance(3) {
        g.refuse(PAYLOAD); // `with` is required
        return;
    }
    let with = if g.rng.chance(5) {
        p.as_ref()
            .map(|p| p.text.iter().collect())
            .unwrap_or_default()
    } else {
        text(g.rng)
    };
    g.field("with", q(&with));
    if let Some(p) = p
        && !ranged
        && plain(&with)
    {
        let old: String = p.text.iter().collect();
        g.sticks = Some((p.r, vec![kept_note_marks(&old, &with)]));
    }
}

fn insert(g: &mut Gen<'_>) {
    let facts = g.facts;
    let table = g
        .rng
        .chance(15)
        .then(|| g.rng.pick(&facts.tables))
        .flatten();
    let anchor = match table {
        Some(t) => {
            g.field("ref", q(&t.r));
            Some(t.r.clone())
        }
        None => g.para_ref().map(|p| p.r),
    };
    if g.rng.chance(3) {
        g.field("place", q("middle"));
        g.refuse(PAYLOAD);
    } else {
        let place = if g.rng.chance(50) { "after" } else { "before" };
        g.field("place", q(place));
    }
    let n = 1 + g.rng.below(3);
    let mut texts = Vec::new();
    let mut items = Vec::new();
    let mut styled = false;
    for _ in 0..n {
        let t = text(g.rng);
        if g.rng.chance(15) {
            styled = true;
            let style = g
                .rng
                .pick(&facts.styles)
                .cloned()
                .unwrap_or_else(|| "Normal".to_string());
            items.push(format!(r#"{{"text":{},"style":{}}}"#, q(&t), q(&style)));
        } else {
            items.push(q(&t));
        }
        texts.push(t);
    }
    g.field("paragraphs", format!("[{}]", items.join(",")));
    if let Some(r) = anchor
        && !styled
        && texts.iter().all(|t| plain(t))
    {
        g.sticks = Some((format!("+{r}"), texts));
    }
}

fn delete(g: &mut Gen<'_>) {
    let facts = g.facts;
    if g.rng.chance(10)
        && let Some(t) = g.rng.pick(&facts.tables)
    {
        g.field("ref", q(&t.r));
        return;
    }
    let Some(p) = g.para_ref() else { return };
    if g.rng.chance(25) {
        // Through a later paragraph of the same story, with offsets now and then.
        let at = facts.paras.iter().position(|x| x.r == p.r).unwrap_or(0);
        let to = facts.paras.get(at + 1 + g.rng.below(3));
        if let Some(to) = to {
            g.field("to", q(&to.r));
            if g.rng.chance(50) {
                let a = g.rng.below(p.text.len() + 1);
                let b = g.rng.below(to.text.len() + 1);
                g.field("start", a.to_string());
                g.field("end", b.to_string());
            }
            return;
        }
    }
    g.within(&p);
}

fn split(g: &mut Gen<'_>) {
    let Some(p) = g.para_ref() else {
        g.field("at", "1");
        return;
    };
    let len = p.text.len();
    match g.rng.below(6) {
        0 => g.field("at", "0"),
        1 => g.field("at", len.to_string()),
        2 | 3 => {
            let v = g.rng.below(len + 1).to_string();
            g.field("at", v)
        }
        k => match word(g.rng, &p.text) {
            Some(w) => {
                g.field(if k == 4 { "before" } else { "after" }, q(&w));
                if g.rng.chance(5) {
                    g.field("at", "1");
                    g.refuse(PAYLOAD);
                }
            }
            None => g.field("at", "1"),
        },
    }
}

fn merge(g: &mut Gen<'_>) {
    let facts = g.facts;
    let Some(p) = g.para_ref() else { return };
    if g.rng.chance(30) {
        let at = facts.paras.iter().position(|x| x.r == p.r).unwrap_or(0);
        if let Some(next) = facts.paras.get(at + 1) {
            g.field("second", q(&next.r));
        }
    }
}

fn format(g: &mut Gen<'_>) {
    if let Some(p) = g.para_ref() {
        g.within(&p);
    }
    let mut any = false;
    while !any || g.rng.chance(40) {
        any = true;
        match g.rng.below(11) {
            0 => {
                let v = if g.rng.chance(70) { "true" } else { "false" };
                g.field("bold", v)
            }
            1 => g.field("italic", "true"),
            2 => {
                let u = *g
                    .rng
                    .pick(&["single", "double", "none", "wave"])
                    .unwrap_or(&"single");
                g.field("underline", q(u));
            }
            3 => g.field("strike", "true"),
            4 => {
                let c = *g.rng.pick(&["FF0000", "auto", "00ff00"]).unwrap_or(&"auto");
                g.field("color", q(c));
            }
            5 => {
                let h = *g
                    .rng
                    .pick(&["yellow", "green", "none"])
                    .unwrap_or(&"yellow");
                g.field("highlight", q(h));
            }
            6 => {
                let s = *g
                    .rng
                    .pick(&["12pt", "9.5pt", "24", "1in"])
                    .unwrap_or(&"12pt");
                g.field("size", q(s));
            }
            7 => g.field("font", q("Courier New")),
            8 => g.field("clear", r#"["bold","italic"]"#),
            9 if g.rng.chance(30) => {
                if g.try_field("size", q("12 parsecs")) {
                    g.refuse(PAYLOAD);
                }
            }
            _ if g.rng.chance(30) => {
                if g.try_field("bold", q("yes")) {
                    g.refuse(PAYLOAD);
                }
            }
            _ => g.field("bold", "true"),
        }
    }
}

fn paragraph(g: &mut Gen<'_>) {
    g.para_ref();
    let mut any = false;
    while !any || g.rng.chance(40) {
        any = true;
        match g.rng.below(10) {
            0 => {
                let a = *g
                    .rng
                    .pick(&["left", "center", "right", "justify"])
                    .unwrap_or(&"left");
                g.field("align", q(a));
            }
            1 => g.field("indent_left", q("0.5in")),
            2 => g.field("indent_right", q("1cm")),
            3 => g.field("first_line", q("18pt")),
            4 => g.field("hanging", q("0.25in")),
            5 => g.field("space_before", q("0pt")),
            6 => g.field("space_after", q("6pt")),
            7 => g.field("line_spacing", q("1.5")),
            8 => {
                let k = *g
                    .rng
                    .pick(&["keep_next", "keep_lines", "page_break_before"])
                    .unwrap_or(&"keep_next");
                {
                    let v = if g.rng.chance(80) { "true" } else { "false" };
                    g.field(k, v)
                };
            }
            _ if g.rng.chance(30) => {
                if g.try_field("align", q("sideways")) {
                    g.refuse(PAYLOAD);
                }
            }
            _ => g.field("clear", r#"["align"]"#),
        }
    }
}

fn style(g: &mut Gen<'_>) {
    let facts = g.facts;
    g.para_ref();
    let style = match g.rng.below(10) {
        0 => {
            // A style that does not exist is `invalid_payload`, with the nearest names (design 2.5).
            g.refuse(PAYLOAD);
            "No Such Style 7".to_string()
        }
        1 => "Normal".to_string(),
        2 => "heading 1".to_string(),
        _ => g
            .rng
            .pick(&facts.styles)
            .cloned()
            .unwrap_or_else(|| "Normal".to_string()),
    };
    g.field("style", q(&style));
}

fn list(g: &mut Gen<'_>) {
    let facts = g.facts;
    // Mostly a list item, when there is one.
    let items: Vec<_> = facts.paras.iter().filter(|p| p.list).collect();
    match g.rng.pick(&items) {
        Some(p) if g.rng.chance(80) => g.field("ref", q(&p.r)),
        _ => {
            g.para_ref();
        }
    }
    match g.rng.below(7) {
        0 | 1 => {
            let v = g.rng.below(4).to_string();
            g.field("level", v)
        }
        2 | 3 => {
            let v = (1 + g.rng.below(5)).to_string();
            g.field("restart", v)
        }
        4 | 5 => g.field("remove", "true"),
        _ => {
            g.field("level", "1");
            g.field("remove", "true");
            g.refuse(PAYLOAD);
        }
    }
}

fn set(g: &mut Gen<'_>) {
    let facts = g.facts;
    let table = g.rng.pick(&facts.tables).cloned();
    let props = match (g.rng.below(10), table) {
        (0, Some(t)) => {
            g.field("ref", q(&t.r));
            r#"[{"name":"jc","val":"center"}]"#
        }
        (1, Some(t)) => {
            let row = g.rng.pick(&t.rows).cloned();
            if let Some((r, cells)) = row {
                g.field("ref", q(&r));
                if g.rng.chance(50) && cells > 0 {
                    {
                        let v = (1 + g.rng.below(cells)).to_string();
                        g.field("cell", v)
                    };
                    r#"[{"name":"shd","attrs":{"val":"clear","fill":"FFFF00"}}]"#
                } else {
                    r#"[{"name":"cantSplit"}]"#
                }
            } else {
                g.field("ref", q(&t.r));
                r#"[{"name":"jc","val":"left"}]"#
            }
        }
        (2, _) => {
            if let Some(p) = g.para_ref() {
                g.within(&p);
            }
            g.field("target", q("run"));
            r#"[{"name":"b"},{"name":"color","val":"FF0000"}]"#
        }
        (3, _) => {
            g.para_ref();
            g.field("target", q("mark"));
            r#"[{"name":"i"}]"#
        }
        (4, _) if g.rng.chance(40) => {
            g.para_ref();
            g.refuse(PAYLOAD);
            *g.rng
                .pick(&[
                    r#"[{"name":"ins"}]"#,
                    r#"[{"name":"pPrChange"}]"#,
                    r#"[{"name":"notARealProperty"}]"#,
                ])
                .unwrap_or(&r#"[{"name":"ins"}]"#)
        }
        _ => {
            g.para_ref();
            *g.rng
                .pick(&[
                    r#"[{"name":"jc","val":"center"}]"#,
                    r#"[{"name":"spacing","attrs":{"before":"12pt","after":"0pt"}}]"#,
                    r#"[{"name":"ind","attrs":{"left":"0.5in"}}]"#,
                    r#"[{"name":"keepNext"},{"name":"widowControl","val":"0"}]"#,
                ])
                .unwrap_or(&r#"[{"name":"keepNext"}]"#)
        }
    };
    g.field("props", props);
}

fn row(g: &mut Gen<'_>) {
    let facts = g.facts;
    let table = g.rng.pick(&facts.tables).cloned();
    let row = table.and_then(|t| g.rng.pick(&t.rows).cloned());
    let cells = match row {
        Some((r, cells)) if !g.rng.chance(5) => {
            g.field("ref", q(&r));
            cells
        }
        _ => {
            // A row op on a paragraph is the wrong kind of ref.
            if g.para_ref().is_some() {
                g.refuse(PAYLOAD);
            }
            1
        }
    };
    if g.rng.chance(40) {
        g.field("action", q("delete"));
        return;
    }
    g.field("action", q("insert"));
    if g.rng.chance(50) {
        let place = if g.rng.chance(50) { "after" } else { "before" };
        g.field("place", q(place));
    }
    let n = match g.rng.below(4) {
        0 => cells + 1,
        1 => cells.saturating_sub(1),
        _ => cells,
    };
    let texts: Vec<String> = (0..n).map(|_| q(&text(g.rng))).collect();
    g.field("cells", format!("[{}]", texts.join(",")));
}

fn comment(g: &mut Gen<'_>) {
    if let Some(p) = g.para_ref() {
        g.within(&p);
    }
    if g.rng.chance(4) {
        g.field("text", q(""));
        g.refuse(PAYLOAD);
    } else {
        {
            let v = q(&text(g.rng));
            g.field("text", v)
        };
    }
    if g.rng.chance(20) {
        g.field("initials", q("SD"));
    }
}

fn comment_id(g: &mut Gen<'_>) {
    let facts = g.facts;
    // The author's own comments first, which `edit-comment` and `delete-comment` may change.
    let own: Vec<_> = facts
        .comments
        .iter()
        .filter(|(_, a)| a.as_deref() == Some("Clauseo"))
        .collect();
    let pick = if !own.is_empty() && g.rng.chance(60) {
        g.rng.pick(&own).map(|(c, _)| c.clone())
    } else {
        g.rng.pick(&facts.comments).map(|(c, _)| c.clone())
    };
    match pick {
        Some(c) if !g.rng.chance(5) => g.field("comment", q(&c)),
        _ => {
            g.field("comment", q("c999999"));
            g.refuse(MISSING);
        }
    }
}

fn note(g: &mut Gen<'_>) {
    let facts = g.facts;
    match g.rng.pick(&facts.notes).cloned() {
        Some(n) if !g.rng.chance(10) => g.field("note", q(&n)),
        _ => {
            g.field("note", q("fn99999"));
            g.refuse(MISSING);
        }
    }
}

/// `accept` and `reject`: all, an author by name or code, or ids the view shows.
fn review(g: &mut Gen<'_>) {
    let facts = g.facts;
    match g.rng.below(5) {
        0 => g.field("all", "true"),
        1 | 2 => {
            let author = match g.rng.pick(&facts.authors).cloned() {
                Some((code, name)) => {
                    if g.rng.chance(50) {
                        code
                    } else {
                        name
                    }
                }
                None => "Clauseo".to_string(),
            };
            g.field("author", q(&author));
        }
        3 if !facts.revisions.is_empty() => {
            let n = 1 + g.rng.below(3);
            let ids: Vec<String> = (0..n)
                .filter_map(|_| g.rng.pick(&facts.revisions).map(i64::to_string))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            g.field("ids", format!("[{}]", ids.join(",")));
        }
        _ if g.rng.chance(30) => {
            g.field("ids", "[999999]");
            g.refuse(MISSING);
        }
        _ => g.field("author", q("Clauseo")),
    }
}
