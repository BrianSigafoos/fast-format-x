# fast-format-x (ffx)

A fast CLI that runs formatter commands on staged files in parallel.

## Development Setup

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the prompts (default installation is fine). Then restart your terminal or run:

```bash
source ~/.cargo/env
```

Verify installation:

```bash
rustc --version
cargo --version
```

### 2. Build and Run

```bash
# Build debug version
cargo build

# Run directly
cargo run

# Run with arguments
cargo run -- --help
cargo run -- --all

# Build optimized release version
cargo build --release
```

The release binary will be at `target/release/ffx`.

### 3. Development Commands

```bash
# Check code compiles without building
cargo check

# Run tests
cargo test

# Format code
cargo fmt

# Lint code
cargo clippy

# Watch for changes and rebuild
cargo install cargo-watch  # one-time install
cargo watch -x check
```

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
    include: ["**/*.md", "**/*.yml", "**/*.yaml"]
    cmd: npx
    args: [prettier, --write]

  - name: ktlint
    include: ["**/*.kt", "**/*.kts"]
    cmd: ktlint
    args: [-F]
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Formatter failure |
| 2 | Config error |
| 3 | Missing executable |

