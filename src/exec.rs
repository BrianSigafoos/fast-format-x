//! Command execution for formatters.
//!
//! Runs formatter commands with batched file arguments in parallel.

use crate::config::Tool;
use anyhow::{Context, Result};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Output};

/// Maximum bytes per command invocation to avoid ARG_MAX limits.
/// 128KB is safe for most systems (macOS ARG_MAX is 256KB, Linux is 2MB+).
/// This leaves headroom for environment variables.
const MAX_BATCH_BYTES: usize = 128 * 1024;

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

/// Calculate the byte size of an OS string (for arg length estimation).
fn arg_bytes(s: &OsStr) -> usize {
    // Use encoded length + 1 for null terminator
    s.len() + 1
}

/// Create batches of files that fit within MAX_BATCH_BYTES.
///
/// Each batch's total arg bytes (cmd + args + files) stays under the limit.
fn create_batches<'a>(tool: &Tool, files: &[&'a Path]) -> Vec<Vec<&'a Path>> {
    // Calculate fixed overhead: command + configured args
    let base_bytes: usize = arg_bytes(OsStr::new(&tool.cmd))
        + tool
            .args
            .iter()
            .map(|a| arg_bytes(OsStr::new(a)))
            .sum::<usize>();

    let mut batches: Vec<Vec<&'a Path>> = Vec::new();
    let mut current_batch: Vec<&'a Path> = Vec::new();
    let mut current_bytes = base_bytes;

    for file in files {
        let file_bytes = arg_bytes(file.as_os_str());

        // If adding this file would exceed limit, start a new batch
        // (unless batch is empty - we must include at least one file)
        if !current_batch.is_empty() && current_bytes + file_bytes > MAX_BATCH_BYTES {
            batches.push(std::mem::take(&mut current_batch));
            current_bytes = base_bytes;
        }

        current_batch.push(file);
        current_bytes += file_bytes;
    }

    // Don't forget the last batch
    if !current_batch.is_empty() {
        batches.push(current_batch);
    }

    batches
}

