/// Self-feedback loop utilities.
///
/// The agent observes the results of its own tool calls and injects
/// structured feedback into the conversation history so the LLM can
/// self-correct without human intervention.
///
/// Inspired by:
///   - ReAct (Reason+Act) loop (Yao et al., 2022)
///   - Reflexion (Shinn et al., 2023)
///   - Self-RAG (Asai et al., 2023)

/// Analyse a bash tool result and decide whether the agent needs to reflect.
///
/// Returns `Some(feedback)` if the command failed and the agent should attempt
/// to self-correct; `None` if the result looks successful.
pub fn bash_feedback(output: &str) -> Option<String> {
    // Extract exit code from "Exit: N\n..." format
    let exit_code = output
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("Exit: "))
        .and_then(|s| s.trim().parse::<i32>().ok())
        .unwrap_or(0);

    if exit_code == 0 {
        return None;
    }

    // Extract the most relevant error lines (prefer stderr)
    let error_section = if let Some(start) = output.find("--- stderr ---\n") {
        &output[start + "--- stderr ---\n".len()..]
    } else {
        output
    };

    let error_preview: String = error_section
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(10)
        .collect::<Vec<_>>()
        .join("\n");

    Some(format!(
        "[Self-Feedback] The command exited with code {exit_code}.\n\
         Error:\n{error_preview}\n\
         Analyse the error, fix the root cause, and retry."
    ))
}

/// After a write_file or edit_file, generate a reminder to verify changes.
pub fn write_feedback(path: &str) -> String {
    format!(
        "[Self-Feedback] You just modified `{path}`. \
         Read the file to verify your changes are correct before proceeding."
    )
}

/// Generate a reminder to run tests after writing code.
pub fn test_reminder(work_dir: &str) -> Option<String> {
    // Check if there's a test command available
    let has_cargo = std::path::Path::new(work_dir).join("Cargo.toml").exists();
    let has_package_json = std::path::Path::new(work_dir).join("package.json").exists();

    if has_cargo {
        Some("[Self-Feedback] Run `cargo test` to verify correctness.".to_string())
    } else if has_package_json {
        Some("[Self-Feedback] Run tests (e.g. `npm test`) to verify correctness.".to_string())
    } else {
        None
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_feedback_none_on_exit_zero() {
        let output = "Exit: 0\n--- stdout ---\nall good\n";
        assert!(bash_feedback(output).is_none());
    }

    #[test]
    fn bash_feedback_some_on_nonzero_exit() {
        let output = "Exit: 1\n--- stderr ---\nerror: cannot find value `foo`\n";
        let fb = bash_feedback(output).unwrap();
        assert!(fb.contains("[Self-Feedback]"));
        assert!(fb.contains("exit code 1") || fb.contains("1"));
        assert!(fb.contains("foo") || fb.contains("error"));
    }

    #[test]
    fn bash_feedback_includes_stderr_content() {
        let output = "Exit: 2\n--- stdout ---\nsome stdout\n--- stderr ---\nactual error here\n";
        let fb = bash_feedback(output).unwrap();
        assert!(fb.contains("actual error here"));
        // Should not include unrelated stdout
        assert!(!fb.contains("some stdout"));
    }

    #[test]
    fn bash_feedback_works_without_stderr_section() {
        let output = "Exit: 127\ncommand not found: cargo\n";
        let fb = bash_feedback(output).unwrap();
        assert!(fb.contains("[Self-Feedback]"));
    }

    #[test]
    fn bash_feedback_on_timeout_message() {
        let output = "Command timed out after 30s";
        // Timeout message has no "Exit:" prefix → exit_code defaults to 0
        // But "timed out" is still a failure worth catching
        // This tests the current behaviour (None) — adjust if we detect timeout strings
        let fb = bash_feedback(output);
        // For now: no "Exit: N" → defaults to 0 → no feedback
        // This is intentional: timeout is already shown to the user
        assert!(fb.is_none());
    }

    #[test]
    fn write_feedback_includes_path() {
        let fb = write_feedback("src/main.rs");
        assert!(fb.contains("src/main.rs"));
        assert!(fb.contains("[Self-Feedback]"));
    }

    #[test]
    fn test_reminder_detects_cargo() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let reminder = test_reminder(dir.path().to_str().unwrap()).unwrap();
        assert!(reminder.contains("cargo test"));
    }

    #[test]
    fn test_reminder_detects_npm() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        let reminder = test_reminder(dir.path().to_str().unwrap()).unwrap();
        assert!(reminder.contains("npm test"));
    }

    #[test]
    fn test_reminder_none_for_unknown_project() {
        let dir = tempfile::TempDir::new().unwrap();
        let reminder = test_reminder(dir.path().to_str().unwrap());
        assert!(reminder.is_none());
    }
}
