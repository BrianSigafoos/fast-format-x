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
