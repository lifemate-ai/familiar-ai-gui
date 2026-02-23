/// Desire system — intrinsic motivation for the familiar.
///
/// Based on:
/// - 3M-Progress / zebrafish agents (2506.00138): ethological grounding for desires
/// - LLM-Driven Intrinsic Motivation (2508.18420): intentionality reasoning before action
/// - From Curiosity to Competence (2507.08210): controllability bias
use std::time::Instant;

/// Internal desire state. Each field is 0.0 (absent) – 1.0 (overwhelming).
#[allow(dead_code)]
pub struct DesireState {
    /// Explore / look around the environment.
    pub look_around: f32,
    /// Talk to or acknowledge the companion.
    pub greet_companion: f32,
    /// Investigate a novel object or area.
    pub explore_object: f32,
    /// Rest after prolonged activity.
    pub rest: f32,

    last_updated: Instant,
}

impl Default for DesireState {
    fn default() -> Self {
        Self {
            // Start with moderate curiosity — agent just woke up
            look_around: 0.4,
            greet_companion: 0.3,
            explore_object: 0.2,
            rest: 0.0,
            last_updated: Instant::now(),
        }
    }
}

impl DesireState {
    /// Advance time — unsatisfied desires grow, satisfied ones stay low.
    /// Call this at the beginning of every user turn.
    pub fn decay(&mut self) {
        let elapsed = self.last_updated.elapsed().as_secs_f32();

        // Idle time raises exploration and companion desire
        self.look_around = (self.look_around + elapsed * 0.008).min(1.0);
        self.greet_companion = (self.greet_companion + elapsed * 0.004).min(1.0);
        // explore_object fades if not engaged
        self.explore_object = (self.explore_object - elapsed * 0.002).max(0.0);

        self.last_updated = Instant::now();
    }

