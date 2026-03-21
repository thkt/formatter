//! Claude Code PostToolUse hook that auto-formats files on Write/Edit/MultiEdit.
//!
//! Reads hook JSON from stdin, selects the appropriate formatter based on file
//! extension and availability, then formats the file in-place.

mod biome;
mod color;
mod config;
mod eof_newline;
mod oxfmt;
mod resolve;

use config::{Config, ConfigSource};
use serde::Deserialize;
use std::io::{self, Read};

const MAX_INPUT_SIZE: u64 = 10_000_000;

#[derive(Debug, Deserialize, PartialEq)]
enum ToolName {
    Write,
    Edit,
    MultiEdit,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct HookInput {
    tool_name: ToolName,
    tool_input: ToolInput,
}

#[derive(Deserialize)]
struct ToolInput {
    file_path: Option<String>,
}

#[derive(Debug, PartialEq)]
enum Formatter {
    Oxfmt,
    Biome,
}

fn select_formatter(config: &Config, file_path: &str) -> Option<Formatter> {
    if config.formatters.oxfmt && oxfmt::is_formattable(file_path) && oxfmt::is_available(file_path)
    {
        return Some(Formatter::Oxfmt);
    }
    if config.formatters.biome && biome::is_formattable(file_path) && biome::is_available(file_path)
    {
        return Some(Formatter::Biome);
    }
    None
}

fn validate_path(raw_path: &str) -> Option<String> {
    let canonical = match std::path::Path::new(raw_path).canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Formatter: cannot resolve path {}: {}", raw_path, e);
            return None;
        }
    };

    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Formatter: cannot determine CWD: {}", e);
            return None;
        }
    };
    if !canonical.starts_with(&cwd) {
        eprintln!("Formatter: file outside project directory, skipping");
        return None;
    }

    match canonical.to_str() {
        Some(s) => Some(s.to_string()),
        None => {
            eprintln!("Formatter: non-UTF-8 path, skipping");
            None
        }
    }
}

const CONFIG_HINT_MESSAGE: &str =
    "Formatter: using defaults. Customize via .claude/tools.json \u{2014} see https://github.com/thkt/formatter#configuration";

fn show_config_hint(config: &Config) {
    if config.git_root.is_none() || config.source != ConfigSource::Default {
        return;
    }
    eprintln!("{}", color::yellow(CONFIG_HINT_MESSAGE));
}

fn run(input_str: &str) {
    let input: HookInput = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Formatter: invalid hook input: {}", e);
            return;
        }
    };

    if input.tool_name == ToolName::Other {
        return;
    }

    let raw_path = match &input.tool_input.file_path {
        Some(p) if !p.is_empty() => p.as_str(),
        _ => {
            eprintln!(
                "Formatter: {:?} without file_path, skipping",
                input.tool_name
            );
            return;
        }
    };

    let Some(file_path) = validate_path(raw_path) else {
        return;
    };

    let config = match Config::default().with_project_overrides() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Formatter: config error (using defaults): {}", e);
            Config::default()
        }
    };

    show_config_hint(&config);

    if !config.enabled {
        eprintln!("Formatter: disabled by project config, skipping");
        return;
    }

    match select_formatter(&config, &file_path) {
        Some(Formatter::Oxfmt) => oxfmt::format(&file_path),
        Some(Formatter::Biome) => biome::format(&file_path),
        None => {
            if oxfmt::is_formattable(&file_path) || biome::is_formattable(&file_path) {
                eprintln!(
                    "Formatter: supported file but no formatter available: {}",
                    raw_path
                );
            }
            if config.formatters.eof_newline {
                eof_newline::ensure(&file_path);
            }
        }
    }
}

