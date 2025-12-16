mod config;
mod exec;
mod git;
mod matcher;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{stdout, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use config::Config;

/// Default config file name.
const CONFIG_FILE_NAME: &str = ".fast-format-x.yaml";

/// One command to auto-format every changed file
#[derive(Parser, Debug)]
#[command(name = "ffx")]
#[command(version)]
#[command(about = "One command to auto-format every changed file. All formatters run in parallel.")]
#[command(after_help = "\
Examples:
  ffx                Format changed files
  ffx --staged       Format staged files only
  ffx --all          Format all matching files
  ffx --all --check  Check all files (CI mode, no modifications)
  ffx --verbose      Show commands being run
  ffx -j4            Limit to 4 parallel jobs

Exit codes:
  0  Success
  1  Formatter failure
  2  Config/general error
  3  Missing executable")]
struct Cli {
    /// Initialize git hooks
    #[command(subcommand)]
    command: Option<Command>,

    /// Run on all files matching config patterns
    #[arg(long)]
    all: bool,

    /// Run only on staged files
    #[arg(long)]
    staged: bool,

    /// Check mode for CI (use check_args instead of args, no file modifications)
    #[arg(long)]
    check: bool,

    /// Path to config file
    #[arg(long, default_value = CONFIG_FILE_NAME)]
    config: String,

    /// Max parallel processes (minimum 1)
    #[arg(long, short = 'j', default_value_t = num_cpus(), value_parser = clap::value_parser!(u64).range(1..))]
    jobs: u64,

    /// Stop on first failure
    #[arg(long)]
    fail_fast: bool,

