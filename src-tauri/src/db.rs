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
use tauri::AppHandle;

use crate::models::{
    all_disciplines, default_llm_protocol, default_llm_provider, AppView, ArticleRecord,
    ContentPoolStat, Discipline, FeedArticle, FitLevel, InterestMemoryRecord,
    MemoryReviewProposal, PendingArticleRecord, ReminderBatchSnapshot, ResourceType, RssSource,
    SettingsPayload, Snapshot, SourceCatalogSummary, SourceKind, UserDisciplinePreference,
    UserModulePreference,
};
use crate::policy;

const MAX_POOL_SIZE_PER_BUCKET: usize = 300;
#[allow(dead_code)]
const SNAPSHOT_HISTORY_LIMIT: usize = 200;
const PUSH_DB_FILE: &str = "briefy-pet-push.db";
const PUSH_BUCKET_MAX_SIZE: usize = 300;
const PUSH_SNOOZE_UNTIL_KEY: &str = "push_snooze_until";

pub fn db_path(app: &AppHandle) -> Result<PathBuf> {
    let app_dir = app
        .path_resolver()
        .app_data_dir()
        .context("failed to resolve app data dir")?;
    fs::create_dir_all(&app_dir)?;
    Ok(app_dir.join("briefy-pet.db"))
}

pub fn push_db_path(app: &AppHandle) -> Result<PathBuf> {
    let app_dir = app
        .path_resolver()
        .app_data_dir()
        .context("failed to resolve app data dir")?;
    fs::create_dir_all(&app_dir)?;
    Ok(app_dir.join(PUSH_DB_FILE))
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
          bucket TEXT NOT NULL DEFAULT 'general',
          source_group TEXT NOT NULL DEFAULT 'general',
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
        CREATE TABLE IF NOT EXISTS user_module_preferences (
          module TEXT PRIMARY KEY,
          enabled INTEGER NOT NULL DEFAULT 0,
          preference TEXT NOT NULL DEFAULT '',
          selected_buckets TEXT NOT NULL DEFAULT '[]'
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
        CREATE TABLE IF NOT EXISTS module_fetch_state (
          module TEXT PRIMARY KEY,
          last_module_run_at TEXT
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
                    note TEXT NOT NULL DEFAULT '',
                    score_status TEXT NOT NULL DEFAULT 'success',
                    score_error TEXT,
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
          module TEXT NOT NULL DEFAULT 'other',
          bucket TEXT NOT NULL DEFAULT 'general',
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
        CREATE TABLE IF NOT EXISTS memory_review_proposals (
          id TEXT PRIMARY KEY,
          week_key TEXT NOT NULL UNIQUE,
          base_summary TEXT NOT NULL DEFAULT '',
          proposed_summary TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL DEFAULT 'pending',
          user_response TEXT,
          created_at TEXT NOT NULL,
          decided_at TEXT
        );
                CREATE TABLE IF NOT EXISTS crawl_cycle_logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    started_at TEXT NOT NULL,
                    ended_at TEXT NOT NULL,
                    status TEXT NOT NULL,
                    due_sources INTEGER NOT NULL DEFAULT 0,
                    pending_articles INTEGER NOT NULL DEFAULT 0,
                    inserted_articles INTEGER NOT NULL DEFAULT 0,
                    failed_scoring INTEGER NOT NULL DEFAULT 0,
                    fetch_duration_ms INTEGER NOT NULL DEFAULT 0,
                    llm_duration_ms INTEGER NOT NULL DEFAULT 0,
                    total_duration_ms INTEGER NOT NULL DEFAULT 0,
                    warning_summary TEXT,
                    error_summary TEXT
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
        "ALTER TABLE articles ADD COLUMN note TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE articles ADD COLUMN score_status TEXT NOT NULL DEFAULT 'success'",
        [],
    );
    let _ = conn.execute("ALTER TABLE articles ADD COLUMN score_error TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE source_catalog ADD COLUMN module TEXT NOT NULL DEFAULT 'other'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE source_catalog ADD COLUMN bucket TEXT NOT NULL DEFAULT 'unspecified'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE source_catalog ADD COLUMN source_group TEXT NOT NULL DEFAULT 'general'",
        [],
    );
    let _ = conn.execute(
        "UPDATE articles SET fetched_at = published_at WHERE fetched_at IS NULL AND published_at IS NOT NULL",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE ranked_content_pool ADD COLUMN module TEXT NOT NULL DEFAULT 'other'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE ranked_content_pool ADD COLUMN bucket TEXT NOT NULL DEFAULT 'unspecified'",
        [],
    );
    let _ = conn.execute(
        "UPDATE source_catalog SET bucket = 'general' WHERE TRIM(bucket) = '' OR bucket = 'unspecified'",
        [],
    );
    let _ = conn.execute(
        "UPDATE ranked_content_pool SET bucket = 'general' WHERE TRIM(bucket) = '' OR bucket = 'unspecified'",
        [],
    );
    let _ = conn.execute(
        r#"
        UPDATE ranked_content_pool
        SET module = COALESCE((
            SELECT sc.module
            FROM articles a
            LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
            WHERE a.id = ranked_content_pool.article_id
            LIMIT 1
          ), 'other'),
            bucket = COALESCE((
            SELECT sc.bucket
            FROM articles a
            LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
            WHERE a.id = ranked_content_pool.article_id
            LIMIT 1
          ), 'unspecified')
        "#,
        [],
    );
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_articles_identity ON articles(source_id, article_key);
        CREATE INDEX IF NOT EXISTS idx_articles_normalized_link ON articles(normalized_link);
        CREATE INDEX IF NOT EXISTS idx_articles_score_status ON articles(score_status);
        CREATE INDEX IF NOT EXISTS idx_ranked_content_pool_bucket ON ranked_content_pool(module, bucket, fit_score DESC);
        "#,
    )?;
    conn.execute(
        r#"
        UPDATE articles
        SET guid = COALESCE(
          NULLIF(article_key, ''),
          NULLIF(source_id || '::' || normalized_link, source_id || '::'),
          source_id || '::legacy::' || id
        )
        WHERE TRIM(guid) = ''
        "#,
        [],
    )?;

    let catalog = load_catalog(app)?;
    seed_defaults(&conn, &catalog)?;
    ensure_push_db_initialized(app, &conn)?;
    Ok(conn)
}

pub fn push_connect(app: &AppHandle) -> Result<Connection> {
    let path = push_db_path(app)?;
    let conn = Connection::open(path)?;
    conn.execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                CREATE TABLE IF NOT EXISTS push_items (
                    article_id INTEGER PRIMARY KEY,
                    module TEXT NOT NULL DEFAULT 'other',
                    bucket TEXT NOT NULL DEFAULT 'general',
                    fit_score INTEGER NOT NULL DEFAULT 0,
                    push_status TEXT NOT NULL DEFAULT 'waiting',
                    queued_at TEXT NOT NULL,
                    status_updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS push_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_push_items_status_rank
                    ON push_items(push_status, fit_score DESC, queued_at DESC, article_id DESC);
                CREATE INDEX IF NOT EXISTS idx_push_items_bucket
                    ON push_items(module, bucket, push_status, fit_score DESC, queued_at DESC, article_id DESC);
                "#,
        )?;
    Ok(conn)
}

fn ensure_push_db_initialized(app: &AppHandle, conn: &Connection) -> Result<()> {
    let push_conn = push_connect(app)?;
    let migrated =
        read_setting(conn, "push_db_migrated_v1")?.unwrap_or_else(|| "false".to_string()) == "true";
    if !migrated {
        migrate_legacy_reminders_to_push_db(conn, &push_conn)?;
        write_setting(conn, "push_db_migrated_v1", "true")?;
    }
    Ok(())
}

