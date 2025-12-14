# fast-format-x (ffx)

**The only formatter your AI needs to know.**

One command. Every file type. All formatters run in parallel.

Stop telling your AI agents how to auto-format your code. Give them one command, `ffx`, that auto-formats all changed files using the right tool for each file type. Written in Rust for speed.

## Installation

### Quick Install (Recommended)

Install the latest release with a single command:

```bash
curl -LsSf https://fast-format-x.briansigafoos.com/install.sh | bash
# Then initialize in your repo to install the pre-commit hook
ffx init
```

This downloads the prebuilt binary for your platform (macOS Apple Silicon or Intel).

### Manual Download

Download binaries directly from [GitHub Releases](https://github.com/BrianSigafoos/fast-format-x/releases):

| Platform            | Download                          |
| ------------------- | --------------------------------- |
| macOS Apple Silicon | `ffx-aarch64-apple-darwin.tar.gz` |
| macOS Intel         | `ffx-x86_64-apple-darwin.tar.gz`  |

```bash
# Example: Download and install manually
curl -LO https://github.com/BrianSigafoos/fast-format-x/releases/latest/download/ffx-aarch64-apple-darwin.tar.gz
tar xzf ffx-aarch64-apple-darwin.tar.gz
mv ffx ~/.local/bin/  # or ~/.cargo/bin/ if you have Rust installed
```

### Install from Source

For contributors or if you prefer building from source:

```bash
# Requires Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone https://github.com/BrianSigafoos/fast-format-x.git
cd fast-format-x
cargo install --path .
```

The binary is installed to `~/.cargo/bin/ffx`. Make sure `~/.cargo/bin` is in your PATH.

## Usage

```bash
# Format changed files (default)
ffx

# Format staged files only
ffx --staged

# Format all matching files
ffx --all

# Check mode for CI (uses check_args, exits non-zero if issues found)
ffx --all --check

# Use custom config
ffx --config path/to/.fast-format-x.yaml

# Limit parallel jobs
ffx --jobs 4

# Stop on first failure
ffx --fail-fast

# Verbose output
ffx --verbose
```

### Pre-commit Hook

Run ffx automatically before every commit and scaffold a starter config if you don't have one yet:

```bash
ffx init
```

This installs a pre-commit hook that:

1. Runs `ffx` on staged files
2. Re-stages any files modified by formatters

If `.fast-format-x.yaml` doesn't exist, `ffx init` also creates a template with common formatters and a reminder to customize the tools for your repository.

### AI Agent Integration

Replace multiple formatting instructions in your [AGENTS.md](https://agents.md) with one line:

```markdown
## Formatting

Run `ffx` to auto-format every changed file (it runs the correct formatter for each file)
```

Instead of teaching your AI agent about prettier, standard, rubocop, gofmt, and rustfmt, just tell it to run `ffx`. One command. No wasted tokens.

## Configuration

Create `.fast-format-x.yaml` in your repo root:

```yaml
version: 1

tools:
  - name: rubocop
    include:
      - "**/*.rb"
      - "**/*.rake"
    exclude:
      - "vendor/**"
    cmd: bundle
    args: [exec, rubocop, -A] # format mode (default)
    check_args: [exec, rubocop] # check mode (--check flag)

  - name: prettier
    include: ["**/*.md", "**/*.yml", "**/*.yaml", "**/*.js", "**/*.ts"]
    cmd: npx
    args: [prettier, --write]
    check_args: [prettier, --check]

  - name: ktlint
    include: ["**/*.kt", "**/*.kts"]
    cmd: ktlint
    args: [-F]
    check_args: [] # ktlint checks by default

  - name: gofmt
    include: ["**/*.go"]
    cmd: gofmt
    args: [-w]
    check_args: [-l] # list files that differ

  - name: rustfmt
    include: ["**/*.rs"]
    cmd: cargo
    args: [fmt, --]
    check_args: [fmt, --, --check]
```

### Check Mode for CI

Use `--check` to verify files are formatted without modifying them:

```bash
# In your CI pipeline
ffx --all --check
```

When `--check` is passed, ffx uses `check_args` instead of `args`. If `check_args` is not defined for a tool, it falls back to `args`.

## Exit Codes

| Code | Meaning            |
| ---- | ------------------ |
| 0    | Success            |
| 1    | Formatter failure  |
| 2    | Config error       |
| 3    | Missing executable |

---

## Development

Contributions are welcome.

### Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verify
rustc --version
cargo --version
```

### Build and Run

```bash
# Build debug version
cargo build

# Run directly
cargo run
cargo run -- --help
cargo run -- --all

# Build optimized release
cargo build --release
```

### Development Commands

```bash
# Check code compiles
cargo check

# Run tests
cargo test

# Format code
cargo fmt

# Lint code
cargo clippy

# Watch for changes
cargo install cargo-watch  # one-time
cargo watch -x check
```

### Releasing

Releases are managed with [cargo-release](https://github.com/crate-ci/cargo-release). This ensures `Cargo.toml` version stays in sync with git tags.

```bash
# Install cargo-release (one-time)
cargo install cargo-release

# Release a new version (updates Cargo.toml, commits, tags, and pushes)
cargo release patch  # 0.1.3 → 0.1.4
cargo release minor  # 0.1.3 → 0.2.0
cargo release major  # 0.1.3 → 1.0.0

# Dry run to see what will happen
cargo release patch --dry-run
```

The push triggers the GitHub Actions release workflow, which builds binaries for all platforms and creates a GitHub Release.
