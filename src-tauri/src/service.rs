use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use futures::future::join_all;
use std::{
    collections::HashSet,
    time::Instant,
};
use tauri::{AppHandle, Manager};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::{
    db, llm,
    models::{
        AppView, FeedArticle, LlmResult, OverlaySnapshot, PetStatus, SettingsPayload, Snapshot,
    },
    policy,
    rss, AppState,
};

const SCORE_BATCH_SIZE: usize = 3;
const MAX_CONCURRENT_SCORE_BATCHES: usize = 20;
const SCHEDULER_TICK_MINUTES: u64 = 15;
const POLLING_PEEK_SECONDS: u64 = 2;
const INITIAL_FETCH_LOOKBACK_DAYS: i64 = 7;
const PUSH_TOP_PER_BUCKET: usize = 3;
const PUSH_MAX_AGE_DAYS: i64 = 2;
const PUSH_MIN_FIT_SCORE: i64 = 60;
const PENDING_BACKLOG_BATCH_SIZE: usize = 180;
pub const EVENT_SNAPSHOT_UPDATED: &str = "briefy://snapshot-updated";
pub const EVENT_OVERLAY_UPDATED: &str = "briefy://overlay-updated";

pub fn requires_configuration(settings: &SettingsPayload, api_key_valid: Option<bool>) -> bool {
    requires_configuration_flags(
        is_settings_complete(settings),
        !settings.api_key.trim().is_empty(),
        api_key_valid,
    )
}

fn requires_configuration_flags(
    discipline_ready: bool,
    has_api_key: bool,
    api_key_valid: Option<bool>,
) -> bool {
    !discipline_ready || !has_api_key || api_key_valid == Some(false)
}

pub fn resolve_requested_view(app: &AppHandle, requested: AppView) -> Result<AppView> {
    if requested == AppView::Settings {
        return Ok(AppView::Settings);
    }

    let conn = db::connect(app)?;
    let settings = db::read_settings(&conn)?;
    let api_key_valid = app
        .state::<AppState>()
        .api_key_valid
        .lock()
        .ok()
        .and_then(|value| *value)
        .or(Some(db::read_api_key_valid(&conn)?));

    if requires_configuration(&settings, api_key_valid) {
        Ok(AppView::Settings)
    } else {
        Ok(AppView::Reading)
    }
}

pub fn derive_pet_status(
    settings: &SettingsPayload,
    api_key_valid: Option<bool>,
    has_active_reminder: bool,
    is_scanning: bool,
    is_loading: bool,
) -> PetStatus {
    if is_loading {
        PetStatus::Loading
    } else if requires_configuration(settings, api_key_valid) {
        PetStatus::NeedsConfig
    } else if is_scanning || api_key_valid.is_none() {
        PetStatus::Scanning
    } else if has_active_reminder {
        PetStatus::NewInfo
    } else {
        PetStatus::Idle
    }
}

pub fn snapshot(app: &AppHandle, is_scanning: bool) -> Result<Snapshot> {
    let conn = db::connect(app)?;
    let settings = db::read_settings(&conn)?;
    let active_reminder = db::push_waiting_reminder(app)?;
    let last_error = app
        .state::<AppState>()
        .last_error
        .lock()
        .ok()
        .and_then(|error| error.clone());
    let api_key_valid = app
        .state::<AppState>()
        .api_key_valid
        .lock()
        .ok()
        .and_then(|value| *value)
        .or(Some(db::read_api_key_valid(&conn)?));
    let last_scan_at = app
        .state::<AppState>()
        .last_scan_at
        .lock()
        .ok()
        .and_then(|value| *value)
        .or(db::read_last_scan_at(&conn)?);
    let is_loading = app
        .state::<AppState>()
        .loading_until
        .lock()
        .ok()
        .and_then(|value| *value)
        .is_some();
    let pet_status = if is_loading {
        PetStatus::Loading
    } else if current_polling(app, Some(Utc::now())) {
        PetStatus::Polling
    } else {
        derive_pet_status(
            &settings,
            api_key_valid,
            active_reminder.is_some(),
            is_scanning,
            false,
        )
    };

    let mut snapshot = db::build_snapshot(
        &conn,
        pet_status,
        last_error,
        api_key_valid.unwrap_or(false),
        last_scan_at,
    )?;

    snapshot.active_reminder = active_reminder;
    snapshot.history_articles = db::list_push_history_articles_page(app, 0, 200)?;

    if let Some(reminder) = &snapshot.active_reminder {
        let existing = snapshot
            .articles
            .iter()
            .map(|article| article.id)
            .collect::<HashSet<_>>();
        for article_id in &reminder.article_ids {
            if existing.contains(article_id) {
                continue;
            }
            if let Some(article) = db::fetch_article_record(&conn, *article_id)? {
                snapshot.articles.push(article);
            }
        }
    }

    Ok(snapshot)
}

