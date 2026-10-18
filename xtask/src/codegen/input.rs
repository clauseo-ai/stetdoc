//! The pinned codegen input: Open XML SDK schema data, vendored verbatim under
//! `xtask/codegen/input/` and described by `xtask/codegen/pin.json`.
//!
//! Every file the generator reads must be listed in the pin with its SHA-256. A mismatch,
//! a missing file or an unpinned read is an error, so the generator can never silently
//! run on different data than the one the pin names.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub struct Input {
    pub pin: Value,
    files: BTreeMap<String, String>,
    /// SHA-256 over the sorted `path:sha256` list. Recorded in generated headers.
    pub digest: String,
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Input {
    pub fn load(codegen_dir: &Path) -> Result<Input, String> {
        let pin_path = codegen_dir.join("pin.json");
        let pin_text =
            fs::read_to_string(&pin_path).map_err(|e| format!("{}: {e}", pin_path.display()))?;
        let pin: Value = serde_json::from_str(&pin_text).map_err(|e| format!("pin.json: {e}"))?;
        let root = codegen_dir.join("input");
        let mut files = BTreeMap::new();
        let listed = pin
            .get("files")
            .and_then(Value::as_object)
            .ok_or("pin.json has no `files` object")?;
        let mut manifest = String::new();
        for (path, entry) in listed {
            let want = entry
                .get("sha256")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("pin.json: {path} has no sha256"))?;
            let bytes = fs::read(root.join(path)).map_err(|e| format!("input/{path}: {e}"))?;
            let got = hex(&Sha256::digest(&bytes));
            if got != want {
                return Err(format!(
                    "input/{path} does not match the pin (expected {want}, found {got}). \
                     Restore the file or update pin.json deliberately."
                ));
            }
            manifest.push_str(&format!("{path}:{got}\n"));
            files.insert(
                path.clone(),
                String::from_utf8(bytes).map_err(|e| format!("input/{path}: {e}"))?,
            );
        }
        let digest = hex(&Sha256::digest(manifest.as_bytes()));
        Ok(Input { pin, files, digest })
    }

    pub fn text(&self, path: &str) -> Result<&str, String> {
        self.files
            .get(path)
            .map(String::as_str)
            .ok_or_else(|| format!("input/{path} is not listed in pin.json"))
    }

    pub fn json(&self, path: &str) -> Result<Value, String> {
        serde_json::from_str(self.text(path)?).map_err(|e| format!("input/{path}: {e}"))
    }

    pub fn commit(&self) -> &str {
        self.pin
            .get("commit")
            .and_then(Value::as_str)
            .unwrap_or("?")
    }
}
