//! Configuration loading and merging.
//!
//! Supports `.claude/tools.json` (under the `formatter` key) at the git root.
//! Partial override semantics on top of all-enabled defaults.

use crate::resolve::find_git_root_from_dir;
use serde::Deserialize;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum ConfigSource {
    Default,
    Explicit,
}

#[derive(Debug)]
pub struct Config {
    pub enabled: bool,
    pub formatters: FormattersConfig,
    pub source: ConfigSource,
    pub git_root: Option<PathBuf>,
}

#[derive(Debug)]
pub struct FormattersConfig {
    pub oxfmt: bool,
    pub eof_newline: bool,
}

impl Default for FormattersConfig {
    fn default() -> Self {
        Self {
            oxfmt: true,
            eof_newline: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            formatters: FormattersConfig::default(),
            source: ConfigSource::Default,
            git_root: None,
        }
    }
}

#[derive(Deserialize)]
struct ProjectConfig {
    enabled: Option<bool>,
    oxfmt: Option<bool>,
    #[serde(rename = "eofNewline")]
    eof_newline: Option<bool>,
}

pub(crate) const TOOLS_CONFIG_FILE: &str = ".claude/tools.json";

#[derive(Deserialize)]
struct ToolsConfig {
    formatter: Option<ProjectConfig>,
}

impl Config {
    pub fn with_project_overrides(self) -> Result<Self, String> {
        let cwd =
            env::current_dir().map_err(|e| format!("cannot determine working directory: {}", e))?;
        self.with_overrides_from_root(&cwd)
    }

    pub(crate) fn with_overrides_from_root(mut self, start: &Path) -> Result<Self, String> {
        let Some(git_root) = find_git_root_from_dir(start) else {
            return Ok(self);
        };
        self.git_root = Some(git_root.clone());

        let tools_path = git_root.join(TOOLS_CONFIG_FILE);
        match fs::read_to_string(&tools_path) {
            Ok(content) => {
                let tools: ToolsConfig = serde_json::from_str(&content)
                    .map_err(|e| format!("invalid config {:?}: {}", tools_path, e))?;
                if let Some(project) = tools.formatter {
                    return Ok(self.merge(&project));
                }
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                return Err(format!("cannot read config {:?}: {}", tools_path, e));
            }
            Err(_) => {}
        }

        Ok(self)
    }

