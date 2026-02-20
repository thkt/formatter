mod biome;
mod config;
mod oxfmt;
mod resolve;

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

fn main() {
    let config = Config::load();
    if !config.enabled {
        return;
    }

    let mut input_str = String::new();
    if let Err(e) = io::stdin()
        .take(MAX_INPUT_SIZE)
        .read_to_string(&mut input_str)
    {
        eprintln!("formatter: stdin read error: {}", e);
        return;
    }

    let input: HookInput = match serde_json::from_str(&input_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("formatter: invalid hook input: {}", e);
            return;
        }
    };

    if input.tool_name == ToolName::Other {
        return;
    }

    let file_path = match &input.tool_input.file_path {
        Some(p) if !p.is_empty() => p.as_str(),
        _ => {
            eprintln!(
                "formatter: {:?} without file_path, skipping",
                input.tool_name
            );
            return;
        }
    };

    match select_formatter(&config, file_path) {
        Some(Formatter::Oxfmt) => oxfmt::format(file_path),
        Some(Formatter::Biome) => biome::format(file_path),
        None => {}
    }
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

    fn config_both_enabled() -> Config {
        Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: true,
                oxfmt: true,
            },
        }
    }

    #[test]
    fn select_formatter_non_formattable_returns_none() {
        let config = config_both_enabled();
        assert_eq!(select_formatter(&config, "src/main.rs"), None);
        assert_eq!(select_formatter(&config, "Makefile"), None);
        assert_eq!(select_formatter(&config, "Dockerfile"), None);
    }

    #[test]
    fn select_formatter_both_disabled_returns_none() {
        let config = Config {
            enabled: true,
            formatters: config::FormattersConfig {
                biome: false,
                oxfmt: false,
            },
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
            },
        };
        // .yaml is oxfmt-only, biome doesn't support it
        assert_eq!(select_formatter(&config, "config.yaml"), None);
    }
}
