use crate::error::{GagentError, Result};
use std::path::PathBuf;

/// Maximum characters per memory file.
pub const MAX_MEMORY_FILE_CHARS: usize = 20_000;

/// A single memory entry (a named file in the memory/ directory).
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    /// File name within the memory directory (e.g., "2024-01-15.md").
    pub name: String,

    /// Full path to the file.
    pub path: PathBuf,

    /// File contents.
    pub content: String,
}

/// Manages reading and writing agent memory files.
///
/// Memory lives in `.gagent/memory/*.md` with a 20,000 char limit per file.
/// The root `.gagent/MEMORY.md` is treated as a summary/index.
pub struct MemoryStore {
    /// Path to the `.gagent/` workspace directory.
    workspace_dir: PathBuf,
}

impl MemoryStore {
    /// Create a MemoryStore for the given workspace directory.
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }

    /// Path to the memory/ subdirectory.
    pub fn memory_dir(&self) -> PathBuf {
        self.workspace_dir.join("memory")
    }

    /// Path to the root MEMORY.md file.
    pub fn summary_path(&self) -> PathBuf {
        self.workspace_dir.join("MEMORY.md")
    }

    /// List all memory entries in the memory/ directory.
    pub fn list(&self) -> Result<Vec<MemoryEntry>> {
        let dir = self.memory_dir();
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let content = std::fs::read_to_string(&path)?;
                entries.push(MemoryEntry { name, path, content });
            }
        }

        // Sort by name (date-based names will sort chronologically)
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    /// Read a specific memory file by name (relative to the memory/ dir).
    pub fn read(&self, name: &str) -> Result<MemoryEntry> {
        let path = self.memory_dir().join(name);
        if !path.exists() {
            return Err(GagentError::Other(format!(
                "Memory file not found: {name}"
            )));
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(MemoryEntry {
            name: name.to_string(),
            path,
            content,
        })
    }

    /// Read the root MEMORY.md summary file.
    pub fn read_summary(&self) -> Result<String> {
        let path = self.summary_path();
        if path.exists() {
            Ok(std::fs::read_to_string(path)?)
        } else {
            Ok(String::new())
        }
    }

    /// Write content to a memory file. Enforces the 20,000 char limit.
    pub fn write(&self, name: &str, content: &str) -> Result<()> {
        if content.chars().count() > MAX_MEMORY_FILE_CHARS {
            return Err(GagentError::Other(format!(
                "Memory file content exceeds limit: {} > {} chars",
                content.chars().count(),
                MAX_MEMORY_FILE_CHARS
            )));
        }

        let dir = self.memory_dir();
        std::fs::create_dir_all(&dir)?;

        let path = dir.join(name);
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Append content to an existing memory file (or create it).
    /// Returns an error if the resulting file would exceed the char limit.
    pub fn append(&self, name: &str, content: &str) -> Result<()> {
        let existing = self.read(name).map(|e| e.content).unwrap_or_default();
        let combined = format!("{existing}\n{content}");
        self.write(name, &combined)
    }

    /// Update the root MEMORY.md summary file.
    pub fn write_summary(&self, content: &str) -> Result<()> {
        if content.chars().count() > MAX_MEMORY_FILE_CHARS {
            return Err(GagentError::Other(format!(
                "Memory summary exceeds limit: {} > {} chars",
                content.chars().count(),
                MAX_MEMORY_FILE_CHARS
            )));
        }
        std::fs::write(self.summary_path(), content)?;
        Ok(())
    }

    /// Search all memory files for lines containing the query (case-insensitive).
    ///
    /// Returns a list of `(file_name, line_number, line_content)` matches.
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Search root MEMORY.md
        if let Ok(summary) = self.read_summary() {
            for (i, line) in summary.lines().enumerate() {
                if line.to_lowercase().contains(&query_lower) {
                    results.push(SearchResult {
                        file_name: "MEMORY.md".to_string(),
                        line_number: i + 1,
                        line_content: line.to_string(),
                    });
                }
            }
        }

        // Search memory/ subdirectory files
        for entry in self.list()? {
            for (i, line) in entry.content.lines().enumerate() {
                if line.to_lowercase().contains(&query_lower) {
                    results.push(SearchResult {
                        file_name: format!("memory/{}", entry.name),
                        line_number: i + 1,
                        line_content: line.to_string(),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Delete a memory file by name.
    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.memory_dir().join(name);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

/// A single search match within the memory store.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// File name where the match was found.
    pub file_name: String,

    /// Line number (1-indexed).
    pub line_number: usize,

    /// The matching line content.
    pub line_content: String,
}

impl std::fmt::Display for SearchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.file_name, self.line_number, self.line_content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_store() -> (TempDir, MemoryStore) {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn test_list_empty() {
        let (_dir, store) = make_store();
        let entries = store.list().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_write_and_read() {
        let (_dir, store) = make_store();
        store.write("2024-01-15.md", "# Notes\nSome content here.").unwrap();
        let entry = store.read("2024-01-15.md").unwrap();
        assert_eq!(entry.name, "2024-01-15.md");
        assert_eq!(entry.content, "# Notes\nSome content here.");
    }

    #[test]
    fn test_write_enforces_char_limit() {
        let (_dir, store) = make_store();
        let oversized = "x".repeat(MAX_MEMORY_FILE_CHARS + 1);
        let result = store.write("test.md", &oversized);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds limit"));
    }

    #[test]
    fn test_append() {
        let (_dir, store) = make_store();
        store.write("log.md", "Line 1").unwrap();
        store.append("log.md", "Line 2").unwrap();
        let entry = store.read("log.md").unwrap();
        assert!(entry.content.contains("Line 1"));
        assert!(entry.content.contains("Line 2"));
    }

    #[test]
    fn test_summary_read_write() {
        let (_dir, store) = make_store();
        store.write_summary("## Summary\nKey facts.").unwrap();
        let summary = store.read_summary().unwrap();
        assert_eq!(summary, "## Summary\nKey facts.");
    }

    #[test]
    fn test_search() {
        let (_dir, store) = make_store();
        store.write_summary("Important: remember this fact.").unwrap();
        store.write("notes.md", "Another note.\nRemember to do X.").unwrap();

        let results = store.search("remember").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.file_name == "MEMORY.md"));
        assert!(results.iter().any(|r| r.file_name == "memory/notes.md"));
    }

    #[test]
    fn test_search_case_insensitive() {
        let (_dir, store) = make_store();
        store.write("case.md", "UPPERCASE term here.").unwrap();
        let results = store.search("uppercase").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_list_sorts_by_name() {
        let (_dir, store) = make_store();
        store.write("2024-03.md", "March").unwrap();
        store.write("2024-01.md", "January").unwrap();
        store.write("2024-02.md", "February").unwrap();
        let entries = store.list().unwrap();
        assert_eq!(entries[0].name, "2024-01.md");
        assert_eq!(entries[1].name, "2024-02.md");
        assert_eq!(entries[2].name, "2024-03.md");
    }

    #[test]
    fn test_delete() {
        let (_dir, store) = make_store();
        store.write("tmp.md", "temp data").unwrap();
        assert!(store.read("tmp.md").is_ok());
        store.delete("tmp.md").unwrap();
        assert!(store.read("tmp.md").is_err());
    }

    #[test]
    fn test_read_nonexistent_file() {
        let (_dir, store) = make_store();
        let result = store.read("ghost.md");
        assert!(result.is_err());
    }
}
