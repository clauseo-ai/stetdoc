//! What the `stetdoc init` tests share: a made-up home folder, `PATH` and project.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A home folder, an empty `PATH` directory and a project, all temporary.
pub struct World {
    _tmp: tempfile::TempDir,
    pub home: PathBuf,
    pub bin: PathBuf,
    pub project: PathBuf,
}

impl World {
    pub fn new() -> World {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin = tmp.path().join("bin");
        let project = tmp.path().join("project");
        for dir in [&home, &bin, &project] {
            fs::create_dir(dir).unwrap();
        }
        World {
            _tmp: tmp,
            home,
            bin,
            project,
        }
    }

    pub fn init(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_stetdoc"))
            .arg("init")
            .args(args)
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("PATH", &self.bin)
            .env_remove("USERPROFILE")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("APPDATA")
            .env_remove("STETDOC_AUTHOR")
            .output()
            .expect("run stetdoc")
    }

    pub fn ok(&self, args: &[&str]) -> Output {
        let out = self.init(args);
        assert!(out.status.success(), "{}", stderr(&out));
        out
    }

    /// Puts an executable called `name` on `PATH`.
    pub fn command(&self, name: &str) {
        let path = self.bin.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

pub fn stub() -> String {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("skill/stub.md")).unwrap();
    source.replace("{{version}}", env!("CARGO_PKG_VERSION"))
}

pub fn skill(base: &Path, folder: &str) -> PathBuf {
    base.join(folder).join("skills/stetdoc/SKILL.md")
}
