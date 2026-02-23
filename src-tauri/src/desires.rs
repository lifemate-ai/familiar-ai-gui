/// Desire system — intrinsic motivation for the familiar.
///
/// Desire names are intentionally aligned with the Python MCP version
/// (embodied-claude/desire-system) so the two implementations stay in sync.
///
/// Based on:
/// - 3M-Progress / zebrafish agents (2506.00138): ethological grounding for desires
/// - LLM-Driven Intrinsic Motivation (2508.18420): intentionality reasoning before action
/// - From Curiosity to Competence (2507.08210): controllability bias
use std::time::Instant;

use crate::i18n::{t_lang, Lang};

/// Satisfaction half-life in seconds (time to go from 0 → 1.0 at constant rate).
/// Matches the Python version's SATISFACTION_HOURS values.
const HOURS: f32 = 3600.0;
const RATE_OBSERVE_ROOM: f32 = 1.0 / (0.167 * HOURS); // ~10 min → full
const RATE_LOOK_OUTSIDE: f32 = 1.0 / (1.0 * HOURS);   // 1 h → full
const RATE_BROWSE_CURIOSITY: f32 = 1.0 / (2.0 * HOURS); // 2 h → full
const RATE_MISS_COMPANION: f32 = 1.0 / (3.0 * HOURS);  // 3 h → full

/// Internal desire state. Each field is 0.0 (absent) – 1.0 (overwhelming).
pub struct DesireState {
    /// Observe the room / look around indoors.
    pub observe_room: f32,
    /// Look outside — windows, sky, street.
    pub look_outside: f32,
    /// Browse / search / satisfy curiosity about something.
    pub browse_curiosity: f32,
    /// Miss the companion — want to see or talk to them.
    pub miss_companion: f32,

    last_updated: Instant,
}

impl Default for DesireState {
    fn default() -> Self {
        Self {
            // Start with a nudge of curiosity so the agent is active from boot
            observe_room: 0.4,
            look_outside: 0.2,
            browse_curiosity: 0.1,
            miss_companion: 0.1,
            last_updated: Instant::now(),
        }
    }
}

impl DesireState {
    /// Advance time — unsatisfied desires grow toward 1.0.
    /// Call this at the beginning of every user turn.
    pub fn decay(&mut self) {
        let elapsed = self.last_updated.elapsed().as_secs_f32();

        self.observe_room = (self.observe_room + elapsed * RATE_OBSERVE_ROOM).min(1.0);
        self.look_outside = (self.look_outside + elapsed * RATE_LOOK_OUTSIDE).min(1.0);
        self.browse_curiosity = (self.browse_curiosity + elapsed * RATE_BROWSE_CURIOSITY).min(1.0);
        self.miss_companion = (self.miss_companion + elapsed * RATE_MISS_COMPANION).min(1.0);

        self.last_updated = Instant::now();
    }

    /// Return the strongest desire above the threshold, or None.
    pub fn strongest(&self) -> Option<(&'static str, f32)> {
        const THRESHOLD: f32 = 0.6;
        let candidates: &[(&str, f32)] = &[
            ("observe_room", self.observe_room),
            ("look_outside", self.look_outside),
            ("browse_curiosity", self.browse_curiosity),
            ("miss_companion", self.miss_companion),
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
            "observe_room" => self.observe_room = (self.observe_room - amount).max(0.0),
            "look_outside" => self.look_outside = (self.look_outside - amount).max(0.0),
            "browse_curiosity" => {
                self.browse_curiosity = (self.browse_curiosity - amount).max(0.0)
            }
            "miss_companion" => self.miss_companion = (self.miss_companion - amount).max(0.0),
            _ => {}
        }
    }

    /// Boost a desire from an external trigger (novelty / surprise).
    pub fn boost(&mut self, desire: &str, amount: f32) {
        match desire {
            "observe_room" => self.observe_room = (self.observe_room + amount).min(1.0),
            "look_outside" => self.look_outside = (self.look_outside + amount).min(1.0),
            "browse_curiosity" => {
                self.browse_curiosity = (self.browse_curiosity + amount).min(1.0)
            }
            "miss_companion" => self.miss_companion = (self.miss_companion + amount).min(1.0),
            _ => {}
        }
    }

