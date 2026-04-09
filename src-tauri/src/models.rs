use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PetStatus {
    Loading,
    NeedsConfig,
    Scanning,
    Idle,
    NewInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FitLevel {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppView {
    Reading,
    Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RssSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPayload {
    pub api_key: String,
    pub interest_profile: String,
    pub auto_start: bool,
    pub rss_sources: Vec<RssSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleRecord {
    pub id: i64,
    pub title: String,
    pub link: String,
    pub source_name: String,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub summary: String,
    pub fit_level: FitLevel,
    pub fit_score: i64,
    pub recommendation_reason: String,
    pub is_favorite: bool,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderBatchSnapshot {
    pub id: String,
    pub article_ids: Vec<i64>,
    pub article_count: usize,
    pub top_article_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: SettingsPayload,
    pub pet_status: PetStatus,
    pub articles: Vec<ArticleRecord>,
    pub active_reminder: Option<ReminderBatchSnapshot>,
    pub selected_article_id: Option<i64>,
    pub active_view: AppView,
    pub last_error: Option<String>,
    pub api_key_valid: bool,
    pub last_scan_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct FeedArticle {
    pub source_name: String,
    pub title: String,
    pub link: String,
    pub guid: String,
    pub published_at: Option<DateTime<Utc>>,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LlmResult {
    pub summary: String,
    pub fit_level: FitLevel,
    pub fit_score: i64,
    pub recommendation_reason: String,
}
