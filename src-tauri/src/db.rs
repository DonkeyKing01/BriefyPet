use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use tauri::AppHandle;

use crate::models::{
    AppView, ArticleRecord, FitLevel, ReminderBatchSnapshot, RssSource, SettingsPayload, Snapshot,
};

pub fn db_path(app: &AppHandle) -> Result<PathBuf> {
    let app_dir = app
        .path_resolver()
        .app_data_dir()
        .context("failed to resolve app data dir")?;
    fs::create_dir_all(&app_dir)?;
    Ok(app_dir.join("briefy-pet.db"))
}

pub fn connect(app: &AppHandle) -> Result<Connection> {
    let path = db_path(app)?;
    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS articles (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          guid TEXT NOT NULL UNIQUE,
          title TEXT NOT NULL,
          link TEXT NOT NULL,
          source_name TEXT NOT NULL,
          published_at TEXT,
          fetched_at TEXT,
          raw_content TEXT NOT NULL,
          summary TEXT NOT NULL,
          fit_level TEXT NOT NULL,
          fit_score INTEGER NOT NULL,
          recommendation_reason TEXT NOT NULL,
          is_favorite INTEGER NOT NULL DEFAULT 0,
          is_new INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS reminder_batches (
          id TEXT PRIMARY KEY,
          status TEXT NOT NULL,
          remind_at TEXT,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS reminder_batch_articles (
          batch_id TEXT NOT NULL,
          article_id INTEGER NOT NULL,
          PRIMARY KEY(batch_id, article_id)
        );
        "#
    )?;
    let _ = conn.execute("ALTER TABLE articles ADD COLUMN fetched_at TEXT", []);
    let _ = conn.execute(
        "UPDATE articles SET fetched_at = published_at WHERE fetched_at IS NULL AND published_at IS NOT NULL",
        [],
    );
    let configured_sources = load_configured_rss_sources(app)?;
    seed_defaults(&conn, &configured_sources)?;
    Ok(conn)
}

fn seed_defaults(conn: &Connection, configured_sources: &[RssSource]) -> Result<()> {
    if read_setting(conn, "api_key")?.is_none() {
        write_setting(conn, "api_key", "")?;
        write_setting(conn, "interest_profile", "")?;
        write_setting(conn, "auto_start", "false")?;
        write_setting(conn, "active_view", "\"settings\"")?;
        write_setting(conn, "selected_article_id", "null")?;
        write_setting(conn, "api_key_valid", "false")?;
        write_setting(conn, "last_scan_at", "null")?;
        let rss_sources = serde_json::to_string(configured_sources)?;
        write_setting(conn, "rss_sources", &rss_sources)?;
    } else {
        if read_setting(conn, "api_key_valid")?.is_none() {
            write_setting(conn, "api_key_valid", "false")?;
        }
        if read_setting(conn, "last_scan_at")?.is_none() {
            write_setting(conn, "last_scan_at", "null")?;
        }
        if let Some(rss_sources_json) = read_setting(conn, "rss_sources")? {
            let existing_sources: Vec<RssSource> = serde_json::from_str(&rss_sources_json)?;
            let normalized_sources = normalize_rss_sources(existing_sources, configured_sources);
            let normalized_json = serde_json::to_string(&normalized_sources)?;
            if normalized_json != rss_sources_json {
                write_setting(conn, "rss_sources", &normalized_json)?;
            }
        }
    }
    Ok(())
}

pub fn read_settings(conn: &Connection) -> Result<SettingsPayload> {
    let rss_sources_json = read_setting(conn, "rss_sources")?.unwrap_or_else(|| "[]".into());
    Ok(SettingsPayload {
        api_key: read_setting(conn, "api_key")?.unwrap_or_default(),
        interest_profile: read_setting(conn, "interest_profile")?.unwrap_or_default(),
        auto_start: read_setting(conn, "auto_start")?.unwrap_or_else(|| "false".into()) == "true",
        rss_sources: serde_json::from_str(&rss_sources_json)?,
    })
}

pub fn write_settings(conn: &Connection, settings: &SettingsPayload) -> Result<()> {
    write_setting(conn, "api_key", settings.api_key.trim())?;
    write_setting(conn, "interest_profile", settings.interest_profile.trim())?;
    write_setting(conn, "auto_start", if settings.auto_start { "true" } else { "false" })?;
    write_setting(conn, "rss_sources", &serde_json::to_string(&settings.rss_sources)?)?;
    Ok(())
}

pub fn read_active_view(conn: &Connection) -> Result<AppView> {
    let raw = read_setting(conn, "active_view")?.unwrap_or_else(|| "\"reading\"".into());
    Ok(serde_json::from_str(&raw)?)
}

pub fn write_active_view(conn: &Connection, view: &AppView) -> Result<()> {
    write_setting(conn, "active_view", &serde_json::to_string(view)?)?;
    Ok(())
}

pub fn read_selected_article_id(conn: &Connection) -> Result<Option<i64>> {
    let raw = read_setting(conn, "selected_article_id")?.unwrap_or_else(|| "null".into());
    Ok(serde_json::from_str(&raw)?)
}

pub fn write_selected_article_id(conn: &Connection, article_id: Option<i64>) -> Result<()> {
    write_setting(conn, "selected_article_id", &serde_json::to_string(&article_id)?)?;
    Ok(())
}

pub fn list_articles(conn: &Connection) -> Result<Vec<ArticleRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, title, link, source_name, published_at, summary, fit_level, fit_score,
               recommendation_reason, is_favorite, is_new, fetched_at
        FROM articles
        ORDER BY COALESCE(published_at, '1970-01-01T00:00:00Z') DESC, id DESC
        "#
    )?;

    let rows = stmt.query_map([], |row| {
        let fit_level_raw: String = row.get(6)?;
        let published_at_raw: Option<String> = row.get(4)?;
        let fetched_at_raw: Option<String> = row.get(11)?;
        let fit_level = match fit_level_raw.as_str() {
            "high" => FitLevel::High,
            "medium" => FitLevel::Medium,
            _ => FitLevel::Low,
        };
        Ok(ArticleRecord {
            id: row.get(0)?,
            title: row.get(1)?,
            link: row.get(2)?,
            source_name: row.get(3)?,
            published_at: published_at_raw
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc)),
            fetched_at: fetched_at_raw
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc)),
            summary: row.get(5)?,
            fit_level,
            fit_score: row.get(7)?,
            recommendation_reason: row.get(8)?,
            is_favorite: row.get::<_, i64>(9)? == 1,
            is_new: row.get::<_, i64>(10)? == 1,
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

