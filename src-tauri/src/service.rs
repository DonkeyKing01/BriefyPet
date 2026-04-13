use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use futures::future::join_all;
use std::{
    collections::{BTreeMap, HashSet},
    time::Instant,
};
use tauri::{AppHandle, Manager};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::{
    diagnostics,
    db, llm,
    models::{
        AppView, FeedArticle, FitLevel, LlmResult, PendingArticleRecord, PetStatus,
        SettingsPayload, Snapshot, SourceKind,
    },
    policy,
    rss, AppState,
};

const SCORE_BATCH_SIZE: usize = 3;
const MAX_CONCURRENT_SCORE_BATCHES: usize = 2;
const SCHEDULER_TICK_MINUTES: u64 = 15;
const MAX_REMINDER_ITEMS_PER_BATCH: usize = 12;

struct PendingProcessingOutcome {
    inserted_count: usize,
    reminder_candidates: Vec<(i64, String, String, SourceKind, i64, Option<DateTime<Utc>>)>,
    errors: Vec<String>,
    remaining_pending: usize,
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
    } else if !is_settings_complete(settings)
        || settings.api_key.trim().is_empty()
        || api_key_valid == Some(false)
    {
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
    let active_reminder = db::active_reminder_batch(&conn)?;
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
    let pet_status = derive_pet_status(
        &settings,
        api_key_valid,
        active_reminder.is_some(),
        is_scanning,
        is_loading,
    );

    db::build_snapshot(
        &conn,
        pet_status,
        last_error,
        api_key_valid.unwrap_or(false),
        last_scan_at,
    )
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
    diagnostics::log(&app_handle, "scheduler", "background scheduler started");
    tauri::async_runtime::spawn(async move {
        loop {
            sleep(TokioDuration::from_secs(SCHEDULER_TICK_MINUTES * 60)).await;
            log_fetch_result(run_fetch_cycle(app_handle.clone()).await, &app_handle);
        }
    });
}

pub fn trigger_fetch_now(app: &AppHandle, delay: Option<std::time::Duration>) {
    let app_handle = app.clone();
    diagnostics::log(
        &app_handle,
        "scheduler",
        format!(
            "manual fetch trigger queued with delay_secs={}",
            delay.map(|value| value.as_secs()).unwrap_or(0)
        ),
    );
    tauri::async_runtime::spawn(async move {
        if let Some(delay) = delay {
            sleep(TokioDuration::from_secs(delay.as_secs())).await;
        }
        log_fetch_result(run_fetch_cycle(app_handle.clone()).await, &app_handle);
    });
}

