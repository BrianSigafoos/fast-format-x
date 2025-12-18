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
    assert!(stdout.contains("auto-format every changed file"));
    assert!(stdout.contains("--staged"));
    assert!(stdout.contains("--all"));
    assert!(stdout.contains("--base"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--verbose"));
    assert!(stdout.contains("init"));
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
    assert!(
        stderr.contains("nonexistent.yaml") && stderr.contains("not found"),
        "Should show config file name and 'not found'. stderr: {stderr}"
    );
    assert!(
        stderr.contains("ffx init"),
        "Should suggest 'ffx init'. stderr: {stderr}"
    );
}

#[test]
fn test_init_installs_pre_commit_hook() {
    let dir = tempfile::tempdir().unwrap();

    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git");

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .arg("init")
        .output()
        .expect("Failed to run ffx init");

    assert!(output.status.success());

    let hook_path = dir.path().join(".git/hooks/pre-commit");
    let hook = fs::read_to_string(&hook_path).expect("Hook should be written");
    assert!(hook.contains("ffx --staged"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(&hook_path)
            .expect("Should read hook metadata")
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "Hook should be executable");
    }
}

#[test]
fn test_init_creates_config_template() {
    let dir = tempfile::tempdir().unwrap();

    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git");

    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .arg("init")
        .output()
        .expect("Failed to run ffx init");

    assert!(output.status.success());

    let config_path = dir.path().join(".fast-format-x.yaml");
    let config = fs::read_to_string(&config_path).expect("Config template should be written");
    assert!(config.contains(".fast-format-x.yaml"));
    assert!(config.contains("version: 1"));
}

#[test]
fn test_init_from_subdirectory_creates_config_in_current_dir() {
    // Regression test: running `ffx init` from a subdirectory should create
    // the config file in the current directory, not the repo root.
    let dir = tempfile::tempdir().unwrap();

    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git");

    // Create a subdirectory
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();

    let output = Command::new(ffx_binary())
        .current_dir(&subdir)
        .arg("init")
        .output()
        .expect("Failed to run ffx init");

    assert!(
        output.status.success(),
        "ffx init should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Config should be in the subdirectory
    let subdir_config = subdir.join(".fast-format-x.yaml");
    assert!(
        subdir_config.exists(),
        "Config should be created in subdirectory"
    );

    // Config should NOT be in the repo root
    let root_config = dir.path().join(".fast-format-x.yaml");
    assert!(
        !root_config.exists(),
        "Config should NOT be created in repo root"
    );

    // Hook should still be in the repo root's .git/hooks
    let hook_path = dir.path().join(".git/hooks/pre-commit");
    assert!(hook_path.exists(), "Hook should be in repo root .git/hooks");
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

#[test]
fn test_all_flag_from_subdirectory_excludes_parent_files() {
    // Regression test: running ffx --all from a subdirectory should ONLY format
    // files in that subdirectory, not files in the parent directory.
    //
    // We use 'touch' as a "formatter" to detect which files get processed.
    // We'll check file modification times to verify only subdir files are touched.
    let config = r#"
version: 1
tools:
  - name: touch-test
    include: ["**/*.txt"]
    cmd: touch
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create a file in the root
    fs::write(dir.path().join("root.txt"), "root content").unwrap();

    // Create a subdirectory with a file
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("sub.txt"), "sub content").unwrap();

    // Add all files
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Run ffx from the subdirectory
    let config_path = dir.path().join(".fast-format-x.yaml");
    let output = Command::new(ffx_binary())
        .current_dir(&subdir)
        .args(["--all", "--verbose", "--config"])
        .arg(&config_path)
        .output()
        .expect("Failed to run ffx");

    assert!(
        output.status.success(),
        "ffx should succeed. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should only process 1 file (the one in subdir)
    assert!(
        stdout.contains("1 file"),
        "Should only process 1 file from subdir. stdout: {stdout}"
    );

    // The verbose output should show subdir/sub.txt, not root.txt
    assert!(
        stderr.contains("subdir/sub.txt") || stdout.contains("subdir/sub.txt"),
        "Should process subdir/sub.txt. stdout: {stdout}, stderr: {stderr}"
    );

    // Should NOT contain root.txt
    assert!(
        !stderr.contains("root.txt") && !stdout.contains("root.txt"),
        "Should NOT process root.txt. stdout: {stdout}, stderr: {stderr}"
    );
}

#[test]
fn test_changed_files_from_subdirectory_uses_default_config() {
    // Regression test: running ffx from a subdirectory should find the config
    // file in the repo root by default, and only process changed files in the
    // current directory subtree.
    let config = r#"
version: 1
tools:
  - name: touch-test
    include: ["**/*.txt"]
    cmd: touch
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create a subdirectory
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();

    // Create and stage a file in the subdirectory
    fs::write(subdir.join("test.txt"), "content").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Run ffx from the subdirectory (no --config flag)
    let output = Command::new(ffx_binary())
        .current_dir(&subdir)
        .output()
        .expect("Failed to run ffx");

    assert!(
        output.status.success(),
        "ffx should succeed from subdirectory. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 file"));
    assert!(stdout.contains("Formatted"));
}

#[test]
fn test_subdirectory_scoping_with_similar_directory_names() {
    // Regression test: running ffx from a subdirectory should only process files
    // in that exact subdirectory, not in directories with similar names.
    // This tests the fix for the prefix trimming bug in filter_by_prefix.
    let config = r#"
version: 1
tools:
  - name: touch-test
    include: ["**/*.txt"]
    cmd: touch
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create multiple directories with similar names
    let src_dir = dir.path().join("src");
    let src2_dir = dir.path().join("src2");
    let src_nested_dir = src_dir.join("utils");

    fs::create_dir(&src_dir).unwrap();
    fs::create_dir(&src2_dir).unwrap();
    fs::create_dir(&src_nested_dir).unwrap();

    // Create files in different locations
    fs::write(src_dir.join("file.txt"), "src content").unwrap();
    fs::write(src2_dir.join("file.txt"), "src2 content").unwrap();
    fs::write(src_nested_dir.join("nested.txt"), "nested content").unwrap();
    fs::write(dir.path().join("root.txt"), "root content").unwrap();

    // Add and commit all files
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

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
        .unwrap();

    // Modify files to make them "changed"
    fs::write(src_dir.join("file.txt"), "modified src content").unwrap();
    fs::write(src2_dir.join("file.txt"), "modified src2 content").unwrap();
    fs::write(src_nested_dir.join("nested.txt"), "modified nested content").unwrap();
    fs::write(dir.path().join("root.txt"), "modified root content").unwrap();

    // Run ffx from the src/ subdirectory (no --config flag, should use repo root config)
    let output = Command::new(ffx_binary())
        .current_dir(&src_dir)
        .arg("--verbose")
        .output()
        .expect("Failed to run ffx");

    assert!(
        output.status.success(),
        "ffx should succeed from src/ subdirectory. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should only process files in src/ directory (src/file.txt and src/utils/nested.txt)
    // Should NOT process files in src2/ or root
    assert!(
        stdout.contains("2 files"),
        "Should process exactly 2 files from src/ directory. stdout: {stdout}"
    );

    // Should contain src/file.txt and src/utils/nested.txt in verbose output (stderr)
    assert!(
        stderr.contains("src/file.txt"),
        "Should process src/file.txt. stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stderr.contains("src/utils/nested.txt"),
        "Should process src/utils/nested.txt. stdout: {stdout}, stderr: {stderr}"
    );

    // Should NOT contain src2/file.txt or root.txt
    assert!(
        !stderr.contains("src2/file.txt"),
        "Should NOT process src2/file.txt. stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        !stderr.contains("root.txt"),
        "Should NOT process root.txt. stdout: {stdout}, stderr: {stderr}"
    );
}

#[test]
fn test_base_flag_finds_branch_changes() {
    // Test that --base flag finds files changed between a base ref and HEAD
    let config = r#"
version: 1
tools:
  - name: touch-test
    include: ["**/*.txt"]
    cmd: touch
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo with explicit branch name (CI may default to master)
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create and commit initial file
    fs::write(dir.path().join("initial.txt"), "initial content").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

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
        .unwrap();

    // Create a branch
    Command::new("git")
        .args(["checkout", "-b", "feature"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Add a new file on the feature branch
    fs::write(dir.path().join("feature.txt"), "feature content").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test User",
            "commit",
            "-m",
            "Add feature file",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Run ffx --base main to find files changed vs main
    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .args(["--base", "main", "--verbose"])
        .output()
        .expect("Failed to run ffx");

    assert!(
        output.status.success(),
        "ffx --base should succeed. stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should only process the new feature file
    assert!(
        stdout.contains("1 file"),
        "Should process exactly 1 file. stdout: {stdout}"
    );
    assert!(
        stderr.contains("feature.txt") || stdout.contains("feature.txt"),
        "Should process feature.txt. stdout: {stdout}, stderr: {stderr}"
    );
    // Should NOT process initial.txt (it existed before branching)
    assert!(
        !stderr.contains("initial.txt") && !stdout.contains("initial.txt"),
        "Should NOT process initial.txt. stdout: {stdout}, stderr: {stderr}"
    );
}

#[test]
fn test_base_flag_shows_correct_message_when_no_changes() {
    // Test that --base flag shows the correct message when no files changed
    let config = r#"
version: 1
tools:
  - name: touch-test
    include: ["**/*.txt"]
    cmd: touch
"#;
    let dir = setup_test_dir(config);

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create and commit initial file
    fs::write(dir.path().join("initial.txt"), "initial content").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();

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
        .unwrap();

    // Run ffx --base HEAD (same commit, no changes)
    let output = Command::new(ffx_binary())
        .current_dir(dir.path())
        .args(["--base", "HEAD"])
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No files changed vs HEAD"),
        "Should show 'No files changed vs HEAD'. stdout: {stdout}"
    );
}

#[test]
fn test_base_flag_conflicts_with_all() {
    let output = Command::new(ffx_binary())
        .args(["--base", "main", "--all"])
        .output()
        .expect("Failed to run ffx");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflict"),
        "Should show conflict error. stderr: {stderr}"
    );
}

#[test]
fn test_base_flag_conflicts_with_staged() {
    let output = Command::new(ffx_binary())
        .args(["--base", "main", "--staged"])
        .output()
        .expect("Failed to run ffx");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflict"),
        "Should show conflict error. stderr: {stderr}"
    );
}

#[test]
fn test_check_mode_shows_failure_details_after_summary() {
    // Test that --check mode shows failure details after the summary
    // We use a script that outputs to both stdout and stderr and fails
    let config = r#"
version: 1
tools:
  - name: failing-linter
    include: ["**/*.txt"]
    cmd: sh
    check_args: ["-c", "echo 'stdout: file needs formatting'; echo 'stderr: error detail' >&2; exit 1"]
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
        .args(["--all", "--check"])
        .output()
        .expect("Failed to run ffx");

    // Should fail
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show "Details:" section after summary
    assert!(
        stdout.contains("Details:"),
        "Should show Details section. stdout: {stdout}"
    );

    // Should show the tool name in details
    assert!(
        stdout.contains("[failing-linter]"),
        "Should show tool name in details. stdout: {stdout}"
    );

    // Should show stdout from the failing command
    assert!(
        stdout.contains("stdout: file needs formatting"),
        "Should show stdout from failing command. stdout: {stdout}"
    );

    // Should show stderr from the failing command
    assert!(
        stderr.contains("stderr: error detail"),
        "Should show stderr from failing command. stderr: {stderr}"
    );

    // Should show the command that was run
    assert!(
        stdout.contains("$ sh -c"),
        "Should show command in details. stdout: {stdout}"
    );
}

#[test]
fn test_check_mode_no_details_on_success() {
    // Test that --check mode does NOT show Details section when all pass
    let config = r#"
version: 1
tools:
  - name: passing-linter
    include: ["**/*.txt"]
    cmd: true
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
        .args(["--all", "--check"])
        .output()
        .expect("Failed to run ffx");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should NOT show "Details:" section when all pass
    assert!(
        !stdout.contains("Details:"),
        "Should NOT show Details section on success. stdout: {stdout}"
    );
}
