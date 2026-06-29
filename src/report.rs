//! Structured, opt-in action reporting for agent-friendly observability.
//!
//! Emits one JSON line per file to stderr when `FORMATTER_VERBOSE=1` or in
//! dry-run mode. Default operation stays silent so the PostToolUse hook keeps
//! the "format without the developer noticing" behavior; the structured record
//! is what lets an agent see which formatter touched which file and how.

use serde::Serialize;
use std::env;
use std::ffi::OsString;
use std::sync::LazyLock;

/// Treats only `1` and `true` as on, mirroring common env-flag conventions.
fn parse_flag(value: Option<OsString>) -> bool {
    value.is_some_and(|v| v == "1" || v == "true")
}

/// `FORMATTER_DRY_RUN=1` reports what would change without writing any file.
pub fn dry_run() -> bool {
    static DRY_RUN: LazyLock<bool> = LazyLock::new(|| parse_flag(env::var_os("FORMATTER_DRY_RUN")));
    *DRY_RUN
}

/// `FORMATTER_VERBOSE=1` emits the structured record on every formatted file.
fn verbose() -> bool {
    static VERBOSE: LazyLock<bool> = LazyLock::new(|| parse_flag(env::var_os("FORMATTER_VERBOSE")));
    *VERBOSE
}

#[derive(Serialize)]
struct Report<'a> {
    file: &'a str,
    formatter: &'a str,
    action: &'a str,
}

/// One JSON line describing what happened to `file`, e.g.
/// `{"file":"/p/app.ts","formatter":"oxfmt","action":"formatted"}`.
fn render(file: &str, formatter: &str, action: &str) -> String {
    let report = Report {
        file,
        formatter,
        action,
    };
    serde_json::to_string(&report).unwrap_or_default()
}

/// Emit a structured action record to stderr.
///
/// Always emits in dry-run mode (its whole purpose is to report); otherwise only
/// when `FORMATTER_VERBOSE=1`. Stays on stderr to leave stdout free for hook
/// control JSON, and never errors out — reporting must not break formatting.
pub fn emit(file: &str, formatter: &str, action: &str) {
    if !verbose() && !dry_run() {
        return;
    }
    eprintln!("{}", render(file, formatter, action));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flag_accepts_one_and_true() {
        assert!(parse_flag(Some(OsString::from("1"))));
        assert!(parse_flag(Some(OsString::from("true"))));
    }

    #[test]
    fn parse_flag_rejects_other_values() {
        assert!(!parse_flag(None));
        assert!(!parse_flag(Some(OsString::from("0"))));
        assert!(!parse_flag(Some(OsString::from("yes"))));
        assert!(!parse_flag(Some(OsString::from(""))));
    }

    #[test]
    fn render_emits_expected_keys_and_values() {
        let line = render("/path/to/app.ts", "oxfmt", "formatted");
        assert_eq!(
            line,
            r#"{"file":"/path/to/app.ts","formatter":"oxfmt","action":"formatted"}"#
        );
    }

    #[test]
    fn render_escapes_special_characters_in_path() {
        // A path containing a quote and backslash must produce valid JSON, not a
        // broken line an agent cannot parse.
        let line = render(r#"/a"b\c.ts"#, "oxfmt", "would-format");
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["file"], r#"/a"b\c.ts"#);
        assert_eq!(parsed["action"], "would-format");
    }
}