pub async fn validate_api_key_for_settings(app: &AppHandle, api_key: &str) -> Result<()> {
    if api_key.trim().is_empty() {
        set_api_key_valid(app, Some(false));
        let conn = db::connect(app)?;
        db::write_api_key_valid(&conn, false)?;
        return Err(anyhow!("API Key validation failed: missing API Key"));
    }

    if let Err(err) = llm::validate_api_key(api_key).await {
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

pub async fn run_fetch_cycle(app: AppHandle) -> Result<()> {
    set_scanning(&app, true);
    clear_loading_until(&app);
    let started_at = Instant::now();

    let conn = db::connect(&app)?;
    let settings = db::read_settings(&conn)?;
    diagnostics::log(
        &app,
        "fetch",
        format!(
            "cycle start: api_key_present={} selected_disciplines={} memory_mode_enabled={}",
            !settings.api_key.trim().is_empty(),
            settings.disciplines.iter().filter(|item| item.enabled).count(),
            settings.memory_mode_enabled
        ),
    );
    if settings.api_key.trim().is_empty() {
        diagnostics::log(&app, "fetch", "cycle skipped: missing api key");
        set_api_key_valid(&app, Some(false));
        db::write_api_key_valid(&conn, false)?;
        clear_last_error(&app);
        set_scanning(&app, false);
        sync_windows(&app, false)?;
        return Ok(());
    }

    if !is_settings_complete(&settings) {
        diagnostics::log(&app, "fetch", "cycle skipped: settings incomplete");
        clear_last_error(&app);
        set_scanning(&app, false);
        sync_windows(&app, false)?;
        return Ok(());
    }

    ensure_api_key_ready(&app, &settings.api_key).await?;

    let now = Utc::now();
    let due_sources = db::list_due_sources(&conn, now)?;
    let pending_before = db::pending_articles_count(&conn)?;
    diagnostics::log(
        &app,
        "fetch",
        format!(
            "due sources resolved: {} pending_queue_before={}",
            due_sources.len(),
            pending_before
        ),
    );
    if due_sources.is_empty() && pending_before == 0 {
        diagnostics::log(&app, "fetch", "cycle finished early: no due sources");
        let memory = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)?;
        if memory.is_some() {
            clear_last_error(&app);
        }
        set_scanning(&app, false);
        set_last_scan_at(&app, Some(now));
        db::write_last_scan_at(&conn, Some(now))?;
        sync_windows(&app, false)?;
        return Ok(());
    }

    let mut fetch_elapsed = std::time::Duration::ZERO;
    let mut fetch_errors = Vec::new();
    let mut fetched_source_stage_counts = Vec::<(String, usize)>::new();

    if !due_sources.is_empty() {
        let fetch_started_at = Instant::now();
        let fetch_outcome = rss::fetch_sources(&due_sources)
            .await
            .context("RSS fetch/parse failed")?;
        fetch_elapsed = fetch_started_at.elapsed();

        for result in fetch_outcome.results {
            if let Some(error) = result.error {
                db::update_fetch_state(&conn, &result.source.id, now, None, Some(&error))?;
                fetch_errors.push(format!("{}: {}", result.source.name, error));
                continue;
            }

            let filtered = collect_pending_articles(&conn, result.articles)?;
            let mut staged_count = 0usize;
            for article in filtered {
                let article_key = build_article_key(&article);
                if db::insert_pending_article(&conn, &article, &article_key, now)? {
                    staged_count += 1;
                }
            }
            fetched_source_stage_counts.push((result.source.id, staged_count));
        }
    }

    let pending_articles = db::list_pending_articles(&conn)?;
    diagnostics::log(
        &app,
        "fetch",
        format!(
            "rss phase complete: due_sources={} pending_articles={} feed_errors={}",
            due_sources.len(),
            pending_articles.len(),
            fetch_errors.len()
        ),
    );

    if pending_articles.is_empty() {
        for (source_id, _) in fetched_source_stage_counts {
            db::update_fetch_state(&conn, &source_id, now, Some(now), None)?;
        }

        let memory = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)?;
        set_scanning(&app, false);
        set_last_scan_at(&app, Some(now));
        db::write_last_scan_at(&conn, Some(now))?;
        clear_last_error(&app);
        if memory.is_none() && settings.memory_mode_enabled {
            diagnostics::log(&app, "fetch", "warning: interest memory has not been generated yet");
            set_last_error(
                &app,
                "Warning: interest memory has not been generated yet".to_string(),
            );
        }
        sync_windows(&app, false)?;
        return Ok(());
    }

    let interest_context = build_interest_context(&settings);
    let score_started_at = Instant::now();
    let pending_outcome =
        process_pending_articles(&app, &settings.api_key, &interest_context, &pending_articles)
            .await?;
    let score_elapsed = score_started_at.elapsed();
    let reminder_candidates = select_partition_top_candidates(pending_outcome.reminder_candidates);
    if !reminder_candidates.is_empty() {
        let batch_id = match db::current_active_batch_for_updates(&conn)? {
            Some(batch_id) => batch_id,
            None => db::create_reminder_batch(&conn)?,
        };
        for article_id in &reminder_candidates {
            db::attach_article_to_batch(&conn, &batch_id, *article_id)?;
        }
    }

    for (source_id, staged_count) in fetched_source_stage_counts {
        if staged_count == 0 || db::count_pending_articles_for_source(&conn, &source_id)? == 0 {
            db::update_fetch_state(&conn, &source_id, now, Some(now), None)?;
        }
    }

    let memory = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)?;
    set_scanning(&app, false);
    let finished_at = Utc::now();
    set_last_scan_at(&app, Some(finished_at));
    db::write_last_scan_at(&conn, Some(finished_at))?;

    let mut warnings = collect_cycle_warnings(fetch_errors, pending_outcome.errors);
    if pending_outcome.remaining_pending > 0 {
        warnings.push(format!(
            "{} pending articles waiting for LLM scoring retry",
            pending_outcome.remaining_pending
        ));
    }
    diagnostics::log(
        &app,
        "fetch",
        format!(
            "cycle finished: inserted_articles={} reminder_candidates={} warnings={} rss_ms={} llm_ms={} total_ms={}",
            pending_outcome.inserted_count,
            reminder_candidates.len(),
            warnings.len(),
            fetch_elapsed.as_millis(),
            score_elapsed.as_millis(),
            started_at.elapsed().as_millis()
        ),
    );
    if warnings.is_empty() {
        clear_last_error(&app);
    } else {
        diagnostics::log(&app, "fetch", format!("warnings: {}", warnings.join(" | ")));
        set_last_error(&app, format!("Warning: {}", warnings.join(" | ")));
    }
    if memory.is_none() && settings.memory_mode_enabled {
        diagnostics::log(&app, "fetch", "warning: interest memory has not been generated yet");
        set_last_error(
            &app,
            "Warning: interest memory has not been generated yet".to_string(),
        );
    }
    sync_windows(&app, false)?;

    eprintln!(
        "briefy-pet fetch cycle ok: due_sources={} pending={} inserted={} rss_ms={} llm_ms={} total_ms={}",
        due_sources.len(),
        pending_articles.len(),
        pending_outcome.inserted_count,
        fetch_elapsed.as_millis(),
        score_elapsed.as_millis(),
        started_at.elapsed().as_millis(),
    );

    Ok(())
}

