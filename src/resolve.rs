//! Path resolution utilities: git root discovery and local binary lookup.

use std::env;
use std::path::{Path, PathBuf};

/// Caps the parent-directory search at 20 levels to bound the number of
/// `.git` / `node_modules/.bin` `exists()` checks on deeply nested trees. The
/// walk uses `Path::parent()` (purely lexical), so it always terminates at the
/// filesystem root on its own; this is a search-depth bound, not a loop guard.
const MAX_TRAVERSAL_DEPTH: usize = 20;

pub fn has_extension(path: &str, extensions: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.contains(&e))
}

/// The directories to inspect when walking up from `start`: `start` first, then
/// each parent. Stops once `$HOME` has been yielded — `$HOME` is inspected but
/// nothing above it — and caps the run at [`MAX_TRAVERSAL_DEPTH`] directories.
/// The shared skeleton both resolvers walk; each applies its own predicate to
/// the yielded directories, so the bound and the `$HOME` fence stay in one place
/// rather than being hand-rolled (and drifting) per resolver.
fn bounded_ancestors(start: &Path) -> impl Iterator<Item = &Path> {
    let stop_at = env::var_os("HOME").map(PathBuf::from);
    let mut past_home = false;
    start
        .ancestors()
        .take(MAX_TRAVERSAL_DEPTH)
        .take_while(move |dir| {
            if past_home {
                return false;
            }
            // Yield `$HOME` itself, then fence out everything above it.
            past_home = stop_at.as_deref() == Some(*dir);
            true
        })
}

pub fn find_git_root_from_dir(start: &Path) -> Option<PathBuf> {
    bounded_ancestors(start)
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

pub fn resolve_bin(name: &str, file_path: &str) -> PathBuf {
    let Some(start) = Path::new(file_path).parent() else {
        return PathBuf::from(name);
    };
    for dir in bounded_ancestors(start) {
        let candidate = dir.join("node_modules/.bin").join(name);
        if candidate.exists() {
            return candidate;
        }
        // A `.git` dir marks the project root, so don't search above it. Unlike
        // `find_git_root_from_dir`, here `.git` is a fence, not the target.
        if dir.join(".git").exists() {
            break;
        }
    }
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn has_extension_matches() {
        assert!(has_extension("src/app.ts", &["ts", "js"]));
        assert!(!has_extension("src/app.rs", &["ts", "js"]));
        assert!(!has_extension("Makefile", &["ts", "js"]));
        assert!(!has_extension(".ts", &["ts"]));
    }

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

    // [T-014] find_git_root_from_dir finds git root in current directory
    #[test]
    fn t_014_find_git_root_from_dir_finds_root() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();

        let result = find_git_root_from_dir(tmp.path());
        assert_eq!(result, Some(tmp.path().to_path_buf()));
    }

    // [T-015] find_git_root_from_dir returns None without .git
    #[test]
    fn t_015_find_git_root_from_dir_none_without_git() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(find_git_root_from_dir(tmp.path()), None);
    }

    // [T-016] find_git_root_from_dir finds git root from deep subdirectory
    #[test]
    fn t_016_find_git_root_from_dir_deep_subdir() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        let deep = tmp.path().join("src/components/ui");
        fs::create_dir_all(&deep).unwrap();

        let result = find_git_root_from_dir(&deep);
        assert_eq!(result, Some(tmp.path().to_path_buf()));
    }

    /// Builds `root/d0/.../d{levels-1}/app.ts` with a `node_modules/.bin/biome`
    /// at `root`, then resolves from the file. `root` sits at ancestor index
    /// `levels` of the file's parent, so it is inspected only when
    /// `levels < MAX_TRAVERSAL_DEPTH`. Returns whether the local bin was found.
    fn resolves_local_bin_at_nesting(levels: usize) -> bool {
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("node_modules/.bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("biome"), "").unwrap();

        let mut deep = tmp.path().to_path_buf();
        for i in 0..levels {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();

        resolve_bin("biome", deep.join("app.ts").to_str().unwrap()) == bin.join("biome")
    }

    #[test]
    fn depth_cap_includes_the_twentieth_ancestor_but_not_the_twenty_first() {
        // The walk inspects exactly MAX_TRAVERSAL_DEPTH (20) directories from the
        // file's parent. With 19 nesting levels the bin's dir (root) is the 20th
        // inspected ancestor (reachable); with 20 levels it is the 21st (beyond
        // the cap). Pinning both sides catches a ±1 drift in the `take` bound —
        // respects_depth_limit alone has 5 levels of slack.
        assert!(
            resolves_local_bin_at_nesting(19),
            "20th ancestor must be reachable"
        );
        assert!(
            !resolves_local_bin_at_nesting(20),
            "21st ancestor must be beyond the cap"
        );
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