fn main() {
    let mut input_str = String::new();
    if let Err(e) = io::stdin()
        .take(MAX_INPUT_SIZE)
        .read_to_string(&mut input_str)
    {
        eprintln!("Formatter: stdin read error: {}", e);
        return;
    }

    run(&input_str);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_known_tool_names() {
        for name in ["Write", "Edit", "MultiEdit"] {
            let json = format!(r#"{{"tool_name": "{name}", "tool_input": {{}}}}"#);
            let input: HookInput = serde_json::from_str(&json).unwrap();
            assert_ne!(input.tool_name, ToolName::Other, "{name}");
        }
    }

    #[test]
    fn deserialize_unknown_tool_name() {
        let json = r#"{"tool_name": "Read", "tool_input": {}}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.tool_name, ToolName::Other);
    }

    #[test]
    fn deserialize_file_path() {
        let json = r#"{"tool_name": "Write", "tool_input": {"file_path": "src/app.ts"}}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.tool_input.file_path.as_deref(), Some("src/app.ts"));
    }

    #[test]
    fn deserialize_missing_file_path() {
        let json = r#"{"tool_name": "Write", "tool_input": {}}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(input.tool_input.file_path.is_none());
    }

    #[test]
    fn biome_extensions_subset_of_oxfmt() {
        for ext in biome::EXTENSIONS {
            assert!(
                oxfmt::EXTENSIONS.contains(ext),
                "biome extension '{}' not in oxfmt EXTENSIONS",
                ext
            );
        }
    }

    #[test]
    fn select_formatter_non_formattable_returns_none() {
        let config = Config::default();
        assert_eq!(select_formatter(&config, "Makefile"), None);
        assert_eq!(select_formatter(&config, "Dockerfile"), None);
    }

    #[test]
    fn select_formatter_all_disabled_returns_none() {
        let config = Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: false,
                oxfmt: false,
                eof_newline: true,
            },
            source: ConfigSource::Default,
            git_root: None,
        };
        assert_eq!(select_formatter(&config, "src/app.ts"), None);
    }

    #[test]
    fn select_formatter_oxfmt_disabled_never_selects_oxfmt() {
        let config = Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: true,
                oxfmt: false,
                eof_newline: true,
            },
            source: ConfigSource::Default,
            git_root: None,
        };
        assert_ne!(
            select_formatter(&config, "src/app.ts"),
            Some(Formatter::Oxfmt)
        );
    }

    #[test]
    fn select_formatter_oxfmt_only_extension() {
        let config = Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: true,
                oxfmt: false,
                eof_newline: true,
            },
            source: ConfigSource::Default,
            git_root: None,
        };
        // .yaml is oxfmt-only, biome doesn't support it
        assert_eq!(select_formatter(&config, "config.yaml"), None);
    }

    #[test]
    fn run_invalid_json_does_not_panic() {
        run("not valid json");
    }

    #[test]
    fn run_other_tool_skips() {
        run(r#"{"tool_name": "Read", "tool_input": {}}"#);
    }

    #[test]
    fn run_missing_file_path_skips() {
        run(r#"{"tool_name": "Write", "tool_input": {}}"#);
    }

    #[test]
    fn run_nonexistent_file_skips() {
        run(r#"{"tool_name": "Write", "tool_input": {"file_path": "/nonexistent/path.ts"}}"#);
    }

    fn make_config(source: ConfigSource, git_root: Option<std::path::PathBuf>) -> Config {
        Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: true,
                oxfmt: true,
                eof_newline: true,
            },
            source,
            git_root,
        }
    }

    // [T-008] source=Explicit -> show_config_hint is noop
    #[test]
    fn t_008_show_config_hint_skip_when_explicit_source() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = make_config(ConfigSource::Explicit, Some(tmp.path().to_path_buf()));
        show_config_hint(&config); // should not panic or print
    }

    // [T-009] source=Default with git_root -> show_config_hint outputs warning
    #[test]
    fn t_009_show_config_hint_when_default_with_git_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = make_config(ConfigSource::Default, Some(tmp.path().to_path_buf()));
        show_config_hint(&config); // should not panic
    }

    // [T-010] source=Default without git_root -> show_config_hint is noop
    #[test]
    fn t_010_show_config_hint_noop_without_git_root() {
        let config = make_config(ConfigSource::Default, None);
        show_config_hint(&config); // should not panic
    }

    // [T-017] show_config_hint with Default source and git_root does not panic
    #[test]
    fn t_017_show_config_hint_outputs_message() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = make_config(ConfigSource::Default, Some(tmp.path().to_path_buf()));
        show_config_hint(&config); // should not panic
    }
}
