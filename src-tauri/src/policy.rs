use chrono::Duration;
use std::collections::BTreeMap;

use crate::models::{FitLevel, SourceKind};

#[derive(Debug, Clone, Copy)]
pub struct SourceRuntimePolicy {
    pub fetch_interval: Duration,
    pub high_cutoff: i64,
    pub medium_cutoff: i64,
}

pub fn all_modules() -> &'static [&'static str] {
    &[
        "technology",
        "social_science",
        "business",
        "growth",
        "news_opinion",
        "entertainment",
        "science",
        "medicine",
        "other",
    ]
}

pub fn normalize_module(raw: &str) -> String {
    let value = raw.trim().to_ascii_lowercase().replace('-', "_");
    match value.as_str() {
        "technology" => "technology".to_string(),
        "social_science" | "socialscience" => "social_science".to_string(),
        "business" => "business".to_string(),
        "personal_growth" | "growth" => "growth".to_string(),
        "news_and_social_opinion" | "news_opinion" => "news_opinion".to_string(),
        "entertainment" => "entertainment".to_string(),
        "science" => "science".to_string(),
        "medicine" => "medicine".to_string(),
        _ => "other".to_string(),
    }
}

pub fn normalize_bucket(module: &str, raw: &str) -> String {
    let module = normalize_module(module);
    let value = raw.trim().to_ascii_lowercase().replace('-', "_");

    if value.is_empty() {
        return "unspecified".to_string();
    }

    if !allowed_buckets_for_module(&module).contains(&value.as_str()) {
        return default_bucket_for_module(&module).to_string();
    }

    match (module.as_str(), value.as_str()) {
        // social_science 在部分目录中仍使用 research，这里统一到 academic_frontier。
        ("social_science", "research") => "academic_frontier".to_string(),
        _ => value,
    }
}

pub fn allowed_buckets_for_module(module: &str) -> &'static [&'static str] {
    match normalize_module(module).as_str() {
        "technology" => &["research", "official", "blogs", "community"],
        "social_science" => &["academic_frontier", "blogs", "community"],
        "business" => &["blogs", "community", "streaming"],
        "growth" => &["blogs", "community", "streaming"],
        "news_opinion" => &[
            "news",
            "personal_opinion",
            "streaming_opinion",
            "community_opinion",
            "media_opinion",
        ],
        "entertainment" => &["lite_pool"],
        "science" => &["physics", "chemistry", "biology"],
        "medicine" => &["academic_frontier", "blogs", "community"],
        _ => &["unspecified"],
    }
}

pub fn default_bucket_for_module(module: &str) -> &'static str {
    allowed_buckets_for_module(module)
        .first()
        .copied()
        .unwrap_or("unspecified")
}

pub fn default_module_fetch_interval_hours(module: &str) -> i64 {
    match normalize_module(module).as_str() {
        "technology" => 6,
        _ => 12,
    }
}

pub fn default_module_push_top_n(module: &str) -> i64 {
    let _ = normalize_module(module);
    6
}

pub fn default_module_fetch_intervals() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("technology".to_string(), default_module_fetch_interval_hours("technology")),
        (
            "social_science".to_string(),
            default_module_fetch_interval_hours("social_science"),
        ),
        ("business".to_string(), default_module_fetch_interval_hours("business")),
        ("growth".to_string(), default_module_fetch_interval_hours("growth")),
        (
            "news_opinion".to_string(),
            default_module_fetch_interval_hours("news_opinion"),
        ),
        (
            "entertainment".to_string(),
            default_module_fetch_interval_hours("entertainment"),
        ),
        ("science".to_string(), default_module_fetch_interval_hours("science")),
        ("medicine".to_string(), default_module_fetch_interval_hours("medicine")),
        ("other".to_string(), default_module_fetch_interval_hours("other")),
    ])
}

pub fn default_module_push_top_n_map() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("technology".to_string(), default_module_push_top_n("technology")),
        (
            "social_science".to_string(),
            default_module_push_top_n("social_science"),
        ),
        ("business".to_string(), default_module_push_top_n("business")),
        ("growth".to_string(), default_module_push_top_n("growth")),
        ("news_opinion".to_string(), default_module_push_top_n("news_opinion")),
        (
            "entertainment".to_string(),
            default_module_push_top_n("entertainment"),
        ),
        ("science".to_string(), default_module_push_top_n("science")),
        ("medicine".to_string(), default_module_push_top_n("medicine")),
        ("other".to_string(), default_module_push_top_n("other")),
    ])
}

