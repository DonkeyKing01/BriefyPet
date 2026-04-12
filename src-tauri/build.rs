use std::{
    collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use quick_xml::{events::Event, Reader};
use serde::Serialize;
use serde_json::Value;

const INPUT_FILES: &[&str] = &[
    "../reference/BestBlogs/BestBlogs_RSS_ALL.opml",
    "../reference/BestBlogs/BestBlogs_RSS_Articles.opml",
    "../reference/BestBlogs/BestBlogs_RSS_Podcasts.opml",
    "../reference/BestBlogs/BestBlogs_RSS_Twitters.opml",
    "../reference/BestBlogs/BestBlogs_RSS_Videos.opml",
    "../reference/BestBlogs/archive/BestBlogs_RSS_V2.opml",
    "../reference/BestBlogs/archive/WeRSS.opml",
    "../reference/BestBlogs/archive/WeWeRSS.opml",
    "../reference/High_quality_in_AI.opml",
    "../reference/social_science_frontier_radar.opml",
    "../reference/High_quality_in_life.json",
];

#[derive(Clone, Debug)]
struct RawSource {
    name: String,
    url: String,
    origin_file: String,
    language: Option<String>,
    category_name: Option<String>,
    subcategory_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogEntry {
    id: String,
    name: String,
    url: String,
    normalized_url: String,
    discipline: String,
    source_kind: String,
    resource_type: String,
    language: Option<String>,
    enabled_by_default: bool,
    postponed: bool,
    origin_files: Vec<String>,
}

#[derive(Clone, Debug)]
struct CatalogAccumulator {
    name: String,
    url: String,
    normalized_url: String,
    discipline: String,
    source_kind: String,
    resource_type: String,
    language: Option<String>,
    origin_files: BTreeSet<String>,
}

fn main() {
    for input in INPUT_FILES {
        println!("cargo:rerun-if-changed={input}");
    }

    if let Err(err) = generate_catalog_artifacts() {
        panic!("failed to generate rss catalog artifacts: {err}");
    }

    tauri_build::build()
}

fn generate_catalog_artifacts() -> Result<(), String> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(to_string)?);
    let resource_dir = manifest_dir.join("resources");
    fs::create_dir_all(&resource_dir).map_err(to_string)?;

    let mut raw_sources = Vec::new();
    for relative in INPUT_FILES {
        let path = manifest_dir.join(relative);
        if relative.ends_with(".opml") {
            raw_sources.extend(parse_opml(&path)?);
        } else if relative.ends_with(".json") {
            raw_sources.extend(parse_json_sources(&path)?);
        }
    }

    let raw_count = raw_sources.len();
    let mut grouped: BTreeMap<String, CatalogAccumulator> = BTreeMap::new();

    for raw in raw_sources {
        let normalized_url = normalize_url(&raw.url);
        let discipline = classify_discipline(&raw);
        let source_kind = classify_source_kind(&raw, &discipline);
        let resource_type = classify_resource_type(&raw);

        let entry = grouped
            .entry(normalized_url.clone())
            .or_insert_with(|| CatalogAccumulator {
                name: raw.name.clone(),
                url: raw.url.clone(),
                normalized_url: normalized_url.clone(),
                discipline: discipline.clone(),
                source_kind: source_kind.clone(),
                resource_type: resource_type.clone(),
                language: raw.language.clone(),
                origin_files: BTreeSet::new(),
            });

        if rank_discipline(&discipline) > rank_discipline(&entry.discipline) {
            entry.discipline = discipline.clone();
        }
        if rank_source_kind(&source_kind) > rank_source_kind(&entry.source_kind) {
            entry.source_kind = source_kind.clone();
        }
        if rank_resource_type(&resource_type) > rank_resource_type(&entry.resource_type) {
            entry.resource_type = resource_type.clone();
        }
        if entry.language.is_none() && raw.language.is_some() {
            entry.language = raw.language.clone();
        }
        if better_name(&raw.name, &entry.name) {
            entry.name = raw.name.clone();
        }
        entry.origin_files.insert(raw.origin_file);
    }

    let mut catalog = grouped
        .into_values()
        .map(|item| {
            CatalogEntry {
                id: build_source_id(&item.name, &item.normalized_url),
                name: item.name,
                url: item.url,
                normalized_url: item.normalized_url,
                discipline: item.discipline,
                source_kind: item.source_kind,
                resource_type: item.resource_type,
                language: item.language,
                enabled_by_default: true,
                postponed: false,
                origin_files: item.origin_files.into_iter().collect(),
            }
        })
        .collect::<Vec<_>>();

    catalog.sort_by(|left, right| {
        left.postponed
            .cmp(&right.postponed)
            .then_with(|| left.discipline.cmp(&right.discipline))
            .then_with(|| left.source_kind.cmp(&right.source_kind))
            .then_with(|| left.name.cmp(&right.name))
    });

    let catalog_json =
        serde_json::to_string_pretty(&catalog).map_err(|err| format!("catalog json: {err}"))?;
    write_if_changed(&resource_dir.join("rss-catalog-v2-1.json"), &catalog_json)?;

    let compatibility_sources = catalog
        .iter()
        .map(|entry| {
            serde_json::json!({
                "id": entry.id,
                "name": entry.name,
                "url": entry.url,
                "enabled": entry.enabled_by_default
            })
        })
        .collect::<Vec<_>>();
    let compatibility_json = serde_json::to_string_pretty(&compatibility_sources)
        .map_err(|err| format!("compat json: {err}"))?;
    write_if_changed(&resource_dir.join("rss-sources.json"), &compatibility_json)?;

    let report = build_report(&catalog, raw_count);
    write_if_changed(&resource_dir.join("rss-dedup-report-v2-1.md"), &report)?;

    Ok(())
}

