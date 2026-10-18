//! The user's config file, `stetdoc/config.toml` in the config folder: for now only `author = "..."`, the name the user
//! confirmed with `stetdoc init`. It is a source of the author after `--author` and `STETDOC_AUTHOR`, never a guess.
//!
//! The folder is `XDG_CONFIG_HOME` when set, then `%APPDATA%` on Windows, then `~/.config`. The file is read and written
//! by hand: the one key is a TOML basic string, and every other line of the file is kept as it is.

use crate::fsio;
use crate::output::Failure;
use std::path::PathBuf;

/// The largest config file read.
const MAX_CONFIG: u64 = 64 << 10;

/// The config file's path, or `None` when no folder for it is known.
pub fn path() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = var("XDG_CONFIG_HOME")
        .or_else(|| if cfg!(windows) { var("APPDATA") } else { None })
        .or_else(|| var("HOME").map(|home| home.join(".config")))?;
    Some(dir.join("stetdoc").join("config.toml"))
}

/// The saved author, when the file says one. A file that cannot be read gives none.
pub fn author() -> Option<String> {
    let text = fsio::read_text_if_any(&path()?, MAX_CONFIG).ok()??;
    text.lines().find_map(author_line)
}

/// Saves the author, replacing an earlier `author` line and keeping every other line.
pub fn save_author(name: &str) -> Result<PathBuf, Failure> {
    let path = path().ok_or_else(|| {
        Failure::usage(
            "There is no folder for the config file: neither XDG_CONFIG_HOME nor HOME is set.",
            "Set HOME, or set STETDOC_AUTHOR instead.",
        )
    })?;
    let current = fsio::read_text_if_any(&path, MAX_CONFIG)?.unwrap_or_default();
    let line = format!("author = {}", quote(name));
    let mut lines: Vec<String> = Vec::new();
    let mut replaced = false;
    for existing in current.lines() {
        if author_line(existing).is_some() && !replaced {
            lines.push(line.clone());
            replaced = true;
        } else if author_line(existing).is_none() {
            lines.push(existing.to_string());
        }
    }
    if !replaced {
        lines.push(line);
    }
    let text = lines.join("\n") + "\n";
    fsio::ensure_dir(&fsio::parent(&path))?;
    fsio::write_atomic(&path, text.as_bytes())?;
    Ok(path)
}

/// The value of an `author = "..."` line.
fn author_line(line: &str) -> Option<String> {
    let rest = line
        .trim()
        .strip_prefix("author")?
        .trim_start()
        .strip_prefix('=')?;
    let body = rest.trim().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out).filter(|name| !name.trim().is_empty()),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// A TOML basic string.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A suggestion for the name `stetdoc init` asks for: `git config user.name`, then the account's full name. Only ever shown
/// for the user to confirm or change, never used on its own.
pub fn suggested_author() -> Option<String> {
    let run = |program: &str, args: &[&str]| {
        let out = std::process::Command::new(program)
            .args(args)
            .output()
            .ok()?;
        let text = String::from_utf8(out.stdout).ok()?;
        Some(text.trim().to_string()).filter(|t| out.status.success() && !t.is_empty())
    };
    if let Some(name) = run("git", &["config", "--get", "user.name"]) {
        return Some(name);
    }
    if cfg!(target_os = "macos") {
        return run("id", &["-F"]);
    }
    if cfg!(unix) {
        let user = std::env::var("USER").ok()?;
        let entry = run("getent", &["passwd", &user])?;
        let gecos = entry
            .split(':')
            .nth(4)?
            .split(',')
            .next()?
            .trim()
            .to_string();
        return Some(gecos).filter(|g| !g.is_empty());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{author_line, quote};

    #[test]
    fn an_author_line_round_trips() {
        for name in ["Clauseo", "Anne \"Annie\" O'Neil", "back\\slash"] {
            assert_eq!(
                author_line(&format!("author = {}", quote(name))).as_deref(),
                Some(name)
            );
        }
        assert_eq!(author_line("author = \"\"").as_deref(), None);
        assert_eq!(author_line("authored = \"x\"").as_deref(), None);
        assert_eq!(author_line("# author = \"x\"").as_deref(), None);
    }
}
