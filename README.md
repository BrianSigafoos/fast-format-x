# fast-format-x (ffx)

A blazing fast CLI that runs formatter commands on staged files in parallel. Written in Rust.

## Installation

### Prerequisites

Install Rust if you don't have it:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Install

```bash
git clone https://github.com/briansigafoos/fast-format-x.git
cd fast-format-x
cargo install --path .
```

The binary is installed to `~/.cargo/bin/ffx`. Make sure `~/.cargo/bin` is in your PATH.

## Usage

```bash
# Format staged files (default)
ffx

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

### AI Agent Integration

Add to your `AGENTS.md` or Cursor rules to save LLM tokens on formatting:

```markdown
## Formatting

Run `ffx` to auto-format all staged files. Don't manually format code.
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

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Formatter failure |
| 2 | Config error |
| 3 | Missing executable |

---

## Development

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