pub fn policy_for_source(
    module: &str,
    bucket: &str,
    source_kind: &SourceKind,
) -> SourceRuntimePolicy {
    let module = normalize_module(module);
    let bucket = normalize_bucket(&module, bucket);

    let (high_cutoff, medium_cutoff) = match (module.as_str(), bucket.as_str(), source_kind) {
        ("news_opinion", _, _) => (82, 64),
        ("science", _, _) => (76, 58),
        ("medicine", "academic_frontier", _) => (79, 61),
        (_, _, SourceKind::AcademicJournal) => (78, 60),
        (_, _, SourceKind::OfficialAnnouncement) => (74, 56),
        (_, _, SourceKind::TechnicalBlog) => (72, 54),
        (_, _, SourceKind::CommunityHotspot) => (80, 62),
    };

    policy(
        default_module_fetch_interval_hours(&module),
        high_cutoff,
        medium_cutoff,
    )
}

pub fn fetch_interval_for_source(module: &str, bucket: &str, source_kind: &SourceKind) -> Duration {
    policy_for_source(module, bucket, source_kind).fetch_interval
}

pub fn fetch_retry_interval_for_failed_source(
    module: &str,
    bucket: &str,
    source_kind: &SourceKind,
) -> Duration {
    let regular = fetch_interval_for_source(module, bucket, source_kind);
    let half = regular / 2;
    let floor = Duration::minutes(30);
    let cap = Duration::hours(2);
    if half < floor {
        floor
    } else if half > cap {
        cap
    } else {
        half
    }
}

pub fn fit_level_for_score(
    module: &str,
    bucket: &str,
    source_kind: &SourceKind,
    fit_score: i64,
) -> FitLevel {
    let policy = policy_for_source(module, bucket, source_kind);
    let score = fit_score.clamp(0, 100);
    if score >= policy.high_cutoff {
        FitLevel::High
    } else if score >= policy.medium_cutoff {
        FitLevel::Medium
    } else {
        FitLevel::Low
    }
}

fn policy(fetch_hours: i64, high_cutoff: i64, medium_cutoff: i64) -> SourceRuntimePolicy {
    SourceRuntimePolicy {
        fetch_interval: Duration::hours(fetch_hours),
        high_cutoff,
        medium_cutoff,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        allowed_buckets_for_module, default_bucket_for_module, normalize_bucket, normalize_module,
        policy_for_source,
    };
    use crate::models::SourceKind;

    #[test]
    fn normalizes_v3_module_aliases() {
        assert_eq!(normalize_module("personal_growth"), "growth");
        assert_eq!(normalize_module("news_and_social_opinion"), "news_opinion");
        assert_eq!(normalize_module("social-science"), "social_science");
    }

    #[test]
    fn normalizes_social_science_research_bucket() {
        assert_eq!(
            normalize_bucket("social_science", "research"),
            "academic_frontier"
        );
    }

    #[test]
    fn defaults_invalid_bucket_to_module_baseline() {
        assert_eq!(normalize_bucket("technology", "streaming"), "research");
        assert_eq!(default_bucket_for_module("technology"), "research");
        assert!(!allowed_buckets_for_module("technology").contains(&"streaming"));
    }

    #[test]
    fn exposes_bucket_specific_policy() {
        let tech_research =
            policy_for_source("technology", "research", &SourceKind::AcademicJournal);
        let news_breaking =
            policy_for_source("news_opinion", "news", &SourceKind::CommunityHotspot);
        let growth_blogs = policy_for_source("growth", "blogs", &SourceKind::TechnicalBlog);
        let medicine_research = policy_for_source(
            "medicine",
            "academic_frontier",
            &SourceKind::AcademicJournal,
        );

        assert_eq!(tech_research.fetch_interval.num_hours(), 6);
        assert_eq!(news_breaking.fetch_interval.num_hours(), 12);
        assert_eq!(growth_blogs.fetch_interval.num_hours(), 12);
        assert_eq!(medicine_research.fetch_interval.num_hours(), 12);
    }
}
