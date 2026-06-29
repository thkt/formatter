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

pub fn is_available(file_path: &str) -> bool {
    Command::new(resolve_bin("oxfmt", file_path))
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
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

/// The diagnostic notes for a failed `oxfmt` run: the first meaningful line of
/// its stderr, or the exit status when stderr is blank. Shared so the check and
/// write paths report failures alike. oxfmt prefixes its diagnostic with a blank
/// line, so the first non-blank line is taken rather than `lines().next()`
/// (which would be empty).
fn failure_notes(o: &Output) -> Vec<String> {
    let stderr = String::from_utf8_lossy(&o.stderr);
    match first_diagnostic(&stderr) {
        Some(line) => vec![line.to_owned()],
        None => vec![format!("oxfmt exited with {}", o.status)],
    }
}

/// The first non-blank line of formatter output, or `None` when it is all blank.
fn first_diagnostic(stderr: &str) -> Option<&str> {
    stderr.lines().find(|line| !line.trim().is_empty())
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
    fn first_diagnostic_skips_leading_blank_lines() {
        // oxfmt prefixes its diagnostic with a blank line; the reported message
        // must be the first line with real content, not that blank.
        assert_eq!(
            first_diagnostic("\n  x Unexpected token\n  more"),
            Some("  x Unexpected token")
        );
        assert_eq!(first_diagnostic(""), None);
        assert_eq!(first_diagnostic("\n  \n"), None);
    }

    #[test]
    fn failure_notes_extracts_diagnostic_from_parse_error() {
        // A real oxfmt parse failure must land its diagnostic in the notes so an
        // agent reading the JSON record sees the cause without parsing stderr.
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
        assert_eq!(notes.len(), 1, "expected one diagnostic note");
        assert!(
            !notes[0].trim().is_empty(),
            "note must carry the diagnostic, got: {notes:?}"
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
