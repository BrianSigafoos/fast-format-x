//! Git operations for file discovery.
//!
//! Provides functions to discover staged files and find the repo root.
//! All file-listing functions run from the repo root to ensure paths are
//! always relative to the repo root, regardless of the current working directory.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Get the root directory of the git repository.
///
/// Used to run formatters from the repo root, ensuring paths resolve correctly
/// even when ffx is invoked from a subdirectory.
pub fn repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to run git rev-parse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Not a git repository: {}", stderr.trim());
    }

    let path = String::from_utf8(output.stdout)
        .context("Git output was not valid UTF-8")?
        .trim()
        .to_string();

    Ok(PathBuf::from(path))
}

/// Get all tracked files in the repository.
///
/// Uses `git ls-files` to list all files tracked by git.
/// This respects .gitignore and excludes untracked files.
/// Returns paths relative to the repo root.
///
/// The `work_dir` parameter specifies the directory to run git from (should be repo root).
pub fn all_files(work_dir: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git ls-files")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git ls-files failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout).context("Git output was not valid UTF-8")?;

    let files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();

    Ok(files)
}

/// Get list of staged files (excludes deleted files).
///
/// Returns paths relative to the repo root.
///
/// The `work_dir` parameter specifies the directory to run git from (should be repo root).
pub fn staged_files(work_dir: &Path) -> Result<Vec<PathBuf>> {
    // --diff-filter=d excludes deleted files
    // --name-only shows only file paths
    // --cached shows staged (index) changes
    let output = Command::new("git")
        .args(["diff", "--name-only", "--cached", "--diff-filter=d"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git diff")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout).context("Git output was not valid UTF-8")?;

    let files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();

    Ok(files)
}

/// Get list of changed files (staged, unstaged, and untracked).
///
/// Excludes deleted files and returns paths relative to the repo root.
///
/// The `work_dir` parameter specifies the directory to run git from (should be repo root).
pub fn changed_files(work_dir: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=normal"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git status failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout).context("Git output was not valid UTF-8")?;

    // Use BTreeSet for deterministic ordering and deduplication
    let mut files: BTreeSet<PathBuf> = BTreeSet::new();

    for line in stdout.lines() {
        if line.len() < 3 {
            continue;
        }

        let status = &line[..2];
        // Skip deleted files (either staged or unstaged)
        if status.contains('D') {
            continue;
        }

        let path_part = line[3..].trim();

        // For renames, git status outputs "old -> new"; take the new path
        let path_str = if let Some(idx) = path_part.rfind(" -> ") {
            &path_part[idx + 4..]
        } else {
            path_part
        };

        if path_str.is_empty() {
            continue;
        }

        files.insert(PathBuf::from(path_str));
    }

    Ok(files.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_root_returns_path() {
        // This test only works when run inside a git repo
        let result = repo_root();
        assert!(result.is_ok(), "Should find repo root: {:?}", result);
        let path = result.unwrap();
        assert!(path.exists(), "Repo root should exist");
        assert!(path.join(".git").exists(), "Should have .git directory");
    }

    #[test]
    fn test_staged_files_returns_vec() {
        // This test only works when run inside a git repo
        // It should at least not error, even if no files are staged
        let root = repo_root().unwrap();
        let result = staged_files(&root);
        assert!(result.is_ok(), "Should get staged files: {:?}", result);
    }

    #[test]
    fn test_changed_files_returns_vec() {
        // This test only works when run inside a git repo
        let root = repo_root().unwrap();
        let result = changed_files(&root);
        assert!(result.is_ok(), "Should get changed files: {:?}", result);
    }

    #[test]
    fn test_all_files_returns_tracked_files() {
        // This test only works when run inside a git repo
        let root = repo_root().unwrap();
        let result = all_files(&root);
        assert!(result.is_ok(), "Should get all files: {:?}", result);
        let files = result.unwrap();
        // Should have at least some files in a git repo
        assert!(!files.is_empty(), "Should have some tracked files");
        // Should include Cargo.toml which is always tracked
        assert!(
            files.iter().any(|f| f.ends_with("Cargo.toml")),
            "Should include Cargo.toml"
        );
    }
}
