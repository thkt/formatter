//! oxfmt formatter integration (broad language support including markup/config).

use crate::resolve::{has_extension, resolve_bin};
use std::process::Command;

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

    match Command::new(&oxfmt).arg(file_path).output() {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.is_empty() {
                eprintln!("Formatter: oxfmt: exited with {}", o.status);
            } else {
                eprintln!(
                    "Formatter: oxfmt: {}",
                    stderr.lines().next().unwrap_or_default()
                );
            }
        }
        Err(e) => {
            eprintln!("Formatter: oxfmt: {}", e);
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