    /// Return the strongest desire above the threshold, or None.
    pub fn strongest(&self) -> Option<(&'static str, f32)> {
        const THRESHOLD: f32 = 0.6;
        let candidates: &[(&str, f32)] = &[
            ("look_around", self.look_around),
            ("greet_companion", self.greet_companion),
            ("explore_object", self.explore_object),
        ];
        candidates
            .iter()
            .filter(|(_, v)| *v >= THRESHOLD)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(name, level)| (*name, *level))
    }

    /// Partially satisfy a desire after acting on it.
    pub fn satisfy(&mut self, desire: &str, amount: f32) {
        match desire {
            "look_around" => self.look_around = (self.look_around - amount).max(0.0),
            "greet_companion" => {
                self.greet_companion = (self.greet_companion - amount).max(0.0)
            }
            "explore_object" => self.explore_object = (self.explore_object - amount).max(0.0),
            _ => {}
        }
    }

    /// Boost a desire from an external trigger (novelty / surprise).
    pub fn boost(&mut self, desire: &str, amount: f32) {
        match desire {
            "look_around" => self.look_around = (self.look_around + amount).min(1.0),
            "greet_companion" => {
                self.greet_companion = (self.greet_companion + amount).min(1.0)
            }
            "explore_object" => {
                self.explore_object = (self.explore_object + amount).min(1.0)
            }
            _ => {}
        }
    }

    /// Generate a human-readable desire context for the system prompt.
    /// Returns None if no desire is strong enough to warrant mention.
    pub fn context_string(&self) -> Option<String> {
        match self.strongest() {
            None => None,
            Some((name, level)) => {
                let intensity = if level >= 0.85 {
                    "strongly"
                } else if level >= 0.7 {
                    "moderately"
                } else {
                    "slightly"
                };
                let why = match name {
                    "look_around" => {
                        "I haven't observed the environment recently and feel drawn to check it."
                    }
                    "greet_companion" => {
                        "I miss interacting with my companion and want to acknowledge them."
                    }
                    "explore_object" => {
                        "Something caught my attention and I want to investigate further."
                    }
                    _ => "I feel an urge to do something.",
                };
                let action = match name {
                    "look_around" => "consider using see() or look() to observe the surroundings",
                    "greet_companion" => "consider saying hello or checking in with the companion",
                    "explore_object" => "consider looking more closely at interesting objects",
                    _ => "follow your instinct",
                };
                Some(format!(
                    "Current desire: I {intensity} want to {name}.\n\
                     Why: {why}\n\
                     Suggestion: {action}."
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Default values ─────────────────────────────────────────────

    #[test]
    fn default_initial_values() {
        let ds = DesireState::default();
        assert!((ds.look_around - 0.4).abs() < 1e-6);
        assert!((ds.greet_companion - 0.3).abs() < 1e-6);
        assert!((ds.explore_object - 0.2).abs() < 1e-6);
        assert!((ds.rest - 0.0).abs() < 1e-6);
    }

    // ── decay ─────────────────────────────────────────────────────

    #[test]
    fn decay_grows_look_around_and_greet() {
        let mut ds = DesireState::default();
        let before_look = ds.look_around;
        let before_greet = ds.greet_companion;
        // Add artificial elapsed time by manipulating last_updated
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(100);
        ds.decay();
        assert!(ds.look_around >= before_look);
        assert!(ds.greet_companion >= before_greet);
    }

    #[test]
    fn decay_reduces_explore_object() {
        let mut ds = DesireState::default();
        ds.explore_object = 0.5;
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(100);
        ds.decay();
        assert!(ds.explore_object <= 0.5);
    }

    #[test]
    fn decay_clamps_look_around_at_1() {
        let mut ds = DesireState::default();
        ds.look_around = 0.99;
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(3600);
        ds.decay();
        assert!(ds.look_around <= 1.0);
    }

    #[test]
    fn decay_clamps_explore_object_at_0() {
        let mut ds = DesireState::default();
        ds.explore_object = 0.0;
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(3600);
        ds.decay();
        assert!(ds.explore_object >= 0.0);
    }

    // ── satisfy ───────────────────────────────────────────────────

    #[test]
    fn satisfy_reduces_look_around() {
        let mut ds = DesireState::default();
        ds.look_around = 0.8;
        ds.satisfy("look_around", 0.3);
        assert!((ds.look_around - 0.5).abs() < 1e-5);
    }

    #[test]
    fn satisfy_reduces_greet_companion() {
        let mut ds = DesireState::default();
        ds.greet_companion = 0.7;
        ds.satisfy("greet_companion", 0.4);
        assert!((ds.greet_companion - 0.3).abs() < 1e-5);
    }

    #[test]
    fn satisfy_reduces_explore_object() {
        let mut ds = DesireState::default();
        ds.explore_object = 0.6;
        ds.satisfy("explore_object", 0.6);
        assert!((ds.explore_object - 0.0).abs() < 1e-5);
    }

    #[test]
    fn satisfy_clamps_at_zero() {
        let mut ds = DesireState::default();
        ds.look_around = 0.3;
        ds.satisfy("look_around", 1.0);
        assert!(ds.look_around >= 0.0);
        assert!((ds.look_around - 0.0).abs() < 1e-5);
    }

    #[test]
    fn satisfy_unknown_desire_is_noop() {
        let mut ds = DesireState::default();
        let before = (ds.look_around, ds.greet_companion, ds.explore_object);
        ds.satisfy("nonexistent", 0.5);
        assert!((ds.look_around - before.0).abs() < 1e-6);
        assert!((ds.greet_companion - before.1).abs() < 1e-6);
        assert!((ds.explore_object - before.2).abs() < 1e-6);
    }

    // ── boost ────────────────────────────────────────────────────

    #[test]
    fn boost_increases_look_around() {
        let mut ds = DesireState::default();
        ds.look_around = 0.3;
        ds.boost("look_around", 0.2);
        assert!((ds.look_around - 0.5).abs() < 1e-5);
    }

    #[test]
    fn boost_increases_greet_companion() {
        let mut ds = DesireState::default();
        ds.greet_companion = 0.4;
        ds.boost("greet_companion", 0.3);
        assert!((ds.greet_companion - 0.7).abs() < 1e-5);
    }

    #[test]
    fn boost_increases_explore_object() {
        let mut ds = DesireState::default();
        ds.explore_object = 0.5;
        ds.boost("explore_object", 0.3);
        assert!((ds.explore_object - 0.8).abs() < 1e-5);
    }

    #[test]
    fn boost_clamps_at_one() {
        let mut ds = DesireState::default();
        ds.look_around = 0.9;
        ds.boost("look_around", 0.5);
        assert!(ds.look_around <= 1.0);
        assert!((ds.look_around - 1.0).abs() < 1e-5);
    }

    #[test]
    fn boost_unknown_desire_is_noop() {
        let mut ds = DesireState::default();
        let before = (ds.look_around, ds.greet_companion, ds.explore_object);
        ds.boost("nonexistent", 0.5);
        assert!((ds.look_around - before.0).abs() < 1e-6);
        assert!((ds.greet_companion - before.1).abs() < 1e-6);
        assert!((ds.explore_object - before.2).abs() < 1e-6);
    }

    // ── strongest ────────────────────────────────────────────────

    #[test]
    fn strongest_returns_none_when_all_below_threshold() {
        let ds = DesireState::default(); // 0.4, 0.3, 0.2 — all < 0.6
        assert!(ds.strongest().is_none());
    }

    #[test]
    fn strongest_returns_highest_desire_above_threshold() {
        let mut ds = DesireState::default();
        ds.look_around = 0.7;
        ds.greet_companion = 0.9;
        ds.explore_object = 0.65;
        let (name, level) = ds.strongest().expect("should have a strongest desire");
        assert_eq!(name, "greet_companion");
        assert!((level - 0.9).abs() < 1e-5);
    }

    #[test]
    fn strongest_returns_none_when_exactly_at_threshold_boundary() {
        let mut ds = DesireState::default();
        // THRESHOLD is 0.6; value exactly at 0.6 passes the filter (>=)
        ds.look_around = 0.6;
        let result = ds.strongest();
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "look_around");
    }

    #[test]
    fn strongest_returns_none_just_below_threshold() {
        let mut ds = DesireState::default();
        ds.look_around = 0.599;
        assert!(ds.strongest().is_none());
    }

    // ── context_string ───────────────────────────────────────────

    #[test]
    fn context_string_none_when_no_strong_desire() {
        let ds = DesireState::default(); // all below 0.6
        assert!(ds.context_string().is_none());
    }

    #[test]
    fn context_string_slightly_for_level_below_0_7() {
        let mut ds = DesireState::default();
        ds.look_around = 0.65;
        let ctx = ds.context_string().expect("should produce context");
        assert!(ctx.contains("slightly"));
        assert!(ctx.contains("look_around"));
    }

    #[test]
    fn context_string_moderately_for_level_0_7_to_0_85() {
        let mut ds = DesireState::default();
        ds.greet_companion = 0.75;
        let ctx = ds.context_string().expect("should produce context");
        assert!(ctx.contains("moderately"));
        assert!(ctx.contains("greet_companion"));
    }

    #[test]
    fn context_string_strongly_for_level_above_0_85() {
        let mut ds = DesireState::default();
        ds.explore_object = 0.9;
        let ctx = ds.context_string().expect("should produce context");
        assert!(ctx.contains("strongly"));
        assert!(ctx.contains("explore_object"));
    }

    #[test]
    fn context_string_look_around_contains_why_and_suggestion() {
        let mut ds = DesireState::default();
        ds.look_around = 0.8;
        let ctx = ds.context_string().unwrap();
        assert!(ctx.contains("Why:"));
        assert!(ctx.contains("Suggestion:"));
        assert!(ctx.contains("see()") || ctx.contains("look()"));
    }

    #[test]
    fn context_string_greet_companion_suggests_saying_hello() {
        let mut ds = DesireState::default();
        ds.greet_companion = 0.8;
        let ctx = ds.context_string().unwrap();
        assert!(ctx.contains("companion"));
    }

    #[test]
    fn context_string_explore_object_suggests_looking_closely() {
        let mut ds = DesireState::default();
        ds.explore_object = 0.8;
        let ctx = ds.context_string().unwrap();
        assert!(ctx.contains("interesting"));
    }

    // ── boundary values ──────────────────────────────────────────

    #[test]
    fn context_string_exactly_at_0_85_is_strongly() {
        let mut ds = DesireState::default();
        ds.look_around = 0.85;
        let ctx = ds.context_string().unwrap();
        assert!(ctx.contains("strongly"), "ctx={ctx}");
    }

    #[test]
    fn context_string_just_below_0_85_is_moderately() {
        let mut ds = DesireState::default();
        ds.look_around = 0.849;
        let ctx = ds.context_string().unwrap();
        assert!(ctx.contains("moderately"), "ctx={ctx}");
    }

    #[test]
    fn context_string_exactly_at_0_7_is_moderately() {
        let mut ds = DesireState::default();
        ds.look_around = 0.7;
        let ctx = ds.context_string().unwrap();
        assert!(ctx.contains("moderately"), "ctx={ctx}");
    }

    #[test]
    fn context_string_just_below_0_7_is_slightly() {
        let mut ds = DesireState::default();
        ds.look_around = 0.699;
        let ctx = ds.context_string().unwrap();
        assert!(ctx.contains("slightly"), "ctx={ctx}");
    }

    // ── strongest tie-breaking ────────────────────────────────────

    #[test]
    fn strongest_returns_higher_of_two_above_threshold() {
        let mut ds = DesireState::default();
        ds.look_around = 0.65;
        ds.greet_companion = 0.80;
        ds.explore_object = 0.70;
        let (name, level) = ds.strongest().unwrap();
        assert_eq!(name, "greet_companion");
        assert!((level - 0.80).abs() < 1e-5);
    }

    #[test]
    fn strongest_when_only_one_above_threshold() {
        let mut ds = DesireState::default();
        ds.look_around = 0.3;
        ds.greet_companion = 0.5;
        ds.explore_object = 0.75; // only this one above 0.6
        let (name, _) = ds.strongest().unwrap();
        assert_eq!(name, "explore_object");
    }

    // ── decay cumulative behavior ─────────────────────────────────

    #[test]
    fn decay_is_cumulative_across_multiple_calls() {
        let mut ds = DesireState::default();
        ds.look_around = 0.0;

        // Simulate elapsed time twice
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(50);
        ds.decay();
        let after_first = ds.look_around;

        ds.last_updated = Instant::now() - std::time::Duration::from_secs(50);
        ds.decay();
        let after_second = ds.look_around;

        assert!(after_second > after_first, "Should grow cumulatively");
    }

    #[test]
    fn decay_with_zero_elapsed_time_changes_nothing_significantly() {
        let mut ds = DesireState::default();
        let before = (ds.look_around, ds.greet_companion, ds.explore_object);
        // last_updated is Instant::now(), so elapsed ≈ 0
        ds.decay();
        // With ~0 elapsed time, changes should be negligible (< 0.001)
        assert!((ds.look_around - before.0).abs() < 0.001);
        assert!((ds.greet_companion - before.1).abs() < 0.001);
        assert!((ds.explore_object - before.2).abs() < 0.001);
    }

    // ── satisfy then strongest interaction ────────────────────────

    #[test]
    fn satisfy_brings_desire_below_threshold() {
        let mut ds = DesireState::default();
        ds.look_around = 0.8;
        assert!(ds.strongest().is_some());
        ds.satisfy("look_around", 0.4);
        // 0.8 - 0.4 = 0.4 < 0.6 threshold
        assert!(ds.strongest().is_none(), "look_around={}", ds.look_around);
    }

    // ── boost + satisfy symmetry ──────────────────────────────────

    #[test]
    fn boost_then_satisfy_returns_to_original() {
        let mut ds = DesireState::default();
        ds.look_around = 0.5;
        ds.boost("look_around", 0.2);
        ds.satisfy("look_around", 0.2);
        assert!((ds.look_around - 0.5).abs() < 1e-5);
    }
}
