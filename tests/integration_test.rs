//! Integration tests for ffx CLI.
//!
//! These tests run the actual ffx binary and verify its behavior.

use std::fs;
use std::process::Command;

/// Get the path to the ffx binary (built by cargo test).
fn ffx_binary() -> std::path::PathBuf {
    // cargo test builds the binary in target/debug
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // Remove test binary name
    path.pop(); // Remove deps
    path.push("ffx");
    path
}

/// Create a temporary directory with a config file.
fn setup_test_dir(config_content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join(".fast-format-x.yaml");
    fs::write(&config_path, config_content).unwrap();
    dir
}

#[test]
fn test_help_flag() {
    let output = Command::new(ffx_binary())
        .arg("--help")
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fast parallel formatter runner"));
    assert!(stdout.contains("--staged"));
    assert!(stdout.contains("--all"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--verbose"));
}

#[test]
fn test_version_flag() {
    let output = Command::new(ffx_binary())
        .arg("--version")
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ffx"));
}

#[test]
fn test_missing_config_file() {
    let dir = tempfile::tempdir().unwrap();

    // Initialize git repo (required since we check repo root first)
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git");

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .arg("--config")
        .arg("nonexistent.yaml")
        .output()
        .expect("Failed to run ffx");

    // Should exit with error code 2 (config error)
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Failed to load config") || stderr.contains("Failed to read"));
}

#[test]
fn test_invalid_config_version() {
    let config = r#"
version: 99
tools:
  - name: test
    include: ["**/*.txt"]
    cmd: echo
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo (required since we check repo root first)
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git");

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .output()
        .expect("Failed to run ffx");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unsupported config version"));
}

#[test]
fn test_no_changed_files_message() {
    let config = r#"
version: 1
tools:
  - name: test
    include: ["**/*.txt"]
    cmd: echo
"#;
    let dir = setup_test_dir(config);

    // Initialize a git repo so staged_files() works
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git");

    // Commit the config file so the working tree is clean
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("Failed to add config");

    Command::new("git")
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test User",
            "commit",
            "-m",
            "Initial commit",
        ])
        .current_dir(dir.path())
        .output()
        .expect("Failed to commit config");

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No changed files"));
}

#[test]
fn test_staged_flag_shows_message_when_empty() {
    let config = r#"
version: 1
tools:
  - name: test
    include: ["**/*.txt"]
    cmd: echo
"#;
    let dir = setup_test_dir(config);

    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git");

    // Commit the config file so no files are staged
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("Failed to add config");

    Command::new("git")
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test User",
            "commit",
            "-m",
            "Initial commit",
        ])
        .current_dir(dir.path())
        .output()
        .expect("Failed to commit config");

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .arg("--staged")
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No staged files"));
}

#[test]
fn test_all_flag_no_files_matched() {
    let config = r#"
version: 1
tools:
  - name: markdown
    include: ["**/*.md"]
    cmd: echo
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo and add a non-matching file
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "hello").unwrap();

    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .arg("--all")
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No files matched any tool patterns"));
}

#[test]
fn test_all_flag_runs_formatter() {
    let config = r#"
version: 1
tools:
  - name: echo-test
    include: ["**/*.txt"]
    cmd: echo
    args: [formatted]
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo and add matching file
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "hello").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .arg("--all")
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[echo-test]"));
    assert!(stdout.contains("1 file")); // Correct grammar: "1 file" not "1 files"
    assert!(stdout.contains("Formatted"));
}

#[test]
fn test_verbose_flag_shows_command() {
    let config = r#"
version: 1
tools:
  - name: verbose-test
    include: ["**/*.txt"]
    cmd: echo
    args: [hello]
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo and add matching file
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "content").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .args(["--all", "--verbose"])
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Verbose output goes to stderr
    assert!(stderr.contains("echo hello"));
}

#[test]
fn test_missing_command_error() {
    let config = r#"
version: 1
tools:
  - name: missing
    include: ["**/*.txt"]
    cmd: this_command_does_not_exist_xyz
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo and add matching file
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "content").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .arg("--all")
        .output()
        .expect("Failed to run ffx");

    // Should fail because command doesn't exist
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(3));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn test_formatter_failure_returns_exit_code_1() {
    let config = r#"
version: 1
tools:
  - name: failing
    include: ["**/*.txt"]
    cmd: "false"
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo and add matching file
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    fs::write(dir.path().join("test.txt"), "content").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .arg("--all")
        .output()
        .expect("Failed to run ffx");

    // Should return exit code 1 for formatter failure
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("✗")); // Failure marker
    assert!(stdout.contains("Some formatters failed"));
}

#[test]
fn test_all_flag_from_subdirectory() {
    // Regression test: running ffx from a subdirectory should find files correctly.
    // The issue was that git ls-files returns paths relative to CWD, but formatters
    // run from repo root, causing path mismatches.
    //
    // We use 'cat' as the formatter because it will fail if the file path is wrong,
    // unlike 'echo' which would succeed with any argument.
    let config = r#"
version: 1
tools:
  - name: cat-test
    include: ["**/*.txt"]
    cmd: cat
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create a subdirectory with a file
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("test.txt"), "hello").unwrap();

    // Add the file (so git ls-files finds it)
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Run ffx from the subdirectory, pointing to config in repo root
    let config_path = dir.path().join(".fast-format-x.yaml");
    let output = Command::new(ffx_binary())
        .current_dir(&subdir)
        .args(["--all", "--config"])
        .arg(&config_path)
        .output()
        .expect("Failed to run ffx");

    assert!(
        output.status.success(),
        "ffx should succeed from subdirectory. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[cat-test]"));
    assert!(stdout.contains("1 file"));
    assert!(stdout.contains("Formatted"));
}