fn parse_opml(path: &Path) -> Result<Vec<RawSource>, String> {
    let content = fs::read_to_string(path).map_err(to_string)?;
    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut items = Vec::new();
    let origin = display_origin(path);

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if event.name().as_ref() == b"outline" =>
            {
                let mut text = None;
                let mut title = None;
                let mut xml_url = None;
                for attr in event.attributes().flatten() {
                    let key = attr.key.as_ref();
                    let value = String::from_utf8_lossy(attr.value.as_ref()).to_string();
                    match key {
                        b"text" => text = Some(value),
                        b"title" => title = Some(value),
                        b"xmlUrl" => xml_url = Some(value),
                        _ => {}
                    }
                }
                if let Some(url) = xml_url {
                    let name = text
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .or(title.as_deref())
                        .unwrap_or("Untitled Source")
                        .trim()
                        .to_string();
                    items.push(RawSource {
                        name,
                        url,
                        origin_file: origin.clone(),
                        language: None,
                        category_name: None,
                        subcategory_name: None,
                    });
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(format!("failed to parse {}: {err}", path.display())),
        }
        buf.clear();
    }

    Ok(items)
}

fn parse_json_sources(path: &Path) -> Result<Vec<RawSource>, String> {
    let content = fs::read_to_string(path).map_err(to_string)?;
    let value: Value = serde_json::from_str(&content).map_err(to_string)?;
    let array = value
        .as_array()
        .ok_or_else(|| format!("{} is not a JSON array", path.display()))?;
    let origin = display_origin(path);
    let mut items = Vec::new();

    for item in array {
        let url = item
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if url.is_empty() {
            continue;
        }
        let name = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Untitled Source")
            .trim()
            .to_string();
        items.push(RawSource {
            name,
            url,
            origin_file: origin.clone(),
            language: item
                .get("language")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
            category_name: item
                .get("category_name")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
            subcategory_name: item
                .get("subcategory_name")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
        });
    }

    Ok(items)
}