async fn ensure_api_key_ready(app: &AppHandle, api_key: &str) -> Result<()> {
    let cached = app
        .state::<AppState>()
        .api_key_valid
        .lock()
        .ok()
        .and_then(|value| *value);

    if cached == Some(true) {
        return Ok(());
    }

    if let Err(err) = llm::validate_api_key(api_key).await {
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
) -> Result<Vec<FeedArticle>> {
    let mut pending = Vec::new();
    let mut seen_keys = HashSet::new();
    for article in articles {
        let article_key = build_article_key(&article);
        let guid = effective_guid(&article, &article_key);
        if !seen_keys.insert(article_key.clone()) {
            continue;
        }
        if !db::article_or_pending_exists_by_identity(
            conn,
            &article.source_id,
            &article_key,
            &article.normalized_link,
            &guid,
        )? {
            pending.push(article);
        }
    }
    Ok(pending)
}

async fn process_pending_articles(
    app: &AppHandle,
    api_key: &str,
    interest_context: &str,
    pending_articles: &[PendingArticleRecord],
) -> Result<PendingProcessingOutcome> {
    if pending_articles.is_empty() {
        return Ok(PendingProcessingOutcome {
            inserted_count: 0,
            reminder_candidates: Vec::new(),
            errors: Vec::new(),
            remaining_pending: 0,
        });
    }

    let mut inserted_count = 0usize;
    let mut errors = Vec::new();
    let mut reminder_candidates = Vec::new();

    for group in pending_articles
        .chunks(SCORE_BATCH_SIZE * MAX_CONCURRENT_SCORE_BATCHES)
    {
        let articles = group
            .iter()
            .map(|item| item.article.clone())
            .collect::<Vec<_>>();
        let score_outcome = score_articles_in_batches(api_key, interest_context, &articles).await?;
        errors.extend(score_outcome.errors);
        let conn = db::connect(app)?;

        let mut pending_lookup = BTreeMap::new();
        for item in group {
            pending_lookup.insert(
                pending_identity(&item.article.source_id, &item.article_key),
                item,
            );
        }

        let mut consumed_ids = Vec::new();
        for (article, analysis) in score_outcome.scored_articles {
            let article_key = build_article_key(&article);
            let guid = effective_guid(&article, &article_key);
            let identity = pending_identity(&article.source_id, &article_key);
            let Some(pending_item) = pending_lookup.get(&identity) else {
                errors.push(format!(
                    "Pending article lookup failed for {} ({})",
                    article.title, article.source_id
                ));
                continue;
            };

            if db::find_article_id_by_identity(
                &conn,
                &article.source_id,
                &article_key,
                &article.normalized_link,
                &guid,
            )?
            .is_some()
            {
                consumed_ids.push(pending_item.id);
                continue;
            }

            let calibrated_fit_level = policy::fit_level_for_score(
                &article.module,
                &article.bucket,
                &article.source_kind,
                analysis.fit_score,
            );
            let article_id = db::insert_article(
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
                pending_item.fetched_at,
                &article.content,
                &analysis.summary,
                &calibrated_fit_level,
                analysis.fit_score,
                &analysis.recommendation_reason,
            )?;
            db::upsert_content_pool_entry(
                &conn,
                article_id,
                &article.source_kind,
                analysis.fit_score,
                article.published_at,
            )?;
            inserted_count += 1;
            consumed_ids.push(pending_item.id);

            if calibrated_fit_level == FitLevel::High {
                reminder_candidates.push((
                    article_id,
                    article.module,
                    article.bucket,
                    article.source_kind,
                    analysis.fit_score,
                    article.published_at,
                ));
            }
        }

        db::delete_pending_articles(&conn, &consumed_ids)?;
    }

    let conn = db::connect(app)?;
    Ok(PendingProcessingOutcome {
        inserted_count,
        reminder_candidates,
        errors,
        remaining_pending: db::pending_articles_count(&conn)?,
    })
}

async fn score_articles_in_batches(
    api_key: &str,
    interest_context: &str,
    articles: &[FeedArticle],
) -> Result<ScoreOutcome> {
    if articles.is_empty() {
        return Ok(ScoreOutcome {
            scored_articles: Vec::new(),
            errors: Vec::new(),
        });
    }

    let mut scored = Vec::new();
    let mut errors = Vec::new();

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
            async move {
                match llm::summarize_and_score_batch(api_key, &interest_context, &batch).await {
                    Ok(analyses) => Ok::<BatchScoreOutcome, anyhow::Error>(BatchScoreOutcome {
                        scored_articles: batch.into_iter().zip(analyses.into_iter()).collect(),
                        errors: Vec::new(),
                    }),
                    Err(batch_err) => {
                        eprintln!(
                            "briefy-pet batch scoring failed, falling back to per-article scoring: {}",
                            batch_err
                        );

                        let mut recovered = Vec::new();
                        let mut fallback_errors = Vec::new();
                        for article in batch {
                            match llm::summarize_and_score_single(api_key, &interest_context, &article).await {
                                Ok(analysis) => recovered.push((article, analysis)),
                                Err(err) => {
                                    let message = format!("LLM scoring failed: {} ({})", article.title, err);
                                    eprintln!("briefy-pet article scoring failed: {message}");
                                    fallback_errors.push(message);
                                }
                            }
                        }

                        Ok(BatchScoreOutcome {
                            scored_articles: recovered,
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
                    scored.append(&mut batch_result.scored_articles);
                    errors.append(&mut batch_result.errors);
                }
                Err(err) => errors.push(err.to_string()),
            }
        }
    }

    Ok(ScoreOutcome {
        scored_articles: scored,
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

fn select_partition_top_candidates(
    candidates: Vec<(i64, String, String, SourceKind, i64, Option<DateTime<Utc>>)>,
) -> Vec<i64> {
    let mut partitions =
        BTreeMap::<(String, String, SourceKind), Vec<(i64, i64, Option<DateTime<Utc>>)>>::new();
    for (article_id, module, bucket, source_kind, fit_score, published_at) in candidates {
        partitions
            .entry((module, bucket, source_kind))
            .or_default()
            .push((article_id, fit_score, published_at));
    }

    let mut selected = Vec::new();
    for ((module, bucket, source_kind), mut items) in partitions {
        items.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| right.2.cmp(&left.2))
                .then_with(|| right.0.cmp(&left.0))
        });

        let take_n = policy::reminder_take_for_source(&module, &bucket, &source_kind).max(1);
        selected.extend(items.into_iter().take(take_n));
    }

    selected.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| right.0.cmp(&left.0))
    });
    selected
        .into_iter()
        .take(MAX_REMINDER_ITEMS_PER_BATCH)
        .map(|item| item.0)
        .collect()
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

    if !hard_fetch_errors.is_empty() {
        warnings.push(format!("some feeds failed: {}", hard_fetch_errors.join(" ; ")));
    }
    if !score_errors.is_empty() {
        warnings.push(format!(
            "some articles failed scoring: {}",
            score_errors.join(" ; ")
        ));
    }
    warnings
}

