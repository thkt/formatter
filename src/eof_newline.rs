//! EOF newline enforcement for files not covered by language-specific formatters.
//!
//! Handles Makefile, Dockerfile, .sh, .gitignore, and other text files that
//! oxfmt/biome/rustfmt do not support.

use std::fs;

pub fn ensure(file_path: &str) -> bool {
    let content = match fs::read(file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("formatter: eof-newline: cannot read {}: {}", file_path, e);
            return false;
        }
    };

    if content.is_empty() || content.last() == Some(&b'\n') {
        return false;
    }

    // Binary detection: NUL byte in first 512 bytes.
    if content[..content.len().min(512)].contains(&0) {
        return false;
    }

    let mut with_newline = content;
    with_newline.push(b'\n');
    match fs::write(file_path, &with_newline) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("formatter: eof-newline: cannot write {}: {}", file_path, e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn appends_newline_when_missing() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("Makefile");
        fs::write(&file, "all:\n\techo hello").unwrap();

        assert!(ensure(file.to_str().unwrap()));

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.ends_with('\n'));
    }

    #[test]
    fn skips_when_newline_exists() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("script.sh");
        fs::write(&file, "#!/bin/bash\necho hi\n").unwrap();

        assert!(!ensure(file.to_str().unwrap()));

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "#!/bin/bash\necho hi\n");
    }

    #[test]
    fn skips_empty_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("empty.txt");
        fs::write(&file, "").unwrap();

        assert!(!ensure(file.to_str().unwrap()));
    }

    #[test]
    fn skips_binary_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("image.png");
        let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        data.extend_from_slice(&[0u8; 100]);
        data.push(b'x'); // no trailing newline
        fs::write(&file, &data).unwrap();

        assert!(!ensure(file.to_str().unwrap()));
    }

    #[test]
    fn nonexistent_file_does_not_panic() {
        assert!(!ensure("/nonexistent/path/to/file.sh"));
    }

    #[test]
    fn appends_exact_content_with_newline() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("Dockerfile");
        fs::write(&file, "FROM node:20\nCOPY . .").unwrap();

        assert!(ensure(file.to_str().unwrap()));

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "FROM node:20\nCOPY . .\n");
    }
}
