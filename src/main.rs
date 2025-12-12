mod config;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::Path;
use std::process::ExitCode;

use config::Config;

#[derive(Parser, Debug)]
#[command(name = "ffx")]
#[command(about = "Fast parallel formatter runner for staged files")]
#[command(version)]
struct Cli {
    /// Run on all files matching config patterns (default: staged files only)
    #[arg(long)]
    all: bool,

    /// Path to config file
    #[arg(long, default_value = ".ffx.yaml")]
    config: String,

    /// Max parallel processes (default: number of CPUs)
    #[arg(long, short = 'j')]
    jobs: Option<usize>,

    /// Stop scheduling new work on first failure
    #[arg(long)]
    fail_fast: bool,

    /// Print full commands and output
    #[arg(long, short = 'v')]
    verbose: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::from(2) // Config/general error
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Load config
    let config_path = Path::new(&cli.config);
    let config = Config::load(config_path)
        .with_context(|| format!("Failed to load config from {}", cli.config))?;

    if cli.verbose {
        println!("Loaded config with {} tools:", config.tools.len());
        for tool in &config.tools {
            println!("  - {} ({} patterns)", tool.name, tool.include.len());
        }
        println!();
    }

    let jobs = cli.jobs.unwrap_or_else(num_cpus);

    println!("ffx v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Mode: {}", if cli.all { "all files" } else { "staged files" });
    println!("Jobs: {}", jobs);
    println!("Tools: {}", config.tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "));
    println!();
    println!("TODO: Implement file discovery and execution!");

    Ok(())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
