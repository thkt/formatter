//! Biome formatter integration (JS/TS/JSON/CSS).

use crate::resolve::{has_extension, resolve_bin};
use std::process::Command;

pub const EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "json", "jsonc", "css",
];

pub fn is_formattable(path: &str) -> bool {
    has_extension(path, EXTENSIONS)
}

pub fn is_available(file_path: &str) -> bool {
    Command::new(resolve_bin("biome", file_path))
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

pub fn format(file_path: &str) {
    let biome = resolve_bin("biome", file_path);

    // Try modern invocation first (biome check --write --linter-enabled=false).
    // Fall back to legacy (biome format --write) on any non-zero exit, which
    // handles older biome versions that don't support --linter-enabled.
    match Command::new(&biome)
        .args(["check", "--write", "--linter-enabled=false", file_path])
        .output()
    {
        Ok(o) if o.status.success() => return,
        Ok(o) => {
            eprintln!(
                "Formatter: biome: modern invocation failed (exit {}), trying legacy",
                o.status
            );
        }
        Err(e) => {
            eprintln!("Formatter: biome: {}", e);
            return;
        }
    }

    match Command::new(&biome)
        .args(["format", "--write", file_path])
        .output()
    {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.is_empty() {
                eprintln!("Formatter: biome: exited with {}", o.status);
            } else {
                eprintln!(
                    "Formatter: biome: {}",
                    stderr.lines().next().unwrap_or_default()
                );
            }
        }
        Err(e) => {
            eprintln!("Formatter: biome: {}", e);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formattable_extensions() {
        for ext in [
            "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "json", "jsonc", "css",
        ] {
            assert!(is_formattable(&format!("src/app.{ext}")), "{ext}");
        }
    }

    #[test]
    fn non_formattable() {
        for path in ["src/main.rs", "README.md", "Cargo.toml", ".env"] {
            assert!(!is_formattable(path), "{path}");
        }
    }

    #[test]
    fn dotfile_not_formattable() {
        assert!(!is_formattable("/tmp/.css"));
        assert!(!is_formattable("/tmp/.ts"));
        assert!(!is_formattable(".json"));
    }

    #[test]
    fn multiple_dots_formattable() {
        assert!(is_formattable("src/app.test.ts"));
        assert!(is_formattable("src/app.module.css"));
    }

    #[test]
    fn no_extension_not_formattable() {
        assert!(!is_formattable("Makefile"));
        assert!(!is_formattable("file"));
    }

    #[test]
    fn format_nonexistent_file_does_not_panic() {
        format("/nonexistent/path/to/file.ts");
    }

    #[test]
    fn format_fixes_json() {
        use std::fs;
        use tempfile::TempDir;

        // Check global PATH since temp dir has no node_modules
        let available = Command::new("biome")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !available {
            eprintln!("biome not available, skipping");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.json");
        fs::write(&file, "{  \"a\":1,  \"b\"  :2  }\n").unwrap();

        format(file.to_str().unwrap());

        let content = fs::read_to_string(&file).unwrap();
        assert!(
            content.contains("\"a\": 1"),
            "Expected formatted JSON, got: {content}"
        );
    }
}
