use std::io::Cursor;

use anyhow::{anyhow, Context, Result};
use atom_syndication::Feed;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use futures::{stream, StreamExt};
use reqwest::{
    header::{ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL},
    Client, RequestBuilder,
};
use rss::Channel;
use tokio::time::{sleep, Duration};

use crate::models::{FeedArticle, RssSource};

const MAX_ITEMS_PER_SOURCE: usize = 12;
const FEED_FETCH_RETRIES: usize = 3;
const FEED_REQUEST_TIMEOUT_SECS: u64 = 20;
const MAX_CONCURRENT_SOURCE_FETCHES: usize = 6;
const SECOND_PASS_RETRY_DELAY_SECS: u64 = 15;
const RETRY_BACKOFF_BASE_MS: u64 = 900;
const FEED_USER_AGENT: &str =
    "Briefy-pet/0.1 (+https://briefy-pet.local; rss-fetcher; contact: rss@briefy-pet.local)";

#[derive(Debug, Clone)]
pub struct SourceFetchResult {
    pub source: RssSource,
    pub articles: Vec<FeedArticle>,
    pub error: Option<String>,
}

pub struct FeedFetchOutcome {
    pub results: Vec<SourceFetchResult>,
}

pub async fn fetch_sources(sources: &[RssSource]) -> Result<FeedFetchOutcome> {
    let client = Client::builder()
        .user_agent(FEED_USER_AGENT)
        .timeout(Duration::from_secs(FEED_REQUEST_TIMEOUT_SECS))
        .build()
        .context("failed to build rss client")?;

    let mut results =
        fetch_sources_with_limit(&client, sources, MAX_CONCURRENT_SOURCE_FETCHES).await;
    let retry_sources = results
        .iter()
        .filter_map(|result| {
            result
                .error
                .as_deref()
                .filter(|error| is_retryable_feed_error_message(error))
                .map(|_| result.source.clone())
        })
        .collect::<Vec<_>>();

    if !retry_sources.is_empty() {
        sleep(Duration::from_secs(SECOND_PASS_RETRY_DELAY_SECS)).await;
        let retry_results = fetch_sources_with_limit(
            &client,
            &retry_sources,
            MAX_CONCURRENT_SOURCE_FETCHES.saturating_sub(2).max(1),
        )
        .await;

        let mut retry_map = retry_results
            .into_iter()
            .map(|result| (result.source.id.clone(), result))
            .collect::<std::collections::HashMap<_, _>>();

        for result in &mut results {
            if result.error.is_none() {
                continue;
            }
            if let Some(retried) = retry_map.remove(&result.source.id) {
                *result = retried;
            }
        }
    }

    Ok(FeedFetchOutcome { results })
}

async fn fetch_sources_with_limit(
    client: &Client,
    sources: &[RssSource],
    limit: usize,
) -> Vec<SourceFetchResult> {
    let mut indexed_results =
        stream::iter(sources.iter().cloned().enumerate().map(|(idx, source)| {
            let client = client.clone();
            async move { (idx, fetch_single_source(client, source).await) }
        }))
        .buffer_unordered(limit.max(1))
        .collect::<Vec<_>>()
        .await;

    indexed_results.sort_by_key(|(idx, _)| *idx);
    indexed_results
        .into_iter()
        .map(|(_, result)| result)
        .collect()
}

async fn fetch_single_source(client: Client, source: RssSource) -> SourceFetchResult {
    let mut last_error = None;

    for attempt in 1..=FEED_FETCH_RETRIES {
        match fetch_single_source_once(&client, &source).await {
            Ok(articles) => {
                return SourceFetchResult {
                    source,
                    articles,
                    error: None,
                };
            }
            Err(err) => {
                let retryable = is_retryable_feed_error(&err);
                last_error = Some(err.to_string());
                if retryable && attempt < FEED_FETCH_RETRIES {
                    sleep(retry_backoff_delay(&source.id, attempt)).await;
                    continue;
                }
                break;
            }
        }
    }

    SourceFetchResult {
        source,
        articles: Vec::new(),
        error: last_error,
    }
}

async fn fetch_single_source_once(client: &Client, source: &RssSource) -> Result<Vec<FeedArticle>> {
    let requested_url = source.url.clone();
    let response = send_feed_request(client.get(&requested_url), &requested_url)
        .await
        .with_context(|| format!("request failed for {}", source.url))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "{} ({}) returned HTTP {}",
            source.name,
            source.url,
            response.status()
        ));
    }

    let content = response
        .bytes()
        .await
        .with_context(|| format!("failed to read body for {}", requested_url))?;

    match Channel::read_from(&content[..]) {
        Ok(channel) => Ok(parse_rss_items(source, &channel)),
        Err(rss_err) => match Feed::read_from(Cursor::new(content.as_ref())) {
            Ok(feed) => Ok(parse_atom_entries(source, &feed)),
            Err(atom_err) => Err(anyhow!(
                "{} ({}) is neither parseable RSS nor Atom. rss_error={rss_err}; atom_error={atom_err}",
                source.name,
                source.url
            )),
        },
    }
}

