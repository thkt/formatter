//! The hook is a PostToolUse formatter classified as Group 3 (Hook tool) in
//! ADR-0066. Its exit codes follow that baseline: 0 for any formatting outcome
//! (the silent-fix policy keeps formatting failures advisory so the hook never
//! blocks the developer, and an input past the size cap is skipped the same
//! way), 64 (EX_USAGE) when the hook input on stdin is not the expected JSON,
//! and 70 (EX_SOFTWARE) on an internal panic. These tests assert
//! the contract end-to-end via the built binary, since the exit code cannot be
//! observed from a unit test on `run()`.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Run the formatter binary with raw `stdin` bytes, optionally from `cwd`, return its exit code.
fn run_with_stdin_bytes(stdin: &[u8], cwd: Option<&Path>) -> i32 {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_formatter"));
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // A BrokenPipe is expected when the child reads its input cap and exits
    // before we finish writing (the oversized-input case), so it is not a failure.
    if let Err(e) = child.stdin.take().unwrap().write_all(stdin) {
        assert_eq!(e.kind(), ErrorKind::BrokenPipe);
    }
    child.wait().unwrap().code().unwrap()
}

/// Run the formatter binary with UTF-8 `stdin`, optionally from `cwd`, return its exit code.
fn run_with_stdin(stdin: &str, cwd: Option<&Path>) -> i32 {
    run_with_stdin_bytes(stdin.as_bytes(), cwd)
}

#[test]
fn exits_zero_after_formatting_a_supported_file() {
    // A syntactically broken .ts file inside the CWD reaches the formatter.
    // Whether oxfmt errors on the bad syntax or is absent and the fallback
    // skips, this is a formatting outcome, so the binary must exit 0.
    let tmp = tempfile::TempDir::new().unwrap();
    let file = tmp.path().join("broken.ts");
    fs::write(&file, "const x = (((;\n").unwrap();
    let input = format!(
        r#"{{"tool_name": "Write", "tool_input": {{"file_path": "{}"}}}}"#,
        file.to_str().unwrap()
    );
    assert_eq!(run_with_stdin(&input, Some(tmp.path())), 0);
}

#[test]
fn exits_zero_when_tool_is_not_write_or_edit() {
    // A non-editing tool is not a formatting target and not a contract
    // violation, so the hook stays silent at exit 0.
    assert_eq!(
        run_with_stdin(r#"{"tool_name": "Read", "tool_input": {}}"#, None),
        0
    );
}

#[test]
fn exits_64_on_invalid_hook_input() {
    // Malformed hook JSON violates the PostToolUse contract on stdin, so the
    // hook fails closed with EX_USAGE (64) rather than masking it as success.
    assert_eq!(run_with_stdin("not valid json", None), 64);
}

#[test]
fn exits_64_on_invalid_utf8_below_the_size_cap() {
    // Invalid UTF-8 bytes under the cap can never be the expected hook JSON, so
    // they fail closed with EX_USAGE (64) like any other malformed input rather
    // than being hidden as success. 0xff/0xfe are not valid UTF-8 lead bytes.
    assert_eq!(run_with_stdin_bytes(&[0xff, 0xfe], None), 64);
}

#[test]
fn exits_zero_when_input_is_exactly_at_the_size_cap() {
    // A payload of exactly MAX_INPUT_SIZE bytes is at the cap, not past it, so it
    // must be processed normally (a formatting outcome, exit 0) and never skipped
    // as oversized. The file does not exist, so formatting stays silent at 0.
    let tmp = tempfile::TempDir::new().unwrap();
    let file = tmp.path().join("exact.ts");
    let prefix = format!(
        r#"{{"tool_name": "Write", "tool_input": {{"file_path": "{}", "content": ""#,
        file.to_str().unwrap()
    );
    let suffix = r#""}}"#;
    let pad = 10_000_000 - prefix.len() - suffix.len();
    let input = format!("{}{}{}", prefix, "A".repeat(pad), suffix);
    assert_eq!(input.len(), 10_000_000);
    assert_eq!(run_with_stdin(&input, None), 0);
}

#[test]
fn exits_zero_when_input_exceeds_the_size_cap() {
    // A Write of a multi-megabyte file produces hook JSON past the 10 MB cap.
    // The cap truncates stdin mid-`content`, leaving well-formed-but-incomplete
    // JSON. That truncation is the formatter's own doing, not a broken hook
    // contract, so it must stay silent at exit 0 rather than be misclassified
    // as EX_USAGE (64).
    let content = "A".repeat(11_000_000);
    let input = format!(
        r#"{{"tool_name": "Write", "tool_input": {{"file_path": "/tmp/x.ts", "content": "{}"}}}}"#,
        content
    );
    assert_eq!(run_with_stdin(&input, None), 0);
}