/// Run a formatter tool on a set of files.
///
/// Files are batched by total argument bytes to avoid ARG_MAX limits.
/// Batches run in parallel using rayon.
/// When `verbose` is true, command strings are captured for logging.
/// `work_dir` sets the working directory for the formatter commands.
pub fn run_tool(
    tool: &Tool,
    files: &[&Path],
    verbose: bool,
    work_dir: &Path,
) -> Result<ToolResult> {
    // Create batches based on total arg bytes
    let batches = create_batches(tool, files);

    // Run batches in parallel
    let results: Vec<Result<BatchResult>> = batches
        .par_iter()
        .map(|batch| run_batch(tool, batch, verbose, work_dir))
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
fn run_batch(tool: &Tool, files: &[&Path], verbose: bool, work_dir: &Path) -> Result<BatchResult> {
    let mut cmd = Command::new(&tool.cmd);

    // Run from repo root so paths resolve correctly
    cmd.current_dir(work_dir);

    // Add configured arguments
    cmd.args(&tool.args);

    // Add file paths
    for file in files {
        cmd.arg(file);
    }

    // Only build command string when verbose (avoids allocation overhead)
    let command = if verbose {
        format!(
            "{} {} {}",
            tool.cmd,
            tool.args.join(" "),
            files
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        )
    } else {
        String::new()
    };

    let output: Output = match cmd.output() {
        Ok(output) => output,
        Err(e) => {
            let message = e.to_string();
            if e.kind() == ErrorKind::InvalidInput || message.contains("Argument list too long") {
                return Ok(BatchResult {
                    success: false,
                    stdout: String::new(),
                    stderr: message,
                    command,
                });
            }

            return Err(e).with_context(|| format!("Failed to execute '{}'", tool.cmd));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(BatchResult {
        success: output.status.success(),
        stdout,
        stderr,
        command,
    })
}

/// Check if a command exists in PATH (cross-platform).
pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
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
        assert!(!command_exists(
            "this_command_definitely_does_not_exist_12345"
        ));
    }

    #[test]
    fn test_run_tool_with_echo() {
        let tool = make_tool("test", "echo", &["hello"]);
        let files: Vec<PathBuf> = vec!["file1.txt".into(), "file2.txt".into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        let work_dir = std::env::current_dir().unwrap();

        let result = run_tool(&tool, &file_refs, false, &work_dir).unwrap();

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
        let work_dir = std::env::current_dir().unwrap();

        let result = run_tool(&tool, &file_refs, false, &work_dir).unwrap();

        assert!(!result.success);
        assert!(!result.batches[0].success);
    }

    #[test]
    fn test_run_tool_nonexistent_command() {
        let tool = make_tool("bad", "nonexistent_command_xyz", &[]);
        let files: Vec<PathBuf> = vec!["file.txt".into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        let work_dir = std::env::current_dir().unwrap();

        let result = run_tool(&tool, &file_refs, false, &work_dir);

        // Should return an error, not a failed result
        assert!(result.is_err());
    }

    #[test]
    fn test_batching_by_bytes() {
        let tool = make_tool("test", "echo", &[]);

        // Create files with predictable sizes
        // Each "fileNNN.txt" is ~12 bytes + 1 null = 13 bytes
        // With 128KB limit and ~5 bytes base overhead (echo + null),
        // we can fit roughly 128*1024 / 13 ≈ 10,000 files per batch
        // So 450 short-named files should fit in 1 batch
        let files: Vec<PathBuf> = (0..450).map(|i| format!("file{}.txt", i).into()).collect();
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        let work_dir = std::env::current_dir().unwrap();

        let result = run_tool(&tool, &file_refs, false, &work_dir).unwrap();

        assert!(result.success);
        // Short filenames should fit in a single batch
        assert_eq!(result.batches.len(), 1);
    }

    #[test]
    fn test_batching_splits_on_byte_limit() {
        let tool = make_tool("test", "echo", &[]);

        // Create files with long paths to force multiple batches
        // Each path is ~200 bytes, so ~640 files should exceed 128KB
        let long_dir = "a".repeat(180);
        let files: Vec<PathBuf> = (0..1000)
            .map(|i| format!("{}/file{}.txt", long_dir, i).into())
            .collect();
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        let work_dir = std::env::current_dir().unwrap();

        let result = run_tool(&tool, &file_refs, false, &work_dir).unwrap();

        assert!(result.success);
        // Long filenames should require multiple batches
        assert!(
            result.batches.len() > 1,
            "Expected multiple batches for long paths, got {}",
            result.batches.len()
        );
    }

    #[test]
    fn test_batching_includes_oversized_file() {
        let tool = make_tool("test", "echo", &[]);

        // Create a file path that alone exceeds MAX_BATCH_BYTES
        // This tests that we still include it (at least one file per batch)
        let huge_path = "x".repeat(200_000);
        let files: Vec<PathBuf> = vec![huge_path.into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        let work_dir = std::env::current_dir().unwrap();

        let result = run_tool(&tool, &file_refs, false, &work_dir).unwrap();

        // Should still run (even if arg might be too long for actual execution)
        // The important thing is we don't panic or create empty batches
        assert_eq!(result.batches.len(), 1);
    }

    #[test]
    fn test_batch_result_contains_command_when_verbose() {
        let tool = make_tool("test", "echo", &["--flag"]);
        let files: Vec<PathBuf> = vec!["myfile.rs".into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        let work_dir = std::env::current_dir().unwrap();

        let result = run_tool(&tool, &file_refs, true, &work_dir).unwrap();

        let cmd = &result.batches[0].command;
        assert!(cmd.contains("echo"));
        assert!(cmd.contains("--flag"));
        assert!(cmd.contains("myfile.rs"));
    }

    #[test]
    fn test_batch_result_empty_command_when_not_verbose() {
        let tool = make_tool("test", "echo", &["--flag"]);
        let files: Vec<PathBuf> = vec!["myfile.rs".into()];
        let file_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        let work_dir = std::env::current_dir().unwrap();

        let result = run_tool(&tool, &file_refs, false, &work_dir).unwrap();

        // Command should be empty when not verbose
        assert!(result.batches[0].command.is_empty());
    }
}
