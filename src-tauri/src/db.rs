use std::{
    collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use quick_xml::{events::Event, Reader};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use tauri::AppHandle;

use crate::models::{
    all_disciplines, AppView, ArticleRecord, ContentPoolStat, Discipline, FitLevel,
    InterestMemoryRecord, ReminderBatchSnapshot, ResourceType, RssSource, SettingsPayload,
    Snapshot, SourceCatalogSummary, SourceKind, UserDisciplinePreference,
};
use crate::policy;

const MAX_POOL_SIZE_PER_KIND: usize = 1000;

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
        CREATE TABLE IF NOT EXISTS source_catalog (
          source_id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          rss_url TEXT NOT NULL,
                    module TEXT NOT NULL DEFAULT 'other',
                    bucket TEXT NOT NULL DEFAULT 'unspecified',
          discipline TEXT NOT NULL,
          source_kind TEXT NOT NULL,
          resource_type TEXT NOT NULL,
          language TEXT,
          enabled_by_default INTEGER NOT NULL DEFAULT 1,
          postponed INTEGER NOT NULL DEFAULT 0,
          origin_files TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS user_interest_profile_v2 (
          discipline TEXT PRIMARY KEY,
          enabled INTEGER NOT NULL DEFAULT 0,
          preference TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS user_source_pool (
          source_id TEXT PRIMARY KEY,
          enabled INTEGER NOT NULL DEFAULT 1,
          updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS source_fetch_state (
          source_id TEXT PRIMARY KEY,
          last_fetched_at TEXT,
          last_success_at TEXT,
          last_error TEXT
        );
        CREATE TABLE IF NOT EXISTS articles (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          guid TEXT NOT NULL UNIQUE,
          article_key TEXT,
          normalized_link TEXT,
          source_id TEXT NOT NULL DEFAULT '',
          title TEXT NOT NULL,
          link TEXT NOT NULL,
          source_name TEXT NOT NULL,
          discipline TEXT NOT NULL DEFAULT 'other',
          source_kind TEXT NOT NULL DEFAULT 'technical-blog',
          resource_type TEXT NOT NULL DEFAULT 'article',
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
        CREATE TABLE IF NOT EXISTS ranked_content_pool (
          article_id INTEGER PRIMARY KEY,
          source_kind TEXT NOT NULL,
          fit_score INTEGER NOT NULL,
          published_at TEXT,
          inserted_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS daily_interest_memory (
          day_key TEXT PRIMARY KEY,
          generated_summary TEXT NOT NULL DEFAULT '',
          confirmed_summary TEXT,
          memory_enabled INTEGER NOT NULL DEFAULT 1,
          event_count INTEGER NOT NULL DEFAULT 0,
          updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS user_behavior_events (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          event_type TEXT NOT NULL,
          article_id INTEGER,
          source_id TEXT,
          created_at TEXT NOT NULL,
          payload TEXT
        );
        "#,
    )?;
    let _ = conn.execute("ALTER TABLE articles ADD COLUMN article_key TEXT", []);
    let _ = conn.execute("ALTER TABLE articles ADD COLUMN normalized_link TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE articles ADD COLUMN source_id TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE articles ADD COLUMN discipline TEXT NOT NULL DEFAULT 'other'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE articles ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'technical-blog'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE articles ADD COLUMN resource_type TEXT NOT NULL DEFAULT 'article'",
        [],
    );
    let _ = conn.execute("ALTER TABLE articles ADD COLUMN fetched_at TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE source_catalog ADD COLUMN module TEXT NOT NULL DEFAULT 'other'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE source_catalog ADD COLUMN bucket TEXT NOT NULL DEFAULT 'unspecified'",
        [],
    );
    let _ = conn.execute(
        "UPDATE articles SET fetched_at = published_at WHERE fetched_at IS NULL AND published_at IS NOT NULL",
        [],
    );
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_articles_identity ON articles(source_id, article_key);
        CREATE INDEX IF NOT EXISTS idx_articles_normalized_link ON articles(normalized_link);
        "#,
    )?;

    let catalog = load_catalog(app)?;
    seed_defaults(&conn, &catalog)?;
    Ok(conn)
}

fn seed_defaults(conn: &Connection, catalog: &[RssSource]) -> Result<()> {
    if read_setting(conn, "api_key")?.is_none() {
        write_setting(conn, "api_key", "")?;
        write_setting(conn, "auto_start", "false")?;
        write_setting(conn, "active_view", "\"settings\"")?;
        write_setting(conn, "selected_article_id", "null")?;
        write_setting(conn, "api_key_valid", "false")?;
        write_setting(conn, "last_scan_at", "null")?;
        write_setting(conn, "memory_mode_enabled", "true")?;
    } else {
        ensure_setting(conn, "auto_start", "false")?;
        ensure_setting(conn, "active_view", "\"settings\"")?;
        ensure_setting(conn, "selected_article_id", "null")?;
        ensure_setting(conn, "api_key_valid", "false")?;
        ensure_setting(conn, "last_scan_at", "null")?;
        ensure_setting(conn, "memory_mode_enabled", "true")?;
    }

    sync_source_catalog(conn, catalog)?;
    sync_discipline_preferences(conn)?;
    sync_user_source_pool(conn, catalog)?;
    sync_source_fetch_state(conn, catalog)?;
    Ok(())
}

fn ensure_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    if read_setting(conn, key)?.is_none() {
        write_setting(conn, key, value)?;
    }
    Ok(())
}

fn sync_source_catalog(conn: &Connection, catalog: &[RssSource]) -> Result<()> {
    let known_ids = catalog
        .iter()
        .map(|source| source.id.clone())
        .collect::<BTreeSet<_>>();

    let existing_ids = {
        let mut stmt = conn.prepare("SELECT source_id FROM source_catalog")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        ids
    };
    for source_id in existing_ids {
        if !known_ids.contains(&source_id) {
            conn.execute(
                "DELETE FROM source_catalog WHERE source_id = ?1",
                params![source_id],
            )?;
        }
    }

    for source in catalog {
        conn.execute(
            r#"
            INSERT INTO source_catalog (
                            source_id, name, rss_url, module, bucket, discipline, source_kind, resource_type,
                            language, enabled_by_default, postponed, origin_files
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(source_id) DO UPDATE SET
              name = excluded.name,
              rss_url = excluded.rss_url,
                            module = excluded.module,
                            bucket = excluded.bucket,
              discipline = excluded.discipline,
              source_kind = excluded.source_kind,
              resource_type = excluded.resource_type,
              language = excluded.language,
              enabled_by_default = excluded.enabled_by_default,
              postponed = excluded.postponed,
              origin_files = excluded.origin_files
            "#,
            params![
                source.id,
                source.name,
                source.url,
                source.module.as_str(),
                source.bucket.as_str(),
                discipline_to_raw(&source.discipline),
                source_kind_to_raw(&source.source_kind),
                resource_type_to_raw(&source.resource_type),
                source.language,
                bool_to_int(source.enabled_by_default),
                bool_to_int(source.postponed),
                serde_json::to_string(&source.origin_files)?,
            ],
        )?;
    }
    Ok(())
}

fn sync_discipline_preferences(conn: &Connection) -> Result<()> {
    for discipline in all_disciplines() {
        conn.execute(
            r#"
            INSERT INTO user_interest_profile_v2 (discipline, enabled, preference)
            VALUES (?1, 0, '')
            ON CONFLICT(discipline) DO NOTHING
            "#,
            params![discipline_to_raw(&discipline)],
        )?;
    }
    Ok(())
}

fn sync_user_source_pool(conn: &Connection, catalog: &[RssSource]) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let known_ids = catalog
        .iter()
        .map(|source| source.id.clone())
        .collect::<BTreeSet<_>>();
    let existing_ids = {
        let mut stmt = conn.prepare("SELECT source_id FROM user_source_pool")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        ids
    };
    for source_id in existing_ids {
        if !known_ids.contains(&source_id) {
            conn.execute(
                "DELETE FROM user_source_pool WHERE source_id = ?1",
                params![source_id],
            )?;
        }
    }
    for source in catalog {
        conn.execute(
            r#"
            INSERT INTO user_source_pool (source_id, enabled, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(source_id) DO NOTHING
            "#,
            params![source.id, bool_to_int(source.enabled_by_default), now],
        )?;
    }
    Ok(())
}

fn sync_source_fetch_state(conn: &Connection, catalog: &[RssSource]) -> Result<()> {
    for source in catalog {
        conn.execute(
            r#"
            INSERT INTO source_fetch_state (source_id, last_fetched_at, last_success_at, last_error)
            VALUES (?1, NULL, NULL, NULL)
            ON CONFLICT(source_id) DO NOTHING
            "#,
            params![source.id],
        )?;
    }
    Ok(())
}

pub fn read_settings(conn: &Connection) -> Result<SettingsPayload> {
    Ok(SettingsPayload {
        api_key: read_setting(conn, "api_key")?.unwrap_or_default(),
        auto_start: read_setting(conn, "auto_start")?.unwrap_or_else(|| "false".into()) == "true",
        disciplines: list_discipline_preferences(conn)?,
        memory_mode_enabled: read_setting(conn, "memory_mode_enabled")?
            .unwrap_or_else(|| "true".into())
            == "true",
        memory_summary: read_latest_memory(conn)?
            .map(|memory| memory.summary)
            .unwrap_or_default(),
        rss_sources: list_user_sources(conn)?,
    })
}

pub fn write_settings(conn: &Connection, settings: &SettingsPayload) -> Result<()> {
    write_setting(conn, "api_key", settings.api_key.trim())?;
    write_setting(
        conn,
        "auto_start",
        if settings.auto_start { "true" } else { "false" },
    )?;
    write_setting(
        conn,
        "memory_mode_enabled",
        if settings.memory_mode_enabled {
            "true"
        } else {
            "false"
        },
    )?;

    let known_disciplines = settings
        .disciplines
        .iter()
        .map(|item| (item.discipline.clone(), item.clone()))
        .collect::<BTreeMap<_, _>>();
    for discipline in all_disciplines() {
        let item = known_disciplines.get(&discipline);
        conn.execute(
            r#"
            INSERT INTO user_interest_profile_v2 (discipline, enabled, preference)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(discipline) DO UPDATE SET
              enabled = excluded.enabled,
              preference = excluded.preference
            "#,
            params![
                discipline_to_raw(&discipline),
                bool_to_int(item.map(|value| value.enabled).unwrap_or(false)),
                item.map(|value| value.preference.trim()).unwrap_or(""),
            ],
        )?;
    }

    let now = Utc::now().to_rfc3339();
    for source in &settings.rss_sources {
        conn.execute(
            r#"
            INSERT INTO user_source_pool (source_id, enabled, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(source_id) DO UPDATE SET
              enabled = excluded.enabled,
              updated_at = excluded.updated_at
            "#,
            params![source.id, bool_to_int(source.enabled), now],
        )?;
    }

    write_memory_summary(
        conn,
        settings.memory_mode_enabled,
        settings.memory_summary.trim(),
    )?;
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
    write_setting(
        conn,
        "selected_article_id",
        &serde_json::to_string(&article_id)?,
    )?;
    Ok(())
}

pub fn list_articles(conn: &Connection) -> Result<Vec<ArticleRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
                    a.id, a.source_id, a.title, a.link, a.source_name, a.discipline, a.source_kind, a.resource_type,
                    a.published_at, a.fetched_at, a.summary, a.fit_level, a.fit_score, a.recommendation_reason,
                    a.raw_content, a.is_favorite, a.is_new
                FROM articles a
                WHERE EXISTS (
                    SELECT 1
                    FROM reminder_batch_articles rba
                    WHERE rba.article_id = a.id
                )
        ORDER BY COALESCE(published_at, fetched_at, '1970-01-01T00:00:00Z') DESC, id DESC
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        let published_at_raw: Option<String> = row.get(8)?;
        let fetched_at_raw: Option<String> = row.get(9)?;
        let fit_level_raw: String = row.get(11)?;
        Ok(ArticleRecord {
            id: row.get(0)?,
            source_id: row.get(1)?,
            title: row.get(2)?,
            link: row.get(3)?,
            source_name: row.get(4)?,
            discipline: parse_discipline(&row.get::<_, String>(5)?),
            source_kind: parse_source_kind(&row.get::<_, String>(6)?),
            resource_type: parse_resource_type(&row.get::<_, String>(7)?),
            published_at: parse_optional_datetime(published_at_raw),
            fetched_at: parse_optional_datetime(fetched_at_raw),
            summary: row.get(10)?,
            fit_level: parse_fit_level(&fit_level_raw),
            fit_score: row.get(12)?,
            recommendation_reason: row.get(13)?,
            raw_content: row.get(14)?,
            is_favorite: row.get::<_, i64>(15)? == 1,
            is_new: row.get::<_, i64>(16)? == 1,
        })
    })?;

    let mut articles = Vec::new();
    for row in rows {
        articles.push(row?);
    }
    Ok(articles)
}

pub fn toggle_favorite(conn: &Connection, article_id: i64) -> Result<bool> {
    conn.execute(
        "UPDATE articles SET is_favorite = CASE WHEN is_favorite = 1 THEN 0 ELSE 1 END WHERE id = ?1",
        params![article_id],
    )?;
    let is_favorite = conn.query_row(
        "SELECT is_favorite FROM articles WHERE id = ?1",
        params![article_id],
        |row| row.get::<_, i64>(0),
    )? == 1;
    Ok(is_favorite)
}

pub fn mark_article_opened(conn: &Connection, article_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE articles SET is_new = 0 WHERE id = ?1",
        params![article_id],
    )?;
    write_selected_article_id(conn, Some(article_id))?;
    Ok(())
}

pub fn article_source_id(conn: &Connection, article_id: i64) -> Result<Option<String>> {
    conn.query_row(
        "SELECT source_id FROM articles WHERE id = ?1",
        params![article_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn find_article_id_by_identity(
    conn: &Connection,
    source_id: &str,
    article_key: &str,
    normalized_link: &str,
) -> Result<Option<i64>> {
    conn.query_row(
        r#"
        SELECT id
        FROM articles
        WHERE (source_id = ?1 AND article_key = ?2)
           OR normalized_link = ?3
        LIMIT 1
        "#,
        params![source_id, article_key, normalized_link],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
pub fn insert_article(
    conn: &Connection,
    source_id: &str,
    guid: &str,
    article_key: &str,
    normalized_link: &str,
    title: &str,
    link: &str,
    source_name: &str,
    discipline: &Discipline,
    source_kind: &SourceKind,
    resource_type: &ResourceType,
    published_at: Option<DateTime<Utc>>,
    fetched_at: DateTime<Utc>,
    raw_content: &str,
    summary: &str,
    fit_level: &FitLevel,
    fit_score: i64,
    recommendation_reason: &str,
) -> Result<i64> {
    conn.execute(
        r#"
        INSERT INTO articles (
          guid, article_key, normalized_link, source_id, title, link, source_name, discipline,
          source_kind, resource_type, published_at, fetched_at, raw_content, summary, fit_level,
          fit_score, recommendation_reason, is_favorite, is_new
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 0, 1)
        "#,
        params![
            guid,
            article_key,
            normalized_link,
            source_id,
            title,
            link,
            source_name,
            discipline_to_raw(discipline),
            source_kind_to_raw(source_kind),
            resource_type_to_raw(resource_type),
            published_at.map(|value| value.to_rfc3339()),
            fetched_at.to_rfc3339(),
            raw_content,
            summary,
            fit_level_to_raw(fit_level),
            fit_score,
            recommendation_reason,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn current_active_batch_for_updates(conn: &Connection) -> Result<Option<String>> {
    let now = Utc::now().to_rfc3339();
    let batch = conn
        .query_row(
            r#"
            SELECT id
            FROM reminder_batches
            WHERE status = 'active'
               OR (status = 'snoozed' AND remind_at IS NOT NULL AND remind_at <= ?1)
            ORDER BY created_at ASC
            LIMIT 1
            "#,
            params![now],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(batch_id) = &batch {
        conn.execute(
            "UPDATE reminder_batches SET status = 'active', remind_at = NULL WHERE id = ?1",
            params![batch_id],
        )?;
    }
    Ok(batch)
}

pub fn active_reminder_batch(conn: &Connection) -> Result<Option<ReminderBatchSnapshot>> {
    let Some(batch_id) = current_active_batch_for_updates(conn)? else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        r#"
                SELECT a.id, a.fit_score, a.source_kind, sc.module, sc.bucket
        FROM reminder_batch_articles rba
        JOIN articles a ON a.id = rba.article_id
                LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
        WHERE rba.batch_id = ?1
          AND a.is_new = 1
        ORDER BY a.fit_score DESC, COALESCE(a.published_at, a.fetched_at) DESC, a.id DESC
        "#,
    )?;

    let rows = stmt.query_map(params![batch_id.clone()], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    let mut article_ids = Vec::new();
    let mut top_article_id = None;
    let mut partitions = BTreeSet::new();
    for (index, row) in rows.enumerate() {
        let (article_id, _fit_score, source_kind, module, bucket) = row?;
        if index == 0 {
            top_article_id = Some(article_id);
        }
        article_ids.push(article_id);
        let module = module.unwrap_or_else(|| "other".to_string());
        let bucket = bucket.unwrap_or_else(|| source_kind.clone());
        partitions.insert(format!("{module}/{bucket}"));
    }

    if article_ids.is_empty() {
        conn.execute(
            "UPDATE reminder_batches SET status = 'opened', remind_at = NULL WHERE id = ?1",
            params![batch_id],
        )?;
        return Ok(None);
    }

    let article_count = article_ids.len();
    Ok(Some(ReminderBatchSnapshot {
        id: batch_id,
        article_ids,
        article_count,
        top_article_id,
        partition_count: partitions.len(),
    }))
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

pub fn set_batch_status(
    conn: &Connection,
    batch_id: &str,
    status: &str,
    remind_at: Option<String>,
) -> Result<()> {
    conn.execute(
        "UPDATE reminder_batches SET status = ?2, remind_at = ?3 WHERE id = ?1",
        params![batch_id, status, remind_at],
    )?;
    Ok(())
}

pub fn update_fetch_state(
    conn: &Connection,
    source_id: &str,
    last_fetched_at: DateTime<Utc>,
    last_success_at: Option<DateTime<Utc>>,
    last_error: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO source_fetch_state (source_id, last_fetched_at, last_success_at, last_error)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(source_id) DO UPDATE SET
          last_fetched_at = excluded.last_fetched_at,
          last_success_at = excluded.last_success_at,
          last_error = excluded.last_error
        "#,
        params![
            source_id,
            last_fetched_at.to_rfc3339(),
            last_success_at.map(|value| value.to_rfc3339()),
            last_error,
        ],
    )?;
    Ok(())
}

pub fn list_due_sources(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<RssSource>> {
    let selected = list_selected_effective_disciplines(conn)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut stmt = conn.prepare(
        r#"
        SELECT
                    sc.source_id, sc.name, sc.rss_url, sc.module, sc.bucket, sc.discipline,
                    sc.source_kind, sc.resource_type, sc.language, sc.enabled_by_default,
                    sc.postponed, sc.origin_files, usp.enabled, sfs.last_fetched_at
        FROM source_catalog sc
        JOIN user_source_pool usp ON usp.source_id = sc.source_id
        LEFT JOIN source_fetch_state sfs ON sfs.source_id = sc.source_id
        ORDER BY sc.name ASC
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        let module: String = row.get(3)?;
        let bucket: String = row.get(4)?;
        let discipline = parse_discipline(&row.get::<_, String>(5)?);
        let source_kind = parse_source_kind(&row.get::<_, String>(6)?);
        let resource_type = parse_resource_type(&row.get::<_, String>(7)?);
        let origin_files_json: String = row.get(11)?;
        let origin_files =
            serde_json::from_str::<Vec<String>>(&origin_files_json).unwrap_or_default();
        let last_fetched_raw: Option<String> = row.get(13)?;
        Ok((
            RssSource {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                module,
                bucket,
                discipline: discipline.clone(),
                source_kind: source_kind.clone(),
                resource_type,
                language: row.get(8)?,
                enabled: row.get::<_, i64>(12)? == 1,
                enabled_by_default: row.get::<_, i64>(9)? == 1,
                postponed: row.get::<_, i64>(10)? == 1,
                origin_files,
            },
            last_fetched_raw,
        ))
    })?;

    let mut sources = Vec::new();
    for row in rows {
        let (source, last_fetched_raw) = row?;
        if !source.enabled || !selected.contains(&source.discipline) {
            continue;
        }
        let due = last_fetched_raw
            .as_deref()
            .and_then(parse_datetime)
            .map(|last_fetched| {
                now - last_fetched
                    >= policy::fetch_interval_for_source(
                        &source.module,
                        &source.bucket,
                        &source.source_kind,
                    )
            })
            .unwrap_or(true);
        if due {
            sources.push(source);
        }
    }
    Ok(sources)
}

pub fn upsert_content_pool_entry(
    conn: &Connection,
    article_id: i64,
    source_kind: &SourceKind,
    fit_score: i64,
    published_at: Option<DateTime<Utc>>,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO ranked_content_pool (article_id, source_kind, fit_score, published_at, inserted_at)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(article_id) DO UPDATE SET
          source_kind = excluded.source_kind,
          fit_score = excluded.fit_score,
          published_at = excluded.published_at,
          inserted_at = excluded.inserted_at
        "#,
        params![
            article_id,
            source_kind_to_raw(source_kind),
            fit_score,
            published_at.map(|value| value.to_rfc3339()),
            Utc::now().to_rfc3339(),
        ],
    )?;
    trim_content_pool(conn, source_kind)?;
    Ok(())
}

fn trim_content_pool(conn: &Connection, source_kind: &SourceKind) -> Result<()> {
    let source_kind_raw = source_kind_to_raw(source_kind);
    conn.execute(
        r#"
        DELETE FROM ranked_content_pool
        WHERE article_id IN (
          SELECT article_id
          FROM ranked_content_pool
          WHERE source_kind = ?1
          ORDER BY fit_score DESC, COALESCE(published_at, inserted_at) DESC, article_id DESC
          LIMIT -1 OFFSET ?2
        )
        "#,
        params![source_kind_raw, MAX_POOL_SIZE_PER_KIND as i64],
    )?;
    Ok(())
}

pub fn content_pool_stats(conn: &Connection) -> Result<Vec<ContentPoolStat>> {
    let mut stats = Vec::new();
    for source_kind in [
        SourceKind::AcademicJournal,
        SourceKind::OfficialAnnouncement,
        SourceKind::TechnicalBlog,
        SourceKind::CommunityHotspot,
    ] {
        let source_kind_raw = source_kind_to_raw(&source_kind);
        let total_articles: usize = conn.query_row(
            "SELECT COUNT(*) FROM ranked_content_pool WHERE source_kind = ?1",
            params![source_kind_raw],
            |row| row.get::<_, i64>(0),
        )? as usize;
        let candidate_count: usize = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM (
              SELECT article_id
              FROM ranked_content_pool rcp
              JOIN articles a ON a.id = rcp.article_id
              WHERE rcp.source_kind = ?1
                AND a.fit_level = 'high'
              ORDER BY rcp.fit_score DESC, COALESCE(rcp.published_at, rcp.inserted_at) DESC
              LIMIT 3
            )
            "#,
            params![source_kind_raw],
            |row| row.get::<_, i64>(0),
        )? as usize;
        let top_score = conn
            .query_row(
                "SELECT fit_score FROM ranked_content_pool WHERE source_kind = ?1 ORDER BY fit_score DESC LIMIT 1",
                params![source_kind_raw],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        stats.push(ContentPoolStat {
            source_kind,
            total_articles,
            candidate_count,
            top_score,
        });
    }
    Ok(stats)
}

pub fn log_user_event(
    conn: &Connection,
    event_type: &str,
    article_id: Option<i64>,
    source_id: Option<&str>,
    payload: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO user_behavior_events (event_type, article_id, source_id, created_at, payload)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            event_type,
            article_id,
            source_id,
            Utc::now().to_rfc3339(),
            payload
        ],
    )?;
    Ok(())
}

pub fn refresh_daily_memory(
    conn: &Connection,
    memory_enabled: bool,
) -> Result<Option<InterestMemoryRecord>> {
    if !memory_enabled {
        return read_latest_memory(conn);
    }

    let day_key = Utc::now().format("%Y-%m-%d").to_string();
    let like_prefix = format!("{day_key}%");
    let opened_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_behavior_events WHERE event_type = 'open-article' AND created_at LIKE ?1",
        params![like_prefix],
        |row| row.get(0),
    )?;
    let favorite_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_behavior_events WHERE event_type = 'favorite-added' AND created_at LIKE ?1",
        params![format!("{day_key}%")],
        |row| row.get(0),
    )?;
    let reminder_view_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_behavior_events WHERE event_type = 'bubble-view' AND created_at LIKE ?1",
        params![format!("{day_key}%")],
        |row| row.get(0),
    )?;
    let reminder_ignore_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_behavior_events WHERE event_type = 'bubble-ignore' AND created_at LIKE ?1",
        params![format!("{day_key}%")],
        |row| row.get(0),
    )?;

    let mut top_disciplines_stmt = conn.prepare(
        r#"
        SELECT a.discipline, COUNT(*)
        FROM user_behavior_events ube
        JOIN articles a ON a.id = ube.article_id
        WHERE ube.created_at LIKE ?1
          AND ube.event_type IN ('open-article', 'favorite-added')
        GROUP BY a.discipline
        ORDER BY COUNT(*) DESC, a.discipline ASC
        LIMIT 2
        "#,
    )?;
    let top_discipline_rows = top_disciplines_stmt
        .query_map(params![format!("{day_key}%")], |row| {
            Ok(parse_discipline(&row.get::<_, String>(0)?))
        })?;
    let mut top_disciplines = Vec::new();
    for row in top_discipline_rows {
        top_disciplines.push(row?);
    }

    let mut top_kind_stmt = conn.prepare(
        r#"
        SELECT a.source_kind, COUNT(*)
        FROM user_behavior_events ube
        JOIN articles a ON a.id = ube.article_id
        WHERE ube.created_at LIKE ?1
          AND ube.event_type IN ('open-article', 'favorite-added')
        GROUP BY a.source_kind
        ORDER BY COUNT(*) DESC, a.source_kind ASC
        LIMIT 1
        "#,
    )?;
    let top_source_kind = top_kind_stmt
        .query_row(params![format!("{day_key}%")], |row| {
            row.get::<_, String>(0)
        })
        .optional()?
        .map(|value| parse_source_kind(&value));

    let mut top_bucket_stmt = conn.prepare(
        r#"
        SELECT sc.module, sc.bucket, COUNT(*)
        FROM user_behavior_events ube
        JOIN articles a ON a.id = ube.article_id
        LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
        WHERE ube.created_at LIKE ?1
          AND ube.event_type IN ('open-article', 'favorite-added')
        GROUP BY sc.module, sc.bucket
        ORDER BY COUNT(*) DESC, sc.module ASC, sc.bucket ASC
        LIMIT 2
        "#,
    )?;
    let top_bucket_rows = top_bucket_stmt.query_map(params![format!("{day_key}%")], |row| {
        let module = row
            .get::<_, Option<String>>(0)?
            .unwrap_or_else(|| "other".to_string());
        let bucket = row
            .get::<_, Option<String>>(1)?
            .unwrap_or_else(|| "unspecified".to_string());
        Ok((module, bucket))
    })?;
    let mut top_buckets = Vec::new();
    for row in top_bucket_rows {
        top_buckets.push(row?);
    }

    let discipline_summary = if top_disciplines.is_empty() {
        "你今天的行为还不够多，系统暂时沿用当前已配置的兴趣方向。".to_string()
    } else {
        format!(
            "你今天更关注 {} 方向的内容，说明这些主题仍然最值得优先推送。",
            top_disciplines
                .iter()
                .map(Discipline::display_name)
                .collect::<Vec<_>>()
                .join("、")
        )
    };
    let action_summary = format!(
        "今天共打开 {} 篇、收藏 {} 篇、主动查看提醒 {} 次、忽略提醒 {} 次{}。",
        opened_count,
        favorite_count,
        reminder_view_count,
        reminder_ignore_count,
        top_source_kind
            .map(|kind| format!("，你对 {} 的响应更积极", source_kind_label(&kind)))
            .unwrap_or_default()
    );

    let bucket_summary = if top_buckets.is_empty() {
        String::new()
    } else {
        format!(
            " 高频子分类集中在 {}。",
            top_buckets
                .iter()
                .map(|(module, bucket)| {
                    format!(
                        "{} / {}",
                        module_label(module),
                        bucket_label(bucket)
                    )
                })
                .collect::<Vec<_>>()
                .join("、")
        )
    };

    let generated_summary = format!("{discipline_summary} {action_summary}{bucket_summary}");
    conn.execute(
        r#"
        INSERT INTO daily_interest_memory (day_key, generated_summary, confirmed_summary, memory_enabled, event_count, updated_at)
        VALUES (?1, ?2, COALESCE((SELECT confirmed_summary FROM daily_interest_memory WHERE day_key = ?1), NULL), ?3, ?4, ?5)
        ON CONFLICT(day_key) DO UPDATE SET
          generated_summary = excluded.generated_summary,
          memory_enabled = excluded.memory_enabled,
          event_count = excluded.event_count,
          updated_at = excluded.updated_at
        "#,
        params![
            day_key,
            generated_summary,
            bool_to_int(memory_enabled),
            (opened_count + favorite_count + reminder_view_count + reminder_ignore_count) as i64,
            Utc::now().to_rfc3339(),
        ],
    )?;

    read_latest_memory(conn)
}

pub fn read_latest_memory(conn: &Connection) -> Result<Option<InterestMemoryRecord>> {
    conn.query_row(
        r#"
        SELECT day_key, generated_summary, COALESCE(NULLIF(confirmed_summary, ''), generated_summary),
               memory_enabled, event_count, updated_at
        FROM daily_interest_memory
        ORDER BY day_key DESC
        LIMIT 1
        "#,
        [],
        |row| {
            let updated_at_raw: Option<String> = row.get(5)?;
            Ok(InterestMemoryRecord {
                day_key: row.get(0)?,
                generated_summary: row.get(1)?,
                summary: row.get(2)?,
                memory_mode_enabled: row.get::<_, i64>(3)? == 1,
                event_count: row.get::<_, i64>(4)? as usize,
                updated_at: parse_optional_datetime(updated_at_raw),
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn write_memory_summary(conn: &Connection, memory_enabled: bool, summary: &str) -> Result<()> {
    if summary.is_empty() && read_latest_memory(conn)?.is_none() {
        return Ok(());
    }
    let day_key = Utc::now().format("%Y-%m-%d").to_string();
    conn.execute(
        r#"
        INSERT INTO daily_interest_memory (day_key, generated_summary, confirmed_summary, memory_enabled, event_count, updated_at)
        VALUES (?1, '', ?2, ?3, 0, ?4)
        ON CONFLICT(day_key) DO UPDATE SET
          confirmed_summary = excluded.confirmed_summary,
          memory_enabled = excluded.memory_enabled,
          updated_at = excluded.updated_at
        "#,
        params![day_key, summary, bool_to_int(memory_enabled), Utc::now().to_rfc3339()],
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
    let settings = read_settings(conn)?;
    let active_reminder = active_reminder_batch(conn)?;
    let due_sources = list_due_sources(conn, Utc::now())?.len();
    let selected_disciplines = settings
        .disciplines
        .iter()
        .filter(|item| item.enabled)
        .count();
    let total_sources: usize = conn.query_row("SELECT COUNT(*) FROM source_catalog", [], |row| {
        row.get::<_, i64>(0)
    })? as usize;
    let postponed_sources: usize = conn.query_row(
        "SELECT COUNT(*) FROM source_catalog WHERE postponed = 1",
        [],
        |row| row.get::<_, i64>(0),
    )? as usize;
    let enabled_sources = list_dueable_enabled_sources_count(conn)? as usize;

    Ok(Snapshot {
        settings,
        pet_status,
        articles: list_articles(conn)?,
        active_reminder,
        selected_article_id: read_selected_article_id(conn)?,
        active_view: read_active_view(conn)?,
        last_error,
        api_key_valid,
        last_scan_at,
        content_pool_stats: content_pool_stats(conn)?,
        memory: read_latest_memory(conn)?,
        source_summary: SourceCatalogSummary {
            total_sources,
            enabled_sources,
            postponed_sources,
            selected_disciplines,
            due_sources,
        },
    })
}

pub fn read_api_key_valid(conn: &Connection) -> Result<bool> {
    Ok(read_setting(conn, "api_key_valid")?.unwrap_or_else(|| "false".into()) == "true")
}

pub fn write_api_key_valid(conn: &Connection, value: bool) -> Result<()> {
    write_setting(conn, "api_key_valid", if value { "true" } else { "false" })
}

pub fn read_last_scan_at(conn: &Connection) -> Result<Option<DateTime<Utc>>> {
    let raw = read_setting(conn, "last_scan_at")?.unwrap_or_else(|| "null".into());
    let value: Option<String> = serde_json::from_str(&raw).unwrap_or(None);
    Ok(value.as_deref().and_then(parse_datetime))
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

fn list_user_sources(conn: &Connection) -> Result<Vec<RssSource>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          sc.source_id, sc.name, sc.rss_url, sc.module, sc.bucket, sc.discipline,
          sc.source_kind, sc.resource_type, sc.language, usp.enabled,
          sc.enabled_by_default, sc.postponed, sc.origin_files
        FROM source_catalog sc
        JOIN user_source_pool usp ON usp.source_id = sc.source_id
        ORDER BY sc.postponed ASC, sc.module ASC, sc.bucket ASC, sc.name ASC
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        let origin_files_json: String = row.get(12)?;
        let origin_files =
            serde_json::from_str::<Vec<String>>(&origin_files_json).unwrap_or_default();
        Ok(RssSource {
            id: row.get(0)?,
            name: row.get(1)?,
            url: row.get(2)?,
            module: row.get(3)?,
            bucket: row.get(4)?,
            discipline: parse_discipline(&row.get::<_, String>(5)?),
            source_kind: parse_source_kind(&row.get::<_, String>(6)?),
            resource_type: parse_resource_type(&row.get::<_, String>(7)?),
            language: row.get(8)?,
            enabled: row.get::<_, i64>(9)? == 1,
            enabled_by_default: row.get::<_, i64>(10)? == 1,
            postponed: row.get::<_, i64>(11)? == 1,
            origin_files,
        })
    })?;

    let mut sources = Vec::new();
    for row in rows {
        sources.push(row?);
    }
    Ok(sources)
}

fn list_discipline_preferences(conn: &Connection) -> Result<Vec<UserDisciplinePreference>> {
    let mut stmt = conn.prepare(
        "SELECT discipline, enabled, preference FROM user_interest_profile_v2 ORDER BY discipline ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(UserDisciplinePreference {
            discipline: parse_discipline(&row.get::<_, String>(0)?),
            enabled: row.get::<_, i64>(1)? == 1,
            preference: row.get(2)?,
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    items.sort_by(|left, right| left.discipline.cmp(&right.discipline));
    Ok(items)
}

fn list_selected_effective_disciplines(conn: &Connection) -> Result<Vec<Discipline>> {
    Ok(list_discipline_preferences(conn)?
        .into_iter()
        .filter(|item| item.enabled)
        .map(|item| item.discipline)
        .collect())
}

fn list_dueable_enabled_sources_count(conn: &Connection) -> Result<i64> {
    conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM source_catalog sc
        JOIN user_source_pool usp ON usp.source_id = sc.source_id
                WHERE usp.enabled = 1
        "#,
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn load_catalog(app: &AppHandle) -> Result<Vec<RssSource>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CatalogResource {
        id: String,
        name: String,
        url: String,
        #[serde(default)]
        module: Option<String>,
        #[serde(default)]
        bucket: Option<String>,
        discipline: Discipline,
        source_kind: SourceKind,
        resource_type: ResourceType,
        language: Option<String>,
        enabled_by_default: bool,
        postponed: bool,
        origin_files: Vec<String>,
    }

    let mut load_errors = Vec::new();
    for candidate in build_resource_candidates(app, "rss_catalog_v3_unified.opml") {
        if !candidate.exists() {
            continue;
        }
        match parse_v3_opml_catalog(&candidate) {
            Ok(catalog) if !catalog.is_empty() => return Ok(catalog),
            Ok(_) => load_errors.push(format!(
                "v3 catalog empty after parse: {}",
                candidate.display()
            )),
            Err(err) => load_errors.push(err.to_string()),
        }
    }

    for candidate in build_resource_candidates(app, "rss-catalog-v2-1.json") {
        if candidate.exists() {
            let raw = fs::read_to_string(&candidate)
                .with_context(|| format!("failed to read rss catalog: {}", candidate.display()))?;
            let resources: Vec<CatalogResource> = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse rss catalog: {}", candidate.display()))?;
            return Ok(resources
                .into_iter()
                .map(|resource| {
                    let CatalogResource {
                        id,
                        name,
                        url,
                        module,
                        bucket,
                        discipline,
                        source_kind,
                        resource_type,
                        language,
                        enabled_by_default,
                        postponed,
                        origin_files,
                    } = resource;

                    let module = module
                        .as_deref()
                        .map(policy::normalize_module)
                        .unwrap_or_else(|| legacy_module_from_discipline(&discipline));
                    let bucket = bucket
                        .as_deref()
                        .map(|value| policy::normalize_bucket(&module, value))
                        .unwrap_or_else(|| legacy_bucket_from_source_kind(&source_kind));

                    RssSource {
                        id,
                        name,
                        url,
                        module,
                        bucket,
                        discipline,
                        source_kind,
                        resource_type,
                        language,
                        enabled: enabled_by_default,
                        enabled_by_default,
                        postponed,
                        origin_files,
                    }
                })
                .collect());
        }
    }

    if load_errors.is_empty() {
        anyhow::bail!("rss catalog file not found")
    } else {
        anyhow::bail!(
            "failed to load v3 catalog and no fallback catalog available: {}",
            load_errors.join(" ; ")
        )
    }
}

fn build_resource_candidates(app: &AppHandle, resource_name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = app.path_resolver().resolve_resource(resource_name) {
        candidates.push(path);
    }
    if let Some(path) = app
        .path_resolver()
        .resolve_resource(format!("resources/{resource_name}"))
    {
        candidates.push(path);
    }
    if let Some(dir) = app.path_resolver().resource_dir() {
        candidates.push(dir.join(resource_name));
        candidates.push(dir.join("resources").join(resource_name));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(resource_name),
    );

    let mut dedup = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|path| dedup.insert(path.clone()))
        .collect()
}

fn parse_v3_opml_catalog(path: &Path) -> Result<Vec<RssSource>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read v3 rss catalog: {}", path.display()))?;
    let mut reader = Reader::from_str(&raw);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut grouped = BTreeMap::<String, RssSource>::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if event.name().as_ref() == b"outline" =>
            {
                let mut attrs = BTreeMap::<String, String>::new();
                for attr in event.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let value = String::from_utf8_lossy(attr.value.as_ref())
                        .trim()
                        .to_string();
                    attrs.insert(key, value);
                }

                let Some(url) = attrs
                    .get("xmlUrl")
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                else {
                    buf.clear();
                    continue;
                };

                let name = attrs
                    .get("text")
                    .map(String::as_str)
                    .or_else(|| attrs.get("title").map(String::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("Untitled Source")
                    .to_string();
                let category = attrs.get("category").map(String::as_str);
                let module = attrs.get("module").map(String::as_str);
                let bucket = attrs.get("bucket").map(String::as_str);
                let module_code = normalize_v3_module(module, category);
                let bucket_code = normalize_v3_bucket(&module_code, bucket, category);
                let language = attrs
                    .get("language")
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let resource_type =
                    map_v3_resource_type(attrs.get("resourceType").map(String::as_str), url);
                let origin_files = attrs
                    .get("origin")
                    .map(|value| split_origin_files(value))
                    .unwrap_or_default();

                let normalized_url = normalize_url(url);
                let source = grouped
                    .entry(normalized_url.clone())
                    .or_insert_with(|| RssSource {
                        id: build_source_id(&name, &normalized_url),
                        name: name.clone(),
                        url: url.to_string(),
                        module: module_code.clone(),
                        bucket: bucket_code.clone(),
                        discipline: map_v3_module_to_discipline(&module_code),
                        source_kind: map_v3_bucket_to_source_kind(&bucket_code),
                        resource_type,
                        language: language.clone(),
                        enabled: true,
                        enabled_by_default: true,
                        postponed: false,
                        origin_files: Vec::new(),
                    });

                if source.name == "Untitled Source" && name != "Untitled Source" {
                    source.name = name;
                }
                if source.language.is_none() {
                    source.language = language;
                }
                source.postponed = false;
                source.enabled = true;
                source.enabled_by_default = true;
                source.module = module_code.clone();
                source.bucket = bucket_code.clone();
                source.discipline = map_v3_module_to_discipline(&module_code);
                source.source_kind = map_v3_bucket_to_source_kind(&bucket_code);
                source.resource_type =
                    map_v3_resource_type(attrs.get("resourceType").map(String::as_str), url);

                for origin in origin_files {
                    if !source.origin_files.contains(&origin) {
                        source.origin_files.push(origin);
                    }
                }
                source.id = build_source_id(&source.name, &normalized_url);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => {
                anyhow::bail!("failed to parse v3 rss catalog {}: {err}", path.display())
            }
        }
        buf.clear();
    }

    let mut catalog = grouped.into_values().collect::<Vec<_>>();
    catalog.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| left.bucket.cmp(&right.bucket))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(catalog)
}

fn normalize_v3_module(module: Option<&str>, category: Option<&str>) -> String {
    module
        .map(policy::normalize_module)
        .or_else(|| {
            category.and_then(|value| {
                value
                    .split(',')
                    .next()
                    .map(|part| policy::normalize_module(part.trim()))
            })
        })
        .unwrap_or_else(|| "other".to_string())
}

fn normalize_v3_bucket(module: &str, bucket: Option<&str>, category: Option<&str>) -> String {
    let module = policy::normalize_module(module);
    bucket
        .map(|value| policy::normalize_bucket(&module, value))
        .or_else(|| {
            category.and_then(|value| {
                value
                    .split(',')
                    .nth(1)
                    .map(|part| policy::normalize_bucket(&module, part.trim()))
            })
        })
        .unwrap_or_else(|| "unspecified".to_string())
}

fn map_v3_module_to_discipline(module: &str) -> Discipline {
    match module {
        "technology" => Discipline::Technology,
        "social_science" => Discipline::SocialScience,
        "business" => Discipline::Other,
        "growth" => Discipline::Life,
        "news_opinion" => Discipline::News,
        "entertainment" => Discipline::Humanities,
        "science" => Discipline::Science,
        "medicine" => Discipline::Medicine,
        _ => Discipline::Other,
    }
}

fn map_v3_bucket_to_source_kind(bucket: &str) -> SourceKind {
    match bucket {
        "research" | "academic_frontier" | "physics" | "chemistry" | "biology" => {
            SourceKind::AcademicJournal
        }
        "official" => SourceKind::OfficialAnnouncement,
        "blogs" => SourceKind::TechnicalBlog,
        "community" | "streaming" | "news" | "personal_opinion" | "streaming_opinion"
        | "community_opinion" | "media_opinion" | "lite_pool" => SourceKind::CommunityHotspot,
        _ => SourceKind::CommunityHotspot,
    }
}

fn legacy_module_from_discipline(discipline: &Discipline) -> String {
    match discipline {
        Discipline::Technology => "technology".to_string(),
        Discipline::SocialScience => "social_science".to_string(),
        Discipline::Other => "business".to_string(),
        Discipline::Life => "growth".to_string(),
        Discipline::News => "news_opinion".to_string(),
        Discipline::Humanities => "entertainment".to_string(),
        Discipline::Science => "science".to_string(),
        Discipline::Medicine => "medicine".to_string(),
    }
}

fn legacy_bucket_from_source_kind(source_kind: &SourceKind) -> String {
    match source_kind {
        SourceKind::AcademicJournal => "academic_frontier".to_string(),
        SourceKind::OfficialAnnouncement => "official".to_string(),
        SourceKind::TechnicalBlog => "blogs".to_string(),
        SourceKind::CommunityHotspot => "community".to_string(),
    }
}

fn map_v3_resource_type(resource_type: Option<&str>, url: &str) -> ResourceType {
    let raw = resource_type
        .map(|value| value.trim().to_lowercase())
        .unwrap_or_default();
    if raw == "podcast" {
        return ResourceType::Podcast;
    }
    if raw == "video" {
        return ResourceType::Video;
    }
    if raw == "social" || raw == "community" {
        return ResourceType::Twitter;
    }
    if raw == "rss" || raw == "community_feed" {
        return ResourceType::Article;
    }

    let url_lower = url.to_lowercase();
    if url_lower.contains("youtube.com") || url_lower.contains("bilibili.com") {
        ResourceType::Video
    } else if url_lower.contains("podcast") || url_lower.contains("xiaoyuzhou") {
        ResourceType::Podcast
    } else {
        ResourceType::Article
    }
}

fn split_origin_files(raw: &str) -> Vec<String> {
    raw.split(';')
        .flat_map(|part| part.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
}

fn normalize_url(url: &str) -> String {
    let trimmed = url.trim().to_lowercase();
    let no_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed.as_str())
        .to_string();
    let no_fragment = no_scheme.split('#').next().unwrap_or("").to_string();
    if no_fragment.ends_with('/') {
        no_fragment.trim_end_matches('/').to_string()
    } else {
        no_fragment
    }
}

fn build_source_id(name: &str, normalized_url: &str) -> String {
    let base = slugify(name);
    let mut hasher = DefaultHasher::new();
    normalized_url.hash(&mut hasher);
    let hash = hasher.finish() as u32;
    let prefix = if base.is_empty() {
        "source".to_string()
    } else {
        base.chars().take(32).collect::<String>()
    };
    format!("{prefix}-{hash:08x}")
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, '/' | ':' | '.' | '_' | '-' | ' ') {
            Some('-')
        } else {
            None
        };
        if let Some(ch) = normalized {
            if ch == '-' {
                if !prev_dash && !out.is_empty() {
                    out.push(ch);
                    prev_dash = true;
                }
            } else {
                out.push(ch);
                prev_dash = false;
            }
        }
    }
    out.trim_matches('-').to_string()
}

fn parse_optional_datetime(raw: Option<String>) -> Option<DateTime<Utc>> {
    raw.as_deref().and_then(parse_datetime)
}

fn parse_datetime(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn discipline_to_raw(value: &Discipline) -> &'static str {
    value.code()
}

fn source_kind_to_raw(value: &SourceKind) -> &'static str {
    value.code()
}

fn resource_type_to_raw(value: &ResourceType) -> &'static str {
    match value {
        ResourceType::Article => "article",
        ResourceType::Podcast => "podcast",
        ResourceType::Video => "video",
        ResourceType::Twitter => "twitter",
        ResourceType::Other => "other",
    }
}

fn fit_level_to_raw(value: &FitLevel) -> &'static str {
    match value {
        FitLevel::High => "high",
        FitLevel::Medium => "medium",
        FitLevel::Low => "low",
    }
}

