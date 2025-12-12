mod config;
mod exec;
mod git;
mod matcher;

use anyhow::{Context, Result};
use clap::Parser;
use rayon::prelude::*;
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use config::Config;

/// Fast parallel formatter runner for staged files
#[derive(Parser, Debug)]
#[command(name = "ffx")]
#[command(version)]
#[command(about = "Fast parallel formatter runner for staged files")]
#[command(after_help = "\
Examples:
  ffx                Format staged files
  ffx --all          Format all matching files
  ffx --verbose      Show commands being run
  ffx -j4            Limit to 4 parallel jobs

Exit codes:
  0  Success
  1  Formatter failure
  2  Config/general error")]
struct Cli {
    /// Run on all files matching config patterns
    #[arg(long)]
    all: bool,

    /// Path to config file
    #[arg(long, default_value = ".ffx.yaml")]
    config: String,

    /// Max parallel processes
    #[arg(long, short = 'j', default_value_t = num_cpus())]
    jobs: usize,

    /// Stop on first failure
    #[arg(long)]
    fail_fast: bool,

    /// Show commands and detailed output
    #[arg(long, short = 'v')]
    verbose: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(success) => {
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let start = Instant::now();
    let cli = Cli::parse();

    // Configure parallelism
    exec::configure_parallelism(cli.jobs)?;

    // Load config
    let config_path = Path::new(&cli.config);
    let config = Config::load(config_path)
        .with_context(|| format!("Failed to load config from {}", cli.config))?;

    if cli.verbose {
        eprintln!("config: {} ({} tools)", cli.config, config.tools.len());
        eprintln!("jobs: {}", cli.jobs);
        eprintln!();
    }

    // Get files to format
    let files = if cli.all {
        eprintln!("--all mode not yet implemented");
        return Ok(true);
    } else {
        git::staged_files().context("Failed to get staged files")?
    };

    if files.is_empty() {
        println!("No staged files.");
        return Ok(true);
    }

    // Match files to tools
    let matches =
        matcher::match_files(&files, &config.tools).context("Failed to match files to tools")?;

    if matches.is_empty() {
        println!("No files matched any tool patterns.");
        return Ok(true);
    }

    // Check that all required commands exist
    for m in &matches {
        if !exec::command_exists(&m.tool.cmd) {
            eprintln!(
                "error: command '{}' not found (required by tool '{}')",
                m.tool.cmd, m.tool.name
            );
            return Ok(false);
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

            let result = exec::run_tool(m.tool, &m.files);

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

    let mut all_success = true;
    let mut total_files = 0;

    for (name, file_count, result) in sorted_results {
        total_files += file_count;

        match result {
            Ok(tool_result) => {
                let status = if tool_result.success { "✓" } else { "✗" };
                println!("{} [{}] {} files", status, name, file_count);

                for batch in &tool_result.batches {
                    if cli.verbose {
                        eprintln!("  $ {}", batch.command);
                    }
                    if !batch.stdout.is_empty() {
                        for line in batch.stdout.lines() {
                            println!("  {}", line);
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
                println!("✗ [{}] error", name);
                eprintln!("  {e:#}");
                all_success = false;
            }
        }
    }

    let elapsed = start.elapsed();

    println!();
    if all_success {
        println!(
            "Formatted {} files in {:.2}s",
            total_files,
            elapsed.as_secs_f64()
        );
    } else {
        println!("Some formatters failed ({:.2}s)", elapsed.as_secs_f64());
    }

    Ok(all_success)
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