    fn merge(mut self, project: &ProjectConfig) -> Self {
        self.source = ConfigSource::Explicit;
        if let Some(enabled) = project.enabled {
            self.enabled = enabled;
        }
        if let Some(v) = project.oxfmt {
            self.formatters.oxfmt = v;
        }
        if let Some(v) = project.eof_newline {
            self.formatters.eof_newline = v;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_all_enabled() {
        let config = Config::default();
        assert!(config.enabled);
        assert!(config.formatters.oxfmt);
        assert!(config.formatters.eof_newline);
    }

    #[test]
    fn merge_partial_formatters_override() {
        let base = Config::default();
        let project: ProjectConfig = serde_json::from_str(r#"{"oxfmt": false}"#).unwrap();

        let merged = base.merge(&project);
        assert!(!merged.formatters.oxfmt);
        assert!(merged.formatters.eof_newline);
    }

    #[test]
    fn merge_eof_newline_override() {
        let base = Config::default();
        let project: ProjectConfig = serde_json::from_str(r#"{"eofNewline": false}"#).unwrap();

        let merged = base.merge(&project);
        assert!(!merged.formatters.eof_newline);
        assert!(merged.formatters.oxfmt);
    }

    #[test]
    fn merge_enabled_override() {
        let base = Config::default();
        let project: ProjectConfig = serde_json::from_str(r#"{"enabled": false}"#).unwrap();

        let merged = base.merge(&project);
        assert!(!merged.enabled);
        assert!(merged.formatters.oxfmt);
    }

    #[test]
    fn merge_empty_project_config_no_change() {
        let base = Config::default();
        let project: ProjectConfig = serde_json::from_str(r#"{}"#).unwrap();

        let merged = base.merge(&project);
        assert!(merged.enabled);
        assert!(merged.formatters.oxfmt);
    }

    fn tmp_repo() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        tmp
    }

    fn tmp_repo_with_claude() -> tempfile::TempDir {
        let tmp = tmp_repo();
        fs::create_dir(tmp.path().join(".claude")).unwrap();
        tmp
    }

    // [T-001] Default Config has ConfigSource::Default
    #[test]
    fn t_001_default_config_has_default_source() {
        let config = Config::default();
        assert_eq!(config.source, ConfigSource::Default);
    }

    // [T-002] After merge, Config has ConfigSource::Explicit
    #[test]
    fn t_002_merge_sets_explicit_source() {
        let base = Config::default();
        let project: ProjectConfig = serde_json::from_str(r#"{}"#).unwrap();
        let merged = base.merge(&project);
        assert_eq!(merged.source, ConfigSource::Explicit);
    }

    // [T-003] tools.json with formatter key -> merged config, source=Explicit
    #[test]
    fn t_003_with_overrides_tools_json_sets_explicit() {
        let tmp = tmp_repo_with_claude();
        fs::write(
            tmp.path().join(TOOLS_CONFIG_FILE),
            r#"{"formatter": {"oxfmt":false}}"#,
        )
        .unwrap();

        let config = Config::default()
            .with_overrides_from_root(tmp.path())
            .unwrap();
        assert!(!config.formatters.oxfmt);
        assert_eq!(config.source, ConfigSource::Explicit);
    }

    // [T-004] tools.json without formatter key -> defaults, source=Default
    #[test]
    fn t_004_with_overrides_no_formatter_key_stays_default() {
        let tmp = tmp_repo_with_claude();
        fs::write(
            tmp.path().join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"some": "config"}}"#,
        )
        .unwrap();

        let config = Config::default()
            .with_overrides_from_root(tmp.path())
            .unwrap();
        assert!(config.formatters.oxfmt);
        assert_eq!(config.source, ConfigSource::Default);
    }

    // Legacy "biome" key in tools.json is ignored, not an error (backward compat)
    #[test]
    fn legacy_biome_key_is_ignored() {
        let tmp = tmp_repo_with_claude();
        fs::write(
            tmp.path().join(TOOLS_CONFIG_FILE),
            r#"{"formatter": {"biome": false, "oxfmt": true}}"#,
        )
        .unwrap();

        let config = Config::default()
            .with_overrides_from_root(tmp.path())
            .unwrap();
        assert!(config.formatters.oxfmt);
        assert_eq!(config.source, ConfigSource::Explicit);
    }

    // [T-005] Invalid JSON in tools.json -> Result::Err
    #[test]
    fn t_005_with_overrides_invalid_json_returns_err() {
        let tmp = tmp_repo_with_claude();
        fs::write(tmp.path().join(TOOLS_CONFIG_FILE), "not valid json{{{").unwrap();

        let result = Config::default().with_overrides_from_root(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid config"));
    }

    // [T-006] tools.json does not exist -> defaults
    #[test]
    fn t_006_with_overrides_no_tools_json_returns_defaults() {
        let tmp = tmp_repo();

        let config = Config::default()
            .with_overrides_from_root(tmp.path())
            .unwrap();
        assert!(config.enabled);
        assert!(config.formatters.oxfmt);
        assert_eq!(config.source, ConfigSource::Default);
    }

    // [T-007] tools.json unreadable (NotFound excluded) -> Result::Err
    #[cfg(unix)]
    #[test]
    fn t_007_with_overrides_permission_denied_returns_err() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tmp_repo_with_claude();
        let tools_path = tmp.path().join(TOOLS_CONFIG_FILE);
        fs::write(&tools_path, r#"{"formatter": {}}"#).unwrap();
        fs::set_permissions(&tools_path, fs::Permissions::from_mode(0o000)).unwrap();

        let result = Config::default().with_overrides_from_root(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot read config"));

        // Restore permissions for cleanup
        fs::set_permissions(&tools_path, fs::Permissions::from_mode(0o644)).unwrap();
    }

    // Verify git_root is set after with_overrides_from_root
    #[test]
    fn with_overrides_sets_git_root() {
        let tmp = tmp_repo();
        let config = Config::default()
            .with_overrides_from_root(tmp.path())
            .unwrap();
        assert_eq!(config.git_root, Some(tmp.path().to_path_buf()));
    }

    // Verify git_root is None when no .git directory
    #[test]
    fn with_overrides_no_git_leaves_git_root_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = Config::default()
            .with_overrides_from_root(tmp.path())
            .unwrap();
        assert_eq!(config.git_root, None);
    }
}