pub fn snapshot_overlay(app: &AppHandle, is_scanning: bool) -> Result<OverlaySnapshot> {
    let conn = db::connect(app)?;
    let active_reminder = db::push_active_reminder(app)?;
    let (has_api_key, discipline_ready) = db::read_runtime_config_flags(&conn)?;

    let api_key_valid = app
        .state::<AppState>()
        .api_key_valid
        .lock()
        .ok()
        .and_then(|value| *value)
        .or(Some(db::read_api_key_valid(&conn)?));
    let is_loading = app
        .state::<AppState>()
        .loading_until
        .lock()
        .ok()
        .and_then(|value| *value)
        .is_some();

    let pet_status = if is_loading {
        PetStatus::Loading
    } else if current_polling(app, Some(Utc::now())) {
        PetStatus::Polling
    } else if requires_configuration_flags(discipline_ready, has_api_key, api_key_valid) {
        PetStatus::NeedsConfig
    } else if is_scanning || api_key_valid.is_none() {
        PetStatus::Scanning
    } else if active_reminder.is_some() {
        PetStatus::NewInfo
    } else {
        PetStatus::Idle
    };

    Ok(OverlaySnapshot {
        pet_status,
        active_reminder,
    })
}

pub fn current_scanning(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .is_scanning
        .lock()
        .map(|value| *value)
        .unwrap_or(false)
}

pub fn publish_snapshot(app: &AppHandle, is_scanning: bool) -> Result<Snapshot> {
    let payload = snapshot(app, is_scanning)?;
    let _ = app.emit_all(EVENT_SNAPSHOT_UPDATED, payload.clone());
    Ok(payload)
}

pub fn publish_overlay(app: &AppHandle, is_scanning: bool) -> Result<OverlaySnapshot> {
    let payload = snapshot_overlay(app, is_scanning)?;
    let _ = app.emit_all(EVENT_OVERLAY_UPDATED, payload.clone());
    Ok(payload)
}

pub fn ensure_scheduler(app: &AppHandle) {
    let should_start = {
        let state = app.state::<AppState>();
        let decision = match state.scheduler_started.lock() {
            Ok(mut started) => {
                if *started {
                    false
                } else {
                    *started = true;
                    true
                }
            }
            Err(_) => false,
        };
        decision
    };

    if !should_start {
        return;
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            sleep(TokioDuration::from_secs(SCHEDULER_TICK_MINUTES * 60)).await;
            run_scheduler_poll_cycle(app_handle.clone()).await;
        }
    });
}

pub fn show_help_window(app: &AppHandle) -> Result<()> {
    if let Some(window) = app.get_window("help") {
        window.show()?;
        window.set_focus()?;
    }
    let _ = app.emit_all("briefy://help-opened", ());
    Ok(())
}

pub fn hide_help_window(app: &AppHandle) -> Result<()> {
    if let Some(window) = app.get_window("help") {
        window.hide()?;
    }
    Ok(())
}

pub fn reveal_pet_on_launch(app: &AppHandle, seconds: i64) -> Result<()> {
    let visible_until = Utc::now() + Duration::seconds(seconds);
    set_pet_visible_until(app, Some(visible_until));
    sync_windows(app, current_scanning(app))
}

pub fn reveal_pet_for_polling(app: &AppHandle) {
    let visible_until = Utc::now() + Duration::seconds(POLLING_PEEK_SECONDS as i64);
    set_pet_visible_until(app, Some(visible_until));
    set_polling_until(app, Some(visible_until));

    let is_scanning = app
        .state::<AppState>()
        .is_scanning
        .lock()
        .map(|value| *value)
        .unwrap_or(false);
    let _ = sync_windows(app, is_scanning);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        sleep(TokioDuration::from_secs(POLLING_PEEK_SECONDS)).await;
        let is_scanning = app_handle
            .state::<AppState>()
            .is_scanning
            .lock()
            .map(|value| *value)
            .unwrap_or(false);
        let _ = sync_windows(&app_handle, is_scanning);
    });
}

async fn run_scheduler_poll_cycle(app: AppHandle) {
    reveal_pet_for_polling(&app);
    sleep(TokioDuration::from_secs(POLLING_PEEK_SECONDS)).await;
    clear_polling_until(&app);

    let Ok(conn) = db::connect(&app) else {
        let _ = sync_windows(&app, false);
        return;
    };
    let Ok(settings) = db::read_settings(&conn) else {
        let _ = sync_windows(&app, false);
        return;
    };
    let api_key_valid = app
        .state::<AppState>()
        .api_key_valid
        .lock()
        .ok()
        .and_then(|value| *value)
        .or(db::read_api_key_valid(&conn).ok());

    if requires_configuration(&settings, api_key_valid) {
        let _ = sync_windows(&app, false);
        let _ = publish_snapshot(&app, false);
        return;
    }

    let due_sources = db::list_due_sources(&conn, Utc::now(), false).unwrap_or_default();
    if due_sources.is_empty() {
        let _ = sync_windows(&app, false);
        let _ = publish_snapshot(&app, false);
        return;
    }

    log_fetch_result(run_fetch_cycle(app.clone(), false).await, &app);
}

