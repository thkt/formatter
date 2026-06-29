//! oxfmt formatter integration (broad language support including markup/config).

use crate::report;
use crate::resolve::{has_extension, resolve_bin};
use std::path::Path;
use std::process::{Command, Output};

/// Remediation for a file `oxfmt` could not format (parse error / exit 2):
/// formatting cannot fix invalid syntax, so the human must.
const PARSE_FAIL_NEXT_STEP: &str =
    "fix the reported error before saving; the file was left unformatted";

/// Remediation for a failure to even run `oxfmt` (spawn error): the binary is
/// missing or not executable on this path.
const EXEC_FAIL_NEXT_STEP: &str = "ensure oxfmt is installed and on PATH";

pub const EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "json", "jsonc", "json5", "css", "scss",
    "less", "html", "vue", "yaml", "yml", "toml", "md", "mdx", "graphql", "gql",
];

pub fn is_formattable(path: &str) -> bool {
    has_extension(path, EXTENSIONS)
}

pub fn format(file_path: &str) {
    let oxfmt = resolve_bin("oxfmt", file_path);
    apply(&oxfmt, file_path, report::dry_run());
}

/// Dispatches to a non-mutating check or an in-place write. The `dry_run` flag is
/// passed in (rather than read from the environment here) so both paths are
/// testable without toggling process env, which is `unsafe` under edition 2024.
fn apply(oxfmt: &Path, file_path: &str, dry_run: bool) {
    if dry_run {
        check(oxfmt, file_path);
    } else {
        write(oxfmt, file_path);
    }
}

/// Reports whether the file would change, using `oxfmt --check`, without writing.
///
/// `oxfmt --check` (0.56.0) exits 0 when already formatted, 1 when the file
/// would change, and 2 on a parse error or missing target. The exit-2 case must
/// surface as `error`, not `would-format`: code that fails to parse mid-edit
/// would not be fixed by formatting, and its diagnostic must not be swallowed.
fn check(oxfmt: &Path, file_path: &str) {
    match Command::new(oxfmt).arg("--check").arg(file_path).output() {
        Ok(o) => {
            let action = classify_check(o.status.code());
            if action == "error" {
                report::emit_degraded(
                    file_path,
                    "oxfmt",
                    "error",
                    PARSE_FAIL_NEXT_STEP,
                    &failure_notes(&o),
                );
            } else {
                report::emit(file_path, "oxfmt", action);
            }
        }
        Err(e) => {
            report::emit_degraded(
                file_path,
                "oxfmt",
                "error",
                EXEC_FAIL_NEXT_STEP,
                &[e.to_string()],
            );
        }
    }
}

/// Maps an `oxfmt --check` exit code to a structured action. A killed-by-signal
/// process (`None`) is treated as an error rather than a formatting verdict.
fn classify_check(code: Option<i32>) -> &'static str {
    match code {
        Some(0) => "unchanged",
        Some(1) => "would-format",
        _ => "error",
    }
}

/// Formats the file in place. `oxfmt` does not report whether it changed bytes,
/// so a success is recorded as `formatted` regardless of whether it was a no-op.
fn write(oxfmt: &Path, file_path: &str) {
    match Command::new(oxfmt).arg(file_path).output() {
        Ok(o) if o.status.success() => report::emit(file_path, "oxfmt", "formatted"),
        Ok(o) => report::emit_degraded(
            file_path,
            "oxfmt",
            "error",
            PARSE_FAIL_NEXT_STEP,
            &failure_notes(&o),
        ),
        Err(e) => report::emit_degraded(
            file_path,
            "oxfmt",
            "error",
            EXEC_FAIL_NEXT_STEP,
            &[e.to_string()],
        ),
    }
}

/// Max diagnostic lines kept in `notes`. An oxfmt parse error is a miette-style
/// block — the error line, the `,-[file:line:col]` location, the offending
/// source line, and a caret (~5 lines) — so the cap leaves headroom while
/// bounding how much stderr a multi-file run can dump into the record.
const MAX_DIAGNOSTIC_LINES: usize = 10;

/// Total byte budget for the kept diagnostic. The line cap alone cannot bound a
/// single pathological line (a minified source preview echoed by oxfmt), so a
/// line is dropped once it would exceed the remaining budget — keeping the
/// earlier error and location lines rather than splitting one mid-way.
const MAX_DIAGNOSTIC_BYTES: usize = 1024;