fn parse_discipline(raw: &str) -> Discipline {
    match raw {
        "technology" => Discipline::Technology,
        "humanities" => Discipline::Humanities,
        "news" => Discipline::News,
        "social-science" => Discipline::SocialScience,
        "science" => Discipline::Science,
        "medicine" => Discipline::Medicine,
        "life" => Discipline::Life,
        _ => Discipline::Other,
    }
}

fn parse_source_kind(raw: &str) -> SourceKind {
    match raw {
        "academic-journal" => SourceKind::AcademicJournal,
        "official-announcement" => SourceKind::OfficialAnnouncement,
        "community-hotspot" => SourceKind::CommunityHotspot,
        _ => SourceKind::TechnicalBlog,
    }
}

fn parse_resource_type(raw: &str) -> ResourceType {
    match raw {
        "podcast" => ResourceType::Podcast,
        "video" => ResourceType::Video,
        "twitter" => ResourceType::Twitter,
        "other" => ResourceType::Other,
        _ => ResourceType::Article,
    }
}

fn parse_fit_level(raw: &str) -> FitLevel {
    match raw {
        "high" => FitLevel::High,
        "medium" => FitLevel::Medium,
        _ => FitLevel::Low,
    }
}

fn source_kind_label(value: &SourceKind) -> &'static str {
    match value {
        SourceKind::AcademicJournal => "学术杂志",
        SourceKind::OfficialAnnouncement => "官方公告",
        SourceKind::TechnicalBlog => "技术博客",
        SourceKind::CommunityHotspot => "社区热点",
    }
}

fn module_label(raw: &str) -> &'static str {
    match policy::normalize_module(raw).as_str() {
        "technology" => "科技",
        "social_science" => "社科",
        "business" => "商业",
        "growth" => "成长",
        "news_opinion" => "新闻观点",
        "entertainment" => "娱乐",
        "science" => "科学",
        "medicine" => "医学",
        _ => "其他",
    }
}

fn bucket_label(raw: &str) -> &'static str {
    match raw {
        "research" => "研究",
        "academic_frontier" => "学术前沿",
        "official" => "官方",
        "blogs" => "博客",
        "community" => "社区",
        "streaming" => "流媒体",
        "news" => "新闻",
        "personal_opinion" => "个人观点",
        "streaming_opinion" => "流媒体观点",
        "community_opinion" => "社区观点",
        "media_opinion" => "媒体观点",
        "lite_pool" => "轻量池",
        "physics" => "物理",
        "chemistry" => "化学",
        "biology" => "生物",
        _ => "未分类",
    }
}
