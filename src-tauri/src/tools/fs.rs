/// Coding tools: read_file, write_file, edit_file, list_files, grep
///
/// Inspired by opencode / Claude Code tool design.
use anyhow::{bail, Result};
use serde_json::Value;

use super::ToolOutput;

pub struct FsTool {
    pub work_dir: String,
}

impl FsTool {
    pub fn new(work_dir: String) -> Self {
        Self { work_dir }
    }

    fn resolve_path(&self, raw: &str) -> std::path::PathBuf {
        let p = std::path::Path::new(raw);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::path::Path::new(&self.work_dir).join(p)
        }
    }

    // ── Tool definitions ─────────────────────────────────────────

    pub fn tool_defs() -> Vec<crate::backend::ToolDef> {
        use serde_json::json;
        vec![
            crate::backend::ToolDef {
                name: "read_file".to_string(),
                description: "Read a file. Optionally specify line range.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path (absolute or relative to work_dir)" },
                        "start_line": { "type": "integer", "description": "First line to read (1-based, optional)" },
                        "end_line": { "type": "integer", "description": "Last line to read (inclusive, optional)" }
                    },
                    "required": ["path"]
                }),
            },
            crate::backend::ToolDef {
                name: "write_file".to_string(),
                description: "Write (overwrite) a file with given content.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }),
            },
            crate::backend::ToolDef {
                name: "edit_file".to_string(),
                description: "Replace an exact string in a file. old_string must be unique.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "old_string": { "type": "string", "description": "Exact text to replace (must appear exactly once)" },
                        "new_string": { "type": "string", "description": "Replacement text" }
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
            },
            crate::backend::ToolDef {
                name: "list_files".to_string(),
                description: "List files matching a glob pattern under a directory.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory to search (default: work_dir)" },
                        "pattern": { "type": "string", "description": "Glob pattern (default: **/*)" }
                    }
                }),
            },
            crate::backend::ToolDef {
                name: "grep".to_string(),
                description: "Search file contents with a regex pattern.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Regex pattern" },
                        "path": { "type": "string", "description": "File or directory to search" },
                        "include": { "type": "string", "description": "File glob filter e.g. *.rs" }
                    },
                    "required": ["pattern"]
                }),
            },
        ]
    }

    // ── Implementations ──────────────────────────────────────────

    pub fn read_file(&self, input: &Value) -> Result<ToolOutput> {
        let raw = input["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
        let path = self.resolve_path(raw);

        if !path.exists() {
            bail!("File not found: {}", path.display());
        }

        let content = std::fs::read_to_string(&path)?;
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        let start = input["start_line"].as_u64().map(|n| (n as usize).saturating_sub(1));
        let end = input["end_line"].as_u64().map(|n| (n as usize).min(total));

        let slice = match (start, end) {
            (Some(s), Some(e)) => &lines[s.min(total)..e],
            (Some(s), None) => &lines[s.min(total)..],
            (None, Some(e)) => &lines[..e],
            (None, None) => &lines[..],
        };

        let offset = start.unwrap_or(0);
        let numbered: String = slice
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:4}: {}\n", i + offset + 1, line))
            .collect();

        Ok((
            format!("File: {} ({} lines total)\n{}", path.display(), total, numbered),
            None,
        ))
    }

    pub fn write_file(&self, input: &Value) -> Result<ToolOutput> {
        let raw = input["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
        let content = input["content"].as_str().ok_or_else(|| anyhow::anyhow!("missing content"))?;
        let path = self.resolve_path(raw);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&path, content)?;
        Ok((format!("Written {} bytes to {}", content.len(), path.display()), None))
    }

    pub fn edit_file(&self, input: &Value) -> Result<ToolOutput> {
        let raw = input["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
        let old = input["old_string"].as_str().ok_or_else(|| anyhow::anyhow!("missing old_string"))?;
        let new = input["new_string"].as_str().ok_or_else(|| anyhow::anyhow!("missing new_string"))?;
        let path = self.resolve_path(raw);

        if !path.exists() {
            bail!("File not found: {}", path.display());
        }

        let content = std::fs::read_to_string(&path)?;

        let count = content.matches(old).count();
        if count == 0 {
            bail!("old_string not found in file");
        }
        if count > 1 {
            bail!("old_string appears {count} times — must be unique. Add more context.");
        }

        let updated = content.replacen(old, new, 1);
        std::fs::write(&path, &updated)?;

        Ok((format!("Edited {} — replaced {} chars with {} chars", path.display(), old.len(), new.len()), None))
    }

    pub fn list_files(&self, input: &Value) -> Result<ToolOutput> {
        let base_raw = input["path"].as_str().unwrap_or(&self.work_dir);
        let base = self.resolve_path(base_raw);
        let pattern = input["pattern"].as_str().unwrap_or("**/*");

        // Use walkdir for traversal, apply simple glob filter
        let full_pattern = base.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        let paths: Vec<String> = glob::glob(&pattern_str)
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .filter_map(|r| r.ok())
            .filter(|p| p.is_file())
            .take(200)
            .map(|p| p.display().to_string())
            .collect();

        if paths.is_empty() {
            return Ok(("No files found".to_string(), None));
        }

        Ok((paths.join("\n"), None))
    }

    pub fn grep(&self, input: &Value) -> Result<ToolOutput> {
        let pattern = input["pattern"].as_str().ok_or_else(|| anyhow::anyhow!("missing pattern"))?;
        let base_raw = input["path"].as_str().unwrap_or(&self.work_dir);
        let base = self.resolve_path(base_raw);
        let include = input["include"].as_str();

        let regex = regex::Regex::new(pattern)?;

        let mut results = Vec::new();
        let walk = walkdir::WalkDir::new(&base)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file());

        for entry in walk {
            let path = entry.path();

            // Apply include filter
            if let Some(pat) = include {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if !glob_match_simple(pat, &name) {
                    continue;
                }
            }

            // Skip binary/large files
            if let Ok(meta) = path.metadata() {
                if meta.len() > 5_000_000 {
                    continue;
                }
            }

            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };

            for (i, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    results.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
                    if results.len() >= 100 {
                        break;
                    }
                }
            }
            if results.len() >= 100 {
                break;
            }
        }

        if results.is_empty() {
            return Ok(("No matches found".to_string(), None));
        }

        Ok((results.join("\n"), None))
    }

    /// Dispatch by tool name.
    pub fn execute(&self, name: &str, input: &Value) -> Result<ToolOutput> {
        match name {
            "read_file" => self.read_file(input),
            "write_file" => self.write_file(input),
            "edit_file" => self.edit_file(input),
            "list_files" => self.list_files(input),
            "grep" => self.grep(input),
            _ => bail!("Unknown fs tool: {name}"),
        }
    }
}

