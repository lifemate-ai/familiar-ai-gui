/// Coding-mode context management.
///
/// When the agent enters "coding mode", it first runs an init step to
/// understand the project structure, then follows a strict
/// read → understand → plan → write → verify workflow.

use std::path::Path;

/// Summary of a project for injection into the system prompt.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub work_dir: String,
    pub project_type: ProjectType,
    /// Key files found (Cargo.toml, package.json, README, etc.)
    pub key_files: Vec<String>,
    /// Brief description parsed from manifest
    pub description: Option<String>,
    /// Detected language(s)
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Mixed,
    Unknown,
}

impl ProjectType {
    pub fn test_command(&self) -> Option<&'static str> {
        match self {
            ProjectType::Rust => Some("cargo test"),
            ProjectType::Node => Some("npm test"),
            ProjectType::Python => Some("python -m pytest"),
            ProjectType::Mixed => None,
            ProjectType::Unknown => None,
        }
    }

    pub fn build_command(&self) -> Option<&'static str> {
        match self {
            ProjectType::Rust => Some("cargo build"),
            ProjectType::Node => Some("npm run build"),
            ProjectType::Python => None,
            ProjectType::Mixed => None,
            ProjectType::Unknown => None,
        }
    }
}

/// Scan a work_dir and build a ProjectContext.
pub fn scan_project(work_dir: &str) -> ProjectContext {
    let base = Path::new(work_dir);

    let has_cargo = base.join("Cargo.toml").exists();
    let has_package_json = base.join("package.json").exists();
    let has_pyproject = base.join("pyproject.toml").exists() || base.join("setup.py").exists();

    let project_type = match (has_cargo, has_package_json, has_pyproject) {
        (true, false, false) => ProjectType::Rust,
        (false, true, false) => ProjectType::Node,
        (false, false, true) => ProjectType::Python,
        (false, false, false) => ProjectType::Unknown,
        _ => ProjectType::Mixed,
    };

    let mut key_files = Vec::new();
    let mut description = None;
    let mut languages = Vec::new();

    // Collect manifest files
    for name in &["Cargo.toml", "package.json", "pyproject.toml", "setup.py", "go.mod"] {
        if base.join(name).exists() {
            key_files.push(name.to_string());
        }
    }

    // README
    for name in &["README.md", "README.rst", "README.txt", "README"] {
        if base.join(name).exists() {
            key_files.push(name.to_string());
            break;
        }
    }

    // Parse description from manifests
    if has_cargo {
        if let Ok(text) = std::fs::read_to_string(base.join("Cargo.toml")) {
            if let Some(desc) = extract_toml_field(&text, "description") {
                description = Some(desc);
            }
        }
        languages.push("Rust".to_string());
    }
    if has_package_json {
        if let Ok(text) = std::fs::read_to_string(base.join("package.json")) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(desc) = json["description"].as_str() {
                    description = Some(desc.to_string());
                }
            }
        }
        languages.push("JavaScript/TypeScript".to_string());
    }
    if has_pyproject {
        languages.push("Python".to_string());
    }

    // Detect source directories
    for dir in &["src", "lib", "app"] {
        if base.join(dir).is_dir() {
            key_files.push(format!("{}/", dir));
        }
    }

    ProjectContext {
        work_dir: work_dir.to_string(),
        project_type,
        key_files,
        description,
        languages,
    }
}

/// Format the project context as a system-prompt section.
pub fn format_context(ctx: &ProjectContext) -> String {
    let proj_type = match &ctx.project_type {
        ProjectType::Rust => "Rust",
        ProjectType::Node => "Node.js / TypeScript",
        ProjectType::Python => "Python",
        ProjectType::Mixed => "Mixed",
        ProjectType::Unknown => "Unknown",
    };

    let mut lines = vec![
        format!("[Project Context]"),
        format!("Directory : {}", ctx.work_dir),
        format!("Type      : {proj_type}"),
    ];

    if !ctx.languages.is_empty() {
        lines.push(format!("Languages : {}", ctx.languages.join(", ")));
    }

    if let Some(desc) = &ctx.description {
        lines.push(format!("About     : {desc}"));
    }

    if !ctx.key_files.is_empty() {
        lines.push(format!("Key files : {}", ctx.key_files.join(", ")));
    }

    if let Some(test_cmd) = ctx.project_type.test_command() {
        lines.push(format!("Test cmd  : {test_cmd}"));
    }

    lines.join("\n")
}

/// The coding workflow rules injected into the system prompt.
pub const CODING_WORKFLOW: &str = r#"[Coding Workflow — follow this strictly]
1. READ FIRST — Before touching any code, read the relevant files.
   Use read_file, list_files, and grep to understand the codebase.
2. PLAN — State your plan in 2-3 sentences before writing any code.
3. WRITE SMALL — Make the smallest possible change that moves toward the goal.
   Prefer edit_file over write_file to avoid clobbering existing code.
4. VERIFY — After every write_file or edit_file, read the file back to confirm.
5. TEST — After any code change, run the project's test command.
   Do not declare success until tests pass.
6. ONE THING AT A TIME — Complete one step fully before moving to the next.