    /// Show commands and detailed output
    #[arg(long, short = 'v')]
    verbose: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Install the pre-commit hook to run ffx automatically
    Init,
}

fn main() -> ExitCode {
    match run() {
        Ok(outcome) => exit_code_from_outcome(&outcome),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn exit_code_from_outcome(outcome: &RunOutcome) -> ExitCode {
    if outcome.success {
        ExitCode::SUCCESS
    } else if outcome.missing_executable {
        ExitCode::from(3)
    } else {
        ExitCode::from(1)
    }
}

struct RunOutcome {
    success: bool,
    missing_executable: bool,
}

impl RunOutcome {
    fn success() -> Self {
        Self {
            success: true,
            missing_executable: false,
        }
    }

    fn missing_executable() -> Self {
        Self {
            success: false,
            missing_executable: true,
        }
    }

    fn from_success(success: bool) -> Self {
        Self {
            success,
            missing_executable: false,
        }
    }
}

fn run() -> Result<RunOutcome> {
    let start = Instant::now();
    let cli = Cli::parse();

    if let Some(Command::Init) = cli.command {
        run_init()?;
        return Ok(RunOutcome::success());
    }

    // Configure parallelism
    exec::configure_parallelism(cli.jobs as usize)?;

    // Get repo root to run formatters from (ensures paths resolve correctly from subdirs)
    let repo_root = git::repo_root().context("Failed to find git repository root")?;

    // Load config - try current directory first, then repo root for default config
    let config_path = Path::new(&cli.config);
    let config = if config_path.exists() {
        Config::load(config_path)
    } else if cli.config == CONFIG_FILE_NAME {
        // Default config file - try repo root
        let repo_config_path = repo_root.join(CONFIG_FILE_NAME);
        Config::load(&repo_config_path)
    } else {
        // Explicitly specified config file - use as-is (will fail with proper error)
        Config::load(config_path)
    }
    .with_context(|| format!("Failed to load config from {}", cli.config))?;

    if cli.verbose {
        eprintln!("repo root: {}", repo_root.display());
        eprintln!("config: {} ({} tools)", cli.config, config.tools.len());
        eprintln!("jobs: {}", cli.jobs);
        if cli.check {
            eprintln!("mode: check (no modifications)");
        }
        eprintln!();
    }

    // Get files to format (respects current directory scope, returns repo-root-relative paths)
    let (files, file_source) = collect_target_files(&cli)?;

    if files.is_empty() {
        println!("No {file_source}.");
        return Ok(RunOutcome::success());
    }

    // Match files to tools
    let matches =
        matcher::match_files(&files, &config.tools).context("Failed to match files to tools")?;

    if matches.is_empty() {
        println!("No files matched any tool patterns.");
        return Ok(RunOutcome::success());
    }

    // Check that all required commands exist
    if let Some(outcome) = ensure_required_commands(&matches) {
        return Ok(outcome);
    }

    // Show planned work - verbose shows file list, non-verbose shows running indicators
    let is_tty = stdout().is_terminal();
    let action = if cli.check { "Checking" } else { "Running" };
    println!("{action} formatters:");

    let indicator_positions = print_planned_work(&matches, cli.verbose, is_tty);

    // Track if we should stop early (for --fail-fast)
    let should_stop = AtomicBool::new(false);

    // Run formatters in parallel and stream results as they complete
    let (tx, rx) = mpsc::channel();

    matches.par_iter().for_each(|m| {
        if cli.fail_fast && should_stop.load(Ordering::Relaxed) {
            let _ = tx.send((m.tool.name.clone(), m.files.len(), None));
            return;
        }

        let result = exec::run_tool(m.tool, &m.files, cli.verbose, cli.check, &repo_root);

        if let Ok(ref r) = result {
            if !r.success {
                should_stop.store(true, Ordering::Relaxed);
            }
        }

        let _ = tx.send((m.tool.name.clone(), m.files.len(), Some(result)));
    });

    let mut results = Vec::with_capacity(matches.len());

    for _ in 0..matches.len() {
        if let Ok((name, file_count, maybe_result)) = rx.recv() {
            if let Some(map) = &indicator_positions {
                if let Some(&line_idx) = map.get(&name) {
                    let total_lines = matches.len();
                    match &maybe_result {
                        Some(Ok(tool_result)) => {
                            let status = if tool_result.success {
                                "✓".green()
                            } else {
                                "✗".red()
                            };
                            update_status_line(
                                line_idx,
                                total_lines,
                                format!(
                                    "{} [{}] {} {}",
                                    status,
                                    name.cyan(),
                                    file_count,
                                    pluralize_files(file_count)
                                ),
                            );
                        }
                        Some(Err(_)) => {
                            update_status_line(
                                line_idx,
                                total_lines,
                                format!("{} [{}] error", "✗".red(), name.cyan()),
                            );
                        }
                        None => {
                            update_status_line(
                                line_idx,
                                total_lines,
                                format!("{} [{}] skipped", "✗".red(), name.cyan()),
                            );
                        }
                    }
                }
            }

            if let Some(result) = maybe_result {
                results.push((name, file_count, result));
            }
        }
    }

    // Sort results by tool name for deterministic output
    let mut sorted_results = results;
    sorted_results.sort_by(|a, b| a.0.cmp(&b.0));

    let mut all_success = true;
    let mut total_files = 0;

    for (name, file_count, result) in sorted_results {
        total_files += file_count;

        match result {
            Ok(tool_result) => {
                let status = if tool_result.success {
                    "✓".green()
                } else {
                    "✗".red()
                };

                if cli.verbose || !is_tty {
                    println!(
                        "{} [{}] {} {}",
                        status,
                        name.cyan(),
                        file_count,
                        pluralize_files(file_count)
                    );
                }

                for batch in &tool_result.batches {
                    if cli.verbose {
                        eprintln!("  $ {}", batch.command);
                        if !batch.stdout.is_empty() {
                            for line in batch.stdout.lines() {
                                println!("  {}", line);
                            }
                        }
                    }
                    if !batch.stderr.is_empty() && (cli.verbose || !batch.success) {
                        for line in batch.stderr.lines() {
                            eprintln!("  {}", line);
                        }
                    }
                }

                if !tool_result.success {
                    all_success = false;
                }
            }
            Err(e) => {
                if cli.verbose || !is_tty {
                    println!("{} [{}] error", "✗".red(), name.cyan());
                }
                eprintln!("  {e:#}");
                all_success = false;
            }
        }
    }

    let elapsed = start.elapsed();

    println!();
    if all_success {
        let done_msg = if cli.check { "Checked" } else { "Formatted" };
        println!(
            "{} {} {} in {:.2}s",
            done_msg.green(),
            total_files,
            pluralize_files(total_files),
            elapsed.as_secs_f64()
        );
    } else {
        let fail_msg = if cli.check {
            "Some checks failed"
        } else {
            "Some formatters failed"
        };
        println!("{} ({:.2}s)", fail_msg.red(), elapsed.as_secs_f64());
    }

    Ok(RunOutcome::from_success(all_success))
}

fn collect_target_files(cli: &Cli) -> Result<(Vec<PathBuf>, &'static str)> {
    if cli.all {
        Ok((
            git::all_files().context("Failed to get all files")?,
            "all tracked files",
        ))
    } else if cli.staged {
        Ok((
            git::staged_files().context("Failed to get staged files")?,
            "staged files",
        ))
    } else {
        Ok((
            git::changed_files().context("Failed to get changed files")?,
            "changed files",
        ))
    }
}

fn ensure_required_commands(matches: &[matcher::MatchResult]) -> Option<RunOutcome> {
    for m in matches {
        if !exec::command_exists(&m.tool.cmd) {
            eprintln!(
                "error: command '{}' not found (required by tool '{}')",
                m.tool.cmd, m.tool.name
            );
            return Some(RunOutcome::missing_executable());
        }
    }

    None
}

fn print_planned_work(
    matches: &[matcher::MatchResult],
    verbose: bool,
    is_tty: bool,
) -> Option<HashMap<String, usize>> {
    if verbose {
        for m in matches {
            let file_list = m
                .files
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "- {} ({} {}): {}",
                m.tool.name,
                m.files.len(),
                pluralize_files(m.files.len()),
                file_list
            );
        }
        println!();
        None
    } else if is_tty {
        for m in matches {
            println!(
                "{} [{}] {} {}",
                "⋯".yellow(),
                m.tool.name.cyan(),
                m.files.len(),
                pluralize_files(m.files.len())
            );
        }

        Some(
            matches
                .iter()
                .enumerate()
                .map(|(idx, m)| (m.tool.name.clone(), idx))
                .collect(),
        )
    } else {
        None
    }
}

fn num_cpus() -> u64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(4)
}

fn update_status_line(line_idx: usize, total_lines: usize, content: String) {
    let (lines_up, lines_down) = cursor_movements(line_idx, total_lines);

    if lines_up > 0 {
        print!("\x1b[{}A", lines_up);
    }

    print!("\r{}\x1b[K\n", content);

    if lines_down > 0 {
        print!("\x1b[{}B", lines_down);
    }

    let _ = stdout().flush();
}

fn cursor_movements(line_idx: usize, total_lines: usize) -> (usize, usize) {
    let lines_up = total_lines.saturating_sub(line_idx);
    let lines_down = total_lines.saturating_sub(line_idx + 1);

    (lines_up, lines_down)
}

/// Return "file" or "files" based on count for correct grammar.
fn pluralize_files(count: usize) -> &'static str {
    if count == 1 {
        "file"
    } else {
        "files"
    }
}

