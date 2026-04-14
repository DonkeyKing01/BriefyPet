use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub enum Discipline {
    Technology,
    Humanities,
    News,
    SocialScience,
    Science,
    Medicine,
    Life,
    Other,
}

impl Discipline {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Technology => "technology",
            Self::Humanities => "humanities",
            Self::News => "news",
            Self::SocialScience => "social-science",
            Self::Science => "science",
            Self::Medicine => "medicine",
            Self::Life => "life",
            Self::Other => "other",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Technology => "科技",
            Self::Humanities => "娱乐",
            Self::News => "新闻观点",
            Self::SocialScience => "社科",
            Self::Science => "科学",
            Self::Medicine => "医学",
            Self::Life => "成长",
            Self::Other => "商业",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    AcademicJournal,
    OfficialAnnouncement,
    TechnicalBlog,
    CommunityHotspot,
}

impl SourceKind {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AcademicJournal => "academic-journal",
            Self::OfficialAnnouncement => "official-announcement",
            Self::TechnicalBlog => "technical-blog",
            Self::CommunityHotspot => "community-hotspot",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Article,
    Podcast,
    Video,
    Twitter,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDisciplinePreference {
    pub discipline: Discipline,
    pub enabled: bool,
    pub preference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RssSource {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub bucket: String,
    pub discipline: Discipline,
    pub source_kind: SourceKind,
    pub resource_type: ResourceType,
    pub language: Option<String>,
    pub enabled: bool,
    pub enabled_by_default: bool,
    pub postponed: bool,
    pub origin_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPayload {
    pub api_key: String,
    #[serde(default = "default_llm_provider")]
    pub llm_provider: String,
    #[serde(default)]
    pub llm_model: String,
    #[serde(default)]
    pub provider_api_keys: BTreeMap<String, String>,
    pub auto_start: bool,
    pub disciplines: Vec<UserDisciplinePreference>,
    pub memory_mode_enabled: bool,
    pub memory_summary: String,
    pub rss_sources: Vec<RssSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleRecord {
    pub id: i64,
    pub source_id: String,
    pub title: String,
    pub link: String,
    pub source_name: String,
    pub discipline: Discipline,
    pub source_kind: SourceKind,
    pub resource_type: ResourceType,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub summary: String,
    pub fit_level: FitLevel,
    pub fit_score: i64,
    pub recommendation_reason: String,
    pub raw_content: String,
    pub note: String,
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
    pub partition_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: i64,
    pub title: String,
    pub link: String,
    pub source_id: String,
    pub source_name: String,
    pub module: String,
    pub bucket: String,
    pub published_at: Option<String>,
    pub summary: String,
    pub fit_score: i64,
    pub fit_level: String,
    pub recommendation_reason: String,
    pub note: String,
    pub is_favorite: bool,
    pub batch_id: String,
    pub batch_created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPoolStat {
    pub source_kind: SourceKind,
    pub total_articles: usize,
    pub candidate_count: usize,
    pub top_score: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterestMemoryRecord {
    pub day_key: String,
    pub generated_summary: String,
    pub summary: String,
    pub memory_mode_enabled: bool,
    pub event_count: usize,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCatalogSummary {
    pub total_sources: usize,
    pub enabled_sources: usize,
    pub postponed_sources: usize,
    pub selected_disciplines: usize,
    pub due_sources: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: SettingsPayload,
    pub pet_status: PetStatus,
    pub articles: Vec<ArticleRecord>,
    pub active_reminder: Option<ReminderBatchSnapshot>,
    pub history_articles: Vec<HistoryItem>,
    pub selected_article_id: Option<i64>,
    pub active_view: AppView,
    pub last_error: Option<String>,
    pub api_key_valid: bool,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub content_pool_stats: Vec<ContentPoolStat>,
    pub memory: Option<InterestMemoryRecord>,
    pub source_summary: SourceCatalogSummary,
}

#[derive(Debug, Clone)]
pub struct FeedArticle {
    pub source_id: String,
    pub source_name: String,
    pub module: String,
    pub bucket: String,
    pub discipline: Discipline,
    pub source_kind: SourceKind,
    pub resource_type: ResourceType,
    pub title: String,
    pub link: String,
    pub normalized_link: String,
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

pub fn default_llm_provider() -> String {
    "deepseek".to_string()
}

pub fn all_disciplines() -> Vec<Discipline> {
    vec![
        Discipline::Technology,
        Discipline::SocialScience,
        Discipline::Other,
        Discipline::Life,
        Discipline::News,
        Discipline::Humanities,
        Discipline::Science,
        Discipline::Medicine,
    ]
}
