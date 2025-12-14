//! Git operations for file discovery.
//!
//! Provides functions to discover staged files and find the repo root.
//! File-listing functions run from the current working directory to respect
//! subdirectory scope, but return paths relative to the repo root so formatters
//! can find them when running from the repo root.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;
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

/// Get the current directory's path relative to the repo root.
///
/// Returns an empty string if at the repo root, otherwise returns the path
/// with a trailing slash (e.g., "src/", "src/utils/").
fn current_prefix() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-prefix"])
        .output()
        .context("Failed to run git rev-parse --show-prefix")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git rev-parse --show-prefix failed: {}", stderr.trim());
    }

    let prefix = String::from_utf8(output.stdout)
        .context("Git output was not valid UTF-8")?
        .trim()
        .to_string();

    Ok(prefix)
}

/// Prepend the current directory prefix to file paths.
///
/// Git commands run from a subdirectory return paths relative to that subdirectory.
/// This function converts them to paths relative to the repo root.
fn prepend_prefix(files: Vec<PathBuf>, prefix: &str) -> Vec<PathBuf> {
    if prefix.is_empty() {
        files
    } else {
        let prefix_path = PathBuf::from(prefix);
        files.into_iter().map(|f| prefix_path.join(f)).collect()
    }
}

/// Get all tracked files in the current directory (and subdirectories).
///
/// Uses `git ls-files` to list all files tracked by git.
/// This respects .gitignore and excludes untracked files.
/// When run from a subdirectory, only returns files in that subdirectory.
/// Returns paths relative to the repo root.
pub fn all_files() -> Result<Vec<PathBuf>> {
    let prefix = current_prefix()?;

    let output = Command::new("git")
        .args(["ls-files"])
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

    Ok(prepend_prefix(files, &prefix))
}

/// Get list of staged files (excludes deleted files).
///
/// When run from a subdirectory, only returns staged files in that subdirectory.
/// Returns paths relative to the repo root.
pub fn staged_files() -> Result<Vec<PathBuf>> {
    let prefix = current_prefix()?;

    // --diff-filter=d excludes deleted files
    // --name-only shows only file paths
    // --cached shows staged (index) changes
    let output = Command::new("git")
        .args(["diff", "--name-only", "--cached", "--diff-filter=d"])
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

    Ok(prepend_prefix(files, &prefix))
}

/// Get list of changed files (staged, unstaged, and untracked).
///
/// Excludes deleted files.
/// When run from a subdirectory, only returns changed files in that subdirectory.
/// Returns paths relative to the repo root.
pub fn changed_files() -> Result<Vec<PathBuf>> {
    let prefix = current_prefix()?;

    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=normal"])
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

    Ok(prepend_prefix(files.into_iter().collect(), &prefix))
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
        let result = staged_files();
        assert!(result.is_ok(), "Should get staged files: {:?}", result);
    }

    #[test]
    fn test_changed_files_returns_vec() {
        // This test only works when run inside a git repo
        let result = changed_files();
        assert!(result.is_ok(), "Should get changed files: {:?}", result);
    }

    #[test]
    fn test_all_files_returns_tracked_files() {
        // This test only works when run inside a git repo
        let result = all_files();
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

    #[test]
    fn test_prepend_prefix_empty() {
        let files = vec![PathBuf::from("file.txt"), PathBuf::from("dir/other.txt")];
        let result = prepend_prefix(files.clone(), "");
        assert_eq!(result, files);
    }

    #[test]
    fn test_prepend_prefix_with_subdir() {
        let files = vec![PathBuf::from("file.txt"), PathBuf::from("sub/other.txt")];
        let result = prepend_prefix(files, "src/");
        assert_eq!(
            result,
            vec![
                PathBuf::from("src/file.txt"),
                PathBuf::from("src/sub/other.txt")
            ]
        );
    }
}