fn seed_defaults(conn: &Connection, catalog: &[RssSource]) -> Result<()> {
    if read_setting(conn, "api_key")?.is_none() {
        write_setting(conn, "api_key", "")?;
        write_setting(conn, "llm_provider", &default_llm_provider())?;
        write_setting(conn, "llm_protocol", &default_llm_protocol())?;
        write_setting(conn, "llm_base_url", "")?;
        write_setting(conn, "llm_custom_provider_name", "")?;
        write_setting(conn, "llm_model_name", "")?;
        write_setting(conn, "llm_model", "")?;
        write_setting(conn, "provider_api_keys", "{}")?;
        write_setting(
            conn,
            "module_fetch_intervals",
            &serde_json::to_string(&policy::default_module_fetch_intervals())?,
        )?;
        write_setting(
            conn,
            "module_push_top_n",
            &serde_json::to_string(&policy::default_module_push_top_n_map())?,
        )?;
        write_setting(conn, "auto_start", "false")?;
        write_setting(conn, "active_view", "\"settings\"")?;
        write_setting(conn, "selected_article_id", "null")?;
        write_setting(conn, "api_key_valid", "false")?;
        write_setting(conn, "last_scan_at", "null")?;
        write_setting(conn, "memory_mode_enabled", "true")?;
        write_setting(conn, "pool_cleanup_v1_done", "false")?;
        write_setting(conn, "push_db_migrated_v1", "false")?;
        write_setting(conn, "onboarding_completed", "false")?;
    } else {
        ensure_setting(conn, "llm_provider", &default_llm_provider())?;
        ensure_setting(conn, "llm_protocol", &default_llm_protocol())?;
        ensure_setting(conn, "llm_base_url", "")?;
        ensure_setting(conn, "llm_custom_provider_name", "")?;
        ensure_setting(conn, "llm_model_name", "")?;
        ensure_setting(conn, "llm_model", "")?;
        ensure_setting(conn, "provider_api_keys", "{}")?;
        ensure_setting(
            conn,
            "module_fetch_intervals",
            &serde_json::to_string(&policy::default_module_fetch_intervals())?,
        )?;
        ensure_setting(
            conn,
            "module_push_top_n",
            &serde_json::to_string(&policy::default_module_push_top_n_map())?,
        )?;
        ensure_setting(conn, "auto_start", "false")?;
        ensure_setting(conn, "active_view", "\"settings\"")?;
        ensure_setting(conn, "selected_article_id", "null")?;
        ensure_setting(conn, "api_key_valid", "false")?;
        ensure_setting(conn, "last_scan_at", "null")?;
        ensure_setting(conn, "memory_mode_enabled", "true")?;
        ensure_setting(conn, "pool_cleanup_v1_done", "false")?;
        ensure_setting(conn, "push_db_migrated_v1", "false")?;
        ensure_setting(conn, "onboarding_completed", "false")?;
    }

    sync_source_catalog(conn, catalog)?;
    sync_discipline_preferences(conn)?;
    sync_module_preferences(conn, catalog)?;
    sync_user_source_pool(conn, catalog)?;
    sync_source_fetch_state(conn, catalog)?;
    sync_module_fetch_state(conn)?;

    let cleanup_done = read_setting(conn, "pool_cleanup_v1_done")?
        .unwrap_or_else(|| "false".to_string())
        == "true";
    if !cleanup_done {
        cleanup_pushed_articles_from_pool(conn)?;
        write_setting(conn, "pool_cleanup_v1_done", "true")?;
    }
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

    let existing_sources = {
        let mut stmt = conn.prepare("SELECT source_id, origin_files FROM source_catalog")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        items
    };
    for (source_id, origin_files_json) in existing_sources {
        if !known_ids.contains(&source_id) && !is_custom_source_origin(&origin_files_json) {
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
              source_id, name, rss_url, module, bucket, source_group, discipline, source_kind,
              resource_type, language, enabled_by_default, postponed, origin_files
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(source_id) DO UPDATE SET
              name = excluded.name,
              rss_url = excluded.rss_url,
              module = excluded.module,
              bucket = excluded.bucket,
              source_group = excluded.source_group,
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
                source.group.as_str(),
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

fn sync_module_preferences(conn: &Connection, catalog: &[RssSource]) -> Result<()> {
    let mut bucket_by_module = BTreeMap::<String, BTreeSet<String>>::new();
    for source in catalog {
        bucket_by_module
            .entry(policy::normalize_module(&source.module))
            .or_default()
            .insert(policy::normalize_bucket(&source.bucket));
    }

    for module in policy::all_modules() {
        let default_buckets = bucket_by_module
            .get(*module)
            .map(|items| items.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        conn.execute(
            r#"
            INSERT INTO user_module_preferences (module, enabled, preference, selected_buckets)
            VALUES (?1, 0, '', ?2)
            ON CONFLICT(module) DO NOTHING
            "#,
            params![module, serde_json::to_string(&default_buckets)?],
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
    let existing_sources = {
        let mut stmt = conn.prepare(
            r#"
            SELECT usp.source_id, COALESCE(sc.origin_files, '[]')
            FROM user_source_pool usp
            LEFT JOIN source_catalog sc ON sc.source_id = usp.source_id
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        items
    };
    for (source_id, origin_files_json) in existing_sources {
        if !known_ids.contains(&source_id) && !is_custom_source_origin(&origin_files_json) {
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

fn sync_module_fetch_state(conn: &Connection) -> Result<()> {
    for module in policy::all_modules() {
        conn.execute(
            r#"
            INSERT INTO module_fetch_state (module, last_module_run_at)
            VALUES (?1, NULL)
            ON CONFLICT(module) DO NOTHING
            "#,
            params![module],
        )?;
    }
    Ok(())
}

pub fn upsert_source(
    conn: &Connection,
    source: &RssSource,
    mark_custom_origin: bool,
) -> Result<()> {
    let module = policy::normalize_module(&source.module);
    let bucket = policy::normalize_bucket(&source.bucket);
    let source_group = policy::normalize_group(&source.group);
    let mut origins = source.origin_files.clone();
    if mark_custom_origin
        && !origins
            .iter()
            .any(|origin| origin.eq_ignore_ascii_case("user-custom"))
    {
        origins.push("user-custom".to_string());
    }

    conn.execute(
        r#"
        INSERT INTO source_catalog (
          source_id, name, rss_url, module, bucket, source_group, discipline, source_kind,
          resource_type, language, enabled_by_default, postponed, origin_files
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(source_id) DO UPDATE SET
          name = excluded.name,
          rss_url = excluded.rss_url,
          module = excluded.module,
          bucket = excluded.bucket,
          source_group = excluded.source_group,
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
            module,
            bucket,
            source_group,
            discipline_to_raw(&source.discipline),
            source_kind_to_raw(&source.source_kind),
            resource_type_to_raw(&source.resource_type),
            source.language,
            bool_to_int(source.enabled_by_default),
            bool_to_int(source.postponed),
            serde_json::to_string(&origins)?,
        ],
    )?;

    conn.execute(
        r#"
        INSERT INTO user_source_pool (source_id, enabled, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(source_id) DO UPDATE SET
          enabled = excluded.enabled,
          updated_at = excluded.updated_at
        "#,
        params![
            source.id,
            bool_to_int(source.enabled),
            Utc::now().to_rfc3339()
        ],
    )?;

    conn.execute(
        r#"
        INSERT INTO source_fetch_state (source_id, last_fetched_at, last_success_at, last_error)
        VALUES (?1, NULL, NULL, NULL)
        ON CONFLICT(source_id) DO NOTHING
        "#,
        params![source.id],
    )?;
    Ok(())
}

pub fn read_settings(conn: &Connection) -> Result<SettingsPayload> {
    let llm_provider = normalize_llm_provider(
        &read_setting(conn, "llm_provider")?.unwrap_or_else(default_llm_provider),
    );
    let llm_protocol = normalize_llm_protocol(
        &read_setting(conn, "llm_protocol")?.unwrap_or_else(default_llm_protocol),
    );
    let llm_base_url = read_setting(conn, "llm_base_url")?.unwrap_or_default();
    let llm_custom_provider_name =
        read_setting(conn, "llm_custom_provider_name")?.unwrap_or_default();
    let llm_model_name = read_setting(conn, "llm_model_name")?.unwrap_or_default();
    let llm_model = read_setting(conn, "llm_model")?.unwrap_or_default();
    let mut module_fetch_intervals = policy::default_module_fetch_intervals();
    if let Some(raw) = read_setting(conn, "module_fetch_intervals")? {
        if let Ok(overrides) = serde_json::from_str::<BTreeMap<String, i64>>(&raw) {
            for (module, hours) in overrides {
                let normalized = policy::normalize_module(&module);
                module_fetch_intervals.insert(normalized, hours.clamp(1, 168));
            }
        }
    }
    let mut module_push_top_n = policy::default_module_push_top_n_map();
    if let Some(raw) = read_setting(conn, "module_push_top_n")? {
        if let Ok(overrides) = serde_json::from_str::<BTreeMap<String, i64>>(&raw) {
            for (module, count) in overrides {
                let normalized = policy::normalize_module(&module);
                module_push_top_n.insert(normalized, count.clamp(1, 24));
            }
        }
    }
    let legacy_api_key = read_setting(conn, "api_key")?.unwrap_or_default();
    let mut provider_api_keys = read_setting(conn, "provider_api_keys")?
        .and_then(|raw| serde_json::from_str::<BTreeMap<String, String>>(&raw).ok())
        .unwrap_or_default();
    if !legacy_api_key.trim().is_empty() {
        let provider_api_key_key =
            active_provider_api_key_key(&llm_provider, &llm_custom_provider_name);
        let needs_backfill = provider_api_keys
            .get(&provider_api_key_key)
            .map(|value| value.trim().is_empty())
            .unwrap_or(true);
        if needs_backfill {
            provider_api_keys.insert(provider_api_key_key, legacy_api_key.clone());
        }
    }
    let provider_api_key_key =
        active_provider_api_key_key(&llm_provider, &llm_custom_provider_name);
    let api_key = provider_api_keys
        .get(&provider_api_key_key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or(legacy_api_key);

    Ok(SettingsPayload {
        api_key,
        llm_provider,
        llm_protocol,
        llm_base_url,
        llm_custom_provider_name,
        llm_model_name,
        llm_model,
        provider_api_keys,
        module_fetch_intervals,
        module_push_top_n,
        auto_start: read_setting(conn, "auto_start")?.unwrap_or_else(|| "false".into()) == "true",
        module_preferences: list_module_preferences(conn)?,
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
    let llm_provider = normalize_llm_provider(&settings.llm_provider);
    let llm_protocol = normalize_llm_protocol(&settings.llm_protocol);
    let llm_base_url = settings.llm_base_url.trim().to_string();
    let llm_custom_provider_name = settings.llm_custom_provider_name.trim().to_string();
    let llm_model_name = settings.llm_model_name.trim().to_string();
    let llm_model = settings.llm_model.trim().to_string();
    let mut module_fetch_intervals = policy::default_module_fetch_intervals();
    for (module, hours) in &settings.module_fetch_intervals {
        let normalized = policy::normalize_module(module);
        module_fetch_intervals.insert(normalized, (*hours).clamp(1, 168));
    }
    let mut module_push_top_n = policy::default_module_push_top_n_map();
    for (module, count) in &settings.module_push_top_n {
        let normalized = policy::normalize_module(module);
        module_push_top_n.insert(normalized, (*count).clamp(1, 24));
    }
    let mut provider_api_keys = settings.provider_api_keys.clone();
    for value in provider_api_keys.values_mut() {
        *value = value.trim().to_string();
    }
    let provider_api_key_key =
        active_provider_api_key_key(&llm_provider, &llm_custom_provider_name);
    if !settings.api_key.trim().is_empty() {
        provider_api_keys.insert(
            provider_api_key_key.clone(),
            settings.api_key.trim().to_string(),
        );
    }
    let active_api_key = provider_api_keys
        .get(&provider_api_key_key)
        .cloned()
        .unwrap_or_default();

    write_setting(conn, "api_key", active_api_key.trim())?;
    write_setting(conn, "llm_provider", &llm_provider)?;
    write_setting(conn, "llm_protocol", &llm_protocol)?;
    write_setting(conn, "llm_base_url", &llm_base_url)?;
    write_setting(conn, "llm_custom_provider_name", &llm_custom_provider_name)?;
    write_setting(conn, "llm_model_name", &llm_model_name)?;
    write_setting(conn, "llm_model", &llm_model)?;
    write_setting(
        conn,
        "module_fetch_intervals",
        &serde_json::to_string(&module_fetch_intervals)?,
    )?;
    write_setting(
        conn,
        "module_push_top_n",
        &serde_json::to_string(&module_push_top_n)?,
    )?;
    write_setting(
        conn,
        "provider_api_keys",
        &serde_json::to_string(&provider_api_keys)?,
    )?;
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

    let previous_modules = list_module_preferences(conn)?
        .into_iter()
        .map(|item| (item.module.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let previous_disciplines = list_discipline_preferences(conn)?
        .into_iter()
        .map(|item| (item.discipline, item.enabled))
        .collect::<BTreeMap<_, _>>();
    let previous_sources = {
        let mut stmt = conn.prepare("SELECT source_id, enabled FROM user_source_pool")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? == 1))
        })?;
        let mut map = BTreeMap::new();
        for row in rows {
            let (source_id, enabled) = row?;
            map.insert(source_id, enabled);
        }
        map
    };

    let known_disciplines = settings
        .disciplines
        .iter()
        .map(|item| (item.discipline.clone(), item.clone()))
        .collect::<BTreeMap<_, _>>();
    let available_buckets = module_bucket_index(&settings.rss_sources);
    let known_modules = settings
        .module_preferences
        .iter()
        .map(|item| {
            let module = policy::normalize_module(&item.module);
            let mut selected_buckets = if item.selected_buckets.is_empty() {
                available_buckets
                    .get(&module)
                    .map(|items| items.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default()
            } else {
                item.selected_buckets
                    .iter()
                    .map(|bucket| policy::normalize_bucket(bucket))
                    .collect::<Vec<_>>()
            };
            selected_buckets.sort();
            selected_buckets.dedup();
            (
                module.clone(),
                UserModulePreference {
                    module,
                    enabled: item.enabled,
                    preference: item.preference.trim().to_string(),
                    selected_buckets,
                },
            )
        })
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
    for module in policy::all_modules() {
        let item = known_modules.get(*module);
        let selected_buckets = item
            .map(|value| value.selected_buckets.clone())
            .unwrap_or_else(|| {
                available_buckets
                    .get(*module)
                    .map(|items| items.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default()
            });
        conn.execute(
            r#"
            INSERT INTO user_module_preferences (module, enabled, preference, selected_buckets)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(module) DO UPDATE SET
              enabled = excluded.enabled,
              preference = excluded.preference,
              selected_buckets = excluded.selected_buckets
            "#,
            params![
                module,
                bool_to_int(item.map(|value| value.enabled).unwrap_or(false)),
                item.map(|value| value.preference.as_str()).unwrap_or(""),
                serde_json::to_string(&selected_buckets)?,
            ],
        )?;
    }

    let mut incoming_ids = BTreeSet::new();
    for source in &settings.rss_sources {
        let mark_custom_origin = source
            .origin_files
            .iter()
            .any(|origin| origin.eq_ignore_ascii_case("user-custom"));
        upsert_source(conn, source, mark_custom_origin)?;

        let was_enabled = previous_sources.get(&source.id).copied().unwrap_or(false);
        if was_enabled && !source.enabled {
            purge_source_history(conn, &source.id)?;
        } else if !was_enabled && source.enabled {
            reset_source_fetch_state(conn, &source.id)?;
        }
        incoming_ids.insert(source.id.clone());
    }

    for (source_id, was_enabled) in previous_sources {
        if incoming_ids.contains(&source_id) {
            continue;
        }
        conn.execute(
            r#"
            INSERT INTO user_source_pool (source_id, enabled, updated_at)
            VALUES (?1, 0, ?2)
            ON CONFLICT(source_id) DO UPDATE SET
              enabled = excluded.enabled,
              updated_at = excluded.updated_at
            "#,
            params![source_id, Utc::now().to_rfc3339()],
        )?;
        if was_enabled {
            purge_source_history(conn, &source_id)?;
        }
    }

    for discipline in all_disciplines() {
        let was_enabled = previous_disciplines
            .get(&discipline)
            .copied()
            .unwrap_or(false);
        let is_enabled = known_disciplines
            .get(&discipline)
            .map(|value| value.enabled)
            .unwrap_or(false);
        if !was_enabled && is_enabled {
            for module in discipline_modules(&discipline) {
                reset_fetch_state_for_module(conn, module)?;
            }
        }
    }
    for module in policy::all_modules() {
        let previous = previous_modules.get(*module);
        let current = known_modules.get(*module);
        let was_enabled = previous.map(|value| value.enabled).unwrap_or(false);
        let is_enabled = current.map(|value| value.enabled).unwrap_or(false);
        let buckets_changed = previous
            .map(|value| value.selected_buckets.clone())
            .unwrap_or_default()
            != current
                .map(|value| value.selected_buckets.clone())
                .unwrap_or_default();
        if (!was_enabled && is_enabled) || (is_enabled && buckets_changed) {
            reset_fetch_state_for_module(conn, module)?;
        }
    }

    if !settings.memory_mode_enabled {
        write_memory_summary(conn, false, "")?;
    } else {
        conn.execute("UPDATE daily_interest_memory SET memory_enabled = 1", [])?;
    }

    write_memory_summary(
        conn,
        settings.memory_mode_enabled,
        if settings.memory_mode_enabled {
            settings.memory_summary.trim()
        } else {
            ""
        },
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

pub fn read_runtime_config_flags(conn: &Connection) -> Result<(bool, bool)> {
    let has_api_key = read_setting(conn, "api_key")?
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    let (enabled_count, enabled_with_pref_count) = conn.query_row(
        r#"
        SELECT
          COALESCE(SUM(CASE WHEN enabled = 1 THEN 1 ELSE 0 END), 0) AS enabled_count,
          COALESCE(SUM(CASE WHEN enabled = 1 AND TRIM(preference) != '' THEN 1 ELSE 0 END), 0) AS enabled_with_pref_count
        FROM user_interest_profile_v2
        "#,
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;

    let discipline_ready = enabled_count > 0 && enabled_count == enabled_with_pref_count;
    Ok((has_api_key, discipline_ready))
}

pub fn list_articles(conn: &Connection) -> Result<Vec<ArticleRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            a.id, a.source_id, a.title, a.link, a.source_name, a.discipline, a.source_kind, a.resource_type,
            a.published_at, a.fetched_at, a.summary, a.fit_level, a.fit_score, a.recommendation_reason,
            a.note, a.is_favorite, a.is_new
        FROM articles a
        WHERE a.score_status = 'success'
        ORDER BY COALESCE(a.published_at, a.fetched_at, '1970-01-01T00:00:00Z') DESC,
                 a.fit_score DESC,
                 a.id DESC
        LIMIT 500
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
            raw_content: String::new(),
            note: row.get(14)?,
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

pub fn fetch_article_raw_content(conn: &Connection, article_id: i64) -> Result<String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT raw_content FROM articles WHERE id = ?1",
            params![article_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(raw.unwrap_or_default())
}

pub fn fetch_article_record(conn: &Connection, article_id: i64) -> Result<Option<ArticleRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            a.id, a.source_id, a.title, a.link, a.source_name, a.discipline, a.source_kind, a.resource_type,
            a.published_at, a.fetched_at, a.summary, a.fit_level, a.fit_score, a.recommendation_reason,
            a.note, a.is_favorite, a.is_new
        FROM articles a
        WHERE a.id = ?1
          AND a.score_status = 'success'
        LIMIT 1
        "#,
    )?;

    stmt.query_row(params![article_id], |row| {
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
            raw_content: String::new(),
            note: row.get(14)?,
            is_favorite: row.get::<_, i64>(15)? == 1,
            is_new: row.get::<_, i64>(16)? == 1,
        })
    })
    .optional()
    .map_err(Into::into)
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
    guid: &str,
) -> Result<Option<i64>> {
    conn.query_row(
        r#"
        SELECT id
        FROM articles
        WHERE guid = ?4
           OR (source_id = ?1 AND article_key = ?2)
           OR normalized_link = ?3
        LIMIT 1
        "#,
        params![source_id, article_key, normalized_link, guid],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
pub fn insert_pending_article(
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
) -> Result<i64> {
    if let Some(article_id) =
        find_article_id_by_identity(conn, source_id, article_key, normalized_link, guid)?
    {
        return Ok(article_id);
    }

    conn.execute(
        r#"
        INSERT INTO articles (
          guid, article_key, normalized_link, source_id, title, link, source_name, discipline,
          source_kind, resource_type, published_at, fetched_at, raw_content, summary, fit_level,
          fit_score, recommendation_reason, note, score_status, score_error, is_favorite, is_new
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, '', 'low', 0, '', '', 'pending', NULL, 0, 0)
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
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn mark_article_scored_success(
    conn: &Connection,
    article_id: i64,
    summary: &str,
    fit_level: &FitLevel,
    fit_score: i64,
    recommendation_reason: &str,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE articles
        SET summary = ?2,
            fit_level = ?3,
            fit_score = ?4,
            recommendation_reason = ?5,
            score_status = 'success',
            score_error = NULL,
            is_new = 1
        WHERE id = ?1
        "#,
        params![
            article_id,
            summary,
            fit_level_to_raw(fit_level),
            fit_score,
            recommendation_reason,
        ],
    )?;
    Ok(())
}

pub fn mark_article_scored_failed(
    conn: &Connection,
    article_id: i64,
    score_error: &str,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE articles
        SET score_status = 'failed',
            score_error = ?2,
            is_new = 0
        WHERE id = ?1
        "#,
        params![article_id, score_error],
    )?;
    Ok(())
}

pub fn update_article_note(conn: &Connection, article_id: i64, note: &str) -> Result<()> {
    conn.execute(
        "UPDATE articles SET note = ?2 WHERE id = ?1",
        params![article_id, note.trim()],
    )?;
    Ok(())
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

/// Like `active_reminder_batch` but also includes `opened` batches so the
/// three-column reading view keeps showing curated articles after the user
/// clicks "view" from the bubble notification.  Does NOT update batch status.
#[allow(dead_code)]
pub fn display_reminder_batch(conn: &Connection) -> Result<Option<ReminderBatchSnapshot>> {
    let batch_id: Option<String> = conn
        .query_row(
            r#"
            SELECT id
            FROM reminder_batches
            WHERE status IN ('active', 'opened')
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    let Some(batch_id) = batch_id else {
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

/// 返回历史推送批次中的高分文章（排除当前正在展示的批次）。
#[allow(dead_code)]
pub fn list_history_articles(
    conn: &Connection,
    current_batch_id: Option<&str>,
) -> Result<Vec<crate::models::HistoryItem>> {
    list_history_articles_page(conn, current_batch_id, 0, SNAPSHOT_HISTORY_LIMIT)
}

#[allow(dead_code)]
pub fn list_history_articles_page(
    conn: &Connection,
    current_batch_id: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Vec<crate::models::HistoryItem>> {
    let safe_limit = limit.max(1).min(1000) as i64;
    let safe_offset = offset as i64;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            a.id,
            a.title,
            COALESCE(a.link, '') AS link,
            a.source_id,
            COALESCE(a.source_name, '') AS source_name,
            COALESCE(sc.module, 'other') AS module,
            COALESCE(sc.bucket, 'general') AS bucket,
            COALESCE(sc.source_group, 'general') AS source_group,
            a.published_at,
            COALESCE(a.summary, '') AS summary,
            a.fit_score,
            COALESCE(a.fit_level, 'low') AS fit_level,
            COALESCE(a.recommendation_reason, '') AS recommendation_reason,
            COALESCE(a.note, '') AS note,
            a.is_favorite,
            rb.id AS batch_id,
            rb.created_at AS batch_created_at
        FROM reminder_batch_articles rba
        JOIN articles a ON a.id = rba.article_id
        JOIN reminder_batches rb ON rb.id = rba.batch_id
        LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
        WHERE rb.status IN ('opened', 'ignored')
          AND (?1 IS NULL OR rb.id != ?1)
        ORDER BY rb.created_at DESC, a.fit_score DESC
        LIMIT ?2 OFFSET ?3
        "#,
    )?;
    let rows = stmt.query_map(params![current_batch_id, safe_limit, safe_offset], |row| {
        Ok(crate::models::HistoryItem {
            id: row.get::<_, i64>(0)?,
            title: row.get::<_, String>(1)?,
            link: row.get::<_, String>(2)?,
            source_id: row.get::<_, String>(3)?,
            source_name: row.get::<_, String>(4)?,
            module: row.get::<_, String>(5)?,
            bucket: row.get::<_, String>(6)?,
            group: row.get::<_, String>(7)?,
            published_at: row.get::<_, Option<String>>(8)?,
            summary: row.get::<_, String>(9)?,
            fit_score: row.get::<_, i64>(10)?,
            fit_level: row.get::<_, String>(11)?,
            recommendation_reason: row.get::<_, String>(12)?,
            note: row.get::<_, String>(13)?,
            is_favorite: row.get::<_, bool>(14)?,
            batch_id: row.get::<_, String>(15)?,
            batch_created_at: row.get::<_, String>(16)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn queue_push_articles(
    conn: &Connection,
    app: &AppHandle,
    article_ids: &[i64],
) -> Result<usize> {
    if article_ids.is_empty() {
        return Ok(0);
    }

    let push_conn = push_connect(app)?;
    let now = Utc::now().to_rfc3339();
    let mut inserted = 0usize;

    let mut lookup_stmt = conn.prepare(
        r#"
        SELECT
            a.id,
            COALESCE(sc.module, 'other') AS module,
            COALESCE(sc.bucket, 'general') AS bucket,
            a.fit_score
        FROM articles a
        LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
        WHERE a.id = ?1
        LIMIT 1
        "#,
    )?;

    let mut upsert_stmt = push_conn.prepare(
        r#"
        INSERT INTO push_items (
            article_id,
            module,
            bucket,
            fit_score,
            push_status,
            queued_at,
            status_updated_at
        )
        VALUES (?1, ?2, ?3, ?4, 'waiting', ?5, ?5)
        ON CONFLICT(article_id) DO UPDATE SET
            module = excluded.module,
            bucket = excluded.bucket,
            fit_score = MAX(push_items.fit_score, excluded.fit_score),
            push_status = push_items.push_status,
            queued_at = push_items.queued_at,
            status_updated_at = push_items.status_updated_at
        "#,
    )?;

    for article_id in article_ids {
        let Some((id, module, bucket, fit_score)) = lookup_stmt
            .query_row(params![article_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .optional()?
        else {
            continue;
        };

        let existing_status: Option<String> = push_conn
            .query_row(
                "SELECT push_status FROM push_items WHERE article_id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        upsert_stmt.execute(params![
            id,
            module.clone(),
            bucket.clone(),
            fit_score,
            now.clone()
        ])?;

        if existing_status.is_none() {
            inserted += 1;
        }

        trim_push_bucket(&push_conn, &module, &bucket)?;
    }

    if inserted > 0 {
        write_push_meta_value(&push_conn, PUSH_SNOOZE_UNTIL_KEY, None)?;
    }

    Ok(inserted)
}

fn push_reminder_snapshot(
    push_conn: &Connection,
    respect_snooze: bool,
) -> Result<Option<ReminderBatchSnapshot>> {
    if respect_snooze {
        if let Some(raw_until) = read_push_meta_value(push_conn, PUSH_SNOOZE_UNTIL_KEY)? {
            if let Some(until) = parse_datetime(&raw_until) {
                if until > Utc::now() {
                    return Ok(None);
                }
            }
            write_push_meta_value(push_conn, PUSH_SNOOZE_UNTIL_KEY, None)?;
        }
    }

    let mut stmt = push_conn.prepare(
        r#"
        SELECT article_id, module, bucket
        FROM push_items
        WHERE push_status = 'waiting'
        ORDER BY fit_score DESC, queued_at DESC, article_id DESC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut article_ids = Vec::new();
    let mut top_article_id = None;
    let mut partitions = BTreeSet::new();
    for (index, row) in rows.enumerate() {
        let (article_id, module, bucket) = row?;
        if index == 0 {
            top_article_id = Some(article_id);
        }
        article_ids.push(article_id);
        partitions.insert(format!("{module}/{bucket}"));
    }

    if article_ids.is_empty() {
        return Ok(None);
    }

    Ok(Some(ReminderBatchSnapshot {
        id: "push-waiting".to_string(),
        article_ids: article_ids.clone(),
        article_count: article_ids.len(),
        top_article_id,
        partition_count: partitions.len(),
    }))
}

pub fn read_onboarding_completed(conn: &Connection) -> Result<bool> {
    Ok(read_setting(conn, "onboarding_completed")?.unwrap_or_else(|| "false".into()) == "true")
}

pub fn write_onboarding_completed(conn: &Connection, value: bool) -> Result<()> {
    write_setting(
        conn,
        "onboarding_completed",
        if value { "true" } else { "false" },
    )
}

pub fn push_active_reminder(app: &AppHandle) -> Result<Option<ReminderBatchSnapshot>> {
    let push_conn = push_connect(app)?;
    push_reminder_snapshot(&push_conn, true)
}

pub fn push_waiting_reminder(app: &AppHandle) -> Result<Option<ReminderBatchSnapshot>> {
    let push_conn = push_connect(app)?;
    push_reminder_snapshot(&push_conn, false)
}

pub fn list_push_history_articles_page(
    app: &AppHandle,
    offset: usize,
    limit: usize,
) -> Result<Vec<crate::models::HistoryItem>> {
    let push_conn = push_connect(app)?;
    let main_conn = connect(app)?;
    let safe_limit = limit.max(1).min(1000) as i64;
    let safe_offset = offset as i64;

    let mut push_stmt = push_conn.prepare(
        r#"
        SELECT article_id, module, bucket, status_updated_at
        FROM push_items
        WHERE push_status = 'pushed'
        ORDER BY status_updated_at DESC, fit_score DESC, article_id DESC
        LIMIT ?1 OFFSET ?2
        "#,
    )?;
    let push_rows = push_stmt.query_map(params![safe_limit, safe_offset], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut article_stmt = main_conn.prepare(
        r#"
        SELECT
            id,
            title,
            COALESCE(link, '') AS link,
            source_id,
            COALESCE(source_name, '') AS source_name,
            published_at,
            COALESCE(summary, '') AS summary,
            fit_score,
            COALESCE(fit_level, 'low') AS fit_level,
            COALESCE(recommendation_reason, '') AS recommendation_reason,
            COALESCE(note, '') AS note,
            is_favorite
        FROM articles
        WHERE id = ?1
        LIMIT 1
        "#,
    )?;

    let mut output = Vec::new();
    for row in push_rows {
        let (article_id, module, bucket, batch_created_at) = row?;
        let Some(history_item) = article_stmt
            .query_row(params![article_id], |article_row| {
                Ok(crate::models::HistoryItem {
                    id: article_row.get::<_, i64>(0)?,
                    title: article_row.get::<_, String>(1)?,
                    link: article_row.get::<_, String>(2)?,
                    source_id: article_row.get::<_, String>(3)?,
                    source_name: article_row.get::<_, String>(4)?,
                    module: module.clone(),
                    bucket: bucket.clone(),
                    group: "general".to_string(),
                    published_at: article_row.get::<_, Option<String>>(5)?,
                    summary: article_row.get::<_, String>(6)?,
                    fit_score: article_row.get::<_, i64>(7)?,
                    fit_level: article_row.get::<_, String>(8)?,
                    recommendation_reason: article_row.get::<_, String>(9)?,
                    note: article_row.get::<_, String>(10)?,
                    is_favorite: article_row.get::<_, bool>(11)?,
                    batch_id: format!("push-{article_id}"),
                    batch_created_at: batch_created_at.clone(),
                })
            })
            .optional()?
        else {
            continue;
        };
        output.push(history_item);
    }

    Ok(output)
}

pub fn mark_push_article_pushed(app: &AppHandle, article_id: i64) -> Result<bool> {
    let push_conn = push_connect(app)?;
    let changed = push_conn.execute(
        r#"
        UPDATE push_items
        SET push_status = 'pushed',
            status_updated_at = ?2
        WHERE article_id = ?1
          AND push_status = 'waiting'
        "#,
        params![article_id, Utc::now().to_rfc3339()],
    )?;
    Ok(changed > 0)
}

pub fn mark_all_waiting_pushed(app: &AppHandle) -> Result<usize> {
    let push_conn = push_connect(app)?;
    let now = Utc::now().to_rfc3339();
    let changed = push_conn.execute(
        r#"
        UPDATE push_items
        SET push_status = 'pushed',
            status_updated_at = ?1
        WHERE push_status = 'waiting'
        "#,
        params![now],
    )?;
    write_push_meta_value(&push_conn, PUSH_SNOOZE_UNTIL_KEY, None)?;
    Ok(changed)
}

pub fn set_push_snooze_until(app: &AppHandle, until: Option<DateTime<Utc>>) -> Result<()> {
    let push_conn = push_connect(app)?;
    let value = until.map(|value| value.to_rfc3339());
    write_push_meta_value(&push_conn, PUSH_SNOOZE_UNTIL_KEY, value.as_deref())
}

#[allow(dead_code)]
pub fn push_db_stats(app: &AppHandle) -> Result<(i64, i64)> {
    let push_conn = push_connect(app)?;
    let waiting = push_conn.query_row(
        "SELECT COUNT(*) FROM push_items WHERE push_status = 'waiting'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let pushed = push_conn.query_row(
        "SELECT COUNT(*) FROM push_items WHERE push_status = 'pushed'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok((waiting, pushed))
}

#[allow(dead_code)]
pub fn reset_push_runtime_data(app: &AppHandle) -> Result<()> {
    let push_conn = push_connect(app)?;
    push_conn.execute("DELETE FROM push_items", [])?;
    push_conn.execute("DELETE FROM push_meta", [])?;
    Ok(())
}

fn trim_push_bucket(conn: &Connection, module: &str, bucket: &str) -> Result<()> {
    let total = conn.query_row(
        "SELECT COUNT(*) FROM push_items WHERE module = ?1 AND bucket = ?2",
        params![module, bucket],
        |row| row.get::<_, i64>(0),
    )?;
    let overflow = total.saturating_sub(PUSH_BUCKET_MAX_SIZE as i64);
    if overflow <= 0 {
        return Ok(());
    }

    conn.execute(
        r#"
        DELETE FROM push_items
        WHERE article_id IN (
            SELECT article_id
            FROM push_items
            WHERE module = ?1 AND bucket = ?2
            ORDER BY
                CASE WHEN push_status = 'waiting' THEN 1 ELSE 0 END ASC,
                fit_score ASC,
                queued_at ASC,
                article_id ASC
            LIMIT ?3
        )
        "#,
        params![module, bucket, overflow],
    )?;
    Ok(())
}

fn read_push_meta_value(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM push_meta WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(Into::into)
}

fn write_push_meta_value(conn: &Connection, key: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(value) => {
            conn.execute(
                "INSERT OR REPLACE INTO push_meta (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        None => {
            conn.execute("DELETE FROM push_meta WHERE key = ?1", params![key])?;
        }
    }
    Ok(())
}

fn migrate_legacy_reminders_to_push_db(
    main_conn: &Connection,
    push_conn: &Connection,
) -> Result<()> {
    let has_legacy_rows: i64 =
        main_conn.query_row("SELECT COUNT(*) FROM reminder_batch_articles", [], |row| {
            row.get::<_, i64>(0)
        })?;
    if has_legacy_rows == 0 {
        return Ok(());
    }

    let mut stmt = main_conn.prepare(
        r#"
        SELECT
            a.id,
            COALESCE(sc.module, 'other') AS module,
            COALESCE(sc.bucket, 'general') AS bucket,
            a.fit_score,
            CASE
                WHEN rb.status = 'active' THEN 'waiting'
                ELSE 'pushed'
            END AS push_status,
            COALESCE(rb.created_at, COALESCE(a.fetched_at, a.published_at, ?1)) AS migrated_at
        FROM reminder_batch_articles rba
        JOIN reminder_batches rb ON rb.id = rba.batch_id
        JOIN articles a ON a.id = rba.article_id
        LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
        ORDER BY migrated_at ASC, a.id ASC
        "#,
    )?;

    let now = Utc::now().to_rfc3339();
    let rows = stmt.query_map(params![now.clone()], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    for row in rows {
        let (article_id, module, bucket, fit_score, push_status, migrated_at) = row?;
        push_conn.execute(
            r#"
            INSERT INTO push_items (
                article_id,
                module,
                bucket,
                fit_score,
                push_status,
                queued_at,
                status_updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(article_id) DO UPDATE SET
                module = excluded.module,
                bucket = excluded.bucket,
                fit_score = excluded.fit_score,
                push_status = excluded.push_status,
                queued_at = excluded.queued_at,
                status_updated_at = excluded.status_updated_at
            "#,
            params![
                article_id,
                module.clone(),
                bucket.clone(),
                fit_score,
                push_status,
                migrated_at
            ],
        )?;
        trim_push_bucket(push_conn, &module, &bucket)?;
    }

    Ok(())
}

#[allow(dead_code)]
pub fn create_reminder_batch(conn: &Connection) -> Result<String> {
    let batch_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO reminder_batches (id, status, remind_at, created_at) VALUES (?1, 'active', NULL, ?2)",
        params![batch_id, Utc::now().to_rfc3339()],
    )?;
    Ok(batch_id)
}

#[allow(dead_code)]
pub fn attach_article_to_batch(conn: &Connection, batch_id: &str, article_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO reminder_batch_articles (batch_id, article_id) VALUES (?1, ?2)",
        params![batch_id, article_id],
    )?;
    Ok(())
}

#[allow(dead_code)]
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
                    last_success_at = COALESCE(excluded.last_success_at, source_fetch_state.last_success_at),
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

pub fn read_module_last_run_at(conn: &Connection, module: &str) -> Result<Option<DateTime<Utc>>> {
    let module = policy::normalize_module(module);
    let raw: Option<Option<String>> = conn
        .query_row(
            "SELECT last_module_run_at FROM module_fetch_state WHERE module = ?1 LIMIT 1",
            params![module],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(raw.flatten().as_deref().and_then(parse_datetime))
}

pub fn update_module_fetch_state(
    conn: &Connection,
    module: &str,
    last_module_run_at: DateTime<Utc>,
) -> Result<()> {
    let module = policy::normalize_module(module);
    conn.execute(
        r#"
        INSERT INTO module_fetch_state (module, last_module_run_at)
        VALUES (?1, ?2)
        ON CONFLICT(module) DO UPDATE SET
          last_module_run_at = excluded.last_module_run_at
        "#,
        params![module, last_module_run_at.to_rfc3339()],
    )?;
    Ok(())
}

pub fn update_module_fetch_states(
    conn: &Connection,
    modules: &BTreeSet<String>,
    last_module_run_at: DateTime<Utc>,
) -> Result<()> {
    for module in modules {
        update_module_fetch_state(conn, module, last_module_run_at)?;
    }
    Ok(())
}

pub fn reset_module_fetch_state(conn: &Connection, module: &str) -> Result<()> {
    let module = policy::normalize_module(module);
    conn.execute(
        r#"
        INSERT INTO module_fetch_state (module, last_module_run_at)
        VALUES (?1, NULL)
        ON CONFLICT(module) DO UPDATE SET
          last_module_run_at = NULL
        "#,
        params![module],
    )?;
    Ok(())
}

pub fn purge_source_history(conn: &Connection, source_id: &str) -> Result<()> {
    conn.execute(
        r#"
        DELETE FROM reminder_batch_articles
        WHERE article_id IN (SELECT id FROM articles WHERE source_id = ?1)
        "#,
        params![source_id],
    )?;
    conn.execute(
        r#"
        DELETE FROM ranked_content_pool
        WHERE article_id IN (SELECT id FROM articles WHERE source_id = ?1)
        "#,
        params![source_id],
    )?;
    conn.execute(
        r#"
        DELETE FROM user_behavior_events
        WHERE source_id = ?1
           OR article_id IN (SELECT id FROM articles WHERE source_id = ?1)
        "#,
        params![source_id],
    )?;
    conn.execute(
        "DELETE FROM articles WHERE source_id = ?1",
        params![source_id],
    )?;
    conn.execute(
        "DELETE FROM source_fetch_state WHERE source_id = ?1",
        params![source_id],
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn reset_runtime_data(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM reminder_batch_articles", [])?;
    conn.execute("DELETE FROM reminder_batches", [])?;
    conn.execute("DELETE FROM ranked_content_pool", [])?;
    conn.execute("DELETE FROM user_behavior_events", [])?;
    conn.execute("DELETE FROM daily_interest_memory", [])?;
    conn.execute("DELETE FROM articles", [])?;
    conn.execute("DELETE FROM source_fetch_state", [])?;
    conn.execute("DELETE FROM module_fetch_state", [])?;

    let mut stmt = conn.prepare("SELECT source_id FROM source_catalog")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        conn.execute(
            r#"
            INSERT INTO source_fetch_state (source_id, last_fetched_at, last_success_at, last_error)
            VALUES (?1, NULL, NULL, NULL)
            "#,
            params![row?],
        )?;
    }
    sync_module_fetch_state(conn)?;

    write_setting(conn, "selected_article_id", "null")?;
    write_setting(conn, "last_scan_at", "null")?;
    Ok(())
}

pub fn hard_reset_all(app: &AppHandle) -> Result<()> {
    let main_db = db_path(app)?;
    let push_db = push_db_path(app)?;

    remove_sqlite_database_files(&main_db)?;
    remove_sqlite_database_files(&push_db)?;

    Ok(())
}

fn remove_sqlite_database_files(path: &Path) -> Result<()> {
    remove_file_if_exists(path)?;
    let base = path.to_string_lossy().to_string();
    remove_file_if_exists(Path::new(&format!("{base}-wal")))?;
    remove_file_if_exists(Path::new(&format!("{base}-shm")))?;
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    if let Err(err) = fs::remove_file(path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            return Err(err.into());
        }
    }
    Ok(())
}

pub fn source_has_successful_fetch(conn: &Connection, source_id: &str) -> Result<bool> {
    let raw: Option<Option<String>> = conn
        .query_row(
            "SELECT last_success_at FROM source_fetch_state WHERE source_id = ?1 LIMIT 1",
            params![source_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(raw
        .flatten()
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false))
}

pub fn source_last_fetched_at(conn: &Connection, source_id: &str) -> Result<Option<DateTime<Utc>>> {
    let raw: Option<Option<String>> = conn
        .query_row(
            "SELECT last_fetched_at FROM source_fetch_state WHERE source_id = ?1 LIMIT 1",
            params![source_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(raw.flatten().as_deref().and_then(parse_datetime))
}

pub fn source_last_success_at(conn: &Connection, source_id: &str) -> Result<Option<DateTime<Utc>>> {
    let raw: Option<Option<String>> = conn
        .query_row(
            "SELECT last_success_at FROM source_fetch_state WHERE source_id = ?1 LIMIT 1",
            params![source_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(raw.flatten().as_deref().and_then(parse_datetime))
}

pub fn reset_source_fetch_state(conn: &Connection, source_id: &str) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO source_fetch_state (source_id, last_fetched_at, last_success_at, last_error)
        VALUES (?1, NULL, NULL, NULL)
        ON CONFLICT(source_id) DO UPDATE SET
          last_fetched_at = NULL,
          last_success_at = NULL,
          last_error = NULL
        "#,
        params![source_id],
    )?;
    Ok(())
}

pub fn reset_fetch_state_for_module(conn: &Connection, module: &str) -> Result<()> {
    let module = policy::normalize_module(module);
    let mut stmt = conn.prepare(
        r#"
        SELECT sc.source_id
        FROM source_catalog sc
        JOIN user_source_pool usp ON usp.source_id = sc.source_id
        WHERE usp.enabled = 1
          AND sc.module = ?1
        "#,
    )?;
    let rows = stmt.query_map(params![module], |row| row.get::<_, String>(0))?;
    for row in rows {
        reset_source_fetch_state(conn, &row?)?;
    }
    reset_module_fetch_state(conn, &module)?;
    Ok(())
}

pub fn list_pending_article_backlog(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<PendingArticleRecord>> {
    let safe_limit = limit.max(1).min(1000) as i64;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            a.id,
            a.article_key,
            a.fetched_at,
            a.source_id,
            a.title,
            a.link,
            a.source_name,
            a.discipline,
            a.source_kind,
            a.resource_type,
            a.published_at,
            a.raw_content,
            COALESCE(sc.module, 'other') AS module,
            COALESCE(sc.bucket, 'general') AS bucket,
            COALESCE(sc.source_group, 'general') AS source_group
        FROM articles a
        LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
        WHERE a.score_status = 'pending'
        ORDER BY COALESCE(a.fetched_at, a.published_at, '1970-01-01T00:00:00Z') ASC, a.id ASC
        LIMIT ?1
        "#,
    )?;
    let rows = stmt.query_map(params![safe_limit], |row| {
        let fetched_at_raw: Option<String> = row.get(2)?;
        let published_at_raw: Option<String> = row.get(10)?;
        let link: String = row.get(5)?;
        Ok(PendingArticleRecord {
            id: row.get(0)?,
            article_key: row.get(1)?,
            fetched_at: parse_optional_datetime(fetched_at_raw)
                .or_else(|| parse_optional_datetime(published_at_raw.clone()))
                .unwrap_or_else(Utc::now),
            article: FeedArticle {
                source_id: row.get(3)?,
                title: row.get(4)?,
                link: link.clone(),
                source_name: row.get(6)?,
                discipline: parse_discipline(&row.get::<_, String>(7)?),
                source_kind: parse_source_kind(&row.get::<_, String>(8)?),
                resource_type: parse_resource_type(&row.get::<_, String>(9)?),
                published_at: parse_optional_datetime(published_at_raw),
                content: row.get(11)?,
                module: row.get(12)?,
                bucket: row.get(13)?,
                group: row.get(14)?,
                normalized_link: canonicalize_source_url(&link),
                guid: row.get::<_, String>(1)?,
            },
        })
    })?;

    let mut output = Vec::new();
    for row in rows {
        output.push(row?);
    }
    Ok(output)
}

#[derive(Clone)]
struct SourceFetchStateRow {
    source: RssSource,
    last_fetched_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    has_error: bool,
}

fn list_selected_source_fetch_rows(conn: &Connection) -> Result<Vec<SourceFetchStateRow>> {
    let module_preferences = list_module_preferences(conn)?
        .into_iter()
        .filter(|item| item.enabled)
        .map(|item| {
            (
                policy::normalize_module(&item.module),
                item.selected_buckets
                    .into_iter()
                    .map(|bucket| policy::normalize_bucket(&bucket))
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut stmt = conn.prepare_cached(
        r#"
        SELECT
          sc.source_id, sc.name, sc.rss_url, sc.module, sc.bucket, sc.source_group, sc.discipline,
          sc.source_kind, sc.resource_type, sc.language, sc.enabled_by_default,
          sc.postponed, sc.origin_files, usp.enabled,
          sfs.last_fetched_at, sfs.last_success_at, sfs.last_error
        FROM source_catalog sc
        JOIN user_source_pool usp ON usp.source_id = sc.source_id
        LEFT JOIN source_fetch_state sfs ON sfs.source_id = sc.source_id
        ORDER BY sc.name ASC
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        let module: String = row.get(3)?;
        let bucket: String = row.get(4)?;
        let source_group: String = row.get(5)?;
        let discipline = parse_discipline(&row.get::<_, String>(6)?);
        let source_kind = parse_source_kind(&row.get::<_, String>(7)?);
        let resource_type = parse_resource_type(&row.get::<_, String>(8)?);
        let origin_files_json: String = row.get(12)?;
        let origin_files =
            serde_json::from_str::<Vec<String>>(&origin_files_json).unwrap_or_default();
        let last_fetched_raw: Option<String> = row.get(14)?;
        let last_success_raw: Option<String> = row.get(15)?;
        let last_error_raw: Option<String> = row.get(16)?;
        Ok((
            RssSource {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                module,
                bucket,
                group: source_group,
                discipline: discipline.clone(),
                source_kind: source_kind.clone(),
                resource_type,
                language: row.get(9)?,
                enabled: row.get::<_, i64>(13)? == 1,
                enabled_by_default: row.get::<_, i64>(10)? == 1,
                postponed: row.get::<_, i64>(11)? == 1,
                origin_files,
            },
            last_fetched_raw,
            last_success_raw,
            last_error_raw,
        ))
    })?;

    let mut sources = Vec::new();
    for row in rows {
        let (source, last_fetched_raw, last_success_raw, last_error_raw) = row?;
        if !source.enabled {
            continue;
        }
        let module = policy::normalize_module(&source.module);
        let bucket = policy::normalize_bucket(&source.bucket);
        let Some(selected_buckets) = module_preferences.get(&module) else {
            continue;
        };
        if !selected_buckets.is_empty() && !selected_buckets.contains(&bucket) {
            continue;
        }
        sources.push(SourceFetchStateRow {
            source,
            last_fetched_at: last_fetched_raw.as_deref().and_then(parse_datetime),
            last_success_at: last_success_raw.as_deref().and_then(parse_datetime),
            has_error: last_error_raw
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false),
        });
    }
    Ok(sources)
}

fn module_fetch_interval(settings: &SettingsPayload, module: &str) -> chrono::Duration {
    let module = policy::normalize_module(module);
    chrono::Duration::hours(
        settings
            .module_fetch_intervals
            .get(&module)
            .copied()
            .unwrap_or_else(|| policy::default_module_fetch_interval_hours(&module))
            .clamp(1, 168),
    )
}

fn module_refresh_due(
    last_module_run_at: Option<DateTime<Utc>>,
    interval: chrono::Duration,
    now: DateTime<Utc>,
) -> bool {
    match last_module_run_at {
        None => true,
        Some(last_run_at) => now - last_run_at >= interval,
    }
}

fn source_due_for_module_refresh(
    last_success_at: Option<DateTime<Utc>>,
    interval: chrono::Duration,
    now: DateTime<Utc>,
) -> bool {
    match last_success_at {
        None => true,
        Some(last_success_at) => now - last_success_at >= interval,
    }
}

fn source_due_for_retry(
    last_fetched_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    has_error: bool,
    retry_interval: chrono::Duration,
    now: DateTime<Utc>,
) -> bool {
    if !has_error {
        return false;
    }
    let Some(last_fetched_at) = last_fetched_at else {
        return false;
    };
    let last_cycle_failed = last_success_at
        .map(|last_success_at| last_success_at < last_fetched_at)
        .unwrap_or(true);
    last_cycle_failed && now - last_fetched_at >= retry_interval
}

pub fn list_enabled_modules(conn: &Connection) -> Result<BTreeSet<String>> {
    let rows = list_selected_source_fetch_rows(conn)?;
    Ok(rows
        .into_iter()
        .map(|row| policy::normalize_module(&row.source.module))
        .collect())
}

pub fn list_due_modules(conn: &Connection, now: DateTime<Utc>) -> Result<BTreeSet<String>> {
    let settings = read_settings(conn)?;
    let enabled_modules = list_enabled_modules(conn)?;
    let mut due_modules = BTreeSet::new();
    for module in enabled_modules {
        if module_refresh_due(
            read_module_last_run_at(conn, &module)?,
            module_fetch_interval(&settings, &module),
            now,
        ) {
            due_modules.insert(module);
        }
    }
    Ok(due_modules)
}

pub fn list_module_refresh_sources(
    conn: &Connection,
    modules: &BTreeSet<String>,
    now: DateTime<Utc>,
    force_all_sources: bool,
) -> Result<Vec<RssSource>> {
    if modules.is_empty() {
        return Ok(Vec::new());
    }

    let settings = read_settings(conn)?;
    let rows = list_selected_source_fetch_rows(conn)?;
    let mut sources = Vec::new();
    for row in rows {
        let module = policy::normalize_module(&row.source.module);
        if !modules.contains(&module) {
            continue;
        }
        if force_all_sources
            || source_due_for_module_refresh(
                row.last_success_at,
                module_fetch_interval(&settings, &module),
                now,
            )
        {
            sources.push(row.source);
        }
    }
    Ok(sources)
}

pub fn list_retry_due_sources(
    conn: &Connection,
    now: DateTime<Utc>,
    excluded_modules: &BTreeSet<String>,
) -> Result<Vec<RssSource>> {
    let rows = list_selected_source_fetch_rows(conn)?;
    let mut sources = Vec::new();
    for row in rows {
        let module = policy::normalize_module(&row.source.module);
        if excluded_modules.contains(&module) {
            continue;
        }
        if source_due_for_retry(
            row.last_fetched_at,
            row.last_success_at,
            row.has_error,
            policy::fetch_retry_interval_for_failed_source(
                &row.source.module,
                &row.source.group,
                &row.source.source_kind,
            ),
            now,
        ) {
            sources.push(row.source);
        }
    }
    Ok(sources)
}

pub fn count_due_sources(conn: &Connection, now: DateTime<Utc>) -> Result<usize> {
    let due_modules = list_due_modules(conn, now)?;
    Ok(list_module_refresh_sources(conn, &due_modules, now, false)?.len()
        + list_retry_due_sources(conn, now, &due_modules)?.len())
}

pub fn upsert_content_pool_entry(
    conn: &Connection,
    article_id: i64,
    module: &str,
    bucket: &str,
    source_kind: &SourceKind,
    fit_score: i64,
    published_at: Option<DateTime<Utc>>,
) -> Result<()> {
    let module = policy::normalize_module(module);
    let bucket = policy::normalize_bucket(bucket);
    conn.execute(
        r#"
        INSERT INTO ranked_content_pool (article_id, module, bucket, source_kind, fit_score, published_at, inserted_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(article_id) DO UPDATE SET
          module = excluded.module,
          bucket = excluded.bucket,
          source_kind = excluded.source_kind,
          fit_score = excluded.fit_score,
          published_at = excluded.published_at,
          inserted_at = excluded.inserted_at
        "#,
        params![
            article_id,
            module,
            bucket,
            source_kind_to_raw(source_kind),
            fit_score,
            published_at.map(|value| value.to_rfc3339()),
            Utc::now().to_rfc3339(),
        ],
    )?;
    trim_content_pool(conn, &module, &bucket)?;
    Ok(())
}

fn trim_content_pool(conn: &Connection, module: &str, bucket: &str) -> Result<()> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ranked_content_pool WHERE module = ?1 AND bucket = ?2",
        params![module, bucket],
        |row| row.get(0),
    )?;
    if total <= MAX_POOL_SIZE_PER_BUCKET as i64 {
        return Ok(());
    }

    conn.execute(
        r#"
        DELETE FROM ranked_content_pool
        WHERE article_id IN (
          SELECT article_id
          FROM ranked_content_pool
          WHERE module = ?1 AND bucket = ?2
          ORDER BY fit_score ASC, COALESCE(published_at, inserted_at) ASC, article_id ASC
          LIMIT 100
        )
        "#,
        params![module, bucket],
    )?;
    Ok(())
}

pub fn remove_content_pool_entries(conn: &Connection, article_ids: &[i64]) -> Result<()> {
    if article_ids.is_empty() {
        return Ok(());
    }

    let mut stmt = conn.prepare_cached("DELETE FROM ranked_content_pool WHERE article_id = ?1")?;
    for article_id in article_ids {
        stmt.execute(params![article_id])?;
    }
    Ok(())
}

fn cleanup_pushed_articles_from_pool(conn: &Connection) -> Result<()> {
    conn.execute(
        r#"
        DELETE FROM ranked_content_pool
        WHERE article_id IN (SELECT DISTINCT article_id FROM reminder_batch_articles)
        "#,
        [],
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

pub fn list_top_module_candidates(
    conn: &Connection,
    modules: &BTreeSet<String>,
    module_push_top_n: &BTreeMap<String, i64>,
    max_age_days: i64,
    min_fit_score: i64,
) -> Result<Vec<i64>> {
    if modules.is_empty() {
        return Ok(Vec::new());
    }

    let cutoff = (Utc::now() - chrono::Duration::days(max_age_days)).to_rfc3339();
    let mut selected = Vec::<(i64, i64, Option<DateTime<Utc>>)>::new();

    for module in modules {
        let max_per_module = module_push_top_n
            .get(module)
            .copied()
            .unwrap_or_else(|| policy::default_module_push_top_n(module))
            .clamp(1, 24) as usize;
        if max_per_module == 0 {
            continue;
        }

        let mut stmt = conn.prepare(
            r#"
            SELECT rcp.article_id, rcp.fit_score, a.published_at, a.fetched_at
            FROM ranked_content_pool rcp
            JOIN articles a ON a.id = rcp.article_id
            WHERE rcp.module = ?1
              AND a.score_status = 'success'
              AND COALESCE(a.published_at, a.fetched_at, '1970-01-01T00:00:00Z') >= ?3
              AND rcp.fit_score >= ?2
            ORDER BY rcp.fit_score DESC, COALESCE(a.published_at, a.fetched_at) DESC, rcp.article_id DESC
            LIMIT ?4
            "#,
        )?;

        let rows = stmt.query_map(
            params![module, min_fit_score, cutoff, max_per_module as i64],
            |row| {
                let published_at_raw: Option<String> = row.get(2)?;
                let fetched_at_raw: Option<String> = row.get(3)?;
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    parse_optional_datetime(published_at_raw)
                        .or_else(|| parse_optional_datetime(fetched_at_raw)),
                ))
            },
        )?;

        for row in rows {
            selected.push(row?);
        }
    }

    selected.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| right.0.cmp(&left.0))
    });

    let mut dedup = BTreeSet::new();
    Ok(selected
        .into_iter()
        .map(|item| item.0)
        .filter(|article_id| dedup.insert(*article_id))
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub fn log_crawl_cycle(
    conn: &Connection,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    status: &str,
    due_sources: usize,
    pending_articles: usize,
    inserted_articles: usize,
    failed_scoring: usize,
    fetch_duration_ms: u128,
    llm_duration_ms: u128,
    total_duration_ms: u128,
    warning_summary: Option<&str>,
    error_summary: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO crawl_cycle_logs (
          started_at, ended_at, status, due_sources, pending_articles, inserted_articles,
          failed_scoring, fetch_duration_ms, llm_duration_ms, total_duration_ms,
          warning_summary, error_summary
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
        params![
            started_at.to_rfc3339(),
            ended_at.to_rfc3339(),
            status,
            due_sources as i64,
            pending_articles as i64,
            inserted_articles as i64,
            failed_scoring as i64,
            fetch_duration_ms as i64,
            llm_duration_ms as i64,
            total_duration_ms as i64,
            warning_summary,
            error_summary,
        ],
    )?;
    Ok(())
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
    _memory_enabled: bool,
) -> Result<Option<InterestMemoryRecord>> {
    read_latest_memory(conn)
}

pub fn list_weekly_memory_signals(
    conn: &Connection,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<Vec<(String, String, String, String)>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT DISTINCT
          a.title,
          COALESCE(a.summary, ''),
          COALESCE(a.note, ''),
          COALESCE(sc.module, 'other') || '/' || COALESCE(sc.bucket, 'general') || '/' || COALESCE(sc.source_group, 'general') AS source_path
        FROM user_behavior_events ube
        JOIN articles a ON a.id = ube.article_id
        LEFT JOIN source_catalog sc ON sc.source_id = a.source_id
        WHERE ube.created_at >= ?1
          AND ube.created_at < ?2
          AND (
            ube.event_type = 'favorite-added'
            OR ube.event_type = 'note-updated'
          )
        ORDER BY ube.created_at DESC, a.id DESC
        LIMIT 24
        "#,
    )?;
    let rows = stmt.query_map(
        params![start_at.to_rfc3339(), end_at.to_rfc3339()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;

    let mut output = Vec::new();
    for row in rows {
        output.push(row?);
    }
    Ok(output)
}

pub fn has_memory_review_for_week(conn: &Connection, week_key: &str) -> Result<bool> {
    conn.query_row(
        "SELECT 1 FROM memory_review_proposals WHERE week_key = ?1 LIMIT 1",
        params![week_key],
        |_| Ok(()),
    )
    .optional()
    .map(|value| value.is_some())
    .map_err(Into::into)
}

pub fn create_memory_review_proposal(
    conn: &Connection,
    week_key: &str,
    base_summary: &str,
    proposed_summary: &str,
) -> Result<MemoryReviewProposal> {
    let id = format!("memory-review-{week_key}");
    let created_at = Utc::now();
    conn.execute(
        r#"
        INSERT OR REPLACE INTO memory_review_proposals (
          id, week_key, base_summary, proposed_summary, status, user_response, created_at, decided_at
        ) VALUES (?1, ?2, ?3, ?4, 'pending', NULL, ?5, NULL)
        "#,
        params![
            id,
            week_key,
            base_summary.trim(),
            proposed_summary.trim(),
            created_at.to_rfc3339(),
        ],
    )?;

    Ok(MemoryReviewProposal {
        id: format!("memory-review-{week_key}"),
        week_key: week_key.to_string(),
        base_summary: base_summary.trim().to_string(),
        proposed_summary: proposed_summary.trim().to_string(),
        status: "pending".to_string(),
        created_at,
    })
}

pub fn read_pending_memory_review(conn: &Connection) -> Result<Option<MemoryReviewProposal>> {
    conn.query_row(
        r#"
        SELECT id, week_key, base_summary, proposed_summary, status, created_at
        FROM memory_review_proposals
        WHERE status = 'pending'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        [],
        |row| {
            let created_at_raw: String = row.get(5)?;
            Ok(MemoryReviewProposal {
                id: row.get(0)?,
                week_key: row.get(1)?,
                base_summary: row.get(2)?,
                proposed_summary: row.get(3)?,
                status: row.get(4)?,
                created_at: parse_datetime(&created_at_raw).unwrap_or_else(Utc::now),
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn resolve_memory_review_proposal(
    conn: &Connection,
    proposal_id: &str,
    status: &str,
    user_response: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE memory_review_proposals
        SET status = ?2,
            user_response = ?3,
            decided_at = ?4
        WHERE id = ?1
        "#,
        params![proposal_id, status, user_response, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub fn reject_pending_memory_reviews(conn: &Connection) -> Result<usize> {
    let changed = conn.execute(
        r#"
        UPDATE memory_review_proposals
        SET status = 'rejected',
            user_response = COALESCE(user_response, 'auto-rejected on app restart'),
            decided_at = ?1
        WHERE status = 'pending'
        "#,
        params![Utc::now().to_rfc3339()],
    )?;
    Ok(changed)
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
    let due_sources = count_due_sources(conn, Utc::now())?;
    let selected_disciplines = settings
        .module_preferences
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

    let selected_article_id = read_selected_article_id(conn)?;
    let articles = list_articles(conn)?;

    Ok(Snapshot {
        settings,
        pet_status,
        articles,
        active_reminder: None,
        history_articles: Vec::new(),
        selected_article_id,
        active_view: read_active_view(conn)?,
        last_error,
        api_key_valid,
        last_scan_at,
        content_pool_stats: content_pool_stats(conn)?,
        memory: read_latest_memory(conn)?,
        memory_review: read_pending_memory_review(conn)?,
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
          sc.source_id, sc.name, sc.rss_url, sc.module, sc.bucket, sc.source_group, sc.discipline,
          sc.source_kind, sc.resource_type, sc.language, usp.enabled,
          sc.enabled_by_default, sc.postponed, sc.origin_files
        FROM source_catalog sc
        JOIN user_source_pool usp ON usp.source_id = sc.source_id
        ORDER BY sc.postponed ASC, sc.module ASC, sc.bucket ASC, sc.name ASC
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        let origin_files_json: String = row.get(13)?;
        let origin_files =
            serde_json::from_str::<Vec<String>>(&origin_files_json).unwrap_or_default();
        Ok(RssSource {
            id: row.get(0)?,
            name: row.get(1)?,
            url: row.get(2)?,
            module: row.get(3)?,
            bucket: row.get(4)?,
            group: row.get(5)?,
            discipline: parse_discipline(&row.get::<_, String>(6)?),
            source_kind: parse_source_kind(&row.get::<_, String>(7)?),
            resource_type: parse_resource_type(&row.get::<_, String>(8)?),
            language: row.get(9)?,
            enabled: row.get::<_, i64>(10)? == 1,
            enabled_by_default: row.get::<_, i64>(11)? == 1,
            postponed: row.get::<_, i64>(12)? == 1,
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

fn list_module_preferences(conn: &Connection) -> Result<Vec<UserModulePreference>> {
    let mut stmt = conn.prepare(
        "SELECT module, enabled, preference, selected_buckets FROM user_module_preferences ORDER BY module ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let selected_buckets_raw: String = row.get(3)?;
        Ok(UserModulePreference {
            module: row.get(0)?,
            enabled: row.get::<_, i64>(1)? == 1,
            preference: row.get(2)?,
            selected_buckets: serde_json::from_str::<Vec<String>>(&selected_buckets_raw)
                .unwrap_or_default()
                .into_iter()
                .map(|bucket| policy::normalize_bucket(&bucket))
                .collect(),
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

fn module_bucket_index(sources: &[RssSource]) -> BTreeMap<String, BTreeSet<String>> {
    let mut output = BTreeMap::<String, BTreeSet<String>>::new();
    for source in sources {
        output
            .entry(policy::normalize_module(&source.module))
            .or_default()
            .insert(policy::normalize_bucket(&source.bucket));
    }
    output
}

fn list_dueable_enabled_sources_count(conn: &Connection) -> Result<i64> {
    Ok(list_selected_source_fetch_rows(conn)?.len() as i64)
}

fn load_catalog(app: &AppHandle) -> Result<Vec<RssSource>> {
    let mut load_errors = Vec::new();
    for resource_name in ["rss_catalog_0425.opml", "rss-catalog.opml"] {
        for candidate in build_resource_candidates(app, resource_name) {
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
    }

    if load_errors.is_empty() {
        anyhow::bail!("rss v3 catalog file not found")
    } else {
        anyhow::bail!("failed to load v3 catalog: {}", load_errors.join(" ; "))
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
                let canonical_url = canonicalize_feed_url(url);
                if should_block_source_url(&canonical_url) {
                    buf.clear();
                    continue;
                }

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
                let group = attrs.get("group").map(String::as_str);
                let module_code = normalize_v3_module(module, category);
                let bucket_code = normalize_v3_bucket(bucket, category);
                let group_code = normalize_v3_group(group, category);
                let language = attrs
                    .get("language")
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let resource_type = map_v3_resource_type(
                    attrs.get("resourceType").map(String::as_str),
                    &canonical_url,
                );
                let origin_files = attrs
                    .get("origin")
                    .map(|value| split_origin_files(value))
                    .unwrap_or_default();

                let normalized_url = normalize_url(&canonical_url);
                let source = grouped
                    .entry(normalized_url.clone())
                    .or_insert_with(|| RssSource {
                        id: build_source_id(&name, &normalized_url),
                        name: name.clone(),
                        url: canonical_url.clone(),
                        module: module_code.clone(),
                        bucket: bucket_code.clone(),
                        group: group_code.clone(),
                        discipline: map_v3_module_to_discipline(&module_code),
                        source_kind: map_v3_group_to_source_kind(&group_code),
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
                source.group = group_code.clone();
                source.discipline = map_v3_module_to_discipline(&module_code);
                source.source_kind = map_v3_group_to_source_kind(&group_code);
                source.resource_type = map_v3_resource_type(
                    attrs.get("resourceType").map(String::as_str),
                    &canonical_url,
                );
                source.url = canonical_url.clone();

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
            .then_with(|| left.group.cmp(&right.group))
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

fn normalize_v3_bucket(bucket: Option<&str>, category: Option<&str>) -> String {
    bucket
        .map(policy::normalize_bucket)
        .or_else(|| {
            category.and_then(|value| {
                value
                    .split(',')
                    .nth(1)
                    .map(|part| policy::normalize_bucket(part.trim()))
            })
        })
        .unwrap_or_else(|| "general".to_string())
}

fn normalize_v3_group(group: Option<&str>, category: Option<&str>) -> String {
    group
        .map(policy::normalize_group)
        .or_else(|| {
            category.and_then(|value| {
                value
                    .split(',')
                    .nth(2)
                    .map(|part| policy::normalize_group(part.trim()))
            })
        })
        .unwrap_or_else(|| "general".to_string())
}

fn map_v3_module_to_discipline(module: &str) -> Discipline {
    match module {
        "technology" => Discipline::Technology,
        "social_science" => Discipline::SocialScience,
        "business" => Discipline::Other,
        "design" => Discipline::Humanities,
        "science" => Discipline::Science,
        "medicine" => Discipline::Medicine,
        _ => Discipline::Other,
    }
}

fn map_v3_group_to_source_kind(source_group: &str) -> SourceKind {
    match source_group {
        "frontier" | "research" | "academic" | "clinical_trials" | "genomics"
        | "biostatistics" | "biomaterials" | "biomechanics" | "computational_biology"
        | "bioinformatics" | "systems_biology" | "pharmacogenomics" | "drug_discovery"
        | "pharmacology" | "toxicology" => SourceKind::AcademicJournal,
        "official" | "regulatory_science" | "clinical_safety" => SourceKind::OfficialAnnouncement,
        "blogs" | "product_engineering" | "cad_and_cae" | "systems_engineering"
        | "medical_devices" | "medical_imaging" => SourceKind::TechnicalBlog,
        _ => SourceKind::CommunityHotspot,
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

fn is_custom_source_origin(origin_files_json: &str) -> bool {
    let origins = serde_json::from_str::<Vec<String>>(origin_files_json).unwrap_or_default();
    origins
        .iter()
        .any(|origin| origin.eq_ignore_ascii_case("user-custom"))
}

fn normalize_llm_provider(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "deepseek" => "deepseek".to_string(),
        "qwen" | "dashscope" | "alibaba" => "qwen".to_string(),
        "minimax" => "minimax".to_string(),
        "glm" | "zhipu" | "zhipuai" => "glm".to_string(),
        "kimi" | "moonshot" => "kimi".to_string(),
        "openai" => "openai".to_string(),
        "gemini" | "google" => "gemini".to_string(),
        "anthropic" | "claude" => "anthropic".to_string(),
        "custom" => "custom".to_string(),
        "siliconflow" => "siliconflow".to_string(),
        _ => default_llm_provider(),
    }
}

fn normalize_llm_protocol(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "openai-compatible" | "openai" => "openai-compatible".to_string(),
        "anthropic-native" | "anthropic" => "anthropic-native".to_string(),
        "gemini-native" | "gemini" => "gemini-native".to_string(),
        _ => default_llm_protocol(),
    }
}

fn active_provider_api_key_key(provider: &str, custom_provider_name: &str) -> String {
    if provider == "custom" {
        let name = custom_provider_name.trim();
        if name.is_empty() {
            "custom".to_string()
        } else {
            format!("custom:{name}")
        }
    } else {
        provider.to_string()
    }
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

fn canonicalize_feed_url(url: &str) -> String {
    let trimmed = url.trim();
    if let Some(channel_id) = extract_youtube_channel_id(trimmed) {
        return format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}");
    }
    trimmed.to_string()
}

pub fn canonicalize_source_url(url: &str) -> String {
    canonicalize_feed_url(url)
}

fn extract_youtube_channel_id(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let without_www = without_scheme
        .strip_prefix("www.")
        .unwrap_or(without_scheme);
    let rest = without_www.strip_prefix("youtube.com/channel/")?;
    let channel_id = rest.split(['/', '?', '#']).next().unwrap_or("").trim();
    if channel_id.is_empty() {
        None
    } else {
        Some(channel_id.to_string())
    }
}

fn should_block_source_url(url: &str) -> bool {
    let normalized = normalize_url(url);
    const BLOCKED_URLS: &[&str] = &[
        "wechat2rss.bestblogs.dev/feed/96b507f9985efa59e549e95a6363c2b6edfa8f2e.xml",
        "blogs.worldbank.org/feed/impactevaluations/rss.xml",
        "rachelbythebay.com/w/atom.xml",
        "feed.tedium.co",
        "tedunangst.com/flak/rss",
        "www.tedunangst.com/flak/rss",
        "rsshub.app/meta/ai/blog",
        "grafana.com/categories/engineering/index.xml",
        "www.llamaindex.ai/blog/feed",
        "utcc.utoronto.ca/~cks/space/blog/?atom",
        "www.bloomberg.com/politics/feeds/site.xml",
        "www.chemistryworld.com/rss",
        "feeds.rsc.org/rss/sc",
        "feeds.rsc.org/rss/an",
        "feeds.rsc.org/rss/cp",
        "stereochemistry.libsyn.com/rss",
        "chemistryinitselement.libsyn.com/rss",
        "twievo.libsyn.com/rss",
        "jamanetwork.com/rss/site_3/67.xml",
        "jamanetwork.com/rss/site_3/onlinefirst_67.xml",
        "jamanetwork.com/rss/site_3/latestissue_67.xml",
        "jamaeditorsaudiosummary.libsyn.com/rss",
        "jamaclinicalreviews.libsyn.com/rss",
        "jamamedicalnews.libsyn.com/rss",
    ];
    BLOCKED_URLS.iter().any(|blocked| normalized == *blocked)
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

pub fn build_custom_source_id(name: &str, url: &str) -> String {
    let canonical = canonicalize_feed_url(url);
    let normalized = normalize_url(&canonical);
    build_source_id(name, &normalized)
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
        "design" => "设计",
        "science" => "科学",
        "medicine" => "医学",
        _ => "其他",
    }
}

fn discipline_modules(discipline: &Discipline) -> &'static [&'static str] {
    match discipline {
        Discipline::Technology => &["technology"],
        Discipline::SocialScience => &["social_science"],
        Discipline::Other => &["business"],
        Discipline::Life => &["business"],
        Discipline::News => &["other"],
        Discipline::Humanities => &["design"],
        Discipline::Science => &["science"],
        Discipline::Medicine => &["medicine"],
    }
}

fn bucket_label(raw: &str) -> &'static str {
    match raw {
        "research" => "研究",
        "frontier" => "前沿",
        "official" => "官方",
        "blogs" => "博客",
        "community" => "社区",
        "news" => "新闻",
        "opinion" => "观点",
        "physics" => "物理",
        "chemistry" => "化学",
        "biology" => "生物",
        _ => "未分类",
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use super::{module_refresh_due, source_due_for_module_refresh, source_due_for_retry};

    #[test]
    fn module_refresh_uses_module_clock() {
        let now = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        assert!(module_refresh_due(None, Duration::hours(6), now));
        assert!(module_refresh_due(
            Some(Utc.with_ymd_and_hms(2026, 4, 24, 6, 0, 0).unwrap()),
            Duration::hours(6),
            now,
        ));
        assert!(!module_refresh_due(
            Some(Utc.with_ymd_and_hms(2026, 4, 24, 8, 30, 0).unwrap()),
            Duration::hours(6),
            now,
        ));
    }

    #[test]
    fn module_refresh_source_uses_last_success_time() {
        let now = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        assert!(source_due_for_module_refresh(None, Duration::hours(12), now));
        assert!(source_due_for_module_refresh(
            Some(Utc.with_ymd_and_hms(2026, 4, 24, 0, 0, 0).unwrap()),
            Duration::hours(12),
            now,
        ));
        assert!(!source_due_for_module_refresh(
            Some(Utc.with_ymd_and_hms(2026, 4, 24, 10, 30, 0).unwrap()),
            Duration::hours(6),
            now,
        ));
    }

    #[test]
    fn retry_only_runs_for_unrecovered_failures() {
        let now = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        let failed_at = Utc.with_ymd_and_hms(2026, 4, 24, 10, 0, 0).unwrap();
        let old_success = Utc.with_ymd_and_hms(2026, 4, 24, 8, 0, 0).unwrap();
        assert!(source_due_for_retry(
            Some(failed_at),
            Some(old_success),
            true,
            Duration::hours(2),
            now,
        ));
        assert!(!source_due_for_retry(
            Some(failed_at),
            Some(Utc.with_ymd_and_hms(2026, 4, 24, 11, 0, 0).unwrap()),
            true,
            Duration::hours(2),
            now,
        ));
        assert!(!source_due_for_retry(
            Some(failed_at),
            Some(old_success),
            false,
            Duration::hours(2),
            now,
        ));
    }
}