pub fn trigger_fetch_now(
    app: &AppHandle,
    delay: Option<std::time::Duration>,
    force_incremental: bool,
) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(delay) = delay {
            sleep(TokioDuration::from_millis(delay.as_millis() as u64)).await;
        }
        log_fetch_result(
            run_fetch_cycle(app_handle.clone(), force_incremental).await,
            &app_handle,
        );
    });
}

pub async fn validate_api_key_for_settings(app: &AppHandle, settings: &SettingsPayload) -> Result<()> {
    if settings.api_key.trim().is_empty() {
        set_api_key_valid(app, Some(false));
        let conn = db::connect(app)?;
        db::write_api_key_valid(&conn, false)?;
        return Err(anyhow!("API Key validation failed: missing API Key"));
    }

    if let Err(err) = llm::validate_api_key(
        &settings.llm_provider,
        Some(&settings.llm_model),
        &settings.api_key,
    )
    .await
    {
        if llm::is_auth_failure(&err) {
            set_api_key_valid(app, Some(false));
            let conn = db::connect(app)?;
            db::write_api_key_valid(&conn, false)?;
        }
        return Err(err).context("API Key validation failed");
    }

    set_api_key_valid(app, Some(true));
    let conn = db::connect(app)?;
    db::write_api_key_valid(&conn, true)?;
    clear_last_error(app);
    Ok(())
}