fn run_init() -> Result<()> {
    let repo_root = git::repo_root().context("Failed to find git repository root")?;
    // Config file goes in current directory (where user ran ffx init)
    let config_path = Path::new(CONFIG_FILE_NAME);
    // Hooks go in the git repo root
    let hooks_dir = repo_root.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).context("Failed to create .git/hooks directory")?;

    let hook_path = hooks_dir.join("pre-commit");

    if !config_path.exists() {
        write_config_template(config_path)?;
    }

    if hook_path.exists() {
        let contents = fs::read_to_string(&hook_path).unwrap_or_default();
        if contents.contains("fast-format-x") || contents.contains("ffx") {
            println!(
                "Pre-commit hook already configured for ffx at {}",
                hook_path.display()
            );
            return Ok(());
        }

        anyhow::bail!(
            "A pre-commit hook already exists at {}. Please add ffx manually.",
            hook_path.display()
        );
    }

    fs::write(&hook_path, PRE_COMMIT_HOOK).context("Failed to write pre-commit hook")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&hook_path)
            .context("Failed to read pre-commit hook metadata")?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook_path, permissions)
            .context("Failed to set pre-commit hook permissions")?;
    }

    println!(
        "Pre-commit hook installed at {}. It will run ffx on staged files before each commit.",
        hook_path.display()
    );

    Ok(())
}

