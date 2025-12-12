# Product spec: fast-format-x (v0.1)

## Summary

A fast Rust CLI that runs formatter commands on staged files in parallel. Mapping from file patterns to commands lives in a simple YAML config. Optional `--all` runs on all matching files.

**Command name**: `ffx`

---

## Goals

- Staged files by default. No other git modes in v0.1.
- Parallel execution. Fast startup. Low overhead.
- Simple, explicit YAML config.
- Deterministic output and exit codes.
- macOS primary. Linux/Windows should work automatically (Rust handles this).

## Non-goals

- Installing formatters or toolchains.
- CI orchestration.
- Advanced caching or git diff modes.
- Check-only mode (v0.2).

---

## CLI

### Usage

```
ffx                     # Format staged files
ffx --all               # Format all matching files
```

### Flags

| Flag | Description |
|------|-------------|
| `--all` | Run on all files matching config patterns |
| `--config <path>` | Config file path (default: `.ffx.yaml`) |
| `--jobs <n>` | Max parallel processes (default: logical CPUs) |
| `--fail-fast` | Stop scheduling new work on first failure |
| `--verbose` | Print full commands and output |

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Formatter failure |
| 2 | Config error |
| 3 | Missing executable |

---

## File discovery

### Default (staged files)

```bash
git diff --name-only --cached
```

- Ignore deleted files.
- Paths are repo-relative.

### With `--all`

- Walk repo from git root.
- Respect `.gitignore`.
- Apply include and exclude patterns from config.

---

## Config file

**Location**: `.fast-format-x.yaml` at repo root.

### Schema (v1)

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

  - name: ktlint
    include: ["**/*.kt", "**/*.kts"]
    cmd: ktlint
    args: [-F]

  - name: prettier
    include: ["**/*.md", "**/*.yml", "**/*.yaml"]
    cmd: npx
    args: [prettier, --write]
```

### Semantics

- Tools evaluated in YAML order.
- `exclude` always wins over `include`.
- Files are appended to the end of `args`.
- Files are batched (max 50 per invocation) to avoid arg length limits.

---

## Execution model

1. Get candidate files (staged or all).
2. Match files to tools using glob patterns.
3. Batch files per tool (max 50 files per batch).
4. Execute batches in parallel (bounded by `--jobs`).
5. Capture stdout and stderr.
6. Print output grouped by tool.
7. Exit non-zero if any batch fails.

---

## Output

### Default

```
[rubocop] 12 files
  corrected app/models/user.rb
  corrected app/controllers/foo.rb

[prettier] 3 files
  docs/readme.md
```

### Verbose

Prints full command and streamed output per batch.

---

## Failure handling

- All scheduled batches run by default.
- Any non-zero exit marks run as failed.
- `--fail-fast` stops scheduling new batches after first failure.

---

## Security

- Commands executed directly via `std::process::Command`. No shell.
- Paths passed as separate args (no injection risk).

---

## Performance targets

- Startup under 20ms on warm cache.
- Zero repo scanning by default (uses git's staged file list).
- Parallelism saturates CPU without overspawn.

---

## Rust implementation

### Crates

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing |
| `serde` + `serde_yaml` | Config parsing |
| `globset` | Glob pattern matching |
| `rayon` | Parallel execution |
| `anyhow` | Error handling |

### Modules

| Module | Purpose |
|--------|---------|
| `config` | Parse and validate YAML |
| `git` | Staged file discovery |
| `matcher` | Glob matching files to tools |
| `planner` | Group files into batches |
| `exec` | Process runner with parallelism |
| `output` | Deterministic reporting |

---

## Build order (learning path)

1. **Skeleton**: `cargo new ffx`, add `clap` and `serde_yaml`
2. **Config parsing**: Load `.fast-format-x.yaml`, deserialize to structs
3. **Git discovery**: Shell out to `git diff --name-only --cached`
4. **Glob matching**: Match files to tools with `globset`
5. **Single-threaded execution**: Run commands with `std::process::Command`
6. **Parallel execution**: Add `rayon` for parallelism
7. **Polish**: Better output, `--verbose`, error codes

---

## Future (v0.2+)

- `ffx check` subcommand with `check_args` per tool
- `per_file` strategy (run formatter once per file)
- Configurable `max_files` per tool
- `.gitignore` respect for `--all` mode (via `ignore` crate)