pub async fn run_fetch_cycle(app: AppHandle, force_incremental: bool) -> Result<()> {
    if !begin_scan(&app) {
        return Ok(());
    }
    clear_polling_until(&app);
    clear_loading_until(&app);
    let _ = sync_windows(&app, true);
    let _ = publish_snapshot(&app, true);
    let cycle_started_at = Utc::now();
    let started_at = Instant::now();

    let conn = db::connect(&app)?;
    let settings = db::read_settings(&conn)?;
    let api_key_valid = app
        .state::<AppState>()
        .api_key_valid
        .lock()
        .ok()
        .and_then(|value| *value)
        .or(Some(db::read_api_key_valid(&conn)?));
    if requires_configuration(&settings, api_key_valid) {
        if settings.api_key.trim().is_empty() || api_key_valid == Some(false) {
            set_api_key_valid(&app, Some(false));
            db::write_api_key_valid(&conn, false)?;
        }
        let cycle_ended_at = Utc::now();
        let _ = db::log_crawl_cycle(
            &conn,
            cycle_started_at,
            cycle_ended_at,
            "skipped-needs-config",
            0,
            0,
            0,
            0,
            0,
            0,
            started_at.elapsed().as_millis(),
            None,
            None,
        );
        clear_last_error(&app);
        set_scanning(&app, false);
        sync_windows(&app, false)?;
        let _ = publish_snapshot(&app, false);
        return Ok(());
    }

    ensure_api_key_ready(&app, &settings).await?;

    let now = Utc::now();
    let backlog_records = db::list_pending_article_backlog(&conn, PENDING_BACKLOG_BATCH_SIZE)?;
    let due_sources = db::list_due_sources(&conn, now, force_incremental)?;
    if due_sources.is_empty() && backlog_records.is_empty() {
        let memory = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)?;
        if memory.is_some() {
            clear_last_error(&app);
        }
        set_scanning(&app, false);
        sync_windows(&app, false)?;
        let _ = publish_snapshot(&app, false);
        let cycle_ended_at = Utc::now();
        let _ = db::log_crawl_cycle(
            &conn,
            cycle_started_at,
            cycle_ended_at,
            "idle-no-due-sources",
            0,
            0,
            0,
            0,
            0,
            0,
            started_at.elapsed().as_millis(),
            None,
            None,
        );
        return Ok(());
    }

    let mut pending_articles = backlog_records
        .into_iter()
        .map(|record| PendingArticle {
            article_id: record.id,
            article: record.article,
        })
        .collect::<Vec<_>>();
    let mut fetch_errors = Vec::new();
    let mut successful_source_ids = Vec::new();
    let fetch_started_at = Instant::now();
    if !due_sources.is_empty() {
        let fetch_outcome = match rss::fetch_sources(&due_sources).await {
            Ok(value) => value,
            Err(err) => {
                let cycle_ended_at = Utc::now();
                let _ = db::log_crawl_cycle(
                    &conn,
                    cycle_started_at,
                    cycle_ended_at,
                    "failed-fetch",
                    due_sources.len(),
                    0,
                    0,
                    0,
                    fetch_started_at.elapsed().as_millis(),
                    0,
                    started_at.elapsed().as_millis(),
                    None,
                    Some(&format_compact_error(&err, 300)),
                );
                return Err(err).context("RSS fetch/parse failed");
            }
        };

        for result in fetch_outcome.results {
            if let Some(error) = result.error {
                db::update_fetch_state(&conn, &result.source.id, now, None, Some(&error))?;
                fetch_errors.push(format!("{}: {}", result.source.name, error));
                continue;
            }

            let is_initial_fetch = !db::source_has_successful_fetch(&conn, &result.source.id)?;
            let incremental_cutoff = if force_incremental && !is_initial_fetch {
                db::source_last_fetched_at(&conn, &result.source.id)?
            } else {
                None
            };
            let filtered = collect_pending_articles(
                &conn,
                result.articles,
                is_initial_fetch,
                now,
                incremental_cutoff,
            )?;
            for article in filtered {
                let article_key = build_article_key(&article);
                let guid = effective_guid(&article, &article_key);
                let article_id = db::insert_pending_article(
                    &conn,
                    &article.source_id,
                    &guid,
                    &article_key,
                    &article.normalized_link,
                    &article.title,
                    &article.link,
                    &article.source_name,
                    &article.discipline,
                    &article.source_kind,
                    &article.resource_type,
                    article.published_at,
                    now,
                    &article.content,
                )?;
                pending_articles.push(PendingArticle { article_id, article });
            }
            successful_source_ids.push(result.source.id);
        }
    }
    let fetch_elapsed = fetch_started_at.elapsed();

    let interest_context = build_interest_context(&settings);
    let score_started_at = Instant::now();
    let score_outcome = score_articles_in_batches(
        &settings.llm_provider,
        Some(&settings.llm_model),
        &settings.api_key,
        &interest_context,
        &pending_articles,
    )
    .await?;
    let score_elapsed = score_started_at.elapsed();

    let mut inserted_count = 0usize;
    let mut failed_scoring_count = 0usize;
    let mut updated_buckets = std::collections::BTreeSet::<(String, String)>::new();
    for result in score_outcome.results {
        if let Some(analysis) = result.analysis {
            let module = policy::normalize_module(&result.article.module);
            let bucket = policy::normalize_bucket(&module, &result.article.bucket);
            let calibrated_fit_level = policy::fit_level_for_score(
                &module,
                &bucket,
                &result.article.source_kind,
                analysis.fit_score,
            );
            db::mark_article_scored_success(
                &conn,
                result.article_id,
                &analysis.summary,
                &calibrated_fit_level,
                analysis.fit_score,
                &analysis.recommendation_reason,
            )?;
            db::upsert_content_pool_entry(
                &conn,
                result.article_id,
                &module,
                &bucket,
                &result.article.source_kind,
                analysis.fit_score,
                result.article.published_at,
            )?;
            updated_buckets.insert((module, bucket));
            inserted_count += 1;
        } else {
            failed_scoring_count += 1;
            db::mark_article_scored_failed(
                &conn,
                result.article_id,
                result.error.as_deref().unwrap_or("llm scoring failed"),
            )?;
        }
    }

    let reminder_candidates = db::list_top_bucket_candidates(
        &conn,
        &updated_buckets,
        PUSH_TOP_PER_BUCKET,
        PUSH_MAX_AGE_DAYS,
        PUSH_MIN_FIT_SCORE,
    )?;
    if !reminder_candidates.is_empty() {
        let _ = db::queue_push_articles(&conn, &app, &reminder_candidates)?;
        db::remove_content_pool_entries(&conn, &reminder_candidates)?;
    }

    for source_id in successful_source_ids {
        db::update_fetch_state(&conn, &source_id, now, Some(now), None)?;
    }

    let _ = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)?;
    set_scanning(&app, false);
    set_last_scan_at(&app, Some(now));
    db::write_last_scan_at(&conn, Some(now))?;

    let warnings = collect_cycle_warnings(fetch_errors, score_outcome.errors);
    clear_last_error(&app);
    sync_windows(&app, false)?;
    let _ = publish_snapshot(&app, false);

    let cycle_ended_at = Utc::now();
    let warning_summary = if warnings.is_empty() {
        None
    } else {
        Some(warnings.join(" | "))
    };
    let cycle_status = if warning_summary.is_some() || failed_scoring_count > 0 {
        "completed-with-warnings"
    } else {
        "completed"
    };
    let _ = db::log_crawl_cycle(
        &conn,
        cycle_started_at,
        cycle_ended_at,
        cycle_status,
        due_sources.len(),
        pending_articles.len(),
        inserted_count,
        failed_scoring_count,
        fetch_elapsed.as_millis(),
        score_elapsed.as_millis(),
        started_at.elapsed().as_millis(),
        warning_summary.as_deref(),
        None,
    );

    eprintln!(
        "briefy-pet fetch cycle ok: due_sources={} pending={} inserted={} failed_scoring={} rss_ms={} llm_ms={} total_ms={}",
        due_sources.len(),
        pending_articles.len(),
        inserted_count,
        failed_scoring_count,
        fetch_elapsed.as_millis(),
        score_elapsed.as_millis(),
        started_at.elapsed().as_millis(),
    );

    Ok(())
}