fn display_origin(path: &Path) -> String {
    path.components()
        .skip_while(|component| component.as_os_str() != "reference")
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
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

fn classify_resource_type(raw: &RawSource) -> String {
    let haystack = build_haystack(raw);
    if haystack.contains("podcast")
        || haystack.contains("soundcloud")
        || haystack.contains("anchor.fm")
    {
        "podcast".to_string()
    } else if haystack.contains("videos.opml")
        || haystack.contains("youtube.com/feeds/videos.xml")
        || haystack.contains("bilibili")
        || haystack.contains("vimeo")
    {
        "video".to_string()
    } else if haystack.contains("twitters.opml")
        || haystack.contains("twitter.com")
        || haystack.contains("x.com/")
        || haystack.contains("nitter")
    {
        "twitter".to_string()
    } else {
        "article".to_string()
    }
}

fn classify_discipline(raw: &RawSource) -> String {
    if let Some(mapped) = map_life_category(raw) {
        return mapped.to_string();
    }

    let haystack = build_haystack(raw);
    if contains_any(
        &haystack,
        &[
            "medicine", "medical", "health", "doctor", "clinical", "pharma", "hospital", "pubmed",
            "nejm", "lancet", "medrxiv",
        ],
    ) {
        return "medicine".to_string();
    }
    if contains_any(
        &haystack,
        &[
            "science",
            "physics",
            "chemistry",
            "biology",
            "astronomy",
            "math",
            "mathematics",
            "genomics",
            "3blue1brown",
            "nature.com",
            "scientific",
        ],
    ) {
        return "science".to_string();
    }
    if contains_any(
        &haystack,
        &[
            "economics",
            "economic",
            "sociology",
            "social science",
            "political",
            "policy",
            "world bank",
            "aea",
            "cepr",
            "our world in data",
            "development impact",
        ],
    ) {
        return "social-science".to_string();
    }
    if contains_any(
        &haystack,
        &[
            "journalism",
            "news",
            "headline",
            "reuters",
            "bbc",
            "nyt",
            "frontpage",
            "media",
        ],
    ) {
        return "news".to_string();
    }
    if contains_any(
        &haystack,
        &[
            "philosophy",
            "history",
            "humanities",
            "literature",
            "poetry",
            "books",
            "culture",
            "review of books",
            "art",
            "arts",
        ],
    ) {
        return "humanities".to_string();
    }
    if contains_any(
        &haystack,
        &[
            "travel",
            "food",
            "fitness",
            "games",
            "gaming",
            "hobby",
            "lifestyle",
            "tabletop",
            "internetisbeautiful",
            "design",
            "home",
            "garden",
        ],
    ) {
        return "life".to_string();
    }
    if contains_any(
        &haystack,
        &[
            "ai",
            "ml",
            "machine learning",
            "programming",
            "developer",
            "software",
            "frontend",
            "backend",
            "cloud",
            "open source",
            "github",
            "langchain",
            "llamaindex",
            "vector",
            "database",
            "devops",
            "engineering",
            "tech",
            "technology",
        ],
    ) {
        return "technology".to_string();
    }
    if haystack.contains("social_science_frontier_radar") {
        return "social-science".to_string();
    }
    if haystack.contains("high_quality_in_ai")
        || haystack.contains("bestblogs")
        || haystack.contains("werss")
        || haystack.contains("wewerss")
    {
        return "technology".to_string();
    }
    "other".to_string()
}

fn map_life_category(raw: &RawSource) -> Option<&'static str> {
    let category = raw.category_name.as_deref()?.to_lowercase();
    let subcategory = raw
        .subcategory_name
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    let combined = format!("{category} {subcategory}");

    if contains_any(
        &combined,
        &[
            "science",
            "mathematics",
            "astronomy",
            "physics",
            "chemistry",
        ],
    ) {
        Some("science")
    } else if contains_any(&combined, &["health", "medicine", "medical", "wellness"]) {
        Some("medicine")
    } else if contains_any(
        &combined,
        &[
            "tech",
            "internet",
            "programming",
            "gadgets",
            "ai",
            "software",
            "computer",
            "3d printing",
        ],
    ) {
        Some("technology")
    } else if contains_any(&combined, &["news", "journalism", "media"]) {
        Some("news")
    } else if contains_any(
        &combined,
        &[
            "business",
            "finance",
            "politics",
            "economics",
            "law",
            "education",
            "history",
            "society",
        ],
    ) {
        Some("social-science")
    } else if contains_any(
        &combined,
        &[
            "philosophy",
            "books",
            "culture",
            "literature",
            "art",
            "religion",
        ],
    ) {
        Some("humanities")
    } else if contains_any(
        &combined,
        &[
            "hobby", "travel", "food", "fitness", "games", "gaming", "sports", "home", "internet",
            "design",
        ],
    ) {
        Some("life")
    } else {
        Some("other")
    }
}

fn classify_source_kind(raw: &RawSource, discipline: &str) -> String {
    let haystack = build_haystack(raw);
    if contains_any(
        &haystack,
        &[
            "reddit",
            "hnrss",
            "hacker news",
            "lobste",
            "twitter",
            "x.com",
            "youtube.com/feeds/videos.xml",
            "bilibili",
            "trending",
            "frontpage",
            "community",
            "forum",
            "subreddit",
            "new/.rss",
            "rsshub.bestblogs.dev/juejin",
            "hot",
            "weekly",
        ],
    ) {
        return "community-hotspot".to_string();
    }
    if contains_any(
        &haystack,
        &[
            "journal",
            "discussion paper",
            "working paper",
            "arxiv",
            "review of books",
            "research highlights",
            "proceedings",
            "academic",
            "research & writing",
            "data insights",
        ],
    ) {
        return "academic-journal".to_string();
    }
    if contains_any(
        &haystack,
        &[
            ".gov/",
            ".gov.",
            ".edu/",
            ".edu.",
            "openai",
            "google",
            "microsoft",
            "aws",
            "github",
            "cloudflare",
            "world bank",
            "cepr",
            "aea",
            "lse",
            "official",
            "research blog",
            "developers",
            "announcements",
            "press release",
            "meta",
            "azure",
            "vercel",
            "jetbrains",
            "docker",
            "node.js",
            "hugging face",
            "deepmind",
            "databricks",
            "mozilla",
            "spring",
            "qdrant",
        ],
    ) {
        return "official-announcement".to_string();
    }
    if discipline == "news" {
        return "official-announcement".to_string();
    }
    "technical-blog".to_string()
}

