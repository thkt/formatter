//! The hook is a PostToolUse formatter: it must never block the tool call, so
//! the binary always exits 0 regardless of input or formatting outcome. These
//! tests assert that contract end-to-end via the built binary, since exit code
//! cannot be observed from a unit test on `run()`.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Run the formatter binary with `stdin`, optionally from `cwd`, return its exit code.
fn run_with_stdin(stdin: &str, cwd: Option<&Path>) -> i32 {
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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait().unwrap().code().unwrap()
}

#[test]
fn exits_zero_after_formatting_a_supported_file() {
    // A syntactically broken .ts file inside the CWD reaches the formatter.
    // Whether oxfmt errors on the bad syntax or is absent and the fallback
    // skips, the binary must still exit 0.
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
fn exits_zero_on_invalid_input() {
    // Malformed hook JSON is rejected, but the hook still must not block.
    assert_eq!(run_with_stdin("not valid json", None), 0);
}