async fn ensure_api_key_ready(app: &AppHandle, settings: &SettingsPayload) -> Result<()> {
    let cached = app
        .state::<AppState>()
        .api_key_valid
        .lock()
        .ok()
        .and_then(|value| *value);

    if cached == Some(true) {
        return Ok(());
    }

    if let Err(err) = llm::validate_api_key(
        &settings.llm_provider,
        Some(&settings.llm_model),
        &settings.api_key,
    )
    .await
    {
        if llm::is_auth_failure(&err) {
            set_api_key_valid(app, Some(false));
            let conn = db::connect(app)?;
            db::write_api_key_valid(&conn, false)?;
            return Err(err).context("API Key validation failed");
        }
        if cached == Some(false) {
            return Err(anyhow!(
                "API Key validation failed: cached API Key is invalid"
            ));
        }
        return Err(err).context("API Key validation failed");
    }

    set_api_key_valid(app, Some(true));
    let conn = db::connect(app)?;
    db::write_api_key_valid(&conn, true)?;
    Ok(())
}

fn collect_pending_articles(
    conn: &rusqlite::Connection,
    articles: Vec<FeedArticle>,
    is_initial_fetch: bool,
    fetched_at: DateTime<Utc>,
    incremental_cutoff: Option<DateTime<Utc>>,
) -> Result<Vec<FeedArticle>> {
    let mut pending = Vec::new();
    let mut seen_keys = HashSet::new();
    let initial_cutoff = if is_initial_fetch {
        Some(fetched_at - Duration::days(INITIAL_FETCH_LOOKBACK_DAYS))
    } else {
        None
    };

    for article in articles {
        if let Some(cutoff) = initial_cutoff {
            if let Some(published_at) = article.published_at.as_ref() {
                if *published_at < cutoff {
                    continue;
                }
            }
        }

        if let Some(cutoff) = incremental_cutoff {
            if let Some(published_at) = article.published_at.as_ref() {
                if *published_at <= cutoff {
                    continue;
                }
            }
        }

        let article_key = build_article_key(&article);
        if !seen_keys.insert(article_key.clone()) {
            continue;
        }
        if db::find_article_id_by_identity(
            conn,
            &article.source_id,
            &article_key,
            &article.normalized_link,
            &effective_guid(&article, &article_key),
        )?
        .is_none()
        {
            pending.push(article);
        }
    }
    Ok(pending)
}

async fn score_articles_in_batches(
    provider: &str,
    model_override: Option<&str>,
    api_key: &str,
    interest_context: &str,
    articles: &[PendingArticle],
) -> Result<ScoreOutcome> {
    if articles.is_empty() {
        return Ok(ScoreOutcome {
            results: Vec::new(),
            errors: Vec::new(),
        });
    }

    let mut results_collected = Vec::new();
    let mut errors = Vec::new();
    let provider = provider.to_string();
    let model_override = model_override.map(|value| value.to_string());
    let api_key = api_key.to_string();

    for group in articles
        .chunks(SCORE_BATCH_SIZE * MAX_CONCURRENT_SCORE_BATCHES)
        .map(|chunk| {
            chunk
                .chunks(SCORE_BATCH_SIZE)
                .map(|inner| inner.to_vec())
                .collect::<Vec<_>>()
        })
    {
        let results = join_all(group.iter().map(|batch| {
            let batch = batch.clone();
            let interest_context = interest_context.to_string();
            let provider = provider.clone();
            let model_override = model_override.clone();
            let api_key = api_key.clone();
            async move {
                let batch_articles = batch
                    .iter()
                    .map(|item| item.article.clone())
                    .collect::<Vec<_>>();

                match llm::summarize_and_score_batch(
                    &provider,
                    model_override.as_deref(),
                    &api_key,
                    &interest_context,
                    &batch_articles,
                )
                .await
                {
                    Ok(analyses) if analyses.len() == batch.len() => {
                        let batch_results = batch
                            .into_iter()
                            .zip(analyses.into_iter())
                            .map(|(item, analysis)| ArticleScoreResult {
                                article_id: item.article_id,
                                article: item.article,
                                analysis: Some(analysis),
                                error: None,
                            })
                            .collect::<Vec<_>>();

                        Ok::<BatchScoreOutcome, anyhow::Error>(BatchScoreOutcome {
                            results: batch_results,
                            errors: Vec::new(),
                        })
                    }
                    Ok(analyses) => {
                        let mismatch_err = anyhow!(
                            "batch result length mismatch: expected {}, got {}",
                            batch.len(),
                            analyses.len()
                        );
                        eprintln!(
                            "briefy-pet batch scoring failed, falling back to per-article scoring: {}",
                            mismatch_err
                        );

                        let mut fallback_results = Vec::new();
                        let mut fallback_errors = Vec::new();
                        for item in batch {
                            match llm::summarize_and_score_single(
                                &provider,
                                model_override.as_deref(),
                                &api_key,
                                &interest_context,
                                &item.article,
                            )
                            .await
                            {
                                Ok(analysis) => fallback_results.push(ArticleScoreResult {
                                    article_id: item.article_id,
                                    article: item.article,
                                    analysis: Some(analysis),
                                    error: None,
                                }),
                                Err(err) => {
                                    let message = format!(
                                        "{} => {}",
                                        truncate_text(item.article.title.trim(), 72),
                                        format_compact_error(&err, 240)
                                    );
                                    fallback_errors.push(message.clone());
                                    fallback_results.push(ArticleScoreResult {
                                        article_id: item.article_id,
                                        article: item.article,
                                        analysis: None,
                                        error: Some(message),
                                    });
                                }
                            }
                        }

                        Ok(BatchScoreOutcome {
                            results: fallback_results,
                            errors: fallback_errors,
                        })
                    }
                    Err(batch_err) => {
                        eprintln!(
                            "briefy-pet batch scoring failed, falling back to per-article scoring: {}",
                            batch_err
                        );

                        let mut fallback_results = Vec::new();
                        let mut fallback_errors = Vec::new();
                        for item in batch {
                            match llm::summarize_and_score_single(
                                &provider,
                                model_override.as_deref(),
                                &api_key,
                                &interest_context,
                                &item.article,
                            )
                            .await
                            {
                                Ok(analysis) => fallback_results.push(ArticleScoreResult {
                                    article_id: item.article_id,
                                    article: item.article,
                                    analysis: Some(analysis),
                                    error: None,
                                }),
                                Err(err) => {
                                    let message = format!(
                                        "{} => {}",
                                        truncate_text(item.article.title.trim(), 72),
                                        format_compact_error(&err, 240)
                                    );
                                    fallback_errors.push(message.clone());
                                    fallback_results.push(ArticleScoreResult {
                                        article_id: item.article_id,
                                        article: item.article,
                                        analysis: None,
                                        error: Some(message),
                                    });
                                }
                            }
                        }

                        Ok(BatchScoreOutcome {
                            results: fallback_results,
                            errors: fallback_errors,
                        })
                    }
                }
            }
        }))
        .await;

        for result in results {
            match result {
                Ok(mut batch_result) => {
                    results_collected.append(&mut batch_result.results);
                    errors.append(&mut batch_result.errors);
                }
                Err(err) => errors.push(format!(
                    "batch execution failure: {}",
                    format_compact_error(&err, 240)
                )),
            }
        }
    }

    Ok(ScoreOutcome {
        results: results_collected,
        errors,
    })
}

