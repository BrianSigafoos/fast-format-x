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
            anyhow::bail!("Unsupported config version: {}. Only version 1 is supported.", self.version);
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
                anyhow::bail!("Tool '{}' must have at least one include pattern", tool.name);
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
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.tools.len(), 1);
        assert_eq!(config.tools[0].name, "prettier");
        assert_eq!(config.tools[0].include, vec!["**/*.md"]);
        assert_eq!(config.tools[0].cmd, "npx");
        assert_eq!(config.tools[0].args, vec!["prettier", "--write"]);
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
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.tools[0].exclude.is_empty());
    }
}