pub fn toggle_favorite(conn: &Connection, article_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE articles SET is_favorite = CASE WHEN is_favorite = 1 THEN 0 ELSE 1 END WHERE id = ?1",
        params![article_id],
    )?;
    Ok(())
}

pub fn mark_article_opened(conn: &Connection, article_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE articles SET is_new = 0 WHERE id = ?1",
        params![article_id],
    )?;
    write_selected_article_id(conn, Some(article_id))?;
    Ok(())
}

pub fn find_article_id_by_guid(conn: &Connection, guid: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM articles WHERE guid = ?1",
        params![guid],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_article(
    conn: &Connection,
    guid: &str,
    title: &str,
    link: &str,
    source_name: &str,
    published_at: Option<DateTime<Utc>>,
    fetched_at: DateTime<Utc>,
    raw_content: &str,
    summary: &str,
    fit_level: &FitLevel,
    fit_score: i64,
    recommendation_reason: &str,
) -> Result<i64> {
    let fit_level_raw = match fit_level {
        FitLevel::High => "high",
        FitLevel::Medium => "medium",
        FitLevel::Low => "low",
    };
    conn.execute(
        r#"
        INSERT INTO articles (
          guid, title, link, source_name, published_at, fetched_at, raw_content, summary, fit_level,
          fit_score, recommendation_reason, is_favorite, is_new
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, 1)
        "#,
        params![
            guid,
            title,
            link,
            source_name,
            published_at.map(|value| value.to_rfc3339()),
            fetched_at.to_rfc3339(),
            raw_content,
            summary,
            fit_level_raw,
            fit_score,
            recommendation_reason
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn active_reminder_batch(conn: &Connection) -> Result<Option<ReminderBatchSnapshot>> {
    let now = Utc::now().to_rfc3339();
    let mut stmt = conn.prepare(
        r#"
        SELECT id
        FROM reminder_batches
        WHERE (status = 'active')
           OR (status = 'snoozed' AND remind_at IS NOT NULL AND remind_at <= ?1)
        ORDER BY created_at ASC
        "#,
    )?;
    let batch_rows = stmt.query_map(params![now], |row| row.get::<_, String>(0))?;

    for batch_row in batch_rows {
        let batch_id = batch_row?;
        let mut article_stmt = conn.prepare(
            r#"
            SELECT a.id, a.fit_score
            FROM reminder_batch_articles rba
            JOIN articles a ON a.id = rba.article_id
            WHERE rba.batch_id = ?1
              AND a.is_new = 1
            ORDER BY a.fit_score DESC, a.id DESC
            "#,
        )?;

        let article_rows = article_stmt.query_map(params![batch_id.clone()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut article_ids = Vec::new();
        let mut top_article_id = None;
        for (index, row) in article_rows.enumerate() {
            let (article_id, _) = row?;
            if index == 0 {
                top_article_id = Some(article_id);
            }
            article_ids.push(article_id);
        }

        if article_ids.is_empty() {
            conn.execute(
                "UPDATE reminder_batches SET status = 'opened', remind_at = NULL WHERE id = ?1",
                params![batch_id],
            )?;
        } else {
            return Ok(Some(ReminderBatchSnapshot {
                id: batch_id,
                article_count: article_ids.len(),
                article_ids,
                top_article_id,
            }));
        }
    }

    Ok(None)
}

pub fn create_reminder_batch(conn: &Connection) -> Result<String> {
    let batch_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO reminder_batches (id, status, remind_at, created_at) VALUES (?1, 'active', NULL, ?2)",
        params![batch_id, Utc::now().to_rfc3339()],
    )?;
    Ok(batch_id)
}

pub fn attach_article_to_batch(conn: &Connection, batch_id: &str, article_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO reminder_batch_articles (batch_id, article_id) VALUES (?1, ?2)",
        params![batch_id, article_id],
    )?;
    Ok(())
}

pub fn set_batch_status(conn: &Connection, batch_id: &str, status: &str, remind_at: Option<String>) -> Result<()> {
    conn.execute(
        "UPDATE reminder_batches SET status = ?2, remind_at = ?3 WHERE id = ?1",
        params![batch_id, status, remind_at],
    )?;
    Ok(())
}

pub fn build_snapshot(
    conn: &Connection,
    pet_status: crate::models::PetStatus,
    last_error: Option<String>,
    api_key_valid: bool,
    last_scan_at: Option<DateTime<Utc>>,
) -> Result<Snapshot> {
    Ok(Snapshot {
        settings: read_settings(conn)?,
        pet_status,
        articles: list_articles(conn)?,
        active_reminder: active_reminder_batch(conn)?,
        selected_article_id: read_selected_article_id(conn)?,
        active_view: read_active_view(conn)?,
        last_error,
        api_key_valid,
        last_scan_at,
    })
}

pub fn read_api_key_valid(conn: &Connection) -> Result<bool> {
    Ok(read_setting(conn, "api_key_valid")?
        .unwrap_or_else(|| "false".into())
        == "true")
}

pub fn write_api_key_valid(conn: &Connection, value: bool) -> Result<()> {
    write_setting(conn, "api_key_valid", if value { "true" } else { "false" })
}

pub fn read_last_scan_at(conn: &Connection) -> Result<Option<DateTime<Utc>>> {
    let raw = read_setting(conn, "last_scan_at")?.unwrap_or_else(|| "null".into());
    let value: Option<String> = serde_json::from_str(&raw).unwrap_or(None);
    Ok(value
        .as_deref()
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|value| value.with_timezone(&Utc)))
}

pub fn write_last_scan_at(conn: &Connection, value: Option<DateTime<Utc>>) -> Result<()> {
    write_setting(
        conn,
        "last_scan_at",
        &serde_json::to_string(&value.map(|timestamp| timestamp.to_rfc3339()))?,
    )
}

fn read_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn write_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO settings (key, value) VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
        params![key, value],
    )?;
    Ok(())
}

fn load_configured_rss_sources(app: &AppHandle) -> Result<Vec<RssSource>> {
    let mut resource_candidates = Vec::new();
    if let Some(path) = app.path_resolver().resolve_resource("rss-sources.json") {
        resource_candidates.push(path);
    }
    if let Some(path) = app.path_resolver().resolve_resource("resources/rss-sources.json") {
        resource_candidates.push(path);
    }
    if let Some(dir) = app.path_resolver().resource_dir() {
        resource_candidates.push(dir.join("rss-sources.json"));
        resource_candidates.push(dir.join("resources").join("rss-sources.json"));
    }
    resource_candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("rss-sources.json"),
    );

    for candidate in resource_candidates {
        if candidate.exists() {
            let raw = fs::read_to_string(&candidate)
                .with_context(|| format!("failed to read rss config: {}", candidate.display()))?;
            let sources: Vec<RssSource> = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse rss config: {}", candidate.display()))?;
            return Ok(sources);
        }
    }

    Err(anyhow::anyhow!("rss config file not found"))
}