fn build_interest_context(settings: &SettingsPayload) -> String {
    let mut sections = Vec::new();
    for item in settings.disciplines.iter().filter(|item| item.enabled) {
        sections.push(format!(
            "- {}: {}",
            item.discipline.display_name(),
            item.preference.trim()
        ));
    }
    if settings.memory_mode_enabled && !settings.memory_summary.trim().is_empty() {
        sections.push(format!("每日兴趣记忆: {}", settings.memory_summary.trim()));
    }
    sections.join("\n")
}

fn build_article_key(article: &FeedArticle) -> String {
    if article.guid.trim().is_empty() {
        format!("{}::{}", article.source_id, article.normalized_link)
    } else {
        format!(
            "{}::{}",
            article.source_id,
            article.guid.trim().to_lowercase()
        )
    }
}

fn effective_guid(article: &FeedArticle, article_key: &str) -> String {
    let guid = article.guid.trim();
    if guid.is_empty() {
        article_key.to_string()
    } else {
        guid.to_string()
    }
}

fn is_settings_complete(settings: &SettingsPayload) -> bool {
    let selected = settings
        .disciplines
        .iter()
        .filter(|item| item.enabled)
        .collect::<Vec<_>>();
    !selected.is_empty()
        && selected
            .iter()
            .all(|item| !item.preference.trim().is_empty())
}

fn collect_cycle_warnings(fetch_errors: Vec<String>, score_errors: Vec<String>) -> Vec<String> {
    let mut warnings = Vec::new();
    let hard_fetch_errors = fetch_errors
        .into_iter()
        .filter(|item| !is_soft_feed_error(item))
        .collect::<Vec<_>>();

    if let Some(message) = summarize_error_list("some feeds failed", hard_fetch_errors, 3) {
        warnings.push(message);
    }
    if let Some(message) = summarize_error_list(
        "some articles failed LLM scoring and were marked failed",
        score_errors,
        5,
    ) {
        warnings.push(message);
    }
    warnings
}

fn is_soft_feed_error(value: &str) -> bool {
    value.contains("HTTP 403") || value.contains("HTTP 404")
}

fn summarize_error_list(prefix: &str, errors: Vec<String>, sample_size: usize) -> Option<String> {
    if errors.is_empty() {
        return None;
    }

    let total = errors.len();
    let samples = errors
        .into_iter()
        .take(sample_size)
        .collect::<Vec<_>>()
        .join(" | ");

    if samples.is_empty() {
        Some(format!("{prefix}: {total}"))
    } else {
        Some(format!("{prefix}: {total} (examples: {samples})"))
    }
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}

fn format_compact_error(err: &anyhow::Error, max_chars: usize) -> String {
    let chain = format!("{:#}", err).replace('\n', " | ");
    truncate_text(chain.trim(), max_chars)
}

