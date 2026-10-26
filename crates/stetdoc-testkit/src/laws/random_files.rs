//! Random edits of the readable files, for the edit checks beside the random batches of `laws::random`: lines of `document.md`
//! changed, added and deleted at random, each of which must show in the Final view as written, through the laws of
//! `laws::files` (H13, H14, output validity, T6 and T8's reject-all check). The seed is the run's (`laws::random::seed`).

use super::files::{final_lines, one, plain, replaced};
use super::generate::{Rng, fnv};
use super::random::{file_edit_count, seed};
use stetdoc_core::{Document, OpenOptions, Receipt};

fn open(bytes: &[u8]) -> Result<Document, String> {
    Document::open(bytes.to_vec(), &OpenOptions::default()).map_err(|e| format!("open: {e}"))
}

/// Runs [`file_edit_count`] random edits of files on a document, with the run's seed.
pub fn file_edits(bytes: &[u8]) -> Result<(), String> {
    file_edits_with(bytes, seed(), file_edit_count())
}

/// Runs `n` random edits of `document.md` on a document from `seed`: a word changed, a line added, a line deleted, or two of
/// those on different lines. Each must show in the Final view as written, through the laws of `laws::files`.
pub fn file_edits_with(bytes: &[u8], seed: u64, n: usize) -> Result<(), String> {
    let mut rng = Rng::new(seed ^ fnv(bytes) ^ 0x5eed);
    let doc = open(bytes)?;
    let files = doc.files().map_err(|e| format!("files: {e}"))?;
    let body = files
        .files
        .iter()
        .find(|(n, _)| n == "document.md")
        .map(|(_, t)| t.clone())
        .unwrap_or_default();
    let shown = final_lines(&doc)?;
    let candidates: Vec<(String, String)> = body
        .lines()
        .filter_map(plain)
        .filter(|(r, text)| shown.iter().any(|l| l == &format!("{r} {text}")))
        .map(|(r, t)| (r.to_string(), t.to_string()))
        .collect();
    if candidates.is_empty() {
        return Ok(());
    }
    for k in 0..n {
        let mut edited = files.clone();
        let mut want = shown.clone();
        let mut added_at = None;
        let mut what = Vec::new();
        let mut used = Vec::new();
        for _ in 0..1 + usize::from(rng.chance(30)) {
            let Some((r, text)) = rng.pick(&candidates).cloned() else {
                break;
            };
            if used.contains(&r) {
                continue;
            }
            used.push(r.clone());
            let line = format!("{r} {text}");
            let Some(at) = want.iter().position(|l| *l == line) else {
                continue;
            };
            match rng.below(4) {
                0 | 1 => {
                    let words: Vec<&str> = text.split(' ').collect();
                    let i = rng.below(words.len());
                    let mut words: Vec<String> = words.iter().map(|w| (*w).to_string()).collect();
                    if let Some(w) = words.get_mut(i) {
                        *w = format!("stetdoc{k}");
                    }
                    let new_line = format!("{r} {}", words.join(" "));
                    edited = replaced(&edited, &line, &new_line);
                    what.push(format!("{r}: word {i} changed"));
                    if let Some(slot) = want.get_mut(at) {
                        *slot = new_line;
                    }
                }
                2 if added_at.is_none() => {
                    edited = replaced(
                        &edited,
                        &line,
                        &format!("{line}\nA paragraph that stetdoc added."),
                    );
                    what.push(format!("{r}: a line added after it"));
                    added_at = Some(at + 1);
                    want.insert(at + 1, "+ A paragraph that stetdoc added.".to_string());
                }
                _ => {
                    edited = replaced(&edited, &line, "");
                    what.push(format!("{r}: deleted"));
                    if let Some(slot) = want.get_mut(at) {
                        *slot = format!("?{r}");
                    }
                }
            }
        }
        if what.is_empty() {
            continue;
        }
        let expected = |receipt: &Receipt| -> Vec<String> {
            let new_ref = receipt
                .operations
                .iter()
                .flat_map(|o| o.added.iter())
                .next()
                .cloned()
                .unwrap_or_default();
            want.iter()
                .map(|l| match l.strip_prefix("+ ") {
                    Some(text) => format!("{new_ref} {text}"),
                    None => l.clone(),
                })
                .collect()
        };
        let what = format!("seed {seed}, file edit {k}: {}", what.join("; "));
        one(bytes, &what, edited, &expected)?;
    }
    Ok(())
}
