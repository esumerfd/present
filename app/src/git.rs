use anyhow::Result;
use std::process::Command;

/// Abstracts the git operations `present` performs after an edit or reorder, so app
/// logic can be tested without shelling out to real `git` on every test run.
pub(crate) trait GitCommitter {
    fn is_repo(&self, dir: &str) -> bool;

    /// Stages exactly `paths` (relative to `dir`) and commits them with `message`,
    /// scoped with a pathspec so nothing else in the working tree is picked up.
    /// Returns `Ok(true)` if a commit was made, `Ok(false)` if there was nothing to
    /// commit for those paths (e.g. nvim was opened but nothing was changed).
    fn commit_paths(&self, dir: &str, paths: &[String], message: &str) -> Result<bool>;
}

pub(crate) struct RealGit;

impl GitCommitter for RealGit {
    fn is_repo(&self, dir: &str) -> bool {
        Command::new("git")
            .args(["-C", dir, "rev-parse", "--is-inside-work-tree"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn commit_paths(&self, dir: &str, paths: &[String], message: &str) -> Result<bool> {
        if paths.is_empty() {
            return Ok(false);
        }

        let add_status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .arg("add")
            .arg("--")
            .args(paths)
            .status()?;
        anyhow::ensure!(add_status.success(), "git add failed");

        // Exit 0 means no staged differences for these paths -- nothing to commit.
        let diff_status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .arg("diff")
            .arg("--cached")
            .arg("--quiet")
            .arg("--")
            .args(paths)
            .status()?;
        if diff_status.success() {
            return Ok(false);
        }

        let commit_status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .arg("commit")
            .arg("-q")
            .arg("-m")
            .arg(message)
            .arg("--")
            .args(paths)
            .status()?;
        anyhow::ensure!(commit_status.success(), "git commit failed");
        Ok(true)
    }
}

#[cfg(test)]
pub(crate) struct NoopGit;

#[cfg(test)]
impl GitCommitter for NoopGit {
    fn is_repo(&self, _dir: &str) -> bool {
        false
    }

    fn commit_paths(&self, _dir: &str, _paths: &[String], _message: &str) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn tempdir(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("present-git-test-{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    fn init_repo(dir: &Path) {
        assert!(Command::new("git").arg("init").arg("-q").current_dir(dir).status().unwrap().success());
        assert!(Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
    }

    fn commit_count(dir: &Path) -> usize {
        let output = Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(dir)
            .output()
            .unwrap();
        if !output.status.success() {
            return 0;
        }
        String::from_utf8_lossy(&output.stdout).lines().count()
    }

    #[test]
    fn is_repo_true_inside_a_git_repo() {
        let dir = tempdir("is-repo-true");
        init_repo(&dir);
        assert!(RealGit.is_repo(dir.to_str().unwrap()));
        cleanup(&dir);
    }

    #[test]
    fn is_repo_false_outside_a_git_repo() {
        let dir = tempdir("is-repo-false");
        assert!(!RealGit.is_repo(dir.to_str().unwrap()));
        cleanup(&dir);
    }

    #[test]
    fn commit_paths_commits_only_the_given_paths() {
        let dir = tempdir("commit-scoped");
        init_repo(&dir);
        fs::create_dir_all(dir.join("panel-a")).unwrap();
        fs::write(dir.join("panel-a").join("text.md"), "a").unwrap();
        fs::create_dir_all(dir.join("panel-b")).unwrap();
        fs::write(dir.join("panel-b").join("text.md"), "b").unwrap();

        let committed = RealGit
            .commit_paths(dir.to_str().unwrap(), &["panel-a".to_string()], "Edit panel-a")
            .unwrap();
        assert!(committed);

        // panel-a is committed...
        let status_a = Command::new("git")
            .args(["status", "--porcelain", "--", "panel-a"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&status_a.stdout).trim().is_empty(), "panel-a should be committed");

        // ...but panel-b was never staged or committed.
        let status_b = Command::new("git")
            .args(["status", "--porcelain", "--", "panel-b"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&status_b.stdout).contains("panel-b"),
            "panel-b should remain untouched/untracked"
        );
        cleanup(&dir);
    }

    #[test]
    fn commit_paths_returns_false_when_nothing_changed() {
        let dir = tempdir("commit-noop");
        init_repo(&dir);
        fs::create_dir_all(dir.join("panel-a")).unwrap();
        fs::write(dir.join("panel-a").join("text.md"), "a").unwrap();
        RealGit.commit_paths(dir.to_str().unwrap(), &["panel-a".to_string()], "Initial").unwrap();
        let count_after_first = commit_count(&dir);

        let committed_again =
            RealGit.commit_paths(dir.to_str().unwrap(), &["panel-a".to_string()], "No-op").unwrap();

        assert!(!committed_again);
        assert_eq!(commit_count(&dir), count_after_first, "no new commit should be made");
        cleanup(&dir);
    }

    #[test]
    fn commit_paths_can_commit_two_paths_together() {
        let dir = tempdir("commit-two-paths");
        init_repo(&dir);
        fs::create_dir_all(dir.join("10")).unwrap();
        fs::write(dir.join("10").join("text.md"), "a").unwrap();
        fs::create_dir_all(dir.join("20")).unwrap();
        fs::write(dir.join("20").join("text.md"), "b").unwrap();
        RealGit
            .commit_paths(dir.to_str().unwrap(), &["10".to_string(), "20".to_string()], "Initial")
            .unwrap();

        // Swap the contents, as H/L would.
        fs::write(dir.join("10").join("text.md"), "b").unwrap();
        fs::write(dir.join("20").join("text.md"), "a").unwrap();

        let committed = RealGit
            .commit_paths(dir.to_str().unwrap(), &["10".to_string(), "20".to_string()], "Swap panels 10 and 20")
            .unwrap();
        assert!(committed);

        let log = Command::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&log.stdout).trim(), "Swap panels 10 and 20");
        cleanup(&dir);
    }

    #[test]
    fn commit_paths_records_a_swap_as_renames_when_panels_hold_different_files() {
        // Mirrors real panel content: two panels rarely share the exact same set of
        // filenames, so a swap_panels-style directory rename (via a temp dir, exactly
        // like assets::swap_panels) deletes each file's old path outright and creates
        // a genuinely new path at the sibling directory -- which is exactly what git's
        // own rename detection needs to pair them up, as opposed to same-filename
        // panels where the path persists and only its content changes (a plain edit).
        let dir = tempdir("commit-swap-renames");
        init_repo(&dir);
        fs::create_dir_all(dir.join("02")).unwrap();
        fs::write(dir.join("02").join("prompt.md"), "prompt body").unwrap();
        fs::write(dir.join("02").join("text.md"), "text body").unwrap();
        fs::create_dir_all(dir.join("03")).unwrap();
        fs::write(dir.join("03").join("diagram.md"), "diagram body").unwrap();
        RealGit
            .commit_paths(dir.to_str().unwrap(), &["02".to_string(), "03".to_string()], "Initial")
            .unwrap();

        // The same three-step dance assets::swap_panels performs.
        fs::rename(dir.join("02"), dir.join(".swap-tmp")).unwrap();
        fs::rename(dir.join("03"), dir.join("02")).unwrap();
        fs::rename(dir.join(".swap-tmp"), dir.join("03")).unwrap();

        let committed = RealGit
            .commit_paths(dir.to_str().unwrap(), &["02".to_string(), "03".to_string()], "Swap panels 02 and 03")
            .unwrap();
        assert!(committed);

        let show = Command::new("git")
            .args(["show", "--stat", "-M", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&show.stdout);
        assert!(
            stat.contains("=>"),
            "expected git to record renamed files (=> in --stat output), got:\n{stat}"
        );
        cleanup(&dir);
    }
}