fn glob_match_simple(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(ext) = pattern.strip_prefix("*.") {
        return name.ends_with(&format!(".{ext}"));
    }
    pattern == name
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn tmp_tool() -> (FsTool, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = FsTool::new(dir.path().to_string_lossy().to_string());
        (tool, dir)
    }

    fn write_tmp(dir: &tempfile::TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn read_file_returns_numbered_lines() {
        let (tool, dir) = tmp_tool();
        write_tmp(&dir, "hello.txt", "line1\nline2\nline3\n");
        let out = tool.read_file(&json!({ "path": dir.path().join("hello.txt") })).unwrap();
        assert!(out.0.contains("   1: line1"));
        assert!(out.0.contains("   3: line3"));
    }

    #[test]
    fn read_file_with_line_range() {
        let (tool, dir) = tmp_tool();
        write_tmp(&dir, "multi.txt", "a\nb\nc\nd\ne\n");
        let out = tool.read_file(&json!({
            "path": dir.path().join("multi.txt"),
            "start_line": 2,
            "end_line": 4
        })).unwrap();
        assert!(out.0.contains("   2: b"));
        assert!(out.0.contains("   4: d"));
        assert!(!out.0.contains("   1: a"));
        assert!(!out.0.contains("   5: e"));
    }

    #[test]
    fn read_file_missing_returns_error() {
        let (tool, _dir) = tmp_tool();
        let err = tool.read_file(&json!({ "path": "/nonexistent/path.txt" }));
        assert!(err.is_err());
    }

    #[test]
    fn write_file_creates_file() {
        let (tool, dir) = tmp_tool();
        let path = dir.path().join("out.txt");
        tool.write_file(&json!({ "path": path, "content": "hello" })).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn write_file_creates_parent_dirs() {
        let (tool, dir) = tmp_tool();
        let path = dir.path().join("nested/dir/file.txt");
        tool.write_file(&json!({ "path": path, "content": "hi" })).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn edit_file_replaces_unique_string() {
        let (tool, dir) = tmp_tool();
        let path = write_tmp(&dir, "src.rs", "fn hello() {}\n");
        tool.edit_file(&json!({
            "path": path,
            "old_string": "fn hello() {}",
            "new_string": "fn hello() { println!(\"hi\"); }"
        })).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("println!"));
        assert!(!content.contains("fn hello() {}"));
    }

    #[test]
    fn edit_file_fails_if_old_string_not_found() {
        let (tool, dir) = tmp_tool();
        let path = write_tmp(&dir, "src.rs", "fn hello() {}\n");
        let err = tool.edit_file(&json!({
            "path": path,
            "old_string": "NONEXISTENT",
            "new_string": "replacement"
        }));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn edit_file_fails_if_old_string_not_unique() {
        let (tool, dir) = tmp_tool();
        let path = write_tmp(&dir, "src.rs", "hello hello\n");
        let err = tool.edit_file(&json!({
            "path": path,
            "old_string": "hello",
            "new_string": "world"
        }));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("2 times"));
    }

    #[test]
    fn list_files_returns_files_in_dir() {
        let (tool, dir) = tmp_tool();
        write_tmp(&dir, "a.rs", "");
        write_tmp(&dir, "b.rs", "");
        let out = tool.list_files(&json!({ "path": dir.path() })).unwrap();
        assert!(out.0.contains("a.rs"));
        assert!(out.0.contains("b.rs"));
    }

    #[test]
    fn grep_finds_matching_lines() {
        let (tool, dir) = tmp_tool();
        write_tmp(&dir, "code.rs", "fn main() {}\nfn helper() {}\n");
        let out = tool.grep(&json!({
            "pattern": "fn main",
            "path": dir.path()
        })).unwrap();
        assert!(out.0.contains("fn main"));
        assert!(!out.0.contains("fn helper"));
    }

    #[test]
    fn grep_no_match_returns_message() {
        let (tool, dir) = tmp_tool();
        write_tmp(&dir, "code.rs", "fn main() {}\n");
        let out = tool.grep(&json!({
            "pattern": "NONEXISTENT",
            "path": dir.path()
        })).unwrap();
        assert!(out.0.contains("No matches"));
    }
}
