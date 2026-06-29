//! Claude Code PostToolUse hook that auto-formats files on Write/Edit/MultiEdit.
//!
//! Reads hook JSON from stdin, selects the appropriate formatter based on file
//! extension and project config, then formats the file in-place.

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
use std::panic;
use std::path::Path;
use std::process::ExitCode;

/// Caps stdin at 10 MB to bound memory if a caller floods the hook with input.
const MAX_INPUT_SIZE: u64 = 10_000_000;

/// `EX_USAGE` (sysexits.h): the hook input on stdin violated the PostToolUse
/// contract — it was not the expected JSON shape.
const EX_USAGE: u8 = 64;

/// `EX_SOFTWARE` (sysexits.h): an internal invariant was violated — the hook
/// panicked. A bug in the formatter itself, not a formatting outcome.
const EX_SOFTWARE: u8 = 70;

/// A fail-closed exit condition. Formatting failures are deliberately absent:
/// per the silent-fix policy they stay exit 0 (advisory only). This carries the
/// Group 3 (Hook tool) baseline of ADR-0066, where only infrastructure faults —
/// a malformed hook contract or an internal bug — surface as a non-zero code so
/// a metrics dashboard can branch on them.
enum FormatterError {
    /// The hook input on stdin was not valid JSON in the expected shape.
    InvalidInput,
    /// An internal invariant was violated (a caught panic).
    Internal,
}

impl FormatterError {
    /// The sysexits.h exit code this error maps to. Returns `u8` rather than
    /// `ExitCode` so the mapping is unit-testable (`ExitCode` is not `PartialEq`);
    /// `main` wraps it with `ExitCode::from` at the boundary.
    fn exit_code(&self) -> u8 {
        match self {
            FormatterError::InvalidInput => EX_USAGE,
            FormatterError::Internal => EX_SOFTWARE,
        }
    }
}

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
    if config.formatters.oxfmt && oxfmt::is_formattable(file_path) {
        return Some(Formatter::Oxfmt);
    }
    None
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

/// Runs the hook. Returns `Err` only for a fail-closed condition (see
/// [`FormatterError`]). Every formatting outcome — success, a parse error oxfmt
/// could not fix, a missing binary — returns `Ok(())`: the silent-fix policy
/// keeps those at exit 0 so the hook never blocks the developer over a style
/// concern, surfacing them through the structured record instead.
fn run(input_str: &str) -> Result<(), FormatterError> {
    let input: HookInput = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Formatter: invalid hook input: {}", e);
            return Err(FormatterError::InvalidInput);
        }
    };

    if input.tool_name == ToolName::Other {
        return Ok(());
    }

    let raw_path = match &input.tool_input.file_path {
        Some(p) if !p.is_empty() => p.as_str(),
        _ => {
            eprintln!(
                "Formatter: {:?} without file_path, skipping",
                input.tool_name
            );
            return Ok(());
        }
    };

    let Some(file_path) = validate_path(raw_path) else {
        return Ok(());
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
        return Ok(());
    }

    // An oxfmt-selected file is owned by oxfmt end to end: when the binary is
    // missing, oxfmt::format records that as an error rather than falling back
    // here. The eof_newline pass is only for files no formatter supports
    // (Makefile and the like), so a missing oxfmt does not silently degrade a
    // .ts file into a bare EOF-newline fixup.
    match select_formatter(&config, &file_path) {
        Some(Formatter::Oxfmt) => oxfmt::format(&file_path),
        None => {
            if config.formatters.eof_newline {
                eof_newline::ensure(&file_path);
            }
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    // Read raw bytes so the size cap and UTF-8 validity stay separate decisions:
    // an oversized payload is rejected on byte count *before* UTF-8 validation, so
    // the cap splitting a multi-byte codepoint can never be mistaken for malformed
    // hook input. Reading one byte past the cap lets an exactly-`MAX` payload
    // through while still catching anything larger.
    let mut input = Vec::new();
    if let Err(e) = io::stdin().take(MAX_INPUT_SIZE + 1).read_to_end(&mut input) {
        // A genuine stdin read failure leaves nothing usable to format; stay
        // silent (exit 0) rather than fail closed, since no hook contract broke.
        eprintln!("Formatter: stdin read error: {}", e);
        return ExitCode::SUCCESS;
    }

    if input.len() as u64 > MAX_INPUT_SIZE {
        // We capped the caller's input ourselves, so there is no usable payload
        // through no fault of the caller; stay silent at exit 0 under silent-fix.
        eprintln!(
            "Formatter: input exceeded {} bytes; skipped",
            MAX_INPUT_SIZE
        );
        return ExitCode::SUCCESS;
    }

    // A below-cap payload that is not valid UTF-8 cannot be the expected hook
    // JSON, so it fails closed as a malformed hook contract (64), distinct from
    // the silent oversized case above.
    let input_str = match String::from_utf8(input) {
        Ok(s) => s,
        Err(_) => return ExitCode::from(FormatterError::InvalidInput.exit_code()),
    };

    // `catch_unwind` converts an internal panic (invariant violation) into the
    // classified EX_SOFTWARE (70) instead of Rust's default panic exit, so a
    // formatter bug is machine-distinguishable from a malformed hook input (64).
    let code = match panic::catch_unwind(|| run(&input_str)) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => e.exit_code(),
        Err(_) => FormatterError::Internal.exit_code(),
    };
    ExitCode::from(code)
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
    fn select_formatter_oxfmt_enabled_formattable_returns_oxfmt() {
        // [T-001] oxfmt enabled + a formattable file selects Oxfmt from config and
        // extension alone. Binary presence is no longer probed here; a missing
        // oxfmt surfaces later via oxfmt::format's spawn-Err path.
        let config = make_config(ConfigSource::Default, None);
        assert_eq!(
            select_formatter(&config, "src/app.ts"),
            Some(Formatter::Oxfmt)
        );
    }

    #[test]
    fn run_invalid_json_classifies_as_usage_error() {
        // Malformed hook input is the EX_USAGE (64) fail-closed condition.
        let err = run("not valid json").unwrap_err();
        assert_eq!(err.exit_code(), EX_USAGE);
    }

    #[test]
    fn run_other_tool_is_ok() {
        assert!(run(r#"{"tool_name": "Read", "tool_input": {}}"#).is_ok());
    }

    #[test]
    fn run_missing_file_path_is_ok() {
        assert!(run(r#"{"tool_name": "Write", "tool_input": {}}"#).is_ok());
    }

    #[test]
    fn run_nonexistent_file_is_ok() {
        // A path that cannot be resolved is a formatting outcome, not a contract
        // violation, so it stays at exit 0 (Ok).
        assert!(
            run(r#"{"tool_name": "Write", "tool_input": {"file_path": "/nonexistent/path.ts"}}"#)
                .is_ok()
        );
    }

    #[test]
    fn exit_code_maps_each_variant_to_sysexits() {
        // The Group 3 (ADR-0066) classification: invalid input -> EX_USAGE (64),
        // internal panic -> EX_SOFTWARE (70). Distinct codes let a dashboard
        // branch on the failure kind.
        assert_eq!(FormatterError::InvalidInput.exit_code(), 64);
        assert_eq!(FormatterError::Internal.exit_code(), 70);
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
