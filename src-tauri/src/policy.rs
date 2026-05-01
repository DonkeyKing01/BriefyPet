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
        "design",
        "science",
        "medicine",
        "other",
    ]
}

pub fn normalize_module(raw: &str) -> String {
    let value = slugify(raw);
    match value.as_str() {
        "technology" => "technology".to_string(),
        "socialscience" | "social_science" | "social-science" => "social_science".to_string(),
        "business" | "growth" | "life" => "business".to_string(),
        "design" | "entertainment" | "humanities" => "design".to_string(),
        "science" => "science".to_string(),
        "medicine" => "medicine".to_string(),
        "news" | "news_opinion" | "other" => "other".to_string(),
        _ if value.is_empty() => "other".to_string(),
        _ => "other".to_string(),
    }
}

pub fn normalize_bucket(raw: &str) -> String {
    let value = slugify(raw);
    match value.as_str() {
        "" => "general".to_string(),
        "academic_frontier" => "frontier".to_string(),
        "personal_opinion" | "media_opinion" | "streaming_opinion" | "community_opinion" => {
            "opinion".to_string()
        }
        _ => value,
    }
}

pub fn normalize_group(raw: &str) -> String {
    let value = slugify(raw);
    match value.as_str() {
        "" => "general".to_string(),
        "academic_frontier" => "frontier".to_string(),
        "personal_opinion" | "media_opinion" | "streaming_opinion" | "community_opinion" => {
            "opinion".to_string()
        }
        _ => value,
    }
}

pub fn default_module_fetch_interval_hours(module: &str) -> i64 {
    match normalize_module(module).as_str() {
        "technology" => 6,
        _ => 12,
    }
}

pub fn default_module_push_top_n(_module: &str) -> i64 {
    6
}

pub fn default_module_fetch_intervals() -> BTreeMap<String, i64> {
    BTreeMap::from([
        (
            "technology".to_string(),
            default_module_fetch_interval_hours("technology"),
        ),
        (
            "social_science".to_string(),
            default_module_fetch_interval_hours("social_science"),
        ),
        (
            "business".to_string(),
            default_module_fetch_interval_hours("business"),
        ),
        (
            "design".to_string(),
            default_module_fetch_interval_hours("design"),
        ),
        (
            "science".to_string(),
            default_module_fetch_interval_hours("science"),
        ),
        (
            "medicine".to_string(),
            default_module_fetch_interval_hours("medicine"),
        ),
        (
            "other".to_string(),
            default_module_fetch_interval_hours("other"),
        ),
    ])
}

pub fn default_module_push_top_n_map() -> BTreeMap<String, i64> {
    BTreeMap::from([
        (
            "technology".to_string(),
            default_module_push_top_n("technology"),
        ),
        (
            "social_science".to_string(),
            default_module_push_top_n("social_science"),
        ),
        (
            "business".to_string(),
            default_module_push_top_n("business"),
        ),
        ("design".to_string(), default_module_push_top_n("design")),
        ("science".to_string(), default_module_push_top_n("science")),
        (
            "medicine".to_string(),
            default_module_push_top_n("medicine"),
        ),
        ("other".to_string(), default_module_push_top_n("other")),
    ])
}

pub fn policy_for_source(
    module: &str,
    source_group: &str,
    source_kind: &SourceKind,
) -> SourceRuntimePolicy {
    let module = normalize_module(module);
    let source_group = normalize_group(source_group);

    let (high_cutoff, medium_cutoff) = match (module.as_str(), source_group.as_str(), source_kind) {
        ("medicine", "clinical_trials", _) => (82, 66),
        ("medicine", "regulatory_science", _) | ("medicine", "clinical_safety", _) => (80, 64),
        ("science", _, SourceKind::AcademicJournal) => (79, 61),
        (_, "official", SourceKind::OfficialAnnouncement) => (78, 60),
        (_, "frontier", SourceKind::AcademicJournal)
        | (_, "academic", SourceKind::AcademicJournal) => (77, 59),
        (_, "research", SourceKind::AcademicJournal) => (76, 58),
        (_, "opinion", _) => (74, 56),
        (_, "blogs", _) => (72, 54),
        (_, "community", _) | (_, "news", _) => (75, 57),
        _ => match source_kind {
            SourceKind::AcademicJournal => (76, 58),
            SourceKind::OfficialAnnouncement => (75, 57),
            SourceKind::TechnicalBlog => (72, 54),
            SourceKind::CommunityHotspot => (74, 56),
        },
    };

    policy(
        default_module_fetch_interval_hours(&module),
        high_cutoff,
        medium_cutoff,
    )
}

pub fn fetch_interval_for_source(
    module: &str,
    source_group: &str,
    source_kind: &SourceKind,
) -> Duration {
    policy_for_source(module, source_group, source_kind).fetch_interval
}

pub fn fetch_retry_interval_for_failed_source(
    module: &str,
    source_group: &str,
    source_kind: &SourceKind,
) -> Duration {
    let regular = fetch_interval_for_source(module, source_group, source_kind);
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
    source_group: &str,
    source_kind: &SourceKind,
    fit_score: i64,
) -> FitLevel {
    let policy = policy_for_source(module, source_group, source_kind);
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

fn slugify(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .replace('&', " and ")
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' => ch,
            _ => '_',
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::{
        fit_level_for_score, normalize_bucket, normalize_group, normalize_module, policy_for_source,
    };
    use crate::models::SourceKind;

    #[test]
    fn normalizes_modules_with_legacy_aliases() {
        assert_eq!(normalize_module("social-science"), "social_science");
        assert_eq!(normalize_module("growth"), "business");
        assert_eq!(normalize_module("entertainment"), "design");
    }

    #[test]
    fn normalizes_bucket_and_group_aliases() {
        assert_eq!(normalize_bucket("academic_frontier"), "frontier");
        assert_eq!(normalize_group("media_opinion"), "opinion");
    }

    #[test]
    fn exposes_group_aware_policy() {
        let official =
            policy_for_source("technology", "official", &SourceKind::OfficialAnnouncement);
        let frontier =
            policy_for_source("social_science", "frontier", &SourceKind::AcademicJournal);

        assert_eq!(official.fetch_interval.num_hours(), 6);
        assert_eq!(frontier.fetch_interval.num_hours(), 12);
        assert!(
            fit_level_for_score(
                "technology",
                "official",
                &SourceKind::OfficialAnnouncement,
                79
            ) == crate::models::FitLevel::High
        );
    }
}
