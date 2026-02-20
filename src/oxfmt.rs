use crate::resolve::resolve_bin;
use std::path::Path;
use std::process::Command;

pub const EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "json", "jsonc", "json5", "css", "scss",
    "less", "html", "vue", "yaml", "yml", "toml", "md", "mdx", "graphql", "gql",
];

pub fn is_formattable(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e))
}

pub fn is_available(file_path: &str) -> bool {
    Command::new(resolve_bin("oxfmt", file_path))
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn format(file_path: &str) {
    let oxfmt = resolve_bin("oxfmt", file_path);

    match Command::new(&oxfmt).arg(file_path).output() {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stderr.is_empty() {
                eprintln!(
                    "formatter: oxfmt: {}",
                    stderr.lines().next().unwrap_or("(no details)")
                );
            }
        }
        Err(e) => {
            eprintln!("formatter: oxfmt: {}", e);
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
}
