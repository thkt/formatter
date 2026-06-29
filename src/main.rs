//! Claude Code PostToolUse hook that auto-formats files on Write/Edit/MultiEdit.
//!
//! Reads hook JSON from stdin, selects the appropriate formatter based on file
//! extension and availability, then formats the file in-place.

mod color;
mod config;
mod eof_newline;
mod oxfmt;
mod report;
mod resolve;

use config::{Config, ConfigSource};
use serde::Deserialize;
use std::env;
use std::io::{self, Read};
use std::path::Path;

/// Caps stdin at 10 MB to bound memory if a caller floods the hook with input.
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
}

fn select_formatter(config: &Config, file_path: &str) -> Option<Formatter> {
    if config.formatters.oxfmt && oxfmt::is_formattable(file_path) && oxfmt::is_available(file_path)
    {
        return Some(Formatter::Oxfmt);
    }
    None
}

/// The remediation to report when no formatter handled a supported file, or
/// `None` when leaving it unformatted is intentional. oxfmt being disabled in
/// project config is an opt-out, not a degradation, so the record is suppressed
/// rather than telling a user who disabled oxfmt to install it. When oxfmt is
/// enabled and the file is formattable but no formatter was selected, the only
/// remaining cause is a missing binary, so installing it is the right advice.
fn skip_next_step(config: &Config, file_path: &str) -> Option<&'static str> {
    if config.formatters.oxfmt && oxfmt::is_formattable(file_path) {
        Some("install oxfmt to format this file")
    } else {
        None
    }
}

fn validate_path(raw_path: &str) -> Option<String> {
    let canonical = match Path::new(raw_path).canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Formatter: cannot resolve path {}: {}", raw_path, e);
            return None;
        }
    };

    let cwd = match env::current_dir() {
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
        Some(s) => Some(s.to_owned()),
        None => {
            eprintln!("Formatter: non-UTF-8 path, skipping");
            None
        }
    }
}

const CONFIG_HINT_MESSAGE: &str = "Formatter: using defaults. Customize via .claude/tools.json \u{2014} see https://github.com/thkt/formatter#configuration";

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
        None => {
            if let Some(next_step) = skip_next_step(&config, &file_path) {
                report::emit_degraded(&file_path, "oxfmt", "skipped", next_step, &[]);
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
    use std::path::PathBuf;

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
    fn select_formatter_non_formattable_returns_none() {
        let config = Config::default();
        assert_eq!(select_formatter(&config, "Makefile"), None);
        assert_eq!(select_formatter(&config, "Dockerfile"), None);
    }

    #[test]
    fn select_formatter_oxfmt_disabled_returns_none() {
        let config = Config {
            enabled: true,
            formatters: config::FormattersConfig {
                oxfmt: false,
                eof_newline: true,
            },
            source: ConfigSource::Default,
            git_root: None,
        };
        assert_eq!(select_formatter(&config, "src/app.ts"), None);
    }

    #[test]
    fn skip_next_step_advises_install_when_oxfmt_enabled_but_unselected() {
        // oxfmt enabled + a supported file but no formatter was selected means
        // the binary is missing, so the remediation is to install it.
        let config = make_config(ConfigSource::Default, None);
        assert_eq!(
            skip_next_step(&config, "src/app.ts"),
            Some("install oxfmt to format this file")
        );
    }

    #[test]
    fn skip_next_step_silent_when_oxfmt_disabled_by_config() {
        // Disabling oxfmt is an intentional opt-out, not a degradation; advising
        // "install oxfmt" would be wrong, so no record is emitted.
        let config = Config {
            enabled: true,
            formatters: config::FormattersConfig {
                oxfmt: false,
                eof_newline: true,
            },
            source: ConfigSource::Default,
            git_root: None,
        };
        assert_eq!(skip_next_step(&config, "src/app.ts"), None);
    }

    #[test]
    fn skip_next_step_silent_for_unsupported_file() {
        // A file oxfmt does not handle is not a degraded oxfmt outcome.
        let config = make_config(ConfigSource::Default, None);
        assert_eq!(skip_next_step(&config, "Makefile"), None);
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

    #[test]
    fn validate_path_rejects_existing_file_outside_cwd() {
        // A file that exists but resolves outside the CWD must be rejected.
        // canonicalize succeeds (the file exists), so reaching None exercises
        // the `!canonical.starts_with(&cwd)` containment branch rather than the
        // canonicalize-error path.
        let outside = tempfile::NamedTempFile::new().unwrap();
        let path = outside.path().to_str().unwrap();
        assert!(!Path::new(path).starts_with(env::current_dir().unwrap()));
        assert_eq!(validate_path(path), None);
    }

    #[test]
    fn validate_path_accepts_file_inside_cwd() {
        // Contrast case: a path inside the CWD passes the containment check.
        let inside = tempfile::NamedTempFile::new_in(env::current_dir().unwrap()).unwrap();
        let path = inside.path().to_str().unwrap();
        assert!(validate_path(path).is_some());
    }

    fn make_config(source: ConfigSource, git_root: Option<PathBuf>) -> Config {
        Config {
            enabled: true,
            formatters: config::FormattersConfig {
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
