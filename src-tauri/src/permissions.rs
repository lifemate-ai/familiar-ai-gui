/// Permission system for coding tools.
///
/// Inspired by Claude Code's approval modes:
///   - Full: no confirmation needed
///   - Prompt: ask for destructive operations (write, bash)
///   - Custom: allow/deny patterns like "allow:read_file:*", "deny:bash:rm *"
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TrustMode {
    /// Ask for confirmation on writes and bash.
    #[default]
    Prompt,
    /// Never ask — full trust.
    Full,
    /// Use allow/deny pattern lists.
    Custom,
}

/// A rule like "allow:bash:cargo *" or "deny:write_file:/etc/**"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermRule {
    pub allow: bool, // true = allow, false = deny
    pub tool: String,
    /// Glob-style pattern matched against the argument string.
    pub pattern: String,
}

impl PermRule {
    pub fn matches(&self, tool: &str, arg: &str) -> bool {
        if self.tool != tool && self.tool != "*" {
            return false;
        }
        glob_match(&self.pattern, arg)
    }
}

/// Result of a permission check.
#[derive(Debug, Clone, PartialEq)]
pub enum PermCheck {
    /// Immediately allowed (read-only tools, or Full mode, or explicit allow rule).
    Allow,
    /// Needs user confirmation.
    NeedsPrompt,
    /// Explicitly denied by a rule.
    Deny,
}

/// Which tools are safe to run without any confirmation.
const READ_ONLY_TOOLS: &[&str] = &["read_file", "list_files", "grep", "glob"];

pub fn check_permission(mode: &TrustMode, rules: &[PermRule], tool: &str, arg: &str) -> PermCheck {
    match mode {
        TrustMode::Full => PermCheck::Allow,
        TrustMode::Prompt => {
            if READ_ONLY_TOOLS.contains(&tool) {
                PermCheck::Allow
            } else {
                PermCheck::NeedsPrompt
            }
        }
        TrustMode::Custom => {
            // Check rules in order — first match wins.
            for rule in rules {
                if rule.matches(tool, arg) {
                    return if rule.allow {
                        PermCheck::Allow
                    } else {
                        PermCheck::Deny
                    };
                }
            }
            // Default: read-only tools are allowed, others need prompt.
            if READ_ONLY_TOOLS.contains(&tool) {
                PermCheck::Allow
            } else {
                PermCheck::NeedsPrompt
            }
        }
    }
}

/// Minimal glob matching: `*` matches anything within a segment, `**` matches across segments.
fn glob_match(pattern: &str, input: &str) -> bool {
    glob_match_inner(pattern.as_bytes(), input.as_bytes())
}

