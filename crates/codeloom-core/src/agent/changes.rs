//! Resuelve los números (+/-) de un fichero tocado por un `edit`.

use std::path::PathBuf;
use std::process::Command;

/// Resuelve los números de un `edit`. Con git usa `diff --numstat`; si no,
/// cuenta las líneas de `old`/`new` (exacto, porque `edit` reemplaza `old`).
pub struct ChangeTracker {
    cwd: PathBuf,
    git: bool,
}

impl ChangeTracker {
    pub fn detect(cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let git = Command::new("git")
            .arg("-C")
            .arg(&cwd)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        Self { cwd, git }
    }

    #[cfg(test)]
    pub fn without_git(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            git: false,
        }
    }

    /// `(added, removed)` para un `edit` de `old` a `new`.
    pub fn numbers(&self, path: &str, old: &str, new: &str) -> (usize, usize) {
        if self.git {
            if let Some(n) = self.git_numbers(path) {
                return n;
            }
        }
        (new.lines().count(), old.lines().count())
    }

    fn git_numbers(&self, path: &str) -> Option<(usize, usize)> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.cwd)
            .args(["diff", "--numstat", "--"])
            .arg(path)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().next()?;
        let mut it = line.split('\t');
        let added = it.next()?.trim().parse::<usize>().ok()?;
        let removed = it.next()?.trim().parse::<usize>().ok()?;
        Some((added, removed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_counts_lines_of_old_and_new() {
        let t = ChangeTracker::without_git(".");
        assert_eq!(t.numbers("x", "a\nb\nc", "a\nb"), (2, 3));
        assert_eq!(t.numbers("x", "", "nuevo"), (1, 0));
        assert_eq!(t.numbers("x", "viejo", ""), (0, 1));
    }

    #[test]
    fn detect_without_repo_does_not_panic_and_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let t = ChangeTracker::detect(dir.path());
        // tempdir no es repo git -> respaldo.
        assert_eq!(t.numbers("x", "a", "a\nb"), (2, 1));
    }

    #[test]
    fn tracked_file_counts_come_from_git_numstat() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap()
        };

        std::fs::write(dir.path().join("a.txt"), "old\n").unwrap();
        assert!(git(&["init", "--quiet"]).success());
        assert!(git(&["add", "a.txt"]).success());
        assert!(git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--quiet",
            "-m",
            "baseline",
        ])
        .success());
        std::fs::write(dir.path().join("a.txt"), "new\nline\n").unwrap();

        let tracker = ChangeTracker::detect(dir.path());
        assert!(tracker.git);
        assert_eq!(tracker.numbers("a.txt", "old\n", "new\nline\n"), (2, 1));
    }
}
