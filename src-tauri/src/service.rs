use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use futures::future::join_all;
use std::collections::HashSet;
use std::time::Instant;
use tauri::{AppHandle, Manager};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::{
    db, llm,
    models::{AppView, FeedArticle, LlmResult, PetStatus, SettingsPayload, Snapshot},
    rss, AppState,
};

const SCORE_BATCH_SIZE: usize = 2;
const MAX_CONCURRENT_SCORE_BATCHES: usize = 2;
const MAX_ARTICLES_TO_KEEP_PER_CYCLE: usize = 5;

pub fn derive_pet_status(
    settings: &SettingsPayload,
    api_key_valid: Option<bool>,
    has_active_reminder: bool,
    is_scanning: bool,
    is_loading: bool,
) -> PetStatus {
    if is_loading {
        PetStatus::Loading
    } else if settings.api_key.trim().is_empty() || api_key_valid == Some(false) {
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
    tauri::async_runtime::spawn(async move {
        loop {
            sleep(TokioDuration::from_secs(3 * 60 * 60)).await;
            log_fetch_result(run_fetch_cycle(app_handle.clone()).await, &app_handle);
        }
    });
}

pub fn trigger_fetch_now(app: &AppHandle, delay: Option<std::time::Duration>) {
    let app_handle = app.clone();
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
    if settings.api_key.trim().is_empty() {
        set_api_key_valid(&app, Some(false));
        let conn = db::connect(&app)?;
        db::write_api_key_valid(&conn, false)?;
        clear_last_error(&app);
        set_scanning(&app, false);
        sync_windows(&app, false)?;
        return Ok(());
    }

    ensure_api_key_ready(&app, &settings.api_key).await?;

    let fetch_started_at = Instant::now();
    let fetch_outcome = rss::fetch_enabled_sources(&settings.rss_sources)
        .await
        .context("RSS fetch/parse failed")?;
    let fetch_elapsed = fetch_started_at.elapsed();

    let conn = db::connect(&app)?;
    let pending_articles = collect_pending_articles(&conn, fetch_outcome.articles)?;
    let batch_count = pending_articles.len().div_ceil(SCORE_BATCH_SIZE);

    let score_started_at = Instant::now();
    let score_outcome = score_articles_in_batches(
        &settings.api_key,
        &settings.interest_profile,
        &pending_articles,
    )
    .await?;
    let score_elapsed = score_started_at.elapsed();

    let mut inserted_count = 0usize;
    let mut selected_article_ids = Vec::new();

    let fetched_at = Utc::now();
    let mut ranked_articles = score_outcome.scored_articles;
    ranked_articles.sort_by(|left, right| {
        right
            .1
            .fit_score
            .cmp(&left.1.fit_score)
            .then_with(|| left.0.title.cmp(&right.0.title))
    });

    for (article, analysis) in ranked_articles
        .into_iter()
        .take(MAX_ARTICLES_TO_KEEP_PER_CYCLE)
    {
        let article_id = db::insert_article(
            &conn,
            &article.guid,
            &article.title,
            &article.link,
            &article.source_name,
            article.published_at,
            fetched_at,
            &article.content,
            &analysis.summary,
            &analysis.fit_level,
            analysis.fit_score,
            &analysis.recommendation_reason,
        )?;
        inserted_count += 1;
        selected_article_ids.push(article_id);
    }

    if !selected_article_ids.is_empty() {
        let batch_id = db::create_reminder_batch(&conn)?;
        for article_id in selected_article_ids {
            db::attach_article_to_batch(&conn, &batch_id, article_id)?;
        }
    }

    set_scanning(&app, false);
    set_last_scan_at(&app, Some(fetched_at));
    db::write_last_scan_at(&conn, Some(fetched_at))?;
    let warnings = collect_cycle_warnings(fetch_outcome.errors, score_outcome.errors);
    if warnings.is_empty() {
        clear_last_error(&app);
    } else {
        set_last_error(&app, format!("Warning: {}", warnings.join(" | ")));
    }
    sync_windows(&app, false)?;

    eprintln!(
        "briefy-pet fetch cycle ok: fetched={} pending={} inserted={} rss_ms={} llm_ms={} total_ms={} batch_size={} batches={}",
        pending_articles.len(),
        pending_articles.len(),
        inserted_count,
        fetch_elapsed.as_millis(),
        score_elapsed.as_millis(),
        started_at.elapsed().as_millis(),
        SCORE_BATCH_SIZE,
        batch_count
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
            return Err(anyhow!("API Key validation failed: cached API Key is invalid"));
        }

        return Err(err).context("API Key validation failed");
    }
    set_api_key_valid(app, Some(true));
    let conn = db::connect(app)?;
    db::write_api_key_valid(&conn, true)?;
    Ok(())
}