fn is_soft_feed_error(value: &str) -> bool {
    value.contains("HTTP 403") || value.contains("HTTP 404")
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
        }
        set_last_error(app, message.clone());
        let _ = sync_windows(app, false);
        diagnostics::log(app, "fetch", format!("cycle failed: {message}"));
        eprintln!("briefy-pet fetch cycle failed: {message}");
    }
}

fn pending_identity(source_id: &str, article_key: &str) -> String {
    format!("{source_id}::{article_key}")
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
        .and_then(|value| *value);
    let view = if !is_settings_complete(&settings)
        || settings.api_key.trim().is_empty()
        || api_key_valid == Some(false)
    {
        AppView::Settings
    } else {
        AppView::Reading
    };
    open_main(app, view)
}

pub fn handle_bubble_action(app: &AppHandle, action: &str) -> Result<Snapshot> {
    let conn = db::connect(app)?;
    if let Some(batch) = db::active_reminder_batch(&conn)? {
        match action {
            "view" => {
                db::set_batch_status(&conn, &batch.id, "opened", None)?;
                db::write_active_view(&conn, &AppView::Reading)?;
                db::log_user_event(&conn, "bubble-view", batch.top_article_id, None, None)?;
                if let Some(top_article_id) = batch.top_article_id {
                    db::mark_article_opened(&conn, top_article_id)?;
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
                let remind_at = (Utc::now() + Duration::minutes(30)).to_rfc3339();
                db::set_batch_status(&conn, &batch.id, "snoozed", Some(remind_at))?;
                db::log_user_event(&conn, "bubble-snooze", None, None, None)?;
            }
            "ignore" => {
                db::set_batch_status(&conn, &batch.id, "ignored", None)?;
                db::log_user_event(&conn, "bubble-ignore", None, None, None)?;
            }
            _ => {}
        }
    }

    let settings = db::read_settings(&conn)?;
    let _ = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)?;
    sync_windows(app, false)?;
    snapshot(app, false)
}

pub fn sync_windows(app: &AppHandle, is_scanning: bool) -> Result<()> {
    let current = snapshot(app, is_scanning)?;
    if let Some(window) = app.get_window("bubble") {
        if current.active_reminder.is_some() {
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
    }
    if let Some(window) = app.get_window("pet") {
        let _ = window.show();
    }
    Ok(())
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

fn set_scanning(app: &AppHandle, value: bool) {
    if let Ok(mut scanning) = app.state::<AppState>().is_scanning.lock() {
        *scanning = value;
    }
}

struct ScoreOutcome {
    scored_articles: Vec<(FeedArticle, LlmResult)>,
    errors: Vec<String>,
}

struct BatchScoreOutcome {
    scored_articles: Vec<(FeedArticle, LlmResult)>,
    errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::{classify_error_for_display, derive_pet_status, is_settings_complete};
    use crate::models::{
        Discipline, PetStatus, RssSource, SettingsPayload, SourceKind, UserDisciplinePreference,
    };

    fn sample_settings() -> SettingsPayload {
        SettingsPayload {
            api_key: "demo-key".into(),
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
                origin_files: vec!["reference/demo.opml".into()],
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