fn glob_match_inner(pat: &[u8], inp: &[u8]) -> bool {
    match (pat.first(), inp.first()) {
        (None, None) => true,
        (None, _) => false,
        (Some(b'*'), _) => {
            // Both * and ** match anything (including spaces and slashes).
            // This is intentional for shell-command patterns like "rm *" or "cargo *".
            // For file-path use, prefer "**/*.rs" style patterns.
            let rest_pat = if pat.get(1) == Some(&b'*') {
                &pat[2..]
            } else {
                &pat[1..]
            };
            for i in 0..=inp.len() {
                if glob_match_inner(rest_pat, &inp[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(&p), Some(&i)) if p == i => glob_match_inner(&pat[1..], &inp[1..]),
        _ => false,
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn no_rules() -> Vec<PermRule> {
        vec![]
    }

    // ── TrustMode::Full ──────────────────────────────────────────

    #[test]
    fn full_mode_always_allows_all_tools() {
        for tool in &["bash", "write_file", "edit_file", "read_file"] {
            assert_eq!(
                check_permission(&TrustMode::Full, &no_rules(), tool, "anything"),
                PermCheck::Allow,
                "Full mode should allow {tool}"
            );
        }
    }

    // ── TrustMode::Prompt ─────────────────────────────────────────

    #[test]
    fn prompt_mode_allows_read_only_tools() {
        for tool in &["read_file", "list_files", "grep", "glob"] {
            assert_eq!(
                check_permission(&TrustMode::Prompt, &no_rules(), tool, "/any/path"),
                PermCheck::Allow,
                "Prompt mode should allow read-only tool {tool}"
            );
        }
    }

    #[test]
    fn prompt_mode_requires_confirmation_for_write() {
        assert_eq!(
            check_permission(&TrustMode::Prompt, &no_rules(), "write_file", "/any/path"),
            PermCheck::NeedsPrompt
        );
    }

    #[test]
    fn prompt_mode_requires_confirmation_for_bash() {
        assert_eq!(
            check_permission(&TrustMode::Prompt, &no_rules(), "bash", "cargo build"),
            PermCheck::NeedsPrompt
        );
    }

    #[test]
    fn prompt_mode_requires_confirmation_for_edit() {
        assert_eq!(
            check_permission(&TrustMode::Prompt, &no_rules(), "edit_file", "src/main.rs"),
            PermCheck::NeedsPrompt
        );
    }

    // ── TrustMode::Custom ─────────────────────────────────────────

    #[test]
    fn custom_mode_explicit_allow_rule_matches() {
        let rules = vec![PermRule {
            allow: true,
            tool: "bash".to_string(),
            pattern: "cargo *".to_string(),
        }];
        assert_eq!(
            check_permission(&TrustMode::Custom, &rules, "bash", "cargo build"),
            PermCheck::Allow
        );
    }

    #[test]
    fn custom_mode_explicit_deny_rule_matches() {
        let rules = vec![PermRule {
            allow: false,
            tool: "bash".to_string(),
            pattern: "rm *".to_string(),
        }];
        assert_eq!(
            check_permission(&TrustMode::Custom, &rules, "bash", "rm -rf /"),
            PermCheck::Deny
        );
    }

    #[test]
    fn custom_mode_first_matching_rule_wins() {
        let rules = vec![
            PermRule {
                allow: false,
                tool: "bash".to_string(),
                pattern: "rm *".to_string(),
            },
            PermRule {
                allow: true,
                tool: "bash".to_string(),
                pattern: "*".to_string(),
            },
        ];
        // rm matches the deny rule first
        assert_eq!(
            check_permission(&TrustMode::Custom, &rules, "bash", "rm file.txt"),
            PermCheck::Deny
        );
        // cargo doesn't match deny, matches allow
        assert_eq!(
            check_permission(&TrustMode::Custom, &rules, "bash", "cargo test"),
            PermCheck::Allow
        );
    }

    #[test]
    fn custom_mode_falls_back_to_prompt_for_unmatched_write() {
        let rules = vec![];
        assert_eq!(
            check_permission(&TrustMode::Custom, &rules, "write_file", "/any/file"),
            PermCheck::NeedsPrompt
        );
    }

    #[test]
    fn custom_mode_falls_back_to_allow_for_unmatched_read() {
        let rules = vec![];
        assert_eq!(
            check_permission(&TrustMode::Custom, &rules, "read_file", "/any/file"),
            PermCheck::Allow
        );
    }

    // ── Glob matching ─────────────────────────────────────────────

    #[test]
    fn glob_star_matches_anything_including_spaces() {
        // * matches spaces and slashes for shell-command patterns
        assert!(glob_match("cargo *", "cargo build"));
        assert!(glob_match("cargo *", "cargo test"));
        assert!(glob_match("cargo *", "cargo build --release extra"));
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "src/main.rs")); // * crosses slashes
        assert!(!glob_match("*.rs", "main.txt"));
    }

    #[test]
    fn glob_double_star_matches_across_segments() {
        assert!(glob_match("**/*.rs", "src/main.rs"));
        assert!(glob_match("**/*.rs", "src/nested/deep/file.rs"));
        assert!(!glob_match("**/*.rs", "src/main.ts"));
    }

    #[test]
    fn glob_exact_match() {
        assert!(glob_match("rm", "rm"));
        assert!(!glob_match("rm", "rm "));
    }

    #[test]
    fn glob_wildcard_matches_empty() {
        assert!(glob_match("*", ""));
        assert!(glob_match("*", "anything"));
    }
}
