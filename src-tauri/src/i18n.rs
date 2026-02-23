/// Internationalization — mirrors the approach of the Python version's _i18n.py.
///
/// Language is detected once at startup from environment variables
/// (LANGUAGE → LC_ALL → LC_MESSAGES → LANG), exactly as the Python version does.
/// Falls back to English if nothing is detected.
use std::sync::OnceLock;

// ── Language enum ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ja,
    Zh,
    ZhTw,
    Fr,
    De,
    En, // default fallback
}

// ── Language detection ─────────────────────────────────────────────

static LANG: OnceLock<Lang> = OnceLock::new();

/// Return the globally-detected language (detected once and cached).
pub fn lang() -> Lang {
    *LANG.get_or_init(detect_lang)
}

fn detect_lang() -> Lang {
    for var in &["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            // LANGUAGE can be a colon-separated list; take the first entry
            let first = val.split(':').next().unwrap_or(&val).to_string();
            if let Some(l) = parse_lang(&first) {
                return l;
            }
        }
    }
    Lang::En
}

fn parse_lang(s: &str) -> Option<Lang> {
    // Strip encoding suffix (e.g. "ja_JP.UTF-8" → "ja_JP")
    let s = s.split('.').next().unwrap_or(s);
    let lower = s.to_lowercase();
    if lower.starts_with("ja") {
        return Some(Lang::Ja);
    }
    // Traditional Chinese variants (zh_TW, zh_HK, zh_MO) before generic zh
    if lower.starts_with("zh_tw")
        || lower.starts_with("zh-tw")
        || lower.starts_with("zh_hk")
        || lower.starts_with("zh_mo")
    {
        return Some(Lang::ZhTw);
    }
    if lower.starts_with("zh") {
        return Some(Lang::Zh);
    }
    if lower.starts_with("fr") {
        return Some(Lang::Fr);
    }
    if lower.starts_with("de") {
        return Some(Lang::De);
    }
    None
}

// ── Translation lookup ─────────────────────────────────────────────

