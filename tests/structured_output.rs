//! C-10 structured output and C-07 dry-run are opt-in stderr contracts that
//! only manifest end-to-end through the built binary: the env flags are read
//! once via `LazyLock`, and `std::env::set_var` is `unsafe` under edition 2024
//! (`unsafe_code = "forbid"`), so a unit test cannot toggle them in-process.
//! These tests drive a real process to assert the JSON line reaches stderr.
//!
//! The `.txt` fixture is not an oxfmt-supported type, so it routes straight to
//! `eof-newline`. That keeps every assertion deterministic and independent of
//! whether oxfmt is installed on the runner.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Run the formatter binary with `stdin` from `cwd` and the given env vars,
/// returning captured stderr.
fn run_capturing_stderr(stdin: &str, cwd: &Path, env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_formatter"));
    cmd.current_dir(cwd);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    String::from_utf8(output.stderr).unwrap()
}

fn hook_input(file: &Path) -> String {
    format!(
        r#"{{"tool_name": "Write", "tool_input": {{"file_path": "{}"}}}}"#,
        file.to_str().unwrap()
    )
}

/// The single JSON report line in stderr (the only line carrying a `formatter`
/// key), parsed. Other stderr lines are human-facing diagnostics.
fn report_line(stderr: &str) -> serde_json::Value {
    let line = stderr
        .lines()
        .find(|l| l.contains("\"formatter\""))
        .unwrap_or_else(|| panic!("expected a JSON report line in stderr:\n{stderr}"));
    serde_json::from_str(line).unwrap()
}

#[test]
fn verbose_emits_json_line_for_the_formatting_action() {
    // FORMATTER_VERBOSE=1 -> the eof-newline write is reported as one JSON line.
    let tmp = tempfile::TempDir::new().unwrap();
    let file = tmp.path().join("notes.txt");
    fs::write(&file, "no trailing newline").unwrap();

    let stderr = run_capturing_stderr(
        &hook_input(&file),
        tmp.path(),
        &[("FORMATTER_VERBOSE", "1")],
    );

    let report = report_line(&stderr);
    assert_eq!(report["formatter"], "eof-newline");
    assert_eq!(report["action"], "formatted");
    assert!(
        report["file"].as_str().unwrap().ends_with("notes.txt"),
        "file field should name the formatted file: {report}"
    );
}

#[test]
fn dry_run_reports_would_format_without_writing() {
    // FORMATTER_DRY_RUN=1 -> the pending change is reported but the file on disk
    // is left byte-for-byte unchanged (the C-07 invariant).
    let tmp = tempfile::TempDir::new().unwrap();
    let file = tmp.path().join("notes.txt");
    let original = "no trailing newline";
    fs::write(&file, original).unwrap();

    let stderr = run_capturing_stderr(
        &hook_input(&file),
        tmp.path(),
        &[("FORMATTER_DRY_RUN", "1")],
    );

    let report = report_line(&stderr);
    assert_eq!(report["formatter"], "eof-newline");
    assert_eq!(report["action"], "would-format");
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
}

#[test]
fn default_operation_emits_no_json_line() {
    // Without either flag the hook stays silent so it does not distract an agent.
    let tmp = tempfile::TempDir::new().unwrap();
    let file = tmp.path().join("notes.txt");
    fs::write(&file, "no trailing newline").unwrap();

    let stderr = run_capturing_stderr(&hook_input(&file), tmp.path(), &[]);

    assert!(
        !stderr.contains("\"formatter\""),
        "default operation must not emit a JSON report line: {stderr}"
    );
}