fn log_fetch_result(result: Result<()>, app: &AppHandle) {
    if let Err(err) = result {
        set_scanning(app, false);
        let message = classify_error_for_display(&err);
        if message.starts_with("API Key validation failed") && llm::is_auth_failure(&err) {
            set_api_key_valid(app, Some(false));
            if let Ok(conn) = db::connect(app) {
                let _ = db::write_api_key_valid(&conn, false);
            }
            set_last_error(app, "API Key validation failed, please update settings.".to_string());
        } else {
            clear_last_error(app);
        }

        if let Ok(conn) = db::connect(app) {
            let now = Utc::now();
            let _ = db::log_crawl_cycle(
                &conn,
                now,
                now,
                "failed-runtime",
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                None,
                Some(&format_compact_error(&err, 400)),
            );
        }

        let _ = sync_windows(app, false);
        let _ = publish_snapshot(app, false);
        eprintln!("briefy-pet fetch cycle failed: {message}");
    }
}

fn classify_error_for_display(err: &anyhow::Error) -> String {
    let text = err.to_string();
    if text.contains("API Key validation failed") || text.contains("api key validation failed") {
        format!("API Key validation failed: {text}")
    } else if text.contains("RSS fetch/parse failed") {
        format!("RSS fetch/parse failed: {text}")
    } else if text.contains("LLM scoring failed") || text.contains("llm request failed") {
        format!("LLM scoring failed: {text}")
    } else {
        format!("Fetch or analysis failed: {text}")
    }
}