fn collect_pending_articles(conn: &rusqlite::Connection, articles: Vec<FeedArticle>) -> Result<Vec<FeedArticle>> {
    let mut pending = Vec::new();
    let mut seen_guids = HashSet::new();
    for article in articles {
        if !seen_guids.insert(article.guid.clone()) {
            continue;
        }
        if db::find_article_id_by_guid(conn, &article.guid)?.is_none() {
            pending.push(article);
        }
    }
    Ok(pending)
}

async fn score_articles_in_batches(
    api_key: &str,
    interest_profile: &str,
    articles: &[FeedArticle],
) -> Result<ScoreOutcome> {
    let mut scored = Vec::new();
    let mut errors = Vec::new();

    for group in articles
        .chunks(SCORE_BATCH_SIZE * MAX_CONCURRENT_SCORE_BATCHES)
        .map(|chunk| chunk.chunks(SCORE_BATCH_SIZE).map(|inner| inner.to_vec()).collect::<Vec<_>>())
    {
        let results = join_all(group.iter().map(|batch| {
            let batch = batch.clone();
            async move {
                match llm::summarize_and_score_batch(api_key, interest_profile, &batch).await {
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
                            match llm::summarize_and_score_single(
                                api_key,
                                interest_profile,
                                &article,
                            )
                            .await
                            {
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

fn collect_cycle_warnings(fetch_errors: Vec<String>, score_errors: Vec<String>) -> Vec<String> {
    let mut warnings = Vec::new();
    if !fetch_errors.is_empty() {
        warnings.push(format!("some feeds failed: {}", fetch_errors.join(" ; ")));
    }
    if !score_errors.is_empty() {
        warnings.push(format!("some articles failed scoring: {}", score_errors.join(" ; ")));
    }
    warnings
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
        .and_then(|value| *value);
    let view = if settings.api_key.trim().is_empty() || api_key_valid == Some(false) {
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
                if let Some(top_article_id) = batch.top_article_id {
                    db::mark_article_opened(&conn, top_article_id)?;
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
            }
            "ignore" => {
                db::set_batch_status(&conn, &batch.id, "ignored", None)?;
            }
            _ => {}
        }
    }

    sync_windows(app, false)?;
    snapshot(app, false)
}

pub fn sync_windows(app: &AppHandle, is_scanning: bool) -> Result<()> {
    let _ = snapshot(app, is_scanning)?;
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

    use super::{classify_error_for_display, derive_pet_status};
    use crate::models::{PetStatus, SettingsPayload};

    #[test]
    fn classifies_api_key_errors() {
        let err = anyhow!("API Key validation failed: api key validation failed: unauthorized");
        let message = classify_error_for_display(&err);
        assert!(message.starts_with("API Key validation failed:"));
    }

    #[test]
    fn classifies_rss_errors() {
        let err = anyhow!("RSS fetch/parse failed: all enabled feeds failed");
        let message = classify_error_for_display(&err);
        assert!(message.starts_with("RSS fetch/parse failed:"));
    }

    #[test]
    fn classifies_llm_errors() {
        let err = anyhow!("LLM scoring failed: Example Article");
        let message = classify_error_for_display(&err);
        assert!(message.starts_with("LLM scoring failed:"));
    }

    #[test]
    fn scanning_takes_priority_over_new_info() {
        let settings = SettingsPayload {
            api_key: "demo-key".into(),
            interest_profile: String::new(),
            auto_start: false,
            rss_sources: Vec::new(),
        };
        let status = derive_pet_status(&settings, Some(true), true, true);
        assert_eq!(status, PetStatus::Scanning);
    }
}
