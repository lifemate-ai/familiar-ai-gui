/// Memory tool — episodic long-term memory.
///
/// Faithful Rust port of the Python familiar-ai observation memory system.
///
/// Storage  : SQLite (~/.familiar_ai/observations.db) — same path as Python version
/// Embedding: fastembed multilingual-e5-small (384d, intfloat/multilingual-e5-small)
/// Recall   : 3-tier fallback — vector similarity → LIKE keyword → recency
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rusqlite::{params, Connection};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::backend::ToolDef;

use super::ToolOutput;

// ── Global model singleton ────────────────────────────────────────
// fastembed model is expensive to load (~seconds); cache globally.
static EMBEDDING_MODEL: OnceLock<Mutex<Option<fastembed::TextEmbedding>>> = OnceLock::new();

fn get_model_lock() -> &'static Mutex<Option<fastembed::TextEmbedding>> {
    EMBEDDING_MODEL.get_or_init(|| {
        let model = fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(fastembed::EmbeddingModel::MultilingualE5Small),
        )
        .ok();
        Mutex::new(model)
    })
}

// ── DB path ───────────────────────────────────────────────────────
fn db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".familiar_ai")
        .join("observations.db")
}

// ── Public struct ─────────────────────────────────────────────────

pub struct MemoryTool {
    db_path: PathBuf,
}

impl MemoryTool {
    pub fn new(custom_path: Option<PathBuf>) -> Self {
        // Trigger model loading in the background on first MemoryTool creation.
        // The lock call is intentionally fire-and-forget.
        let _ = get_model_lock();
        Self {
            db_path: custom_path.unwrap_or_else(db_path),
        }
    }

    // ── Tool definitions (Python-compatible) ──────────────────────

