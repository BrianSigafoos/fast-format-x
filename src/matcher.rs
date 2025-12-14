//! Glob pattern matching for files to tools.
//!
//! Matches files against tool include/exclude patterns to determine
//! which formatter should process each file.

use crate::config::Tool;
use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

/// A compiled matcher for a single tool.
struct ToolMatcher {
    include: GlobSet,
    exclude: GlobSet,
}

impl ToolMatcher {
    /// Create a new matcher from a tool's patterns.
    fn new(tool: &Tool) -> Result<Self> {
        let include = build_globset(&tool.include)
            .with_context(|| format!("Invalid include patterns for tool '{}'", tool.name))?;

        let exclude = build_globset(&tool.exclude)
            .with_context(|| format!("Invalid exclude patterns for tool '{}'", tool.name))?;

        Ok(Self { include, exclude })
    }

    /// Check if a file matches this tool (included and not excluded).
    fn matches(&self, path: &Path) -> bool {
        self.include.is_match(path) && !self.exclude.is_match(path)
    }
}

/// Build a GlobSet from a list of pattern strings.
fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();

    for pattern in patterns {
        let glob =
            Glob::new(pattern).with_context(|| format!("Invalid glob pattern: {}", pattern))?;
        builder.add(glob);
    }

    builder.build().context("Failed to build glob set")
}

/// Result of matching files to tools.
pub struct MatchResult<'a> {
    /// The tool configuration
    pub tool: &'a Tool,
    /// Files that matched this tool
    pub files: Vec<&'a Path>,
}

/// Match files against tools and return which files each tool should process.
///
/// A file is matched to the FIRST tool whose patterns match it.
/// This ensures each file is only processed once.
pub fn match_files<'a>(
    files: &'a [impl AsRef<Path>],
    tools: &'a [Tool],
) -> Result<Vec<MatchResult<'a>>> {
    // Build matchers for all tools
    let matchers: Vec<ToolMatcher> = tools
        .iter()
        .map(ToolMatcher::new)
        .collect::<Result<Vec<_>>>()?;

    // Track which files have been matched
    let mut matched: Vec<bool> = vec![false; files.len()];

    // Collect results per tool
    let mut results: Vec<MatchResult<'a>> = Vec::new();

    for (tool, matcher) in tools.iter().zip(matchers.iter()) {
        let mut tool_files: Vec<&Path> = Vec::new();

        for (i, file) in files.iter().enumerate() {
            if matched[i] {
                continue; // Already matched to an earlier tool
            }

            let path = file.as_ref();
            if matcher.matches(path) {
                tool_files.push(path);
                matched[i] = true;
            }
        }

        if !tool_files.is_empty() {
            results.push(MatchResult {
                tool,
                files: tool_files,
            });
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_tool(name: &str, include: &[&str], exclude: &[&str]) -> Tool {
        Tool {
            name: name.to_string(),
            include: include.iter().map(|s| s.to_string()).collect(),
            exclude: exclude.iter().map(|s| s.to_string()).collect(),
            cmd: "echo".to_string(),
            args: vec![],
            check_args: None,
        }
    }

    #[test]
    fn test_basic_matching() {
        let tools = vec![
            make_tool("rust", &["**/*.rs"], &[]),
            make_tool("markdown", &["**/*.md"], &[]),
        ];

        let files: Vec<PathBuf> = vec![
            "src/main.rs".into(),
            "src/lib.rs".into(),
            "README.md".into(),
            "docs/guide.md".into(),
        ];

        let results = match_files(&files, &tools).unwrap();

        assert_eq!(results.len(), 2);

        assert_eq!(results[0].tool.name, "rust");
        assert_eq!(results[0].files.len(), 2);

        assert_eq!(results[1].tool.name, "markdown");
        assert_eq!(results[1].files.len(), 2);
    }

    #[test]
    fn test_exclude_patterns() {
        let tools = vec![make_tool("rust", &["**/*.rs"], &["target/**"])];

        let files: Vec<PathBuf> = vec!["src/main.rs".into(), "target/debug/build.rs".into()];

        let results = match_files(&files, &tools).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].files.len(), 1);
        assert_eq!(results[0].files[0], Path::new("src/main.rs"));
    }

    #[test]
    fn test_first_match_wins() {
        // Both tools match .rs files, but first tool should win
        let tools = vec![
            make_tool("first", &["**/*.rs"], &[]),
            make_tool("second", &["**/*.rs"], &[]),
        ];

        let files: Vec<PathBuf> = vec!["src/main.rs".into()];

        let results = match_files(&files, &tools).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool.name, "first");
    }

    #[test]
    fn test_no_matches() {
        let tools = vec![make_tool("rust", &["**/*.rs"], &[])];

        let files: Vec<PathBuf> = vec!["README.md".into()];

        let results = match_files(&files, &tools).unwrap();

        assert!(results.is_empty());
    }
}
