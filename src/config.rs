//! Configuration parsing and validation for ffx.
//!
//! The config file (.ffx.yaml) defines which tools run on which file patterns.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Root configuration structure matching .ffx.yaml schema.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Schema version (must be 1)
    pub version: u32,

    /// List of formatter tools to run
    pub tools: Vec<Tool>,
}

/// A formatter tool configuration.
#[derive(Debug, Deserialize)]
pub struct Tool {
    /// Human-readable name for output (e.g., "rubocop", "prettier")
    pub name: String,

    /// Glob patterns for files to include (e.g., "**/*.rb")
    pub include: Vec<String>,

    /// Glob patterns for files to exclude (e.g., "vendor/**")
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Command to run (e.g., "bundle", "npx", "ktlint")
    pub cmd: String,

    /// Arguments to pass to the command (files appended at end)
    #[serde(default)]
    pub args: Vec<String>,

    /// Arguments to use in check mode (--check flag). Falls back to args if not set.
    #[serde(default)]
    pub check_args: Option<Vec<String>>,
}

impl Tool {
    /// Get the arguments to use, based on check mode.
    /// Returns check_args if check mode is enabled and check_args is set,
    /// otherwise returns args.
    pub fn get_args(&self, check_mode: bool) -> &[String] {
        if check_mode {
            self.check_args.as_deref().unwrap_or(&self.args)
        } else {
            &self.args
        }
    }
}

impl Config {
    /// Load and parse config from a YAML file.
    pub fn load(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        config.validate()?;

        Ok(config)
    }

    /// Validate the config after parsing.
    fn validate(&self) -> Result<()> {
        // Check version
        if self.version != 1 {
            anyhow::bail!(
                "Unsupported config version: {}. Only version 1 is supported.",
                self.version
            );
        }

        // Check we have at least one tool
        if self.tools.is_empty() {
            anyhow::bail!("Config must define at least one tool");
        }

        // Validate each tool
        for tool in &self.tools {
            if tool.name.is_empty() {
                anyhow::bail!("Tool name cannot be empty");
            }
            if tool.include.is_empty() {
                anyhow::bail!(
                    "Tool '{}' must have at least one include pattern",
                    tool.name
                );
            }
            if tool.cmd.is_empty() {
                anyhow::bail!("Tool '{}' must have a cmd", tool.name);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_validate(yaml: &str) -> Result<Config> {
        let config: Config = serde_yaml::from_str(yaml)?;
        config.validate()?;
        Ok(config)
    }

    #[test]
    fn test_parse_valid_config() {
        let yaml = r#"
version: 1

tools:
  - name: prettier
    include: ["**/*.md"]
    cmd: npx
    args: [prettier, --write]
"#;
        let config = parse_and_validate(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.tools.len(), 1);
        assert_eq!(config.tools[0].name, "prettier");
        assert_eq!(config.tools[0].include, vec!["**/*.md"]);
        assert_eq!(config.tools[0].cmd, "npx");
        assert_eq!(config.tools[0].args, vec!["prettier", "--write"]);
        assert!(config.tools[0].check_args.is_none());
    }

    #[test]
    fn test_parse_config_with_check_args() {
        let yaml = r#"
version: 1

tools:
  - name: prettier
    include: ["**/*.md"]
    cmd: npx
    args: [prettier, --write]
    check_args: [prettier, --check]
"#;
        let config = parse_and_validate(yaml).unwrap();
        assert_eq!(config.tools[0].args, vec!["prettier", "--write"]);
        assert_eq!(
            config.tools[0].check_args,
            Some(vec!["prettier".to_string(), "--check".to_string()])
        );
    }

    #[test]
    fn test_get_args_normal_mode() {
        let yaml = r#"
version: 1

tools:
  - name: prettier
    include: ["**/*.md"]
    cmd: npx
    args: [prettier, --write]
    check_args: [prettier, --check]
"#;
        let config = parse_and_validate(yaml).unwrap();
        let tool = &config.tools[0];

        // Normal mode should use args
        assert_eq!(tool.get_args(false), vec!["prettier", "--write"]);
    }

    #[test]
    fn test_get_args_check_mode_with_check_args() {
        let yaml = r#"
version: 1

tools:
  - name: prettier
    include: ["**/*.md"]
    cmd: npx
    args: [prettier, --write]
    check_args: [prettier, --check]
"#;
        let config = parse_and_validate(yaml).unwrap();
        let tool = &config.tools[0];

        // Check mode should use check_args
        assert_eq!(tool.get_args(true), vec!["prettier", "--check"]);
    }

    #[test]
    fn test_get_args_check_mode_without_check_args() {
        let yaml = r#"
version: 1

tools:
  - name: prettier
    include: ["**/*.md"]
    cmd: npx
    args: [prettier, --write]
"#;
        let config = parse_and_validate(yaml).unwrap();
        let tool = &config.tools[0];

        // Check mode without check_args should fall back to args
        assert_eq!(tool.get_args(true), vec!["prettier", "--write"]);
    }

    #[test]
    fn test_exclude_defaults_to_empty() {
        let yaml = r#"
version: 1
tools:
  - name: test
    include: ["**/*.rs"]
    cmd: echo
"#;
        let config = parse_and_validate(yaml).unwrap();
        assert!(config.tools[0].exclude.is_empty());
    }

    #[test]
    fn test_args_defaults_to_empty() {
        let yaml = r#"
version: 1
tools:
  - name: test
    include: ["**/*.rs"]
    cmd: echo
"#;
        let config = parse_and_validate(yaml).unwrap();
        assert!(config.tools[0].args.is_empty());
    }

    #[test]
    fn test_invalid_version() {
        let yaml = r#"
version: 2
tools:
  - name: test
    include: ["**/*.rs"]
    cmd: echo
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unsupported config version"));
        assert!(err.contains("2"));
    }

    #[test]
    fn test_empty_tools() {
        let yaml = r#"
version: 1
tools: []
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least one tool"));
    }

    #[test]
    fn test_empty_tool_name() {
        let yaml = r#"
version: 1
tools:
  - name: ""
    include: ["**/*.rs"]
    cmd: echo
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("name cannot be empty"));
    }

    #[test]
    fn test_empty_include() {
        let yaml = r#"
version: 1
tools:
  - name: test
    include: []
    cmd: echo
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least one include pattern"));
    }

    #[test]
    fn test_empty_cmd() {
        let yaml = r#"
version: 1
tools:
  - name: test
    include: ["**/*.rs"]
    cmd: ""
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must have a cmd"));
    }

    #[test]
    fn test_multiple_tools() {
        let yaml = r#"
version: 1
tools:
  - name: rust
    include: ["**/*.rs"]
    cmd: cargo
    args: [fmt, --]
  - name: prettier
    include: ["**/*.md", "**/*.json"]
    exclude: ["node_modules/**"]
    cmd: npx
    args: [prettier, --write]
"#;
        let config = parse_and_validate(yaml).unwrap();
        assert_eq!(config.tools.len(), 2);
        assert_eq!(config.tools[0].name, "rust");
        assert_eq!(config.tools[1].name, "prettier");
        assert_eq!(config.tools[1].exclude, vec!["node_modules/**"]);
    }
}
