use chrono::Duration;

use crate::models::{FitLevel, SourceKind};

#[derive(Debug, Clone, Copy)]
pub struct SourceRuntimePolicy {
    pub fetch_interval: Duration,
    pub reminder_take: usize,
    pub high_cutoff: i64,
    pub medium_cutoff: i64,
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

    match (module.as_str(), value.as_str()) {
        // social_science 在部分目录中仍使用 research，这里统一到 academic_frontier。
        ("social_science", "research") => "academic_frontier".to_string(),
        _ => value,
    }
}

pub fn policy_for_source(module: &str, bucket: &str, source_kind: &SourceKind) -> SourceRuntimePolicy {
    let module = normalize_module(module);
    let bucket = normalize_bucket(&module, bucket);

    let default_policy = match source_kind {
        SourceKind::AcademicJournal => policy(72, 2, 78, 60),
        SourceKind::OfficialAnnouncement => policy(6, 2, 74, 56),
        SourceKind::TechnicalBlog => policy(3, 2, 72, 54),
        SourceKind::CommunityHotspot => policy(3, 1, 80, 62),
    };

    match (module.as_str(), bucket.as_str()) {
        ("technology", "research") => policy(24, 2, 78, 60),
        ("technology", "official") => policy(4, 2, 74, 56),
        ("technology", "blogs") => policy(6, 3, 72, 54),
        ("technology", "community") => policy(2, 2, 78, 60),
        ("technology", "streaming") => policy(3, 2, 76, 58),

        ("social_science", "academic_frontier") => policy(36, 2, 78, 60),
        ("social_science", "blogs") => policy(12, 2, 73, 55),
        ("social_science", "community") => policy(8, 1, 79, 61),

        ("business", "blogs") => policy(8, 2, 72, 54),
        ("business", "community") => policy(6, 1, 78, 60),
        ("business", "streaming") => policy(6, 1, 77, 59),

        ("growth", "blogs") => policy(12, 2, 72, 54),
        ("growth", "community") => policy(8, 1, 77, 59),
        ("growth", "streaming") => policy(8, 1, 76, 58),

        ("news_opinion", "news") => policy(1, 3, 82, 64),
        ("news_opinion", "media_opinion") => policy(2, 2, 80, 62),
        ("news_opinion", "personal_opinion") => policy(3, 1, 81, 63),
        ("news_opinion", "streaming_opinion") => policy(3, 1, 79, 61),
        ("news_opinion", "community_opinion") => policy(2, 1, 82, 64),

        ("entertainment", "lite_pool") => policy(6, 1, 80, 62),

        ("science", "physics") => policy(72, 1, 76, 58),
        ("science", "chemistry") => policy(72, 1, 76, 58),
        ("science", "biology") => policy(72, 1, 76, 58),

        ("medicine", "academic_frontier") => policy(48, 2, 79, 61),
        ("medicine", "blogs") => policy(12, 1, 74, 56),
        ("medicine", "community") => policy(8, 1, 80, 62),
        _ => default_policy,
    }
}

pub fn fetch_interval_for_source(module: &str, bucket: &str, source_kind: &SourceKind) -> Duration {
    policy_for_source(module, bucket, source_kind).fetch_interval
}

pub fn reminder_take_for_source(module: &str, bucket: &str, source_kind: &SourceKind) -> usize {
    policy_for_source(module, bucket, source_kind).reminder_take
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

fn policy(fetch_hours: i64, reminder_take: usize, high_cutoff: i64, medium_cutoff: i64) -> SourceRuntimePolicy {
    SourceRuntimePolicy {
        fetch_interval: Duration::hours(fetch_hours),
        reminder_take,
        high_cutoff,
        medium_cutoff,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_bucket, normalize_module, policy_for_source};
    use crate::models::SourceKind;

    #[test]
    fn normalizes_v3_module_aliases() {
        assert_eq!(normalize_module("personal_growth"), "growth");
        assert_eq!(normalize_module("news_and_social_opinion"), "news_opinion");
        assert_eq!(normalize_module("social-science"), "social_science");
    }

    #[test]
    fn normalizes_social_science_research_bucket() {
        assert_eq!(normalize_bucket("social_science", "research"), "academic_frontier");
    }

    #[test]
    fn exposes_bucket_specific_policy() {
        let tech_research = policy_for_source("technology", "research", &SourceKind::AcademicJournal);
        let news_breaking = policy_for_source("news_opinion", "news", &SourceKind::CommunityHotspot);

        assert_eq!(tech_research.fetch_interval.num_hours(), 24);
        assert_eq!(tech_research.reminder_take, 2);
        assert_eq!(news_breaking.fetch_interval.num_hours(), 1);
        assert_eq!(news_breaking.reminder_take, 3);
    }
}