fn write_config_template(config_path: &Path) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(config_path)
        .with_context(|| format!("Failed to create {}", config_path.display()))?;

    file.write_all(CONFIG_TEMPLATE.as_bytes())
        .context("Failed to write config template")?;

    println!(
        "Created {}. Update tools to match your project before running ffx.",
        config_path.display()
    );

    Ok(())
}

const PRE_COMMIT_HOOK: &str = r#"#!/bin/sh
set -e

if ! command -v ffx >/dev/null 2>&1; then
    echo "ffx not found. Install it with:"
    echo "  curl -LsSf https://raw.githubusercontent.com/BrianSigafoos/fast-format-x/main/install.sh | bash"
    exit 1
fi

ffx --staged

git diff --name-only | while read -r file; do
    if git diff --cached --name-only | grep -q "^$file$"; then
        git add "$file"
    fi
done
"#;

/// Config template embedded from docs/.fast-format-x.yaml at compile time.
/// This keeps the template in one place for both `ffx init` and the website.
const CONFIG_TEMPLATE: &str = include_str!("../docs/.fast-format-x.yaml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_movement_counts_account_for_position() {
        assert_eq!(cursor_movements(0, 3), (3, 2));
        assert_eq!(cursor_movements(1, 3), (2, 1));
        assert_eq!(cursor_movements(2, 3), (1, 0));
        assert_eq!(cursor_movements(2, 2), (0, 0));
    }

    #[test]
    fn exit_code_success_when_all_pass() {
        let outcome = RunOutcome {
            success: true,
            missing_executable: false,
        };

        assert_eq!(exit_code_from_outcome(&outcome), ExitCode::SUCCESS);
    }

    #[test]
    fn exit_code_missing_executable_uses_code_three() {
        let outcome = RunOutcome {
            success: false,
            missing_executable: true,
        };

        assert_eq!(exit_code_from_outcome(&outcome), ExitCode::from(3));
    }

    #[test]
    fn exit_code_failure_without_missing_executable_uses_one() {
        let outcome = RunOutcome {
            success: false,
            missing_executable: false,
        };

        assert_eq!(exit_code_from_outcome(&outcome), ExitCode::from(1));
    }

    #[test]
    fn run_outcome_convenience_builders_set_flags() {
        assert!(RunOutcome::success().success);
        assert!(RunOutcome::missing_executable().missing_executable);
        assert!(!RunOutcome::from_success(false).success);
    }

    #[test]
    fn ensure_required_commands_reports_missing_executables() {
        use crate::config::Tool;

        let missing_tool = Tool {
            name: "missing".to_string(),
            include: vec![],
            exclude: vec![],
            cmd: "definitely_not_installed".to_string(),
            args: vec![],
            check_args: None,
        };

        let matches = vec![matcher::MatchResult {
            tool: &missing_tool,
            files: vec![Path::new("file.rs")],
        }];

        let outcome = ensure_required_commands(&matches);

        assert!(outcome.is_some());
        assert!(outcome.unwrap().missing_executable);
    }

    #[test]
    fn print_planned_work_returns_positions_for_tty() {
        use crate::config::Tool;

        let tool = Tool {
            name: "test".to_string(),
            include: vec![],
            exclude: vec![],
            cmd: "echo".to_string(),
            args: vec![],
            check_args: None,
        };

        let matches = vec![matcher::MatchResult {
            tool: &tool,
            files: vec![Path::new("file.rs")],
        }];

        let positions = print_planned_work(&matches, false, true).unwrap();

        assert_eq!(positions.get("test"), Some(&0));
    }
}