    pub fn tool_defs() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "remember".to_string(),
                description: "Save something to long-term memory. Use this to remember important \
                              things: what you saw, what happened, how you felt, conversations. \
                              If you just took a photo with see(), pass the image_path to attach it."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "What to remember (1-3 sentences)."
                        },
                        "emotion": {
                            "type": "string",
                            "enum": ["neutral", "happy", "sad", "curious", "excited", "moved"],
                            "description": "Emotional tone of this memory."
                        },
                        "image_path": {
                            "type": "string",
                            "description": "Optional path to an image file to attach (e.g. from see())."
                        }
                    },
                    "required": ["content"]
                }),
            },
            ToolDef {
                name: "recall".to_string(),
                description: "Search long-term memory for things related to a topic. \
                              Use this to remember past observations, conversations, or feelings."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "What to search for."
                        },
                        "n": {
                            "type": "integer",
                            "description": "Number of memories to return (default 3)."
                        }
                    },
                    "required": ["query"]
                }),
            },
        ]
    }

    // ── remember ──────────────────────────────────────────────────

    pub fn remember(
        &self,
        content: &str,
        emotion: &str,
        image_path: Option<&str>,
    ) -> Result<ToolOutput> {
        let conn = self.open_db()?;
        let id = uuid::Uuid::new_v4().to_string();
        let (ts, date, time_str) = now_parts();

        // Thumbnail: resize to 320×240, JPEG q60, store as base64
        let (stored_path, stored_data): (Option<String>, Option<String>) = match image_path {
            Some(p) => (Some(p.to_string()), make_thumbnail(p)),
            None => (None, None),
        };

        conn.execute(
            "INSERT INTO observations \
             (id, content, timestamp, date, time, direction, kind, emotion, image_path, image_data) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                id,
                content,
                ts,
                date,
                time_str,
                "unknown",
                "observation",
                emotion,
                stored_path,
                stored_data,
            ],
        )?;

        // Embed and store (best-effort; silently skipped if model not ready)
        if let Ok(guard) = get_model_lock().lock() {
            if let Some(model) = guard.as_ref() {
                let text = format!("passage: {content}");
                if let Ok(embeds) = model.embed(vec![text.as_str()], None) {
                    if let Some(vec) = embeds.into_iter().next() {
                        let bytes: Vec<u8> =
                            vec.iter().flat_map(|f| f.to_le_bytes()).collect();
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO obs_embeddings (obs_id, vector) VALUES (?1,?2)",
                            params![id, bytes],
                        );
                    }
                }
            }
        }

        let suffix = if stored_path.is_some() { " (with image)" } else { "" };
        let preview = &content[..content.len().min(60)];
        Ok((format!("Remembered{suffix}: {preview}"), None))
    }

    // ── recall (3-tier) ───────────────────────────────────────────

    pub fn recall_memories(&self, query: &str, n: usize) -> Result<ToolOutput> {
        let conn = self.open_db()?;
        let n = n.clamp(1, 20);

        // Tier 1: vector similarity
        if let Ok(guard) = get_model_lock().lock() {
            if let Some(model) = guard.as_ref() {
                let q_text = format!("query: {query}");
                if let Ok(embeds) = model.embed(vec![q_text.as_str()], None) {
                    if let Some(q_vec) = embeds.into_iter().next() {
                        let rows = self.vector_search(&conn, &q_vec, n)?;
                        if !rows.is_empty() {
                            return Ok((format_memories(&rows), None));
                        }
                    }
                }
            }
        }

        // Tier 2: LIKE keyword
        let rows = self.keyword_search(&conn, query, n)?;
        if !rows.is_empty() {
            return Ok((format_memories(&rows), None));
        }

        // Tier 3: most recent
        let rows = self.recent_search(&conn, n)?;
        if rows.is_empty() {
            Ok(("No relevant memories found.".to_string(), None))
        } else {
            Ok((format_memories(&rows), None))
        }
    }

    /// Return recent memories as compact text for the system prompt.
    pub fn recall_for_context(&self, n: usize) -> String {
        let conn = match self.open_db() {
            Ok(c) => c,
            Err(_) => return String::new(),
        };
        let rows = self.recent_search(&conn, n).unwrap_or_default();
        if rows.is_empty() {
            return String::new();
        }
        rows.iter()
            .map(|r| format!("  - [{} {}] {}", r.date, r.time, r.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // ── Private: DB helpers ───────────────────────────────────────

    fn open_db(&self) -> Result<Connection> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; \
             PRAGMA synchronous=NORMAL; \
             PRAGMA foreign_keys=ON;",
        )?;
        self.ensure_schema(&conn)?;
        Ok(conn)
    }

    fn ensure_schema(&self, conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS observations (
                id         TEXT PRIMARY KEY,
                content    TEXT NOT NULL,
                timestamp  TEXT NOT NULL,
                date       TEXT NOT NULL,
                time       TEXT NOT NULL,
                direction  TEXT NOT NULL DEFAULT 'unknown',
                kind       TEXT NOT NULL DEFAULT 'observation',
                emotion    TEXT NOT NULL DEFAULT 'neutral',
                image_path TEXT,
                image_data TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_obs_timestamp ON observations(timestamp);
            CREATE INDEX IF NOT EXISTS idx_obs_date      ON observations(date);
            CREATE INDEX IF NOT EXISTS idx_obs_kind      ON observations(kind);
            CREATE TABLE IF NOT EXISTS obs_embeddings (
                obs_id TEXT PRIMARY KEY REFERENCES observations(id) ON DELETE CASCADE,
                vector BLOB NOT NULL
            );",
        )?;
        Ok(())
    }

    // ── Search tiers ──────────────────────────────────────────────

    fn vector_search(
        &self,
        conn: &Connection,
        q_vec: &[f32],
        n: usize,
    ) -> Result<Vec<MemoryRow>> {
        let mut stmt = conn.prepare(
            "SELECT o.id, o.content, o.date, o.time, o.emotion, o.image_path, e.vector \
             FROM observations o \
             JOIN obs_embeddings e ON o.id = e.obs_id",
        )?;

        let mut scored: Vec<(f32, MemoryRow)> = stmt
            .query_map([], |row| {
                let bytes: Vec<u8> = row.get(6)?;
                Ok((
                    bytes,
                    MemoryRow {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        date: row.get(2)?,
                        time: row.get(3)?,
                        emotion: row.get(4)?,
                        image_path: row.get(5)?,
                        score: None,
                    },
                ))
            })?
            .filter_map(|r| r.ok())
            .map(|(bytes, mut row)| {
                let doc_vec: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                let score = cosine_similarity(q_vec, &doc_vec);
                row.score = Some(score);
                (score, row)
            })
            .collect();

        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(scored.into_iter().take(n).map(|(_, row)| row).collect())
    }

    fn keyword_search(
        &self,
        conn: &Connection,
        query: &str,
        n: usize,
    ) -> Result<Vec<MemoryRow>> {
        let keywords: Vec<String> = query
            .split_whitespace()
            .filter(|w| w.len() > 1)
            .take(4)
            .map(|w| format!("%{w}%"))
            .collect();

        if keywords.is_empty() {
            return Ok(Vec::new());
        }

        let clauses: String = keywords
            .iter()
            .map(|_| "content LIKE ?")
            .collect::<Vec<_>>()
            .join(" OR ");

        let sql = format!(
            "SELECT id, content, date, time, emotion, image_path \
             FROM observations WHERE {clauses} \
             ORDER BY timestamp DESC LIMIT {n}"
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(keywords.iter()), |row| {
                Ok(MemoryRow {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    date: row.get(2)?,
                    time: row.get(3)?,
                    emotion: row.get(4)?,
                    image_path: row.get(5)?,
                    score: None,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    }

    fn recent_search(&self, conn: &Connection, n: usize) -> Result<Vec<MemoryRow>> {
        let mut stmt = conn.prepare(
            "SELECT id, content, date, time, emotion, image_path \
             FROM observations \
             ORDER BY timestamp DESC LIMIT ?",
        )?;
        let rows = stmt
            .query_map(params![n as i64], |row| {
                Ok(MemoryRow {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    date: row.get(2)?,
                    time: row.get(3)?,
                    emotion: row.get(4)?,
                    image_path: row.get(5)?,
                    score: None,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}

// ── Internal types ────────────────────────────────────────────────

struct MemoryRow {
    #[allow(dead_code)]
    id: String,
    content: String,
    date: String,
    time: String,
    emotion: String,
    image_path: Option<String>,
    score: Option<f32>,
}

// ── Pure functions ────────────────────────────────────────────────

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b + 1e-10)
}

fn format_memories(rows: &[MemoryRow]) -> String {
    if rows.is_empty() {
        return "No relevant memories found.".to_string();
    }
    rows.iter()
        .map(|r| {
            let score_str = r.score.map(|s| format!(" ({s:.2})")).unwrap_or_default();
            let emo = if r.emotion != "neutral" {
                format!(" [{}]", r.emotion)
            } else {
                String::new()
            };
            let img = if r.image_path.is_some() { " 📷" } else { "" };
            let preview = &r.content[..r.content.len().min(120)];
            format!("- {} {}{}{}{}: {}", r.date, r.time, score_str, emo, img, preview)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns (ISO8601 timestamp, YYYY-MM-DD, HH:MM) without chrono.
fn now_parts() -> (String, String, String) {
    let unix_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hh = (unix_ts % 86400) / 3600;
    let mm = (unix_ts % 3600) / 60;
    let ss = unix_ts % 60;
    let days = unix_ts / 86400;
    let (year, month, day) = days_to_ymd(days);
    let ts = format!("{year:04}-{month:02}-{day:02}T{hh:02}:{mm:02}:{ss:02}");
    let date = format!("{year:04}-{month:02}-{day:02}");
    let time = format!("{hh:02}:{mm:02}");
    (ts, date, time)
}

fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    // Howard Hinnant's algorithm
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

/// Resize image to 320×240, encode as JPEG (quality ~60), return base64.
fn make_thumbnail(image_path: &str) -> Option<String> {
    let img = image::open(image_path).ok()?;
    let thumb = img.resize(320, 240, image::imageops::FilterType::Triangle);
    let mut buf = Vec::new();
    thumb
        .write_to(
            &mut std::io::Cursor::new(&mut buf),
            image::ImageFormat::Jpeg,
        )
        .ok()?;
    Some(B64.encode(&buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> PathBuf {
        let id = uuid::Uuid::new_v4();
        std::env::temp_dir().join(format!("familiar_test_{id}.db"))
    }

    // ── cosine_similarity ────────────────────────────────────────

    #[test]
    fn cosine_same_vector_is_one() {
        let v = vec![1.0f32, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-5, "sim={sim}");
    }

    #[test]
    fn cosine_orthogonal_vectors_is_zero() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-5);
    }

    #[test]
    fn cosine_opposite_vectors_is_minus_one() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![-1.0f32, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-5, "sim={sim}");
    }

    #[test]
    fn cosine_empty_vectors_returns_zero() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn cosine_mismatched_lengths_returns_zero() {
        let a = vec![1.0f32, 2.0];
        let b = vec![1.0f32];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_known_values() {
        // [3,4] normalized = [0.6, 0.8]; [4,3] normalized = [0.8, 0.6]
        // dot = 0.6*0.8 + 0.8*0.6 = 0.48 + 0.48 = 0.96
        let a = vec![3.0f32, 4.0];
        let b = vec![4.0f32, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.96).abs() < 1e-4, "sim={sim}");
    }

    // ── days_to_ymd ───────────────────────────────────────────────

    #[test]
    fn days_to_ymd_unix_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date_2026_02_23() {
        // Manually computed: 20507 days from epoch = 2026-02-23
        assert_eq!(days_to_ymd(20507), (2026, 2, 23));
    }

    #[test]
    fn days_to_ymd_leap_day_2000_02_29() {
        // 2000-02-29 = 10957 + 31 + 28 = 11016 days? Let's verify:
        // 2000-01-01 = 10957 (days from epoch)
        // Jan = 31 days → Feb 1 = 10988
        // Feb 29 = 10988 + 28 = 11016
        assert_eq!(days_to_ymd(11016), (2000, 2, 29));
    }

    #[test]
    fn days_to_ymd_year_2000_jan_01() {
        assert_eq!(days_to_ymd(10957), (2000, 1, 1));
    }

    // ── now_parts ────────────────────────────────────────────────

    #[test]
    fn now_parts_returns_plausible_format() {
        let (ts, date, time) = now_parts();
        // ts should be "YYYY-MM-DDTHH:MM:SS"
        assert_eq!(ts.len(), 19, "ts={ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        // date should be "YYYY-MM-DD"
        assert_eq!(date.len(), 10, "date={date}");
        // time should be "HH:MM"
        assert_eq!(time.len(), 5, "time={time}");
        assert_eq!(&time[2..3], ":");
    }

    #[test]
    fn now_parts_year_is_plausible() {
        let (_, date, _) = now_parts();
        let year: u32 = date[..4].parse().unwrap();
        assert!(year >= 2024 && year <= 2100, "year={year}");
    }

    // ── format_memories ───────────────────────────────────────────

    #[test]
    fn format_memories_empty_returns_no_memories_msg() {
        assert_eq!(format_memories(&[]), "No relevant memories found.");
    }

    #[test]
    fn format_memories_includes_date_time_and_content() {
        let rows = vec![MemoryRow {
            id: "id1".to_string(),
            content: "Saw a beautiful sunset".to_string(),
            date: "2026-02-23".to_string(),
            time: "18:30".to_string(),
            emotion: "neutral".to_string(),
            image_path: None,
            score: None,
        }];
        let result = format_memories(&rows);
        assert!(result.contains("2026-02-23"), "missing date: {result}");
        assert!(result.contains("18:30"), "missing time: {result}");
        assert!(result.contains("Saw a beautiful sunset"), "missing content: {result}");
    }

    #[test]
    fn format_memories_neutral_emotion_not_shown() {
        let rows = vec![MemoryRow {
            id: "id1".to_string(),
            content: "test".to_string(),
            date: "2026-01-01".to_string(),
            time: "00:00".to_string(),
            emotion: "neutral".to_string(),
            image_path: None,
            score: None,
        }];
        let result = format_memories(&rows);
        assert!(!result.contains("[neutral]"));
    }

    #[test]
    fn format_memories_non_neutral_emotion_shown_in_brackets() {
        let rows = vec![MemoryRow {
            id: "id1".to_string(),
            content: "test".to_string(),
            date: "2026-01-01".to_string(),
            time: "00:00".to_string(),
            emotion: "excited".to_string(),
            image_path: None,
            score: None,
        }];
        let result = format_memories(&rows);
        assert!(result.contains("[excited]"), "result={result}");
    }

    #[test]
    fn format_memories_image_shows_camera_emoji() {
        let rows = vec![MemoryRow {
            id: "id1".to_string(),
            content: "test".to_string(),
            date: "2026-01-01".to_string(),
            time: "00:00".to_string(),
            emotion: "neutral".to_string(),
            image_path: Some("/tmp/photo.jpg".to_string()),
            score: None,
        }];
        let result = format_memories(&rows);
        assert!(result.contains("📷"), "result={result}");
    }

    #[test]
    fn format_memories_score_shown_with_two_decimals() {
        let rows = vec![MemoryRow {
            id: "id1".to_string(),
            content: "test".to_string(),
            date: "2026-01-01".to_string(),
            time: "00:00".to_string(),
            emotion: "neutral".to_string(),
            image_path: None,
            score: Some(0.876),
        }];
        let result = format_memories(&rows);
        assert!(result.contains("(0.88)"), "result={result}");
    }

    #[test]
    fn format_memories_content_truncated_at_120_chars() {
        let long = "x".repeat(200);
        let rows = vec![MemoryRow {
            id: "id1".to_string(),
            content: long.clone(),
            date: "2026-01-01".to_string(),
            time: "00:00".to_string(),
            emotion: "neutral".to_string(),
            image_path: None,
            score: None,
        }];
        let result = format_memories(&rows);
        // The displayed content should only be 120 chars of x's
        let x_count = result.chars().filter(|&c| c == 'x').count();
        assert_eq!(x_count, 120, "expected 120 x's, got {x_count}");
    }

    // ── MemoryTool: schema creation ───────────────────────────────

    #[test]
    fn schema_creates_observations_and_embeddings_tables() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        let conn = tool.open_db().expect("open_db should succeed");

        let obs_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
            .expect("observations table should exist");
        assert_eq!(obs_count, 0);

        let emb_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM obs_embeddings", [], |r| r.get(0))
            .expect("obs_embeddings table should exist");
        assert_eq!(emb_count, 0);

        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn schema_is_idempotent_multiple_opens() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        // Open three times — CREATE TABLE IF NOT EXISTS should not error
        tool.open_db().unwrap();
        tool.open_db().unwrap();
        tool.open_db().unwrap();
        let _ = std::fs::remove_file(&db);
    }

    // ── MemoryTool: remember ──────────────────────────────────────

    #[test]
    fn remember_saves_content_to_db() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Kouta brought flowers", "happy", None).unwrap();

        let conn = tool.open_db().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn remember_returns_ok_with_remembered_prefix() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        let (text, img) = tool.remember("Test content here", "neutral", None).unwrap();
        assert!(text.starts_with("Remembered"), "text={text}");
        assert!(img.is_none());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn remember_stores_correct_emotion() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Excited about something", "excited", None).unwrap();

        let conn = tool.open_db().unwrap();
        let emotion: String = conn
            .query_row("SELECT emotion FROM observations LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(emotion, "excited");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn remember_multiple_entries_all_saved() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("First memory", "neutral", None).unwrap();
        tool.remember("Second memory", "happy", None).unwrap();
        tool.remember("Third memory", "curious", None).unwrap();

        let conn = tool.open_db().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn remember_preview_truncated_at_60_chars() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        let long = "a".repeat(100);
        let (text, _) = tool.remember(&long, "neutral", None).unwrap();
        // "Remembered: " + 60 a's
        let a_count = text.chars().filter(|&c| c == 'a').count();
        assert_eq!(a_count, 60, "a_count={a_count}, text={text}");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn remember_with_image_path_shows_with_image_suffix() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        // Use a nonexistent path — thumbnail will fail silently, but stored_path is still set
        let (text, _) = tool
            .remember("Saw something", "neutral", Some("/nonexistent/path.jpg"))
            .unwrap();
        assert!(text.contains("(with image)"), "text={text}");
        let _ = std::fs::remove_file(&db);
    }

    // ── MemoryTool: keyword_search (Tier 2) ──────────────────────

    #[test]
    fn keyword_search_finds_matching_content() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("The cat sat on the mat", "neutral", None).unwrap();
        tool.remember("Sunny day outside", "happy", None).unwrap();

        let conn = tool.open_db().unwrap();
        let results = tool.keyword_search(&conn, "cat", 5).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("cat"));
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn keyword_search_no_match_returns_empty() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Something completely different", "neutral", None).unwrap();

        let conn = tool.open_db().unwrap();
        let results = tool.keyword_search(&conn, "zyxwvutsr", 5).unwrap();
        assert!(results.is_empty());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn keyword_search_single_char_returns_empty() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("A short word", "neutral", None).unwrap();

        let conn = tool.open_db().unwrap();
        // Single-char words are filtered out, so returns empty
        let results = tool.keyword_search(&conn, "A", 5).unwrap();
        assert!(results.is_empty(), "results={}", results.len());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn keyword_search_empty_query_returns_empty() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Something", "neutral", None).unwrap();

        let conn = tool.open_db().unwrap();
        let results = tool.keyword_search(&conn, "", 5).unwrap();
        assert!(results.is_empty());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn keyword_search_multiple_words_returns_any_match() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("The dog barked loudly", "neutral", None).unwrap();
        tool.remember("The cat meowed softly", "neutral", None).unwrap();
        tool.remember("The weather was fine", "neutral", None).unwrap();

        let conn = tool.open_db().unwrap();
        // "dog cat" → matches rows containing dog OR cat
        let results = tool.keyword_search(&conn, "dog cat", 5).unwrap();
        assert_eq!(results.len(), 2);
        let _ = std::fs::remove_file(&db);
    }

    // ── MemoryTool: recent_search (Tier 3) ───────────────────────

    #[test]
    fn recent_search_empty_db_returns_empty() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        let conn = tool.open_db().unwrap();
        let results = tool.recent_search(&conn, 5).unwrap();
        assert!(results.is_empty());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn recent_search_returns_at_most_n_results() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        for i in 0..5 {
            tool.remember(&format!("Memory {i}"), "neutral", None).unwrap();
        }
        let conn = tool.open_db().unwrap();
        let results = tool.recent_search(&conn, 3).unwrap();
        assert_eq!(results.len(), 3);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn recent_search_returns_most_recent_first() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Earlier memory", "neutral", None).unwrap();
        // Small sleep to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));
        tool.remember("Later memory", "neutral", None).unwrap();

        let conn = tool.open_db().unwrap();
        let results = tool.recent_search(&conn, 5).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].content.contains("Later"), "first={}", results[0].content);
        let _ = std::fs::remove_file(&db);
    }

    // ── MemoryTool: recall_for_context ────────────────────────────

    #[test]
    fn recall_for_context_empty_db_returns_empty_string() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        assert!(tool.recall_for_context(5).is_empty());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn recall_for_context_includes_date_time_and_content() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Meeting with Kouta about the project", "happy", None).unwrap();

        let ctx = tool.recall_for_context(5);
        assert!(!ctx.is_empty());
        assert!(ctx.contains("Meeting with Kouta"), "ctx={ctx}");
        // Should have "  - [YYYY-MM-DD HH:MM] ..." format
        assert!(ctx.contains("  - ["), "ctx={ctx}");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn recall_for_context_limits_to_n_memories() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        for i in 0..10 {
            tool.remember(&format!("Memory number {i}"), "neutral", None).unwrap();
        }
        let ctx = tool.recall_for_context(3);
        let line_count = ctx.lines().count();
        assert_eq!(line_count, 3, "expected 3 lines, got {line_count}");
        let _ = std::fs::remove_file(&db);
    }

    // ── MemoryTool: recall_memories (public API) ──────────────────

    #[test]
    fn recall_memories_empty_db_returns_no_memories_msg() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        let (result, img) = tool.recall_memories("anything", 3).unwrap();
        assert_eq!(result, "No relevant memories found.");
        assert!(img.is_none());
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn recall_memories_finds_via_keyword_fallback() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("The robot explored the room", "curious", None).unwrap();

        // Without embeddings loaded (likely in CI), falls to Tier 2
        let (result, _) = tool.recall_memories("robot", 3).unwrap();
        assert!(result.contains("robot") || result.contains("explored"),
            "result={result}");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn recall_memories_falls_back_to_recency_when_no_keyword_match() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Something completely unrelated", "neutral", None).unwrap();

        // "zyxwvuts" won't match any keyword; falls to Tier 3 (recency)
        let (result, _) = tool.recall_memories("zyxwvuts", 3).unwrap();
        // Should return the recency result, not "No relevant memories found."
        assert!(result.contains("Something") || !result.contains("No relevant"),
            "result={result}");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn recall_memories_n_clamped_to_1_at_minimum() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Only memory", "neutral", None).unwrap();
        // n=0 should be clamped to 1
        let (result, _) = tool.recall_memories("only", 0).unwrap();
        assert!(!result.is_empty());
        let _ = std::fs::remove_file(&db);
    }

    // ── cosine_similarity edge cases ──────────────────────────────

    #[test]
    fn cosine_zero_vector_does_not_panic() {
        let zero = vec![0.0f32, 0.0, 0.0];
        let other = vec![1.0f32, 2.0, 3.0];
        // Should not divide by zero; epsilon guard keeps it safe
        let sim = cosine_similarity(&zero, &other);
        assert!(sim.is_finite(), "sim should be finite, got {sim}");
        assert!(sim.abs() < 1e-3, "sim with zero vector should be ~0, got {sim}");
    }

    #[test]
    fn cosine_both_zero_vectors_does_not_panic() {
        let zero = vec![0.0f32, 0.0, 0.0];
        let sim = cosine_similarity(&zero, &zero);
        assert!(sim.is_finite());
    }

    #[test]
    fn cosine_single_element_vectors() {
        let a = vec![3.0f32];
        let b = vec![5.0f32];
        // Both positive → same direction → 1.0
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-5, "sim={sim}");
    }

    // ── vector_search with manually inserted embeddings ───────────

    #[test]
    fn vector_search_ranks_by_cosine_similarity() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));

        // Insert two observations manually
        let conn = tool.open_db().unwrap();
        let id1 = "aaaaaaaa-0000-0000-0000-000000000001";
        let id2 = "aaaaaaaa-0000-0000-0000-000000000002";
        let (ts, date, time) = now_parts();

        for (id, content) in [(id1, "high similarity memory"), (id2, "low similarity memory")] {
            conn.execute(
                "INSERT INTO observations (id, content, timestamp, date, time, direction, kind, emotion) \
                 VALUES (?1,?2,?3,?4,?5,'unknown','observation','neutral')",
                rusqlite::params![id, content, ts, date, time],
            ).unwrap();
        }

        // id1 gets a vector [1,0,0] — aligned with query
        // id2 gets a vector [0,1,0] — orthogonal to query
        let vec_aligned: Vec<u8> = vec![1.0f32, 0.0, 0.0]
            .iter().flat_map(|f: &f32| f.to_le_bytes()).collect();
        let vec_ortho: Vec<u8> = vec![0.0f32, 1.0, 0.0]
            .iter().flat_map(|f: &f32| f.to_le_bytes()).collect();
        conn.execute(
            "INSERT INTO obs_embeddings (obs_id, vector) VALUES (?1, ?2)",
            rusqlite::params![id1, vec_aligned],
        ).unwrap();
        conn.execute(
            "INSERT INTO obs_embeddings (obs_id, vector) VALUES (?1, ?2)",
            rusqlite::params![id2, vec_ortho],
        ).unwrap();

        // Query aligned with id1
        let q_vec = vec![1.0f32, 0.0, 0.0];
        let results = tool.vector_search(&conn, &q_vec, 2).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].content.contains("high"), "Expected high first, got: {}", results[0].content);
        assert!(results[0].score.unwrap() > results[1].score.unwrap());

        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn vector_search_returns_at_most_n_results() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        let conn = tool.open_db().unwrap();
        let (ts, date, time) = now_parts();

        // Insert 5 observations with embeddings
        for i in 0u8..5 {
            let id = format!("aaaaaaaa-0000-0000-0000-00000000000{i}");
            conn.execute(
                "INSERT INTO observations (id, content, timestamp, date, time, direction, kind, emotion) \
                 VALUES (?1,?2,?3,?4,?5,'unknown','observation','neutral')",
                rusqlite::params![id, format!("memory {i}"), ts, date, time],
            ).unwrap();
            let vec_bytes: Vec<u8> = vec![i as f32, 0.0, 0.0]
                .iter().flat_map(|f: &f32| f.to_le_bytes()).collect();
            conn.execute(
                "INSERT INTO obs_embeddings (obs_id, vector) VALUES (?1,?2)",
                rusqlite::params![id, vec_bytes],
            ).unwrap();
        }

        let q = vec![1.0f32, 0.0, 0.0];
        let results = tool.vector_search(&conn, &q, 3).unwrap();
        assert_eq!(results.len(), 3);

        let _ = std::fs::remove_file(&db);
    }

    // ── keyword_search detailed behavior ─────────────────────────

    #[test]
    fn keyword_search_uses_at_most_4_keywords() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        // word5 and word6 should be ignored (only first 4 taken)
        tool.remember("word5 is here", "neutral", None).unwrap();
        tool.remember("word1 content", "neutral", None).unwrap();

        let conn = tool.open_db().unwrap();
        // Query with 6 words; only first 4 used ("word1" "word2" "word3" "word4")
        // "word5" and "word6" are dropped, so "word5 is here" won't match on word5
        let results = tool
            .keyword_search(&conn, "word1 word2 word3 word4 word5 word6", 10)
            .unwrap();
        // "word1 content" matches word1; "word5 is here" does NOT match any of word1-4
        assert_eq!(results.len(), 1, "Only word1 row should match");
        assert!(results[0].content.contains("word1"));

        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn keyword_search_respects_limit_n() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        for i in 0..10 {
            tool.remember(&format!("keyword match {i}"), "neutral", None).unwrap();
        }
        let conn = tool.open_db().unwrap();
        let results = tool.keyword_search(&conn, "keyword", 3).unwrap();
        assert_eq!(results.len(), 3);
        let _ = std::fs::remove_file(&db);
    }

    // ── remember detailed behavior ────────────────────────────────

    #[test]
    fn remember_short_content_shows_full_content_in_preview() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        let short = "Short message";
        let (text, _) = tool.remember(short, "neutral", None).unwrap();
        assert!(text.contains(short), "text={text}");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn remember_stores_direction_unknown_and_kind_observation() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Test content", "neutral", None).unwrap();

        let conn = tool.open_db().unwrap();
        let (direction, kind): (String, String) = conn
            .query_row(
                "SELECT direction, kind FROM observations LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(direction, "unknown");
        assert_eq!(kind, "observation");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn remember_each_entry_has_unique_id() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("First", "neutral", None).unwrap();
        tool.remember("Second", "neutral", None).unwrap();

        let conn = tool.open_db().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(DISTINCT id) FROM observations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let _ = std::fs::remove_file(&db);
    }

    // ── obs_embeddings FK cascade ─────────────────────────────────

    #[test]
    fn embedding_deleted_when_observation_deleted() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        let conn = tool.open_db().unwrap();
        let (ts, date, time) = now_parts();
        let id = "cascade-test-id-0001";

        conn.execute(
            "INSERT INTO observations (id, content, timestamp, date, time, direction, kind, emotion) \
             VALUES (?1,'cascade test',?2,?3,?4,'unknown','observation','neutral')",
            rusqlite::params![id, ts, date, time],
        ).unwrap();
        let vec_bytes: Vec<u8> = vec![1.0f32, 0.0]
            .iter().flat_map(|f: &f32| f.to_le_bytes()).collect();
        conn.execute(
            "INSERT INTO obs_embeddings (obs_id, vector) VALUES (?1,?2)",
            rusqlite::params![id, vec_bytes],
        ).unwrap();

        // Verify embedding exists
        let emb_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM obs_embeddings WHERE obs_id=?1",
                rusqlite::params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(emb_before, 1);

        // Delete observation → cascade should delete embedding
        conn.execute("DELETE FROM observations WHERE id=?1", rusqlite::params![id]).unwrap();

        let emb_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM obs_embeddings WHERE obs_id=?1",
                rusqlite::params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(emb_after, 0, "Embedding should be deleted via FK cascade");

        let _ = std::fs::remove_file(&db);
    }

    // ── recall_memories n clamp ───────────────────────────────────

    #[test]
    fn recall_memories_n_clamped_to_20_at_maximum() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        for i in 0..25 {
            tool.remember(&format!("memory {i}"), "neutral", None).unwrap();
        }
        // n=100 should be clamped to 20; verify at most 20 lines returned
        let (result, _) = tool.recall_memories("memory", 100).unwrap();
        let line_count = result.lines().count();
        assert!(line_count <= 20, "Expected ≤20 results, got {line_count}");
        let _ = std::fs::remove_file(&db);
    }

    // ── format_memories multiple rows ─────────────────────────────

    #[test]
    fn format_memories_multiple_rows_all_shown() {
        let rows: Vec<MemoryRow> = (0..3).map(|i| MemoryRow {
            id: format!("id{i}"),
            content: format!("memory {i}"),
            date: "2026-02-23".to_string(),
            time: "10:00".to_string(),
            emotion: "neutral".to_string(),
            image_path: None,
            score: None,
        }).collect();
        let result = format_memories(&rows);
        let line_count = result.lines().count();
        assert_eq!(line_count, 3, "result={result}");
        assert!(result.contains("memory 0"));
        assert!(result.contains("memory 1"));
        assert!(result.contains("memory 2"));
    }

    // ── recall_for_context line format ────────────────────────────

    #[test]
    fn recall_for_context_line_format_matches_expected_pattern() {
        let db = temp_db();
        let tool = MemoryTool::new(Some(db.clone()));
        tool.remember("Line format test", "neutral", None).unwrap();

        let ctx = tool.recall_for_context(1);
        // Expected: "  - [YYYY-MM-DD HH:MM] content"
        assert!(ctx.starts_with("  - ["), "ctx={ctx}");
        assert!(ctx.contains("] Line format test"), "ctx={ctx}");
        let _ = std::fs::remove_file(&db);
    }

    // ── now_parts consistency ─────────────────────────────────────

    #[test]
    fn now_parts_date_in_ts_matches_date_field() {
        let (ts, date, _) = now_parts();
        // First 10 chars of ts should equal date
        assert_eq!(&ts[..10], date.as_str(), "ts={ts} date={date}");
    }

    #[test]
    fn now_parts_time_in_ts_matches_time_field() {
        let (ts, _, time) = now_parts();
        // Chars 11..16 of ts should equal time (HH:MM)
        assert_eq!(&ts[11..16], time.as_str(), "ts={ts} time={time}");
    }
}
