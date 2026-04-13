use std::io::Cursor;

use anyhow::{anyhow, Context, Result};
use atom_syndication::Feed;
use chrono::{DateTime, Utc};
use futures::future::join_all;
use reqwest::{
    header::{ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL},
    Client, RequestBuilder, StatusCode,
};
use rss::Channel;
use tokio::time::{sleep, Duration};

use crate::models::{FeedArticle, RssSource};

const MAX_ITEMS_PER_SOURCE: usize = 12;
const FEED_FETCH_RETRIES: usize = 3;
const FEED_REQUEST_TIMEOUT_SECS: u64 = 20;
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
    let results = join_all(
        sources
            .iter()
            .cloned()
            .map(|source| fetch_single_source(client.clone(), source)),
    )
    .await;

    Ok(FeedFetchOutcome { results })
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
                    sleep(Duration::from_millis(700 * attempt as u64)).await;
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
    let mut requested_url = source.url.clone();
    let mut response = send_feed_request(client.get(&requested_url), &requested_url)
        .await
        .with_context(|| format!("request failed for {}", source.url))?;

    if response.status() == StatusCode::FORBIDDEN {
        if let Some(fallback_url) = reddit_fallback_url(&source.url) {
            let fallback_response = send_feed_request(client.get(&fallback_url), &fallback_url)
                .await
                .with_context(|| format!("request failed for reddit fallback {}", fallback_url))?;

            if fallback_response.status().is_success() {
                requested_url = fallback_url;
                response = fallback_response;
            } else {
                return Err(anyhow!(
                    "{} ({}) returned HTTP {}; fallback {} returned HTTP {}",
                    source.name,
                    source.url,
                    StatusCode::FORBIDDEN,
                    fallback_url,
                    fallback_response.status()
                ));
            }
        }
    }

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

fn reddit_fallback_url(url: &str) -> Option<String> {
    if url.contains("://www.reddit.com/") {
        return Some(url.replacen("://www.reddit.com/", "://old.reddit.com/", 1));
    }
    if url.contains("://reddit.com/") {
        return Some(url.replacen("://reddit.com/", "://old.reddit.com/", 1));
    }
    None
}

fn is_retryable_feed_error(err: &anyhow::Error) -> bool {
    let text = err.to_string();
    text.contains("HTTP 429")
        || text.contains("HTTP 502")
        || text.contains("HTTP 503")
        || text.contains("HTTP 504")
        || text.contains("request failed")
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
            .map(|guid| guid.value().to_string())
            .unwrap_or_else(|| format!("{}::{link}", source.id));
        let published_at = item
            .pub_date()
            .and_then(parse_datetime)
            .map(|value| value.with_timezone(&Utc));
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
        let published_at = entry
            .published()
            .map(|value| value.with_timezone(&Utc))
            .or_else(|| Some(entry.updated().with_timezone(&Utc)));
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

fn parse_datetime(value: &str) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_rfc2822(value)
        .ok()
        .or_else(|| DateTime::parse_from_rfc3339(value).ok())
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
