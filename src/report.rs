//! Structured, agent-friendly action reporting for observability.
//!
//! Two kinds of records reach stderr:
//!
//! * Neutral outcomes (`formatted`, `unchanged`, `would-format`) go through
//!   [`emit`] and surface only when `FORMATTER_VERBOSE=1` or in dry-run, so the
//!   PostToolUse hook keeps its "format without the developer noticing" default.
//! * Degraded outcomes (a supported file left unformatted, an `oxfmt` failure, a
//!   write error) go through [`emit_degraded`]. These already broke silence
//!   before, so they always surface — as one JSON line under
//!   `FORMATTER_VERBOSE`/dry-run (carrying `degraded`/`next_step`/`notes` so an
//!   agent never has to parse free-form stderr), or as a human-readable line
//!   otherwise.

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

/// True when the agent-readable JSON form is wanted over the human-readable one.
fn json_mode() -> bool {
    verbose() || dry_run()
}

#[derive(Serialize)]
struct Report<'a> {
    file: &'a str,
    formatter: &'a str,
    action: &'a str,
    /// Present (and `true`) only for degraded outcomes, so neutral records keep
    /// their original three-key shape and consumers can branch on one flag.
    #[serde(skip_serializing_if = "not_degraded")]
    degraded: bool,
    /// The single recommended remediation, e.g. "install oxfmt". Omitted when
    /// there is nothing actionable to suggest.
    #[serde(skip_serializing_if = "Option::is_none")]
    next_step: Option<&'a str>,
    /// Factual observations behind the degradation (a parser diagnostic, an I/O
    /// error). Distinct from `next_step`: notes describe, next_step prescribes.
    #[serde(skip_serializing_if = "notes_empty")]
    notes: &'a [String],
}

/// `skip_serializing_if` predicates. serde hands each the field by reference, so
/// the bool predicate takes `&bool` and the slice predicate takes `&&[String]`.
fn not_degraded(degraded: &bool) -> bool {
    !*degraded
}

fn notes_empty(notes: &&[String]) -> bool {
    notes.is_empty()
}

/// One JSON line, e.g.
/// `{"file":"/p/app.ts","formatter":"oxfmt","action":"formatted"}` for a neutral
/// record, or with `degraded`/`next_step`/`notes` appended for a degraded one.
fn render_json(report: &Report) -> String {
    serde_json::to_string(report).unwrap_or_default()
}

/// One human-readable line, with each note indented on its own line, e.g.
/// `Formatter: oxfmt error /p/app.ts — fix the syntax error before saving`.
fn render_human(report: &Report) -> String {
    let mut line = format!(
        "Formatter: {} {} {}",
        report.formatter, report.action, report.file
    );
    if let Some(step) = report.next_step {
        line.push_str(" \u{2014} ");
        line.push_str(step);
    }
    for note in report.notes {
        line.push_str("\n  ");
        line.push_str(note);
    }
    line
}

/// Emit a neutral action record to stderr.
///
/// Only when `FORMATTER_VERBOSE=1` or in dry-run; the default path stays silent
/// so success goes unnoticed. Always on stderr to leave stdout free for hook
/// control JSON, and never errors out — reporting must not break formatting.
pub fn emit(file: &str, formatter: &str, action: &str) {
    if !json_mode() {
        return;
    }
    let report = Report {
        file,
        formatter,
        action,
        degraded: false,
        next_step: None,
        notes: &[],
    };
    eprintln!("{}", render_json(&report));
}

/// Emit a degraded action record to stderr: a supported file the hook could not
/// fully format. `next_step` names the remediation; `notes` carries the raw
/// diagnostics. Always surfaces — JSON under `FORMATTER_VERBOSE`/dry-run (so an
/// agent reads `degraded`/`next_step`/`notes` without parsing stderr), human
/// text otherwise.
pub fn emit_degraded(file: &str, formatter: &str, action: &str, next_step: &str, notes: &[String]) {
    let report = Report {
        file,
        formatter,
        action,
        degraded: true,
        next_step: Some(next_step),
        notes,
    };
    if json_mode() {
        eprintln!("{}", render_json(&report));
    } else {
        eprintln!("{}", render_human(&report));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral<'a>(file: &'a str, formatter: &'a str, action: &'a str) -> Report<'a> {
        Report {
            file,
            formatter,
            action,
            degraded: false,
            next_step: None,
            notes: &[],
        }
    }

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
    fn neutral_record_keeps_three_key_shape() {
        // A neutral outcome must serialize to exactly the original three keys so
        // existing JSON consumers are unaffected; the agent-friendly fields are
        // skipped, not emitted empty.
        let line = render_json(&neutral("/path/to/app.ts", "oxfmt", "formatted"));
        assert_eq!(
            line,
            r#"{"file":"/path/to/app.ts","formatter":"oxfmt","action":"formatted"}"#
        );
    }

    #[test]
    fn degraded_record_carries_next_step_and_notes() {
        // A degraded outcome adds degraded=true, the next_step hint, and the
        // notes array so an agent can act without parsing free-form stderr.
        let notes = [String::from("x Unexpected token")];
        let report = Report {
            file: "/p/app.ts",
            formatter: "oxfmt",
            action: "error",
            degraded: true,
            next_step: Some("fix the syntax error before saving"),
            notes: &notes,
        };
        let parsed: serde_json::Value = serde_json::from_str(&render_json(&report)).unwrap();
        assert_eq!(parsed["degraded"], true);
        assert_eq!(parsed["next_step"], "fix the syntax error before saving");
        assert_eq!(parsed["notes"][0], "x Unexpected token");
    }

    #[test]
    fn degraded_record_omits_empty_notes() {
        // With no diagnostics, notes must be absent (not an empty array) so the
        // record stays minimal.
        let report = Report {
            file: "/p/app.ts",
            formatter: "oxfmt",
            action: "skipped",
            degraded: true,
            next_step: Some("install oxfmt"),
            notes: &[],
        };
        let parsed: serde_json::Value = serde_json::from_str(&render_json(&report)).unwrap();
        assert!(parsed.get("notes").is_none(), "notes should be absent");
        assert_eq!(parsed["next_step"], "install oxfmt");
    }

    #[test]
    fn json_escapes_special_characters_in_path() {
        // A path with a quote and backslash must produce valid JSON, not a
        // broken line an agent cannot parse.
        let line = render_json(&neutral(r#"/a"b\c.ts"#, "oxfmt", "would-format"));
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["file"], r#"/a"b\c.ts"#);
        assert_eq!(parsed["action"], "would-format");
    }

    #[test]
    fn human_record_shows_next_step_and_indented_notes() {
        // The human form names the remediation after an em dash and puts each
        // note on its own indented line.
        let notes = [String::from("x Unexpected token")];
        let report = Report {
            file: "/p/app.ts",
            formatter: "oxfmt",
            action: "error",
            degraded: true,
            next_step: Some("fix the syntax error before saving"),
            notes: &notes,
        };
        assert_eq!(
            render_human(&report),
            "Formatter: oxfmt error /p/app.ts \u{2014} fix the syntax error before saving\n  x Unexpected token"
        );
    }
}