    /// Generate a human-readable desire context for the system prompt
    /// using the system language.
    pub fn context_string(&self) -> Option<String> {
        self.context_string_lang(crate::i18n::lang())
    }

    /// Generate a desire context string for a specific language (also used in tests).
    pub fn context_string_lang(&self, lang: Lang) -> Option<String> {
        let (name, level) = self.strongest()?;

        let intensity_key = if level >= 0.85 {
            "intensity_strongly"
        } else if level >= 0.7 {
            "intensity_moderately"
        } else {
            "intensity_slightly"
        };
        let intensity = t_lang(intensity_key, lang);

        let (why, action) = match name {
            "observe_room" => (
                t_lang("desire_observe_room_why", lang),
                t_lang("desire_observe_room_action", lang),
            ),
            "look_outside" => (
                t_lang("desire_look_outside_why", lang),
                t_lang("desire_look_outside_action", lang),
            ),
            "browse_curiosity" => (
                t_lang("desire_browse_curiosity_why", lang),
                t_lang("desire_browse_curiosity_action", lang),
            ),
            "miss_companion" => (
                t_lang("desire_miss_companion_why", lang),
                t_lang("desire_miss_companion_action", lang),
            ),
            _ => ("I feel an urge to do something.", "follow your instinct"),
        };

        Some(format!(
            "Current desire: I {intensity} want to {name}.\n\
             Why: {why}\n\
             Suggestion: {action}."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    // ── Default values ─────────────────────────────────────────────

    #[test]
    fn default_observe_room_is_0_4() {
        let ds = DesireState::default();
        assert!((ds.observe_room - 0.4).abs() < 1e-6);
    }

    #[test]
    fn default_other_desires_below_threshold() {
        let ds = DesireState::default();
        assert!(ds.look_outside < 0.6);
        assert!(ds.browse_curiosity < 0.6);
        assert!(ds.miss_companion < 0.6);
    }

    // ── decay ─────────────────────────────────────────────────────

    #[test]
    fn decay_grows_all_desires() {
        let mut ds = DesireState::default();
        let before = (ds.observe_room, ds.look_outside, ds.browse_curiosity, ds.miss_companion);
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(600);
        ds.decay();
        assert!(ds.observe_room >= before.0);
        assert!(ds.look_outside >= before.1);
        assert!(ds.browse_curiosity >= before.2);
        assert!(ds.miss_companion >= before.3);
    }

    #[test]
    fn decay_clamps_at_1() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.99;
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(3600);
        ds.decay();
        assert!(ds.observe_room <= 1.0);
    }

    #[test]
    fn decay_with_zero_elapsed_changes_nothing_significantly() {
        let mut ds = DesireState::default();
        let before = (ds.observe_room, ds.look_outside, ds.browse_curiosity, ds.miss_companion);
        ds.decay();
        assert!((ds.observe_room - before.0).abs() < 0.001);
        assert!((ds.look_outside - before.1).abs() < 0.001);
        assert!((ds.browse_curiosity - before.2).abs() < 0.001);
        assert!((ds.miss_companion - before.3).abs() < 0.001);
    }

    #[test]
    fn observe_room_reaches_threshold_in_roughly_10_min() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.0;
        // 10 min = 600 s; RATE_OBSERVE_ROOM = 1.0 / (0.167 * 3600) ≈ 0.00166/s
        // 600 * 0.00166 ≈ 1.0 → should be at or near 1.0
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(600);
        ds.decay();
        assert!(ds.observe_room >= 0.9, "observe_room={}", ds.observe_room);
    }

    // ── satisfy ───────────────────────────────────────────────────

    #[test]
    fn satisfy_reduces_observe_room() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.8;
        ds.satisfy("observe_room", 0.3);
        assert!((ds.observe_room - 0.5).abs() < 1e-5);
    }

