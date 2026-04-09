use std::io::Cursor;

use anyhow::{anyhow, Context, Result};
use atom_syndication::Feed;
use chrono::{DateTime, Utc};
use futures::future::join_all;
use reqwest::Client;
use rss::Channel;
use tokio::time::{sleep, Duration};

use crate::models::{FeedArticle, RssSource};

const MAX_ITEMS_PER_SOURCE: usize = 3;
const FEED_FETCH_RETRIES: usize = 3;

pub struct FeedFetchOutcome {
    pub articles: Vec<FeedArticle>,
    pub errors: Vec<String>,
}

pub async fn fetch_enabled_sources(sources: &[RssSource]) -> Result<FeedFetchOutcome> {
    let client = Client::new();
    let enabled_sources = sources
        .iter()
        .filter(|source| source.enabled)
        .cloned()
        .collect::<Vec<_>>();

    let results = join_all(
        enabled_sources
            .into_iter()
            .map(|source| fetch_single_source(client.clone(), source)),
    )
    .await;

    let mut articles = Vec::new();
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok(mut source_articles) => articles.append(&mut source_articles),
            Err(err) => errors.push(err.to_string()),
        }
    }

    if articles.is_empty() && !errors.is_empty() {
        return Err(anyhow!("all enabled feeds failed: {}", errors.join(" | ")));
    }

    Ok(FeedFetchOutcome { articles, errors })
}

async fn fetch_single_source(client: Client, source: RssSource) -> Result<Vec<FeedArticle>> {
    let mut last_error = None;

    for attempt in 1..=FEED_FETCH_RETRIES {
        match fetch_single_source_once(&client, &source).await {
            Ok(articles) => return Ok(articles),
            Err(err) => {
                let retryable = is_retryable_feed_error(&err);
                last_error = Some(err);
                if retryable && attempt < FEED_FETCH_RETRIES {
                    sleep(Duration::from_millis(600 * attempt as u64)).await;
                    continue;
                }
                break;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("unknown feed fetch failure for {}", source.url)))
}

async fn fetch_single_source_once(client: &Client, source: &RssSource) -> Result<Vec<FeedArticle>> {
    let response = client
        .get(&source.url)
        .send()
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
        .with_context(|| format!("failed to read body for {}", source.url))?;

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
        let title = item.title().unwrap_or("Untitled content").to_string();
        let link = item.link().unwrap_or_default().to_string();
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
            source_name: source.name.clone(),
            title,
            link,
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
        let title = entry.title().to_string();
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
            source_name: source.name.clone(),
            title,
            link,
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
