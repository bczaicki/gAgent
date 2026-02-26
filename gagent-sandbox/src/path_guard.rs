//! Path containment guard — prevents path traversal attacks.
//!
//! `PathGuard` validates that a requested path stays within one of the
//! configured allowed root directories. It canonicalizes both the requested
//! path and the allowed roots before comparison to defeat symlink-based
//! escapes.

use gagent_core::{GagentError, Result};
use std::path::{Path, PathBuf};

/// Guards file access to a set of allowed root directories.
#[derive(Debug, Clone)]
pub struct PathGuard {
    /// Canonicalized allowed roots. Empty means "allow all" (sandbox off).
    allowed_roots: Vec<PathBuf>,
}

impl PathGuard {
    /// Create a guard that allows access only within the given roots.
    ///
    /// Each root is canonicalized immediately; non-existent directories are
    /// skipped with a warning.
    pub fn new(roots: &[PathBuf]) -> Self {
        let allowed_roots = roots
            .iter()
            .filter_map(|r| {
                match r.canonicalize() {
                    Ok(canonical) => Some(canonical),
                    Err(e) => {
                        tracing::warn!("PathGuard: skipping non-existent root {}: {}", r.display(), e);
                        None
                    }
                }
            })
            .collect();

        Self { allowed_roots }
    }

    /// Create a guard that allows all paths (sandbox disabled).
    pub fn allow_all() -> Self {
        Self {
            allowed_roots: Vec::new(),
        }
    }

    /// Check whether the given path is within any allowed root.
    ///
    /// The path is canonicalized before the check so that `..` sequences and
    /// symlinks are resolved. If the path does not yet exist, its closest
    /// existing ancestor is canonicalized and the non-existent suffix is
    /// appended — this allows checking paths before they are created.
    pub fn check(&self, path: &Path) -> Result<PathBuf> {
        if self.allowed_roots.is_empty() {
            // Sandbox is disabled — resolve path without restriction
            return Ok(resolve_path_lenient(path));
        }

        let resolved = resolve_path_lenient(path);

        for root in &self.allowed_roots {
            if resolved.starts_with(root) {
                return Ok(resolved);
            }
        }

        Err(GagentError::PathNotAllowed(format!(
            "'{}' is not within any allowed directory",
            path.display()
        )))
    }

    /// Check a path and return true if allowed, false otherwise.
    pub fn is_allowed(&self, path: &Path) -> bool {
        self.check(path).is_ok()
    }

    /// Return the list of allowed roots.
    pub fn allowed_roots(&self) -> &[PathBuf] {
        &self.allowed_roots
    }
}

/// Resolve a path leniently: canonicalize the deepest existing ancestor and
/// append the non-existent suffix. This allows validation of paths that
/// haven't been created yet (e.g. new files to be written).
fn resolve_path_lenient(path: &Path) -> PathBuf {
    // Try the full path first
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    // Walk up until we find an existing component, then re-append the rest
    let mut existing = PathBuf::new();
    let mut remainder = Vec::new();
    let mut current = path.to_path_buf();

    loop {
        if current.exists() {
            existing = current.canonicalize().unwrap_or(current);
            break;
        }
        if let Some(parent) = current.parent() {
            if let Some(file_name) = current.file_name() {
                remainder.push(file_name.to_os_string());
            }
            current = parent.to_path_buf();
        } else {
            // Reached filesystem root without finding an existing path
            existing = current;
            break;
        }
    }

    // Re-append the non-existent suffix in the original order
    remainder.reverse();
    for part in remainder {
        existing = existing.join(part);
    }

    existing
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_allow_all_permits_anything() {
        let guard = PathGuard::allow_all();
        assert!(guard.is_allowed(Path::new("/etc/passwd")));
        assert!(guard.is_allowed(Path::new("/tmp/anything")));
    }

    #[test]
    fn test_restricts_paths_outside_allowed_root() {
        let dir = TempDir::new().unwrap();
        let guard = PathGuard::new(&[dir.path().to_path_buf()]);

        // Inside allowed root → ok
        let inside = dir.path().join("subdir/file.txt");
        assert!(guard.is_allowed(&inside));

        // Outside allowed root → denied
        let outside = PathBuf::from("/etc/passwd");
        assert!(!guard.is_allowed(&outside));
    }

    #[test]
    fn test_allows_file_inside_root() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.txt"), "content").unwrap();
        let guard = PathGuard::new(&[dir.path().to_path_buf()]);

        let path = dir.path().join("test.txt");
        let result = guard.check(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_denies_path_outside_root() {
        let dir = TempDir::new().unwrap();
        let guard = PathGuard::new(&[dir.path().to_path_buf()]);

        let result = guard.check(Path::new("/etc/shadow"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not within any allowed directory"));
    }

    #[test]
    fn test_multiple_allowed_roots() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        let guard = PathGuard::new(&[
            dir1.path().to_path_buf(),
            dir2.path().to_path_buf(),
        ]);

        assert!(guard.is_allowed(&dir1.path().join("file.txt")));
        assert!(guard.is_allowed(&dir2.path().join("file.txt")));
        assert!(!guard.is_allowed(Path::new("/tmp")));
    }

    #[test]
    fn test_lenient_resolution_for_new_files() {
        let dir = TempDir::new().unwrap();
        let guard = PathGuard::new(&[dir.path().to_path_buf()]);

        // File doesn't exist yet but is inside the allowed root
        let new_file = dir.path().join("new_subdir/new_file.txt");
        assert!(guard.is_allowed(&new_file));
    }

    #[test]
    fn test_empty_roots_skipped_gracefully() {
        // Non-existent path — should result in an empty guard (allow all)
        let guard = PathGuard::new(&[PathBuf::from("/nonexistent/path/xyz")]);
        // Since all roots were skipped, the guard has no roots and allows all
        assert!(guard.allowed_roots().is_empty());
    }

    #[test]
    fn test_path_guard_check_returns_resolved_path() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();

        let guard = PathGuard::new(&[dir.path().to_path_buf()]);
        let result = guard.check(&file).unwrap();

        // Result should be a canonical path
        assert!(result.is_absolute());
    }
}
