mod config;
mod exec;
mod git;
mod matcher;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use rayon::prelude::*;
use std::io::{stdout, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use config::Config;

/// Fast parallel formatter runner for changed files
#[derive(Parser, Debug)]
#[command(name = "ffx")]
#[command(version)]
#[command(about = "Fast parallel formatter runner for changed files")]
#[command(after_help = "\
Examples:
  ffx                Format changed files
  ffx --staged       Format staged files only
  ffx --all          Format all matching files
  ffx --verbose      Show commands being run
  ffx -j4            Limit to 4 parallel jobs

Exit codes:
  0  Success
  1  Formatter failure
  2  Config/general error
  3  Missing executable")]
struct Cli {
    /// Run on all files matching config patterns
    #[arg(long)]
    all: bool,

    /// Run only on staged files
    #[arg(long)]
    staged: bool,

    /// Path to config file
    #[arg(long, default_value = ".fast-format-x.yaml")]
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

fn run() -> Result<RunOutcome> {
    let start = Instant::now();
    let cli = Cli::parse();

    // Configure parallelism
    exec::configure_parallelism(cli.jobs as usize)?;

    // Get repo root to run formatters from (ensures paths resolve correctly from subdirs)
    let repo_root = git::repo_root().context("Failed to find git repository root")?;

    // Load config
    let config_path = Path::new(&cli.config);
    let config = Config::load(config_path)
        .with_context(|| format!("Failed to load config from {}", cli.config))?;

    if cli.verbose {
        eprintln!("repo root: {}", repo_root.display());
        eprintln!("config: {} ({} tools)", cli.config, config.tools.len());
        eprintln!("jobs: {}", cli.jobs);
        eprintln!();
    }

    // Get files to format
    let (files, file_source) = if cli.all {
        (
            git::all_files().context("Failed to get all files")?,
            "all tracked files",
        )
    } else if cli.staged {
        (
            git::staged_files().context("Failed to get staged files")?,
            "staged files",
        )
    } else {
        (
            git::changed_files().context("Failed to get changed files")?,
            "changed files",
        )
    };

    if files.is_empty() {
        println!("No {file_source}.");
        return Ok(RunOutcome {
            success: true,
            missing_executable: false,
        });
    }

    // Match files to tools
    let matches =
        matcher::match_files(&files, &config.tools).context("Failed to match files to tools")?;

    if matches.is_empty() {
        println!("No files matched any tool patterns.");
        return Ok(RunOutcome {
            success: true,
            missing_executable: false,
        });
    }

    // Check that all required commands exist
    for m in &matches {
        if !exec::command_exists(&m.tool.cmd) {
            eprintln!(
                "error: command '{}' not found (required by tool '{}')",
                m.tool.cmd, m.tool.name
            );
            return Ok(RunOutcome {
                success: false,
                missing_executable: true,
            });
        }
    }

    // Show planned work - verbose shows file list, non-verbose shows running indicators
    let is_tty = stdout().is_terminal();
    println!("Running formatters:");

    if cli.verbose {
        for m in &matches {
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
    } else if is_tty {
        // Print running indicators that we'll update in-place
        for m in &matches {
            println!(
                "{} [{}] {} {}",
                "⋯".yellow(),
                m.tool.name.cyan(),
                m.files.len(),
                pluralize_files(m.files.len())
            );
        }
    }

    // Track if we should stop early (for --fail-fast)
    let should_stop = AtomicBool::new(false);

    // Run formatters in parallel
    let results: Vec<_> = matches
        .par_iter()
        .filter_map(|m| {
            if cli.fail_fast && should_stop.load(Ordering::Relaxed) {
                return None;
            }

            let result = exec::run_tool(m.tool, &m.files, cli.verbose, &repo_root);

            if let Ok(ref r) = result {
                if !r.success {
                    should_stop.store(true, Ordering::Relaxed);
                }
            }

            Some((m.tool.name.clone(), m.files.len(), result))
        })
        .collect();

    // Sort results by tool name for deterministic output
    let mut sorted_results = results;
    sorted_results.sort_by(|a, b| a.0.cmp(&b.0));

    // Move cursor back up to overwrite running indicators (non-verbose TTY only)
    if !cli.verbose && is_tty {
        // Move cursor up by the number of tools
        print!("\x1b[{}A", matches.len());
        let _ = stdout().flush();
    }

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

                if !cli.verbose && is_tty {
                    // Overwrite the line and clear to end
                    print!(
                        "\r{} [{}] {} {}\x1b[K\n",
                        status,
                        name.cyan(),
                        file_count,
                        pluralize_files(file_count)
                    );
                    let _ = stdout().flush();
                } else {
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
                if !cli.verbose && is_tty {
                    print!("\r{} [{}] error\x1b[K\n", "✗".red(), name.cyan());
                    let _ = stdout().flush();
                } else {
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
        println!(
            "{} {} {} in {:.2}s",
            "Formatted".green(),
            total_files,
            pluralize_files(total_files),
            elapsed.as_secs_f64()
        );
    } else {
        println!(
            "{} ({:.2}s)",
            "Some formatters failed".red(),
            elapsed.as_secs_f64()
        );
    }

    Ok(RunOutcome {
        success: all_success,
        missing_executable: false,
    })
}

fn num_cpus() -> u64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(4)
}

/// Return "file" or "files" based on count for correct grammar.
fn pluralize_files(count: usize) -> &'static str {
    if count == 1 {
        "file"
    } else {
        "files"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