async fn send_feed_request(
    builder: RequestBuilder,
    url: &str,
) -> reqwest::Result<reqwest::Response> {
    let mut builder = builder
        .header(
            ACCEPT,
            "application/rss+xml, application/atom+xml, application/xml;q=0.9, text/xml;q=0.8, */*;q=0.6",
        )
        .header(ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
        .header(CACHE_CONTROL, "no-cache");

    if let Ok(parsed) = reqwest::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let referer = format!("{}://{}/", parsed.scheme(), host);
            builder = builder.header("Referer", referer);
        }
    }

    builder.send().await
}

fn is_retryable_feed_error(err: &anyhow::Error) -> bool {
    is_retryable_feed_error_message(&err.to_string())
}

fn is_retryable_feed_error_message(text: &str) -> bool {
    text.contains("HTTP 408")
        || text.contains("HTTP 425")
        || text.contains("HTTP 429")
        || text.contains("HTTP 500")
        || text.contains("HTTP 502")
        || text.contains("HTTP 503")
        || text.contains("HTTP 504")
        || text.contains("request failed")
        || text.contains("timed out")
        || text.contains("connection reset")
}

fn retry_backoff_delay(source_id: &str, attempt: usize) -> Duration {
    let exponent = (attempt.saturating_sub(1) as u32).min(4);
    let base = RETRY_BACKOFF_BASE_MS.saturating_mul(2u64.pow(exponent));
    let jitter_seed = source_id
        .as_bytes()
        .iter()
        .fold(0u64, |acc, b| acc.wrapping_add(*b as u64));
    let jitter = (jitter_seed % 400) + (attempt as u64 * 60);
    Duration::from_millis(base.saturating_add(jitter))
}

fn parse_rss_items(source: &RssSource, channel: &Channel) -> Vec<FeedArticle> {
    let mut articles = Vec::new();

    for item in channel.items().iter().take(MAX_ITEMS_PER_SOURCE) {
        let title = item
            .title()
            .unwrap_or("Untitled content")
            .trim()
            .to_string();
        let link = item.link().unwrap_or_default().trim().to_string();
        if link.is_empty() {
            continue;
        }

        let guid = item
            .guid()
            .map(|guid| guid.value().trim().to_string())
            .filter(|guid| !guid.is_empty())
            .unwrap_or_else(|| format!("{}::{link}", source.id));
        let published_at = item.pub_date().and_then(parse_datetime);
        let content = item
            .content()
            .or_else(|| item.description())
            .unwrap_or(&title)
            .to_string();

        articles.push(FeedArticle {
            source_id: source.id.clone(),
            source_name: source.name.clone(),
            module: source.module.clone(),
            bucket: source.bucket.clone(),
            group: source.group.clone(),
            discipline: source.discipline.clone(),
            source_kind: source.source_kind.clone(),
            resource_type: source.resource_type.clone(),
            title,
            link: link.clone(),
            normalized_link: normalize_url(&link),
            guid,
            published_at,
            content,
        });
    }

    articles
}

fn parse_atom_entries(source: &RssSource, feed: &Feed) -> Vec<FeedArticle> {
    let mut articles = Vec::new();

    for entry in feed.entries().iter().take(MAX_ITEMS_PER_SOURCE) {
        let title = entry.title().trim().to_string();
        let link = entry
            .links()
            .iter()
            .find(|link| link.rel() == "alternate" || link.rel().is_empty())
            .or_else(|| entry.links().first())
            .map(|link| link.href().to_string())
            .unwrap_or_default();
        if link.is_empty() {
            continue;
        }

        let guid = if entry.id().is_empty() {
            format!("{}::{link}", source.id)
        } else {
            entry.id().to_string()
        };
        let published_at = entry.published().map(|value| value.with_timezone(&Utc));
        let content = entry
            .content()
            .and_then(|content| content.value())
            .or_else(|| entry.summary().map(|summary| summary.as_str()))
            .unwrap_or(title.as_str())
            .to_string();

        articles.push(FeedArticle {
            source_id: source.id.clone(),
            source_name: source.name.clone(),
            module: source.module.clone(),
            bucket: source.bucket.clone(),
            group: source.group.clone(),
            discipline: source.discipline.clone(),
            source_kind: source.source_kind.clone(),
            resource_type: source.resource_type.clone(),
            title,
            link: link.clone(),
            normalized_link: normalize_url(&link),
            guid,
            published_at,
            content,
        });
    }

    articles
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(parsed) = DateTime::parse_from_rfc2822(trimmed) {
        return Some(parsed.with_timezone(&Utc));
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.with_timezone(&Utc));
    }

    for pattern in [
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y.%m.%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(trimmed, pattern) {
            return Some(Utc.from_utc_datetime(&parsed));
        }
    }

    for pattern in ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"] {
        if let Ok(parsed) = NaiveDate::parse_from_str(trimmed, pattern) {
            return parsed
                .and_hms_opt(0, 0, 0)
                .map(|datetime| Utc.from_utc_datetime(&datetime));
        }
    }

    None
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
