# fast-format-x (ffx)

A blazing fast CLI that runs formatter commands on changed files in parallel. Written in Rust.

## Installation

### Quick Install (Recommended)

Install the latest release with a single command:

```bash
curl -LsSf https://raw.githubusercontent.com/BrianSigafoos/fast-format-x/main/install.sh | bash
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

Run ffx automatically before every commit:

```bash
git config core.hooksPath .githooks
```

This uses the pre-commit hook in `.githooks/pre-commit` that:

1. Runs `ffx` on staged files
2. Re-stages any files modified by formatters

To set this up in a new repo, copy the `.githooks/` directory and run the config command above.

### AI Agent Integration

Add to your `AGENTS.md` or Cursor rules to save LLM tokens on formatting:

```markdown
## Formatting

Run `ffx` to auto-format all changed files. Don't manually format code.
```

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
    args: [exec, rubocop, -A]

  - name: prettier
    include: ["**/*.md", "**/*.yml", "**/*.yaml", "**/*.js", "**/*.ts"]
    cmd: npx
    args: [prettier, --write]

  - name: ktlint
    include: ["**/*.kt", "**/*.kts"]
    cmd: ktlint
    args: [-F]

  - name: gofmt
    include: ["**/*.go"]
    cmd: gofmt
    args: [-w]

  - name: rustfmt
    include: ["**/*.rs"]
    cmd: cargo
    args: [fmt, --]
```

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
