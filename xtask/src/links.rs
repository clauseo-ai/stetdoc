//! The docs link check.
//!
//! Every relative link and anchor in every tracked Markdown file must resolve. Links
//! are found with a Markdown parser, so code blocks and code spans are ignored.
//! Anchors follow GitHub's rules for heading slugs. Links with a scheme (`https:`,
//! `mailto:`) are not checked, because the check never touches the network.

use crate::repo::{Repo, Res};
use crate::violation::{Violation, line_of_offset};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::{BTreeMap, BTreeSet};

/// Runs the check against the repository.
pub fn check(repo: &Repo) -> Res<Vec<Violation>> {
    let files: Vec<String> = repo
        .files()?
        .into_iter()
        .filter(|f| f.ends_with(".md") && !repo.is_symlink(f))
        .collect();
    let mut docs: BTreeMap<String, Doc> = BTreeMap::new();
    for file in &files {
        let text = repo.read(file)?;
        docs.insert(file.clone(), Doc::parse(&text));
    }
    let mut problems = Vec::new();
    for (file, doc) in &docs {
        for link in &doc.links {
            if let Some(problem) = resolve(repo, &docs, file, link) {
                problems.push(problem);
            }
        }
    }
    Ok(problems)
}

struct Link {
    dest: String,
    line: usize,
}

struct Doc {
    links: Vec<Link>,
    anchors: BTreeSet<String>,
}

impl Doc {
    fn parse(text: &str) -> Doc {
        let mut links = Vec::new();
        let mut anchors = BTreeSet::new();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut heading: Option<String> = None;
        for (event, range) in Parser::new_ext(text, Options::empty()).into_offset_iter() {
            match event {
                Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                    links.push(Link {
                        dest: dest_url.to_string(),
                        line: line_of_offset(text, range.start),
                    });
                }
                Event::Start(Tag::Heading { .. }) => heading = Some(String::new()),
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(raw) = heading.take() {
                        let base = slug(&raw);
                        let n = counts.entry(base.clone()).or_insert(0);
                        anchors.insert(if *n == 0 { base } else { format!("{base}-{n}") });
                        *n += 1;
                    }
                }
                Event::Text(t) | Event::Code(t) => {
                    if let Some(h) = heading.as_mut() {
                        h.push_str(&t);
                    }
                }
                _ => {}
            }
        }
        Doc { links, anchors }
    }
}

/// GitHub's heading slug: lowercase, punctuation dropped, spaces to hyphens.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn has_scheme(dest: &str) -> bool {
    if dest.starts_with("//") {
        return true;
    }
    match dest.split_once(':') {
        Some((scheme, _)) => {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        }
        None => false,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Joins `rel` onto the directory of `from`, resolving `.` and `..`. `None` when the
/// result leaves the repository.
fn join(from: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = match from.rsplit_once('/') {
        Some((dir, _)) => dir.split('/').collect(),
        None => Vec::new(),
    };
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

fn resolve(
    repo: &Repo,
    docs: &BTreeMap<String, Doc>,
    file: &str,
    link: &Link,
) -> Option<Violation> {
    let dest = link.dest.trim();
    if dest.is_empty() || has_scheme(dest) {
        return None;
    }
    let (path_part, fragment) = match dest.split_once('#') {
        Some((p, f)) => (p, Some(percent_decode(f))),
        None => (dest, None),
    };
    let path_part = percent_decode(path_part.split('?').next().unwrap_or(""));
    let target = if path_part.is_empty() {
        file.to_string()
    } else if let Some(target) = join(file, &path_part) {
        target
    } else {
        return Some(Violation::new(
            file,
            link.line,
            format!("the link `{dest}` points outside the repository"),
            "link to a file inside the repository, or use a full URL",
        ));
    };
    let target = target.trim_end_matches('/').to_string();
    if !target.is_empty() && !repo.exists(&target) {
        return Some(Violation::new(
            file,
            link.line,
            format!("the link `{dest}` points to `{target}`, which does not exist"),
            "fix the path (it is relative to this file), or restore the target",
        ));
    }
    let fragment = fragment.filter(|f| !f.is_empty())?;
    if !target.ends_with(".md") {
        return None;
    }
    let anchors = &docs.get(&target)?.anchors;
    if anchors.contains(&fragment.to_lowercase()) {
        return None;
    }
    Some(Violation::new(
        file,
        link.line,
        format!("the link `{dest}` has no anchor `#{fragment}` in `{target}`"),
        "check the heading text in the target file: anchors are the heading in lowercase, punctuation dropped, spaces as hyphens",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::testutil::repo;

    fn run(files: &[(&str, &str)]) -> Vec<Violation> {
        let (_dir, repo) = repo(files);
        check(&repo).expect("runs")
    }

    #[test]
    fn good_links_and_anchors_pass() {
        let a = "# A\n\nsee [b](docs/b.md), [its section](docs/b.md#the-big-idea-1), [here](#a),\n[dir](docs), [web](https://example.com/x#y), [mail](mailto:a@b.c)\n";
        let b = "# B\n\n## The big idea\n\n## The big idea\n\nback to [a](../a.md#a)\n";
        assert_eq!(run(&[("a.md", a), ("docs/b.md", b)]), []);
    }

    #[test]
    fn a_broken_file_link_names_the_file_and_line() {
        let found = run(&[("a.md", "# A\n\ntext\n[x](nope.md)\n")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "a.md");
        assert_eq!(found[0].line, 4);
        assert!(found[0].message.contains("nope.md"));
        assert!(found[0].fix.contains("relative to this file"));
    }

    #[test]
    fn a_broken_anchor_fails() {
        let found = run(&[
            ("a.md", "# A\n\n[x](b.md#missing) [y](#nothing)\n"),
            ("b.md", "# B\n"),
        ]);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].message.contains("#missing"));
    }

    #[test]
    fn heading_slugs_follow_github() {
        let b = "# Bird's-eye view\n\n## `cargo xtask check`: what runs?\n";
        let a = "[1](b.md#birds-eye-view) [2](b.md#cargo-xtask-check-what-runs)\n";
        assert_eq!(run(&[("a.md", a), ("b.md", b)]), []);
    }

    #[test]
    fn links_in_code_are_ignored() {
        let a = "# A\n\n```\n[x](nope.md)\n```\n\ninline `[y](nope.md)` text\n";
        assert_eq!(run(&[("a.md", a)]), []);
    }

    #[test]
    fn a_link_that_leaves_the_repository_fails() {
        let found = run(&[("a.md", "[x](../../outside.md)\n")]);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("outside the repository"));
    }

    #[test]
    fn reference_style_links_and_images_are_checked() {
        let found = run(&[("a.md", "[x][ref] ![img](pic.png)\n\n[ref]: gone.md\n")]);
        assert_eq!(found.len(), 2, "{found:?}");
    }

    #[test]
    fn percent_encoded_paths_resolve() {
        assert_eq!(
            run(&[("a.md", "[x](my%20file.md)\n"), ("my file.md", "# T\n")]),
            []
        );
    }
}