    #[test]
    fn satisfy_reduces_look_outside() {
        let mut ds = DesireState::default();
        ds.look_outside = 0.7;
        ds.satisfy("look_outside", 0.4);
        assert!((ds.look_outside - 0.3).abs() < 1e-5);
    }

    #[test]
    fn satisfy_reduces_browse_curiosity() {
        let mut ds = DesireState::default();
        ds.browse_curiosity = 0.6;
        ds.satisfy("browse_curiosity", 0.6);
        assert!((ds.browse_curiosity - 0.0).abs() < 1e-5);
    }

    #[test]
    fn satisfy_reduces_miss_companion() {
        let mut ds = DesireState::default();
        ds.miss_companion = 0.9;
        ds.satisfy("miss_companion", 0.5);
        assert!((ds.miss_companion - 0.4).abs() < 1e-5);
    }

    #[test]
    fn satisfy_clamps_at_zero() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.3;
        ds.satisfy("observe_room", 1.0);
        assert!(ds.observe_room >= 0.0);
        assert!((ds.observe_room - 0.0).abs() < 1e-5);
    }

    #[test]
    fn satisfy_unknown_desire_is_noop() {
        let mut ds = DesireState::default();
        let before = (ds.observe_room, ds.look_outside, ds.browse_curiosity, ds.miss_companion);
        ds.satisfy("nonexistent", 0.5);
        assert!((ds.observe_room - before.0).abs() < 1e-6);
        assert!((ds.look_outside - before.1).abs() < 1e-6);
        assert!((ds.browse_curiosity - before.2).abs() < 1e-6);
        assert!((ds.miss_companion - before.3).abs() < 1e-6);
    }

    // ── boost ────────────────────────────────────────────────────

    #[test]
    fn boost_increases_observe_room() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.3;
        ds.boost("observe_room", 0.2);
        assert!((ds.observe_room - 0.5).abs() < 1e-5);
    }

    #[test]
    fn boost_increases_look_outside() {
        let mut ds = DesireState::default();
        ds.look_outside = 0.4;
        ds.boost("look_outside", 0.3);
        assert!((ds.look_outside - 0.7).abs() < 1e-5);
    }

    #[test]
    fn boost_increases_browse_curiosity() {
        let mut ds = DesireState::default();
        ds.browse_curiosity = 0.5;
        ds.boost("browse_curiosity", 0.3);
        assert!((ds.browse_curiosity - 0.8).abs() < 1e-5);
    }

    #[test]
    fn boost_increases_miss_companion() {
        let mut ds = DesireState::default();
        ds.miss_companion = 0.4;
        ds.boost("miss_companion", 0.3);
        assert!((ds.miss_companion - 0.7).abs() < 1e-5);
    }

    #[test]
    fn boost_clamps_at_one() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.9;
        ds.boost("observe_room", 0.5);
        assert!(ds.observe_room <= 1.0);
        assert!((ds.observe_room - 1.0).abs() < 1e-5);
    }

    #[test]
    fn boost_unknown_desire_is_noop() {
        let mut ds = DesireState::default();
        let before = (ds.observe_room, ds.look_outside, ds.browse_curiosity, ds.miss_companion);
        ds.boost("nonexistent", 0.5);
        assert!((ds.observe_room - before.0).abs() < 1e-6);
        assert!((ds.look_outside - before.1).abs() < 1e-6);
        assert!((ds.browse_curiosity - before.2).abs() < 1e-6);
        assert!((ds.miss_companion - before.3).abs() < 1e-6);
    }

    // ── strongest ────────────────────────────────────────────────

    #[test]
    fn strongest_returns_none_when_all_below_threshold() {
        let ds = DesireState::default(); // 0.4, 0.2, 0.1, 0.1 — all < 0.6
        assert!(ds.strongest().is_none());
    }

    #[test]
    fn strongest_returns_highest_above_threshold() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.7;
        ds.miss_companion = 0.9;
        ds.browse_curiosity = 0.65;
        let (name, level) = ds.strongest().expect("should have a strongest desire");
        assert_eq!(name, "miss_companion");
        assert!((level - 0.9).abs() < 1e-5);
    }

    #[test]
    fn strongest_at_threshold_is_included() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.6;
        let result = ds.strongest();
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "observe_room");
    }

    #[test]
    fn strongest_just_below_threshold_is_none() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.599;
        assert!(ds.strongest().is_none());
    }

    // ── context_string ───────────────────────────────────────────

    #[test]
    fn context_string_none_when_no_strong_desire() {
        let ds = DesireState::default();
        assert!(ds.context_string_lang(Lang::En).is_none());
    }

    #[test]
    fn context_string_slightly_for_level_below_0_7() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.65;
        let ctx = ds.context_string_lang(Lang::En).expect("should produce context");
        assert!(ctx.contains("slightly"));
        assert!(ctx.contains("observe_room"));
    }

    #[test]
    fn context_string_moderately_for_0_7_to_0_85() {
        let mut ds = DesireState::default();
        ds.look_outside = 0.75;
        let ctx = ds.context_string_lang(Lang::En).expect("should produce context");
        assert!(ctx.contains("moderately"));
        assert!(ctx.contains("look_outside"));
    }

    #[test]
    fn context_string_strongly_for_above_0_85() {
        let mut ds = DesireState::default();
        ds.browse_curiosity = 0.9;
        let ctx = ds.context_string_lang(Lang::En).expect("should produce context");
        assert!(ctx.contains("strongly"));
        assert!(ctx.contains("browse_curiosity"));
    }

    #[test]
    fn context_string_contains_why_and_suggestion() {
        let mut ds = DesireState::default();
        ds.miss_companion = 0.8;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("Why:"));
        assert!(ctx.contains("Suggestion:"));
    }

    #[test]
    fn context_string_observe_room_suggests_looking() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.8;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("look") || ctx.contains("see"));
    }

    #[test]
    fn context_string_look_outside_suggests_window() {
        let mut ds = DesireState::default();
        ds.look_outside = 0.8;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("window") || ctx.contains("outside"));
    }

    #[test]
    fn context_string_browse_curiosity_suggests_search() {
        let mut ds = DesireState::default();
        ds.browse_curiosity = 0.8;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("search") || ctx.contains("curiosity") || ctx.contains("interesting"));
    }

    #[test]
    fn context_string_miss_companion_suggests_greeting() {
        let mut ds = DesireState::default();
        ds.miss_companion = 0.8;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("companion") || ctx.contains("hello"));
    }

    // ── intensity boundaries ──────────────────────────────────────

    #[test]
    fn context_string_exactly_0_85_is_strongly() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.85;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("strongly"), "ctx={ctx}");
    }

    #[test]
    fn context_string_just_below_0_85_is_moderately() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.849;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("moderately"), "ctx={ctx}");
    }

    #[test]
    fn context_string_exactly_0_7_is_moderately() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.7;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("moderately"), "ctx={ctx}");
    }

    #[test]
    fn context_string_just_below_0_7_is_slightly() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.699;
        let ctx = ds.context_string_lang(Lang::En).unwrap();
        assert!(ctx.contains("slightly"), "ctx={ctx}");
    }

    // ── satisfy + strongest interaction ───────────────────────────

    #[test]
    fn satisfy_brings_desire_below_threshold() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.8;
        assert!(ds.strongest().is_some());
        ds.satisfy("observe_room", 0.4);
        assert!(ds.strongest().is_none(), "observe_room={}", ds.observe_room);
    }

    #[test]
    fn boost_then_satisfy_returns_to_original() {
        let mut ds = DesireState::default();
        ds.browse_curiosity = 0.5;
        ds.boost("browse_curiosity", 0.2);
        ds.satisfy("browse_curiosity", 0.2);
        assert!((ds.browse_curiosity - 0.5).abs() < 1e-5);
    }

    // ── decay cumulative behavior ─────────────────────────────────

    #[test]
    fn decay_is_cumulative() {
        let mut ds = DesireState::default();
        ds.observe_room = 0.0;
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(50);
        ds.decay();
        let after_first = ds.observe_room;
        ds.last_updated = Instant::now() - std::time::Duration::from_secs(50);
        ds.decay();
        assert!(ds.observe_room > after_first);
    }
}