fn rank_discipline(value: &str) -> usize {
    match value {
        "technology" => 7,
        "social-science" => 6,
        "news" => 5,
        "humanities" => 4,
        "life" => 3,
        "other" => 2,
        "science" => 1,
        "medicine" => 1,
        _ => 0,
    }
}

fn rank_source_kind(value: &str) -> usize {
    match value {
        "academic-journal" => 4,
        "official-announcement" => 3,
        "community-hotspot" => 2,
        "technical-blog" => 1,
        _ => 0,
    }
}

fn rank_resource_type(value: &str) -> usize {
    match value {
        "article" => 5,
        "podcast" => 4,
        "video" => 3,
        "twitter" => 2,
        "other" => 1,
        _ => 0,
    }
}

fn better_name(candidate: &str, current: &str) -> bool {
    let candidate = candidate.trim();
    let current = current.trim();
    !candidate.is_empty()
        && (current.is_empty()
            || (candidate.chars().count() > current.chars().count() && !candidate.contains("http")))
}

fn contains_any(haystack: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| haystack.contains(pattern))
}

fn build_haystack(raw: &RawSource) -> String {
    format!(
        "{} {} {} {} {}",
        raw.name.to_lowercase(),
        raw.url.to_lowercase(),
        raw.origin_file.to_lowercase(),
        raw.category_name
            .as_deref()
            .unwrap_or_default()
            .to_lowercase(),
        raw.subcategory_name
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
    )
}

fn build_source_id(name: &str, normalized_url: &str) -> String {
    let base_name = slugify(name);
    let base = if base_name.is_empty() {
        slugify(normalized_url)
    } else {
        base_name
    };
    let mut hasher = DefaultHasher::new();
    normalized_url.hash(&mut hasher);
    let hash = hasher.finish();
    let trimmed = if base.is_empty() {
        "source".to_string()
    } else {
        base
    };
    format!(
        "{}-{:08x}",
        trimmed.chars().take(32).collect::<String>(),
        hash as u32
    )
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, '/' | ':' | '.' | '_' | '-' | ' ') {
            Some('-')
        } else {
            None
        };
        if let Some(ch) = normalized {
            if ch == '-' {
                if !prev_dash && !out.is_empty() {
                    out.push(ch);
                    prev_dash = true;
                }
            } else {
                out.push(ch);
                prev_dash = false;
            }
        }
    }
    out.trim_matches('-').to_string()
}

fn build_report(catalog: &[CatalogEntry], raw_count: usize) -> String {
    let mut discipline_counts = BTreeMap::<String, usize>::new();
    let mut source_kind_counts = BTreeMap::<String, usize>::new();
    for entry in catalog {
        *discipline_counts
            .entry(entry.discipline.clone())
            .or_default() += 1;
        *source_kind_counts
            .entry(entry.source_kind.clone())
            .or_default() += 1;
    }

    let duplicate_entries = raw_count.saturating_sub(catalog.len());
    let duplicate_groups = catalog
        .iter()
        .filter(|entry| entry.origin_files.len() > 1)
        .count();

    let mut report = String::new();
    report.push_str("# RSS Dedup Report v2.1\n\n");
    report.push_str(&format!("- Raw sources: {raw_count}\n"));
    report.push_str(&format!("- Unique normalized URLs: {}\n", catalog.len()));
    report.push_str(&format!(
        "- Duplicate raw entries removed: {duplicate_entries}\n"
    ));
    report.push_str(&format!("- Duplicate groups: {duplicate_groups}\n\n"));

    report.push_str("## Discipline Coverage\n\n");
    for (discipline, count) in discipline_counts {
        report.push_str(&format!("- {discipline}: {count}\n"));
    }

    report.push_str("\n## Source Kind Coverage\n\n");
    for (source_kind, count) in source_kind_counts {
        report.push_str(&format!("- {source_kind}: {count}\n"));
    }

    report.push_str("\n## Duplicate Trace Samples\n\n");
    for entry in catalog
        .iter()
        .filter(|entry| entry.origin_files.len() > 1)
        .take(60)
    {
        report.push_str(&format!(
            "- `{}` <- {}\n",
            entry.normalized_url,
            entry.origin_files.join(", ")
        ));
    }

    report
}

fn to_string(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn write_if_changed(path: &Path, next: &str) -> Result<(), String> {
    match fs::read_to_string(path) {
        Ok(current) if current == next => Ok(()),
        Ok(_) | Err(_) => fs::write(path, next).map_err(to_string),
    }
}