[Tool Usage Rules]
- read_file   : Always use line ranges for large files (> 200 lines).
- edit_file   : old_string must be unique. Add surrounding context if needed.
- bash        : Prefer short-lived commands. Always check the exit code.
- list_files  : Use to orient yourself at the start of a task.
- grep        : Use to find definitions and usages before editing.

[What NOT to do]
- Do NOT write code without reading first.
- Do NOT skip verification after write/edit.
- Do NOT declare done without running tests.
- Do NOT make sweeping changes across many files in one step."#;

fn extract_toml_field(text: &str, field: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix(&format!("{field} =")) {
            let val = rest.trim().trim_matches('"').to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_dir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    fn write(dir: &tempfile::TempDir, name: &str, content: &str) {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    // ── scan_project ──────────────────────────────────────────────

    #[test]
    fn detects_rust_project() {
        let dir = make_dir();
        write(&dir, "Cargo.toml", "[package]\nname = \"test\"\ndescription = \"A test crate\"\n");
        let ctx = scan_project(dir.path().to_str().unwrap());
        assert_eq!(ctx.project_type, ProjectType::Rust);
        assert!(ctx.languages.contains(&"Rust".to_string()));
        assert!(ctx.key_files.contains(&"Cargo.toml".to_string()));
        assert_eq!(ctx.description.as_deref(), Some("A test crate"));
    }

    #[test]
    fn detects_node_project() {
        let dir = make_dir();
        write(&dir, "package.json", r#"{"name":"test","description":"A node project"}"#);
        let ctx = scan_project(dir.path().to_str().unwrap());
        assert_eq!(ctx.project_type, ProjectType::Node);
        assert!(ctx.languages.contains(&"JavaScript/TypeScript".to_string()));
        assert_eq!(ctx.description.as_deref(), Some("A node project"));
    }

    #[test]
    fn detects_python_project() {
        let dir = make_dir();
        write(&dir, "setup.py", "from setuptools import setup\nsetup(name='test')\n");
        let ctx = scan_project(dir.path().to_str().unwrap());
        assert_eq!(ctx.project_type, ProjectType::Python);
    }

    #[test]
    fn detects_mixed_project() {
        let dir = make_dir();
        write(&dir, "Cargo.toml", "[package]\nname = \"test\"\n");
        write(&dir, "package.json", r#"{"name":"test"}"#);
        let ctx = scan_project(dir.path().to_str().unwrap());
        assert_eq!(ctx.project_type, ProjectType::Mixed);
    }

    #[test]
    fn unknown_for_empty_dir() {
        let dir = make_dir();
        let ctx = scan_project(dir.path().to_str().unwrap());
        assert_eq!(ctx.project_type, ProjectType::Unknown);
    }

    #[test]
    fn includes_readme_in_key_files() {
        let dir = make_dir();
        write(&dir, "README.md", "# Hello");
        let ctx = scan_project(dir.path().to_str().unwrap());
        assert!(ctx.key_files.contains(&"README.md".to_string()));
    }

    #[test]
    fn includes_src_dir_in_key_files() {
        let dir = make_dir();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        let ctx = scan_project(dir.path().to_str().unwrap());
        assert!(ctx.key_files.contains(&"src/".to_string()));
    }

    // ── ProjectType helpers ───────────────────────────────────────

    #[test]
    fn rust_test_command_is_cargo_test() {
        assert_eq!(ProjectType::Rust.test_command(), Some("cargo test"));
    }

    #[test]
    fn node_test_command_is_npm_test() {
        assert_eq!(ProjectType::Node.test_command(), Some("npm test"));
    }

    #[test]
    fn unknown_has_no_test_command() {
        assert_eq!(ProjectType::Unknown.test_command(), None);
    }

    // ── format_context ────────────────────────────────────────────

    #[test]
    fn format_context_includes_work_dir() {
        let dir = make_dir();
        let ctx = scan_project(dir.path().to_str().unwrap());
        let s = format_context(&ctx);
        assert!(s.contains(dir.path().to_str().unwrap()));
    }

    #[test]
    fn format_context_includes_test_command_for_rust() {
        let dir = make_dir();
        write(&dir, "Cargo.toml", "[package]\nname = \"x\"\n");
        let ctx = scan_project(dir.path().to_str().unwrap());
        let s = format_context(&ctx);
        assert!(s.contains("cargo test"));
    }

    #[test]
    fn format_context_includes_description() {
        let dir = make_dir();
        write(&dir, "Cargo.toml", "[package]\nname = \"x\"\ndescription = \"my crate\"\n");
        let ctx = scan_project(dir.path().to_str().unwrap());
        let s = format_context(&ctx);
        assert!(s.contains("my crate"));
    }

    // ── CODING_WORKFLOW content ───────────────────────────────────

    #[test]
    fn workflow_contains_read_first_rule() {
        assert!(CODING_WORKFLOW.contains("READ FIRST"));
    }

    #[test]
    fn workflow_contains_verify_rule() {
        assert!(CODING_WORKFLOW.contains("VERIFY"));
    }

    #[test]
    fn workflow_contains_test_rule() {
        assert!(CODING_WORKFLOW.contains("TEST"));
    }

    #[test]
    fn workflow_contains_no_do_rules() {
        assert!(CODING_WORKFLOW.contains("What NOT to do"));
    }
}
