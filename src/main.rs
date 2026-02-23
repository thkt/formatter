//! Claude Code PostToolUse hook that auto-formats files on Write/Edit/MultiEdit.
//!
//! Reads hook JSON from stdin, selects the appropriate formatter based on file
//! extension and availability, then formats the file in-place.

mod biome;
mod config;
mod eof_newline;
mod oxfmt;
mod resolve;
mod rustfmt;

use config::Config;
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
    Rustfmt,
}

/// Priority: rustfmt (.rs only) > oxfmt (broad, faster) > biome (fallback).
fn select_formatter(config: &Config, file_path: &str) -> Option<Formatter> {
    if config.formatters.rustfmt
        && rustfmt::is_formattable(file_path)
        && rustfmt::is_available(file_path)
    {
        return Some(Formatter::Rustfmt);
    }
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

fn run(input_str: &str) {
    let input: HookInput = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("formatter: invalid hook input: {}", e);
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
                "formatter: {:?} without file_path, skipping",
                input.tool_name
            );
            return;
        }
    };

    let canonical = match std::path::Path::new(raw_path).canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("formatter: cannot resolve path {}: {}", raw_path, e);
            return;
        }
    };

    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("formatter: cannot determine CWD: {}", e);
            return;
        }
    };
    if !canonical.starts_with(&cwd) {
        eprintln!("formatter: file outside project directory, skipping");
        return;
    }

    let Some(file_path) = canonical.to_str() else {
        eprintln!("formatter: non-UTF-8 path, skipping");
        return;
    };

    let config = Config::default().with_project_overrides(file_path);
    if !config.enabled {
        eprintln!("formatter: disabled by project config, skipping");
        return;
    }

    match select_formatter(&config, file_path) {
        Some(Formatter::Oxfmt) => oxfmt::format(file_path),
        Some(Formatter::Biome) => biome::format(file_path),
        Some(Formatter::Rustfmt) => rustfmt::format(file_path),
        None => {
            if rustfmt::is_formattable(file_path)
                || oxfmt::is_formattable(file_path)
                || biome::is_formattable(file_path)
            {
                eprintln!(
                    "formatter: supported file but no formatter available: {}",
                    file_path
                );
            }
            if config.formatters.eof_newline {
                eof_newline::ensure(file_path);
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
        eprintln!("formatter: stdin read error: {}", e);
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

    fn config_all_enabled() -> Config {
        Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: true,
                oxfmt: true,
                rustfmt: true,
                eof_newline: true,
            },
        }
    }

    #[test]
    fn select_formatter_non_formattable_returns_none() {
        let config = config_all_enabled();
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
                rustfmt: false,
                eof_newline: true,
            },
        };
        assert_eq!(select_formatter(&config, "src/app.ts"), None);
        assert_eq!(select_formatter(&config, "src/main.rs"), None);
    }

    #[test]
    fn select_formatter_oxfmt_disabled_never_selects_oxfmt() {
        let config = Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: true,
                oxfmt: false,
                rustfmt: true,
                eof_newline: true,
            },
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
                rustfmt: true,
                eof_newline: true,
            },
        };
        // .yaml is oxfmt-only, biome doesn't support it
        assert_eq!(select_formatter(&config, "config.yaml"), None);
    }

    #[test]
    fn select_formatter_rs_selects_rustfmt() {
        let config = config_all_enabled();
        assert_eq!(
            select_formatter(&config, "src/main.rs"),
            Some(Formatter::Rustfmt)
        );
    }

    #[test]
    fn select_formatter_rustfmt_disabled_returns_none_for_rs() {
        let config = Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: true,
                oxfmt: true,
                rustfmt: false,
                eof_newline: true,
            },
        };
        assert_eq!(select_formatter(&config, "src/main.rs"), None);
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
}