pub fn open_main(app: &AppHandle, view: AppView) -> Result<()> {
    let conn = db::connect(app)?;
    db::write_active_view(&conn, &view)?;
    if let Some(window) = app.get_window("main") {
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

pub fn handle_pet_double_click(app: &AppHandle) -> Result<()> {
    let conn = db::connect(app)?;
    let settings = db::read_settings(&conn)?;
    let api_key_valid = app
        .state::<AppState>()
        .api_key_valid
        .lock()
        .ok()
        .and_then(|value| *value)
        .or(Some(db::read_api_key_valid(&conn)?));
    let view = if requires_configuration(&settings, api_key_valid) {
        AppView::Settings
    } else {
        AppView::Reading
    };
    open_main(app, view)
}

pub fn handle_bubble_action(app: &AppHandle, action: &str) -> Result<Snapshot> {
    let conn = db::connect(app)?;
    if let Some(batch) = db::push_active_reminder(app)? {
        match action {
            "view" => {
                db::set_push_snooze_until(app, None)?;
                db::write_active_view(&conn, &AppView::Reading)?;
                db::log_user_event(&conn, "bubble-view", batch.top_article_id, None, None)?;
                if let Some(top_article_id) = batch.top_article_id {
                    db::write_selected_article_id(&conn, Some(top_article_id))?;
                    let source_id = db::article_source_id(&conn, top_article_id)?;
                    db::log_user_event(
                        &conn,
                        "open-article",
                        Some(top_article_id),
                        source_id.as_deref(),
                        Some(r#"{"origin":"bubble"}"#),
                    )?;
                } else {
                    db::write_selected_article_id(&conn, None)?;
                }
                if let Some(window) = app.get_window("main") {
                    window.show()?;
                    window.set_focus()?;
                }
            }
            "snooze" => {
                let remind_at = Utc::now() + Duration::minutes(30);
                db::set_push_snooze_until(app, Some(remind_at))?;
                db::log_user_event(&conn, "bubble-snooze", None, None, None)?;
            }
            "ignore" => {
                let ignored_count = db::mark_all_waiting_pushed(app)?;
                db::log_user_event(
                    &conn,
                    "bubble-ignore",
                    None,
                    None,
                    Some(&format!(r#"{{"ignoredCount":{ignored_count}}}"#)),
                )?;
            }
            _ => {}
        }
    }

    let settings = db::read_settings(&conn)?;
    let _ = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)?;
    let scanning = current_scanning(app);
    sync_windows(app, scanning)?;
    publish_snapshot(app, scanning)
}

pub fn sync_windows(app: &AppHandle, is_scanning: bool) -> Result<()> {
    let current = publish_overlay(app, is_scanning)?;
    if let Some(window) = app.get_window("bubble") {
        if current.pet_status != PetStatus::Polling && current.active_reminder.is_some() {
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
    }
    if let Some(window) = app.get_window("pet") {
        let should_show = should_show_pet_window(app, current.active_reminder.is_some(), is_scanning);
        if should_show {
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
    }
    Ok(())
}

fn should_show_pet_window(app: &AppHandle, has_active_reminder: bool, is_scanning: bool) -> bool {
    if has_active_reminder || is_scanning || current_polling(app, Some(Utc::now())) {
        return true;
    }

    let now = Utc::now();
    if let Ok(mut visible_until) = app.state::<AppState>().pet_visible_until.lock() {
        if let Some(until) = *visible_until {
            if until > now {
                return true;
            }
            *visible_until = None;
        }
    }

    false
}

fn set_last_error(app: &AppHandle, message: String) {
    if let Ok(mut last_error) = app.state::<AppState>().last_error.lock() {
        *last_error = Some(message);
    }
}

pub fn clear_last_error(app: &AppHandle) {
    if let Ok(mut last_error) = app.state::<AppState>().last_error.lock() {
        *last_error = None;
    }
}

fn set_api_key_valid(app: &AppHandle, value: Option<bool>) {
    if let Ok(mut api_key_valid) = app.state::<AppState>().api_key_valid.lock() {
        *api_key_valid = value;
    }
}

fn set_last_scan_at(app: &AppHandle, value: Option<DateTime<Utc>>) {
    if let Ok(mut last_scan_at) = app.state::<AppState>().last_scan_at.lock() {
        *last_scan_at = value;
    }
}

fn clear_loading_until(app: &AppHandle) {
    if let Ok(mut loading_until) = app.state::<AppState>().loading_until.lock() {
        *loading_until = None;
    }
}

fn clear_polling_until(app: &AppHandle) {
    if let Ok(mut polling_until) = app.state::<AppState>().polling_until.lock() {
        *polling_until = None;
    }
}

fn set_pet_visible_until(app: &AppHandle, value: Option<DateTime<Utc>>) {
    if let Ok(mut pet_visible_until) = app.state::<AppState>().pet_visible_until.lock() {
        *pet_visible_until = value;
    }
}

fn set_scanning(app: &AppHandle, value: bool) {
    if let Ok(mut scanning) = app.state::<AppState>().is_scanning.lock() {
        *scanning = value;
    }
}

fn begin_scan(app: &AppHandle) -> bool {
    if let Ok(mut scanning) = app.state::<AppState>().is_scanning.lock() {
        if *scanning {
            return false;
        }
        *scanning = true;
        return true;
    }
    false
}

fn set_polling_until(app: &AppHandle, value: Option<DateTime<Utc>>) {
    if let Ok(mut polling_until) = app.state::<AppState>().polling_until.lock() {
        *polling_until = value;
    }
}

fn current_polling(app: &AppHandle, now: Option<DateTime<Utc>>) -> bool {
    let now = now.unwrap_or_else(Utc::now);
    if let Ok(mut polling_until) = app.state::<AppState>().polling_until.lock() {
        if let Some(until) = *polling_until {
            if until > now {
                return true;
            }
            *polling_until = None;
        }
    }
    false
}

#[derive(Clone)]
struct PendingArticle {
    article_id: i64,
    article: FeedArticle,
}

struct ArticleScoreResult {
    article_id: i64,
    article: FeedArticle,
    analysis: Option<LlmResult>,
    error: Option<String>,
}

struct ScoreOutcome {
    results: Vec<ArticleScoreResult>,
    errors: Vec<String>,
}

struct BatchScoreOutcome {
    results: Vec<ArticleScoreResult>,
    errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use std::collections::BTreeMap;

    use super::{classify_error_for_display, derive_pet_status, is_settings_complete};
    use crate::models::{
        Discipline, PetStatus, RssSource, SettingsPayload, SourceKind, UserDisciplinePreference,
    };

    fn sample_settings() -> SettingsPayload {
        SettingsPayload {
            api_key: "demo-key".into(),
            llm_provider: "deepseek".into(),
            llm_model: String::new(),
            provider_api_keys: BTreeMap::from([("deepseek".to_string(), "demo-key".to_string())]),
            auto_start: false,
            disciplines: vec![UserDisciplinePreference {
                discipline: Discipline::Technology,
                enabled: true,
                preference: "关注 AI 工具和工程实践".into(),
            }],
            memory_mode_enabled: true,
            memory_summary: String::new(),
            rss_sources: vec![RssSource {
                id: "demo".into(),
                name: "Demo".into(),
                url: "https://example.com/feed.xml".into(),
                module: "technology".into(),
                bucket: "blogs".into(),
                discipline: Discipline::Technology,
                source_kind: SourceKind::TechnicalBlog,
                resource_type: crate::models::ResourceType::Article,
                language: Some("en".into()),
                enabled: true,
                enabled_by_default: true,
                postponed: false,
                origin_files: vec!["catalog/demo.opml".into()],
            }],
        }
    }

    #[test]
    fn settings_need_selected_disciplines_and_preference() {
        let mut settings = sample_settings();
        assert!(is_settings_complete(&settings));
        settings.disciplines[0].preference.clear();
        assert!(!is_settings_complete(&settings));
    }

    #[test]
    fn classifies_api_key_errors() {
        let err = anyhow!("API Key validation failed: api key validation failed: unauthorized");
        let message = classify_error_for_display(&err);
        assert!(message.starts_with("API Key validation failed:"));
    }

    #[test]
    fn scanning_takes_priority_over_new_info() {
        let settings = sample_settings();
        let status = derive_pet_status(&settings, Some(true), true, true, false);
        assert_eq!(status, PetStatus::Scanning);
    }
}
