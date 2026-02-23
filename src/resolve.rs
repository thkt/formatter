//! Path resolution utilities: git root discovery and local binary lookup.

use std::path::{Path, PathBuf};

const MAX_TRAVERSAL_DEPTH: usize = 20;

pub fn find_git_root(file_path: &str) -> Option<PathBuf> {
    let stop_at = std::env::var_os("HOME").map(PathBuf::from);
    let mut dir = Path::new(file_path).parent();
    let mut depth = 0;
    while let Some(d) = dir {
        if depth >= MAX_TRAVERSAL_DEPTH {
            break;
        }
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        if stop_at.as_deref() == Some(d) {
            break;
        }
        dir = d.parent();
        depth += 1;
    }
    None
}

pub fn resolve_bin(name: &str, file_path: &str) -> PathBuf {
    let stop_at = std::env::var_os("HOME").map(PathBuf::from);
    let mut dir = Path::new(file_path).parent();
    let mut depth = 0;
    while let Some(d) = dir {
        if depth >= MAX_TRAVERSAL_DEPTH {
            break;
        }
        let candidate = d.join("node_modules/.bin").join(name);
        if candidate.exists() {
            return candidate;
        }
        if d.join(".git").exists() {
            break;
        }
        if stop_at.as_deref() == Some(d) {
            break;
        }
        dir = d.parent();
        depth += 1;
    }
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn finds_bin_in_node_modules() {
        let tmp = TempDir::new().unwrap();
        let bin_dir = tmp.path().join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin_path = bin_dir.join("biome");
        fs::write(&bin_path, "").unwrap();

        let file_path = tmp.path().join("src/app.ts");
        let result = resolve_bin("biome", file_path.to_str().unwrap());
        assert_eq!(result, bin_path);
    }

    #[test]
    fn falls_back_to_bare_name() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.ts");
        let result = resolve_bin("biome", file_path.to_str().unwrap());
        assert_eq!(result, PathBuf::from("biome"));
    }

    #[test]
    fn stops_at_git_boundary() {
        let tmp = TempDir::new().unwrap();

        let root_bin = tmp.path().join("node_modules/.bin");
        fs::create_dir_all(&root_bin).unwrap();
        fs::write(root_bin.join("biome"), "").unwrap();

        let project = tmp.path().join("project");
        fs::create_dir_all(project.join(".git")).unwrap();

        let file_path = project.join("src/app.ts");
        let result = resolve_bin("biome", file_path.to_str().unwrap());
        assert_eq!(result, PathBuf::from("biome"));
    }

    #[test]
    fn finds_bin_within_git_boundary() {
        let tmp = TempDir::new().unwrap();

        let project = tmp.path().join("project");
        fs::create_dir_all(project.join(".git")).unwrap();

        let bin_dir = project.join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin_path = bin_dir.join("oxfmt");
        fs::write(&bin_path, "").unwrap();

        let file_path = project.join("src/app.ts");
        let result = resolve_bin("oxfmt", file_path.to_str().unwrap());
        assert_eq!(result, bin_path);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_path_does_not_panic() {
        use std::os::unix::fs as unix_fs;
        let tmp = TempDir::new().unwrap();

        let project = tmp.path().join("project");
        let bin_dir = project.join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("biome"), "").unwrap();
        fs::create_dir_all(project.join("src")).unwrap();

        let link = tmp.path().join("link_to_src");
        unix_fs::symlink(project.join("src"), &link).unwrap();

        let file_path = link.join("app.ts");
        let result = resolve_bin("biome", file_path.to_str().unwrap());
        // Path::parent() doesn't resolve symlinks, so won't find project's node_modules
        assert_eq!(result, PathBuf::from("biome"));
    }

    #[test]
    fn respects_depth_limit() {
        let tmp = TempDir::new().unwrap();

        let mut deep = tmp.path().to_path_buf();
        for i in 0..25 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();

        let bin_dir = tmp.path().join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("biome"), "").unwrap();

        let file_path = deep.join("app.ts");
        let result = resolve_bin("biome", file_path.to_str().unwrap());
        assert_eq!(result, PathBuf::from("biome"));
    }
}
