//! Command execution for formatters.
//!
//! Runs formatter commands with batched file arguments in parallel.

use crate::config::Tool;
use anyhow::{Context, Result};
use rayon::prelude::*;
use std::path::Path;
use std::process::{Command, Output};

/// Maximum files per command invocation to avoid arg length limits.
const MAX_FILES_PER_BATCH: usize = 50;

/// Result of running a single batch.
#[derive(Debug)]
pub struct BatchResult {
    /// Whether the command succeeded (exit code 0)
    pub success: bool,
    /// Combined stdout output
    pub stdout: String,
    /// Combined stderr output
    pub stderr: String,
    /// The command that was run (for verbose output)
    pub command: String,
}

/// Result of running all batches for a tool.
#[derive(Debug)]
pub struct ToolResult {
    /// Whether all batches succeeded
    pub success: bool,
    /// Results from each batch
    pub batches: Vec<BatchResult>,
}

/// Run a formatter tool on a set of files.
///
/// Files are batched to avoid command-line length limits.
/// Batches run in parallel using rayon.
pub fn run_tool(tool: &Tool, files: &[&Path]) -> Result<ToolResult> {
    // Create batches
    let batches: Vec<Vec<&Path>> = files
        .chunks(MAX_FILES_PER_BATCH)
        .map(|chunk| chunk.to_vec())
        .collect();

    // Run batches in parallel
    let results: Vec<Result<BatchResult>> = batches
        .par_iter()
        .map(|batch| run_batch(tool, batch))
        .collect();

    // Collect results, propagating any errors
    let mut batch_results = Vec::new();
    let mut all_success = true;

    for result in results {
        let batch = result?;
        if !batch.success {
            all_success = false;
        }
        batch_results.push(batch);
    }

    Ok(ToolResult {
        success: all_success,
        batches: batch_results,
    })
}

/// Run a single batch of files through a formatter.
fn run_batch(tool: &Tool, files: &[&Path]) -> Result<BatchResult> {
    let mut cmd = Command::new(&tool.cmd);

    // Add configured arguments
    cmd.args(&tool.args);

    // Add file paths
    for file in files {
        cmd.arg(file);
    }

    // Build command string for logging
    let command = format!(
        "{} {} {}",
        tool.cmd,
        tool.args.join(" "),
        files
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );

    let output: Output = cmd
        .output()
        .with_context(|| format!("Failed to execute '{}'", tool.cmd))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(BatchResult {
        success: output.status.success(),
        stdout,
        stderr,
        command,
    })
}

/// Check if a command exists in PATH.
pub fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Configure rayon's thread pool size.
pub fn configure_parallelism(jobs: usize) -> Result<()> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build_global()
        .context("Failed to configure thread pool")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Tool;
    use std::path::PathBuf;

    fn make_tool(name: &str, cmd: &str, args: &[&str]) -> Tool {
        Tool {
            name: name.to_string(),
            include: vec!["**/*".to_string()],
            exclude: vec![],
            cmd: cmd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_command_exists_true() {
        // 'echo' should exist on all Unix systems
        assert!(command_exists("echo"));
    }

    #[test]
    fn test_command_exists_false() {
        // This command should not exist
        assert!(!command_exists("this_command_definitely_does_not_exist_12345"));
    }

    #[test]
    fn test_run_tool_with_echo() {
        let tool = make_tool("test", "echo", &["hello"]);
        let files: Vec<PathBuf> = vec!["file1.txt".into(), "file2.txt".into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();

        let result = run_tool(&tool, &file_refs).unwrap();

        assert!(result.success);
        assert_eq!(result.batches.len(), 1);
        assert!(result.batches[0].stdout.contains("hello"));
        assert!(result.batches[0].stdout.contains("file1.txt"));
        assert!(result.batches[0].stdout.contains("file2.txt"));
    }

    #[test]
    fn test_run_tool_failure() {
        // 'false' command always exits with code 1
        let tool = make_tool("fail", "false", &[]);
        let files: Vec<PathBuf> = vec!["file.txt".into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();

        let result = run_tool(&tool, &file_refs).unwrap();

        assert!(!result.success);
        assert!(!result.batches[0].success);
    }

    #[test]
    fn test_run_tool_nonexistent_command() {
        let tool = make_tool("bad", "nonexistent_command_xyz", &[]);
        let files: Vec<PathBuf> = vec!["file.txt".into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();

        let result = run_tool(&tool, &file_refs);

        // Should return an error, not a failed result
        assert!(result.is_err());
    }

    #[test]
    fn test_batching_large_file_list() {
        let tool = make_tool("test", "echo", &[]);

        // Create more files than MAX_FILES_PER_BATCH (50)
        let files: Vec<PathBuf> = (0..120).map(|i| format!("file{}.txt", i).into()).collect();
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();

        let result = run_tool(&tool, &file_refs).unwrap();

        assert!(result.success);
        // Should have 3 batches: 50 + 50 + 20
        assert_eq!(result.batches.len(), 3);
    }

    #[test]
    fn test_batch_result_contains_command() {
        let tool = make_tool("test", "echo", &["--flag"]);
        let files: Vec<PathBuf> = vec!["myfile.rs".into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();

        let result = run_tool(&tool, &file_refs).unwrap();

        let cmd = &result.batches[0].command;
        assert!(cmd.contains("echo"));
        assert!(cmd.contains("--flag"));
        assert!(cmd.contains("myfile.rs"));
    }
}