fn normalize_rss_sources(existing: Vec<RssSource>, configured_sources: &[RssSource]) -> Vec<RssSource> {
    let mut existing_by_id = HashMap::new();
    let mut existing_by_url = HashMap::new();

    for current in existing {
        existing_by_id
            .entry(current.id.clone())
            .or_insert_with(|| current.clone());
        existing_by_url.entry(current.url.clone()).or_insert(current);
    }

    configured_sources
        .iter()
        .cloned()
        .map(|default| {
            let matched_enabled = existing_by_id
                .get(&default.id)
                .map(|source| source.enabled)
                .or_else(|| existing_by_url.get(&default.url).map(|source| source.enabled))
                .unwrap_or(default.enabled);

            RssSource {
                id: default.id,
                name: default.name,
                url: default.url,
                enabled: matched_enabled,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::normalize_rss_sources;
    use crate::models::RssSource;

    #[test]
    fn normalizes_old_source_list_into_current_defaults() {
        let configured_sources = vec![
            RssSource {
                id: "openai-blog".into(),
                name: "OpenAI Blog".into(),
                url: "https://openai.com/news/rss.xml".into(),
                enabled: true,
            },
            RssSource {
                id: "github-blog".into(),
                name: "GitHub Blog".into(),
                url: "https://github.blog/feed/".into(),
                enabled: true,
            },
            RssSource {
                id: "hacker-news".into(),
                name: "Hacker News Frontpage".into(),
                url: "https://hnrss.org/frontpage".into(),
                enabled: false,
            },
            RssSource {
                id: "simon-willison".into(),
                name: "Simon Willison".into(),
                url: "https://simonwillison.net/atom/everything/".into(),
                enabled: true,
            },
        ];
        let old_sources = vec![
            RssSource {
                id: "openai-blog".into(),
                name: "OpenAI Blog".into(),
                url: "https://openai.com/news/rss.xml".into(),
                enabled: true,
            },
            RssSource {
                id: "hacker-news".into(),
                name: "Hacker News Frontpage".into(),
                url: "https://hnrss.org/frontpage".into(),
                enabled: false,
            },
        ];

        let normalized = normalize_rss_sources(old_sources, &configured_sources);
        assert_eq!(normalized.len(), configured_sources.len());
        assert!(normalized.iter().any(|source| source.id == "github-blog"));
        assert!(normalized.iter().any(|source| source.id == "simon-willison"));
        assert!(normalized
            .iter()
            .find(|source| source.id == "hacker-news")
            .map(|source| !source.enabled)
            .unwrap_or(false));
    }
}