/// The diagnostic notes for a failed `oxfmt` run: the diagnostic block from its
/// stderr, or the exit status when stderr is blank. Shared so the check and
/// write paths report failures alike. The block spans multiple lines (error +
/// `,-[file:line:col]` location + source + caret); each becomes its own note so
/// an agent reading the record keeps the error position, not just its first
/// line, and the human renderer can indent the lines independently.
fn failure_notes(o: &Output) -> Vec<String> {
    let stderr = String::from_utf8_lossy(&o.stderr);
    let lines = diagnostic_lines(&stderr);
    if lines.is_empty() {
        vec![format!("oxfmt exited with {}", o.status)]
    } else {
        lines
    }
}

/// The diagnostic block of formatter stderr: the run of lines starting at the
/// first non-blank line (oxfmt prefixes its diagnostic with a blank line),
/// capped at [`MAX_DIAGNOSTIC_LINES`] and [`MAX_DIAGNOSTIC_BYTES`] so the error,
/// its location, and the caret survive without flooding `notes`. Empty when the
/// stderr is all blank.
fn diagnostic_lines(stderr: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut budget = MAX_DIAGNOSTIC_BYTES;
    for line in stderr
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .take(MAX_DIAGNOSTIC_LINES)
    {
        if line.len() > budget {
            break;
        }
        budget -= line.len();
        lines.push(line.to_owned());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn formattable_extensions() {
        for ext in [
            "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "json", "jsonc", "json5", "css",
            "scss", "less", "html", "vue", "yaml", "yml", "toml", "md", "mdx", "graphql", "gql",
        ] {
            assert!(is_formattable(&format!("src/app.{ext}")), "{ext}");
        }
    }

    #[test]
    fn non_formattable() {
        for path in ["src/main.rs", ".env", "Dockerfile", "Makefile"] {
            assert!(!is_formattable(path), "{path}");
        }
    }

    #[test]
    fn dotfile_not_formattable() {
        assert!(!is_formattable("/tmp/.css"));
        assert!(!is_formattable("/tmp/.toml"));
        assert!(!is_formattable(".md"));
    }

    #[test]
    fn multiple_dots_formattable() {
        assert!(is_formattable("src/app.test.ts"));
        assert!(is_formattable("config.prod.yaml"));
    }

    #[test]
    fn format_nonexistent_file_does_not_panic() {
        format("/nonexistent/path/to/file.ts");
    }

    #[test]
    fn check_reports_degraded_when_oxfmt_binary_is_missing() {
        // A missing oxfmt binary makes Command::output return Err, which must
        // surface as a degraded record (EXEC_FAIL_NEXT_STEP) rather than panic.
        check(Path::new("/nonexistent/oxfmt"), "src/app.ts");
    }

    #[test]
    fn write_reports_degraded_when_oxfmt_binary_is_missing() {
        write(Path::new("/nonexistent/oxfmt"), "src/app.ts");
    }

    fn oxfmt_available() -> bool {
        Command::new("oxfmt")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[test]
    fn dry_run_does_not_modify_unformatted_file() {
        use std::fs;
        use tempfile::TempDir;

        if !oxfmt_available() {
            eprintln!("oxfmt not available, skipping");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.json");
        let unformatted = "{  \"a\":1,  \"b\"  :2  }\n";
        fs::write(&file, unformatted).unwrap();

        // dry_run=true must leave the on-disk bytes untouched even though the
        // file is not formatted (this is the C-07 preview guarantee).
        apply(&PathBuf::from("oxfmt"), file.to_str().unwrap(), true);

        assert_eq!(fs::read_to_string(&file).unwrap(), unformatted);
    }

    #[test]
    fn diagnostic_lines_skips_leading_blanks_and_keeps_the_block() {
        // oxfmt prefixes its diagnostic with a blank line; the leading blank is
        // dropped and every following line of the block is kept as its own
        // entry, so the location and caret survive alongside the error line.
        assert_eq!(
            diagnostic_lines("\n  x Unexpected token\n  more"),
            vec!["  x Unexpected token", "  more"]
        );
        assert!(diagnostic_lines("").is_empty());
        assert!(diagnostic_lines("\n  \n").is_empty());
    }

    #[test]
    fn diagnostic_lines_caps_line_count() {
        // A formatter that floods stderr must not flood notes: at most
        // MAX_DIAGNOSTIC_LINES lines are kept.
        let flood: String = (0..30).map(|i| format!("line {i}\n")).collect();
        assert_eq!(diagnostic_lines(&flood).len(), MAX_DIAGNOSTIC_LINES);
    }

    #[test]
    fn diagnostic_lines_drops_a_line_over_the_byte_budget() {
        // A single pathological line (a minified source preview) is dropped at
        // the byte budget, but the earlier error and location lines are kept.
        let huge = "x".repeat(MAX_DIAGNOSTIC_BYTES + 1);
        let stderr = format!("  x error\n  ,-[a.ts:1:1]\n{huge}\n  `----");
        let notes = diagnostic_lines(&stderr);
        assert_eq!(notes, vec!["  x error", "  ,-[a.ts:1:1]"]);
    }

    #[test]
    fn diagnostic_lines_accumulates_byte_budget_across_lines() {
        // The budget spans the whole block, not each line: two lines that each
        // fit alone but together exceed it keep only the first. This pins the
        // per-line decrement — pinning the budget at its full value would
        // wrongly keep both.
        let half = "y".repeat(MAX_DIAGNOSTIC_BYTES * 2 / 3);
        let stderr = format!("{half}\n{half}");
        assert_eq!(diagnostic_lines(&stderr), vec![half]);
    }

    #[test]
    fn failure_notes_preserves_location_from_parse_error() {
        // A real oxfmt parse failure must land its full diagnostic in notes so
        // an agent sees the error position, not just the first line — the whole
        // point of keeping multiple lines.
        use std::fs;
        use tempfile::TempDir;

        if !oxfmt_available() {
            eprintln!("oxfmt not available, skipping");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("broken.json");
        fs::write(&file, "{ \"a\": ").unwrap();
        let out = Command::new("oxfmt")
            .arg("--check")
            .arg(file.to_str().unwrap())
            .output()
            .unwrap();

        let notes = failure_notes(&out);
        assert!(
            notes.len() > 1,
            "expected the multi-line block, got: {notes:?}"
        );
        assert!(
            notes.iter().any(|n| n.contains("broken.json")),
            "notes must carry the `,-[file:line:col]` location, got: {notes:?}"
        );
    }

    #[test]
    fn failure_notes_falls_back_to_status_when_stderr_blank() {
        // A non-zero exit with no stderr (here `false`) must still yield a note,
        // naming the exit status rather than an empty string.
        let Ok(out) = Command::new("false").output() else {
            eprintln!("`false` not available, skipping");
            return;
        };

        let notes = failure_notes(&out);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("oxfmt exited with"), "got: {notes:?}");
    }

    #[test]
    fn classify_check_maps_exit_codes() {
        // 0=already formatted, 1=would change, 2=parse error/no target,
        // None=killed by signal. Only 0 and 1 are formatting verdicts; the rest
        // must report error so a parse failure is not mistaken for would-format.
        assert_eq!(classify_check(Some(0)), "unchanged");
        assert_eq!(classify_check(Some(1)), "would-format");
        assert_eq!(classify_check(Some(2)), "error");
        assert_eq!(classify_check(None), "error");
    }

    #[test]
    fn dry_run_reports_error_not_would_format_on_parse_failure() {
        use std::fs;
        use tempfile::TempDir;

        if !oxfmt_available() {
            eprintln!("oxfmt not available, skipping");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("broken.json");
        // Syntactically broken JSON makes oxfmt --check exit 2, which must not
        // be classified as would-format and must leave the file untouched.
        let broken = "{ \"a\": ";
        fs::write(&file, broken).unwrap();

        let out = Command::new("oxfmt")
            .arg("--check")
            .arg(file.to_str().unwrap())
            .output()
            .unwrap();
        assert_eq!(classify_check(out.status.code()), "error");

        apply(&PathBuf::from("oxfmt"), file.to_str().unwrap(), true);
        assert_eq!(fs::read_to_string(&file).unwrap(), broken);
    }

    #[test]
    fn apply_write_formats_file() {
        use std::fs;
        use tempfile::TempDir;

        if !oxfmt_available() {
            eprintln!("oxfmt not available, skipping");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.json");
        fs::write(&file, "{  \"a\":1  }\n").unwrap();

        apply(&PathBuf::from("oxfmt"), file.to_str().unwrap(), false);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("\"a\": 1"), "got: {content}");
    }

    #[test]
    fn format_fixes_json() {
        use std::fs;
        use tempfile::TempDir;

        let available = Command::new("oxfmt")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !available {
            eprintln!("oxfmt not available, skipping");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.json");
        fs::write(&file, "{  \"a\":1,  \"b\"  :2  }\n").unwrap();

        format(file.to_str().unwrap());

        let content = fs::read_to_string(&file).unwrap();
        assert!(
            content.contains("\"a\": 1") || content.contains("\"a\":1"),
            "Expected formatted JSON, got: {content}"
        );
    }
}