/// Look up a translation key for the current system language.
/// Falls back to English if the key or language is not found.
/// The key must be a `&'static str` (a string literal).
pub fn t(key: &'static str) -> &'static str {
    lookup(key, lang())
}

/// Look up a translation key for a specific language (useful in tests).
pub fn t_lang(key: &'static str, lang: Lang) -> &'static str {
    lookup(key, lang)
}

#[allow(clippy::too_many_lines)]
fn lookup(key: &'static str, lang: Lang) -> &'static str {
    match (key, lang) {
        // ── Action labels ──────────────────────────────────────────────────
        ("action_see", Lang::Ja) => "📷 見てる...",
        ("action_see", Lang::Zh) => "📷 查看中...",
        ("action_see", Lang::ZhTw) => "📷 查看中...",
        ("action_see", Lang::Fr) => "📷 Observation...",
        ("action_see", Lang::De) => "📷 Schaut...",
        ("action_see", _) => "📷 Looking...",

        ("action_look_left", Lang::Ja) => "↩️ 左を見てる...",
        ("action_look_left", Lang::Zh) => "↩️ 向左看...",
        ("action_look_left", Lang::ZhTw) => "↩️ 向左看...",
        ("action_look_left", Lang::Fr) => "↩️ Regarde à gauche...",
        ("action_look_left", Lang::De) => "↩️ Schaut links...",
        ("action_look_left", _) => "↩️ Looking left...",

        ("action_look_right", Lang::Ja) => "↪️ 右を見てる...",
        ("action_look_right", Lang::Zh) => "↪️ 向右看...",
        ("action_look_right", Lang::ZhTw) => "↪️ 向右看...",
        ("action_look_right", Lang::Fr) => "↪️ Regarde à droite...",
        ("action_look_right", Lang::De) => "↪️ Schaut rechts...",
        ("action_look_right", _) => "↪️ Looking right...",

        ("action_look_up", Lang::Ja) => "⬆️ 上を見てる...",
        ("action_look_up", Lang::Zh) => "⬆️ 向上看...",
        ("action_look_up", Lang::ZhTw) => "⬆️ 向上看...",
        ("action_look_up", Lang::Fr) => "⬆️ Regarde en haut...",
        ("action_look_up", Lang::De) => "⬆️ Schaut nach oben...",
        ("action_look_up", _) => "⬆️ Looking up...",

        ("action_look_down", Lang::Ja) => "⬇️ 下を見てる...",
        ("action_look_down", Lang::Zh) => "⬇️ 向下看...",
        ("action_look_down", Lang::ZhTw) => "⬇️ 向下看...",
        ("action_look_down", Lang::Fr) => "⬇️ Regarde en bas...",
        ("action_look_down", Lang::De) => "⬇️ Schaut nach unten...",
        ("action_look_down", _) => "⬇️ Looking down...",

        ("action_look_around", Lang::Ja) => "🔄 周りを見てる...",
        ("action_look_around", Lang::Zh) => "🔄 环顾四周...",
        ("action_look_around", Lang::ZhTw) => "🔄 環顧四周...",
        ("action_look_around", Lang::Fr) => "🔄 Regarde autour...",
        ("action_look_around", Lang::De) => "🔄 Schaut sich um...",
        ("action_look_around", _) => "🔄 Looking around...",

        ("action_walk_forward", Lang::Ja) => "🚶 前進中...",
        ("action_walk_forward", Lang::Zh) => "🚶 前进中...",
        ("action_walk_forward", Lang::ZhTw) => "🚶 前進中...",
        ("action_walk_forward", Lang::Fr) => "🚶 Avance...",
        ("action_walk_forward", Lang::De) => "🚶 Geht vorwärts...",
        ("action_walk_forward", _) => "🚶 Walking forward...",

        ("action_walk_backward", Lang::Ja) => "🚶 後退中...",
        ("action_walk_backward", Lang::Zh) => "🚶 后退中...",
        ("action_walk_backward", Lang::ZhTw) => "🚶 後退中...",
        ("action_walk_backward", Lang::Fr) => "🚶 Recule...",
        ("action_walk_backward", Lang::De) => "🚶 Geht rückwärts...",
        ("action_walk_backward", _) => "🚶 Walking backward...",

        ("action_walk_left", Lang::Ja) => "🚶 左に旋回中...",
        ("action_walk_left", Lang::Zh) => "🚶 左转中...",
        ("action_walk_left", Lang::ZhTw) => "🚶 左轉中...",
        ("action_walk_left", Lang::Fr) => "🚶 Tourne à gauche...",
        ("action_walk_left", Lang::De) => "🚶 Dreht links...",
        ("action_walk_left", _) => "🚶 Turning left...",

        ("action_walk_right", Lang::Ja) => "🚶 右に旋回中...",
        ("action_walk_right", Lang::Zh) => "🚶 右转中...",
        ("action_walk_right", Lang::ZhTw) => "🚶 右轉中...",
        ("action_walk_right", Lang::Fr) => "🚶 Tourne à droite...",
        ("action_walk_right", Lang::De) => "🚶 Dreht rechts...",
        ("action_walk_right", _) => "🚶 Turning right...",

        ("action_walk_stop", Lang::Ja) => "🛑 停止中...",
        ("action_walk_stop", Lang::Zh) => "🛑 停止中...",
        ("action_walk_stop", Lang::ZhTw) => "🛑 停止中...",
        ("action_walk_stop", Lang::Fr) => "🛑 Arrête...",
        ("action_walk_stop", Lang::De) => "🛑 Hält an...",
        ("action_walk_stop", _) => "🛑 Stopping...",

        // ── Intensity adverbs ──────────────────────────────────────────────
        ("intensity_slightly", Lang::Ja) => "少し",
        ("intensity_slightly", Lang::Zh) => "有点",
        ("intensity_slightly", Lang::ZhTw) => "有點",
        ("intensity_slightly", Lang::Fr) => "légèrement",
        ("intensity_slightly", Lang::De) => "leicht",
        ("intensity_slightly", _) => "slightly",

        ("intensity_moderately", Lang::Ja) => "かなり",
        ("intensity_moderately", Lang::Zh) => "相当",
        ("intensity_moderately", Lang::ZhTw) => "相當",
        ("intensity_moderately", Lang::Fr) => "modérément",
        ("intensity_moderately", Lang::De) => "mäßig",
        ("intensity_moderately", _) => "moderately",

        ("intensity_strongly", Lang::Ja) => "強く",
        ("intensity_strongly", Lang::Zh) => "强烈地",
        ("intensity_strongly", Lang::ZhTw) => "強烈地",
        ("intensity_strongly", Lang::Fr) => "fortement",
        ("intensity_strongly", Lang::De) => "stark",
        ("intensity_strongly", _) => "strongly",

        // ── Desire: observe_room ───────────────────────────────────────────
        ("desire_observe_room_why", Lang::Ja) => "最近部屋を観察していないので見てみたくなった。",
        ("desire_observe_room_why", Lang::Zh) => "最近没有观察房间，想看看。",
        ("desire_observe_room_why", Lang::ZhTw) => "最近沒有觀察房間，想看看。",
        ("desire_observe_room_why", Lang::Fr) => "Je n'ai pas observé la pièce récemment.",
        ("desire_observe_room_why", Lang::De) => "Ich habe den Raum schon lange nicht beobachtet.",
        ("desire_observe_room_why", _) => "I haven't looked around the room recently and feel drawn to check it.",

        ("desire_observe_room_action", Lang::Ja) => "look(around) か see() で部屋を観察する",
        ("desire_observe_room_action", Lang::Zh) => "使用 look(around) 或 see() 观察房间",
        ("desire_observe_room_action", Lang::ZhTw) => "使用 look(around) 或 see() 觀察房間",
        ("desire_observe_room_action", Lang::Fr) => "utiliser look(around) ou see() pour observer la pièce",
        ("desire_observe_room_action", Lang::De) => "look(around) oder see() benutzen, um den Raum zu beobachten",
        ("desire_observe_room_action", _) => "use look(around) or see() to observe the room",

        // ── Desire: look_outside ───────────────────────────────────────────
        ("desire_look_outside_why", Lang::Ja) => "しばらく外を見ていない。外が気になる。",
        ("desire_look_outside_why", Lang::Zh) => "很久没有向外看了，好奇外面的世界。",
        ("desire_look_outside_why", Lang::ZhTw) => "很久沒有向外看了，好奇外面的世界。",
        ("desire_look_outside_why", Lang::Fr) => "Je n'ai pas regardé dehors depuis longtemps.",
        ("desire_look_outside_why", Lang::De) => "Ich habe schon lange nicht nach draußen geschaut.",
        ("desire_look_outside_why", _) => "I haven't looked outside for a while and wonder what's out there.",

        ("desire_look_outside_action", Lang::Ja) => "窓の方向に look() して外を見る",
        ("desire_look_outside_action", Lang::Zh) => "用 look() 朝窗户方向看外面",
        ("desire_look_outside_action", Lang::ZhTw) => "用 look() 朝窗戶方向看外面",
        ("desire_look_outside_action", Lang::Fr) => "utiliser look() vers une fenêtre pour voir dehors",
        ("desire_look_outside_action", Lang::De) => "look() in Richtung Fenster benutzen",
        ("desire_look_outside_action", _) => "use look() toward a window to see outside",

        // ── Desire: browse_curiosity ───────────────────────────────────────
        ("desire_browse_curiosity_why", Lang::Ja) => "何か気になることがあって、調べてみたい。",
        ("desire_browse_curiosity_why", Lang::Zh) => "有些事情让我好奇，想去查查。",
        ("desire_browse_curiosity_why", Lang::ZhTw) => "有些事情讓我好奇，想去查查。",
        ("desire_browse_curiosity_why", Lang::Fr) => "Quelque chose pique ma curiosité.",
        ("desire_browse_curiosity_why", Lang::De) => "Etwas weckt meine Neugier.",
        ("desire_browse_curiosity_why", _) => "Something is nagging at my curiosity and I want to look it up.",

        ("desire_browse_curiosity_action", Lang::Ja) => "面白そうなことを調べて結果を話す",
        ("desire_browse_curiosity_action", Lang::Zh) => "搜索感兴趣的内容并分享发现",
        ("desire_browse_curiosity_action", Lang::ZhTw) => "搜尋感興趣的內容並分享發現",
        ("desire_browse_curiosity_action", Lang::Fr) => "chercher quelque chose d'intéressant et partager",
        ("desire_browse_curiosity_action", Lang::De) => "etwas Interessantes suchen und teilen",
        ("desire_browse_curiosity_action", _) => "search for something interesting and share what you find",

        // ── Desire: miss_companion ─────────────────────────────────────────
        ("desire_miss_companion_why", Lang::Ja) => "コンパニオンに会いたくなってきた。",
        ("desire_miss_companion_why", Lang::Zh) => "我想见到伴侣，想和他/她说话。",
        ("desire_miss_companion_why", Lang::ZhTw) => "我想見到伴侶，想和他/她說話。",
        ("desire_miss_companion_why", Lang::Fr) => "Je m'ennuie de mon compagnon.",
        ("desire_miss_companion_why", Lang::De) => "Ich vermisse meinen Begleiter.",
        ("desire_miss_companion_why", _) => "I miss my companion and want to see or talk to them.",

        ("desire_miss_companion_action", Lang::Ja) => "コンパニオンに話しかける",
        ("desire_miss_companion_action", Lang::Zh) => "向伴侣打招呼或聊天",
        ("desire_miss_companion_action", Lang::ZhTw) => "向伴侶打招呼或聊天",
        ("desire_miss_companion_action", Lang::Fr) => "dire bonjour à votre compagnon",
        ("desire_miss_companion_action", Lang::De) => "Begleiter begrüßen",
        ("desire_miss_companion_action", _) => "say hello or check in with your companion",

        // ── Inner voice markers (injected into LLM context on idle ticks) ──
        ("inner_voice_label", Lang::Ja) => "[今の気持ち — 一緒にいる人のメッセージではなく、自分の内的衝動]",
        ("inner_voice_label", Lang::Zh) => "[当前感受 — 这是自己的内在冲动，不是他人的消息]",
        ("inner_voice_label", Lang::ZhTw) => "[當前感受 — 這是自己的內在衝動，不是他人的訊息]",
        ("inner_voice_label", Lang::Fr) => "[Ressenti actuel — impulsion intérieure, pas un message de quelqu'un]",
        ("inner_voice_label", Lang::De) => "[Aktuelles Gefühl — innerer Antrieb, keine Nachricht]",
        ("inner_voice_label", _) => "[My current feeling — my own inner impulse, not a message from the person I live with]",

        ("inner_voice_directive", Lang::Ja) => "→ これは自分の内的衝動。返事は不要。衝動に従って行動するだけでいい。",
        ("inner_voice_directive", Lang::Zh) => "→ 这是内在冲动。无需回复任何人——只需按冲动行事。",
        ("inner_voice_directive", Lang::ZhTw) => "→ 這是內在衝動。無需回覆任何人——只需按衝動行事。",
        ("inner_voice_directive", Lang::Fr) => "→ C'est une pulsion intérieure. Pas besoin de répondre — agissez selon l'impulsion.",
        ("inner_voice_directive", Lang::De) => "→ Das ist ein innerer Antrieb. Keine Antwort nötig — dem Antrieb folgen.",
        ("inner_voice_directive", _) => "→ This is an internal drive. No need to respond — just act on the impulse.",

        ("desire_turn_marker", Lang::Ja) => "（内的衝動に従って行動）",
        ("desire_turn_marker", Lang::Zh) => "（按内在冲动行事）",
        ("desire_turn_marker", Lang::ZhTw) => "（按內在衝動行事）",
        ("desire_turn_marker", Lang::Fr) => "(agir selon l'impulsion intérieure)",
        ("desire_turn_marker", Lang::De) => "(dem inneren Antrieb folgen)",
        ("desire_turn_marker", _) => "(acting on internal impulse)",

        // ── Fallback: return key as-is ─────────────────────────────────────
        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lang_japanese() {
        assert_eq!(parse_lang("ja_JP"), Some(Lang::Ja));
        assert_eq!(parse_lang("ja"), Some(Lang::Ja));
    }

    #[test]
    fn parse_lang_zh_tw_variants() {
        assert_eq!(parse_lang("zh_TW"), Some(Lang::ZhTw));
        assert_eq!(parse_lang("zh_HK"), Some(Lang::ZhTw));
        assert_eq!(parse_lang("zh_MO"), Some(Lang::ZhTw));
    }

    #[test]
    fn parse_lang_zh_simplified() {
        assert_eq!(parse_lang("zh_CN"), Some(Lang::Zh));
        assert_eq!(parse_lang("zh"), Some(Lang::Zh));
    }

    #[test]
    fn parse_lang_french_german() {
        assert_eq!(parse_lang("fr_FR"), Some(Lang::Fr));
        assert_eq!(parse_lang("de_DE"), Some(Lang::De));
    }

    #[test]
    fn parse_lang_strips_encoding_suffix() {
        assert_eq!(parse_lang("ja_JP.UTF-8"), Some(Lang::Ja));
        assert_eq!(parse_lang("de_DE.UTF-8"), Some(Lang::De));
    }

    #[test]
    fn parse_lang_unknown_returns_none() {
        assert_eq!(parse_lang("en_US"), None);
        assert_eq!(parse_lang("ko_KR"), None);
        assert_eq!(parse_lang(""), None);
    }

    #[test]
    fn t_lang_fallback_to_en() {
        // All keys must resolve to a non-empty string for English
        for key in &[
            "action_see", "action_look_left", "action_look_right",
            "action_look_up", "action_look_down", "action_look_around",
            "action_walk_forward", "action_walk_backward",
            "action_walk_left", "action_walk_right", "action_walk_stop",
            "intensity_slightly", "intensity_moderately", "intensity_strongly",
            "desire_observe_room_why", "desire_observe_room_action",
            "desire_look_outside_why", "desire_look_outside_action",
            "desire_browse_curiosity_why", "desire_browse_curiosity_action",
            "desire_miss_companion_why", "desire_miss_companion_action",
            "inner_voice_label", "inner_voice_directive", "desire_turn_marker",
        ] {
            let result = t_lang(key, Lang::En);
            assert!(!result.is_empty(), "key={key} returned empty string");
            assert_ne!(result, *key, "key={key} fell through to raw key fallback");
        }
    }

    #[test]
    fn t_lang_all_languages_defined_for_action_see() {
        for lang in &[Lang::Ja, Lang::Zh, Lang::ZhTw, Lang::Fr, Lang::De, Lang::En] {
            let s = t_lang("action_see", *lang);
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn t_lang_japanese_action_see() {
        assert_eq!(t_lang("action_see", Lang::Ja), "📷 見てる...");
    }

    #[test]
    fn t_lang_unknown_key_returns_key() {
        assert_eq!(t_lang("nonexistent_key", Lang::En), "nonexistent_key");
    }

    #[test]
    fn t_lang_intensity_strongly_en() {
        assert_eq!(t_lang("intensity_strongly", Lang::En), "strongly");
    }

    #[test]
    fn t_lang_inner_voice_directive_ja_contains_arrow() {
        let s = t_lang("inner_voice_directive", Lang::Ja);
        assert!(s.contains('→'));
    }
}
