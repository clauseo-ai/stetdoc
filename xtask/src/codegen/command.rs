//! The `cargo xtask codegen` command and its staleness check.

use super::{generate, output_dir};
use crate::repo::{Repo, Res};
use crate::violation::Violation;
use std::path::Path;

/// Where the pinned input and the generator's data live, relative to the repository root.
pub const INPUT_DIR: &str = "xtask/codegen";

/// Where the generated tables are written, relative to the repository root.
pub const OUTPUT_DIR: &str = "crates/stetdoc-core/src/schema/generated";

/// What the generator would write, which of those files differ from what is on disk, and what
/// else is in the directory.
struct Pending {
    files: Vec<(String, String)>,
    stale: Vec<String>,
    strays: Vec<String>,
}

/// The entries of `dir` that the generator does not write. The directory belongs to the
/// generator, so anything else in it is a leftover or a hand edit.
fn strays(dir: &Path, generated: &[(String, String)]) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| !generated.iter().any(|(g, _)| g == name))
        .collect();
    found.sort();
    found
}

fn pending(repo: &Repo) -> Res<Pending> {
    let files = generate(&repo.root().join(INPUT_DIR))?;
    let dir = output_dir(repo.root());
    let stale = files
        .iter()
        .filter(|(name, content)| {
            std::fs::read_to_string(dir.join(name)).ok().as_deref() != Some(content.as_str())
        })
        .map(|(name, _)| name.clone())
        .collect();
    let strays = strays(&dir, &files);
    Ok(Pending {
        files,
        stale,
        strays,
    })
}

/// Writes the generated tables and removes anything else in their directory. Returns a
/// one-line summary.
pub fn run(repo: &Repo) -> Res<String> {
    let found = pending(repo)?;
    for name in &found.stale {
        if let Some((_, content)) = found.files.iter().find(|(n, _)| n == name) {
            repo.write(&format!("{OUTPUT_DIR}/{name}"), content)?;
        }
    }
    let dir = output_dir(repo.root());
    for name in &found.strays {
        let path = dir.join(name);
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        removed.map_err(|e| format!("cannot remove {OUTPUT_DIR}/{name}: {e}"))?;
    }
    let bytes: usize = found.files.iter().map(|(_, c)| c.len()).sum();
    let mut outcome = if found.stale.is_empty() {
        "unchanged".to_string()
    } else {
        format!("wrote {}", found.stale.join(", "))
    };
    if !found.strays.is_empty() {
        outcome.push_str(&format!("; removed {}", found.strays.join(", ")));
    }
    Ok(format!(
        "schema tables: {} files, {bytes} bytes of source, {outcome}",
        found.files.len()
    ))
}

/// Fails for every generated file that differs from what the generator writes now, and for
/// every other file in the directory. A pinned input file that has changed fails generation
/// itself. It writes nothing.
pub fn check(repo: &Repo) -> Res<Vec<Violation>> {
    let found = pending(repo)?;
    let stale = found.stale.into_iter().map(|name| {
        Violation::new(
            format!("{OUTPUT_DIR}/{name}"),
            0,
            "the generated schema tables are stale",
            "run `cargo xtask codegen` and commit the result",
        )
    });
    let strays = found.strays.into_iter().map(|name| {
        Violation::new(
            format!("{OUTPUT_DIR}/{name}"),
            0,
            "this file is not one the generator writes",
            "delete it; `cargo xtask codegen` removes it",
        )
    });
    Ok(stale.chain(strays).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_the_generator_does_not_write_is_a_stray() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["mod.rs", "names.rs", "old_table.rs", "notes.txt"] {
            std::fs::write(dir.path().join(name), "").expect("write");
        }
        std::fs::create_dir(dir.path().join("sub")).expect("mkdir");
        let generated = vec![
            ("mod.rs".to_string(), String::new()),
            ("names.rs".to_string(), String::new()),
        ];
        assert_eq!(
            strays(dir.path(), &generated),
            ["notes.txt", "old_table.rs", "sub"]
        );
        assert!(strays(&dir.path().join("missing"), &generated).is_empty());
    }
}
