use anyhow::{anyhow, Context, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};

use crate::{
    models::{FeedArticle, FitLevel, LlmResult, SourceKind},
    policy,
};

const LLM_RETRIES: usize = 3;
const LLM_REQUEST_TIMEOUT_SECS: u64 = 20;

struct ProviderConfig {
    base_url: &'static str,
    default_model: &'static str,
}

pub fn normalize_provider(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "deepseek" => "deepseek".to_string(),
        "glm" | "zhipu" | "zhipuai" => "glm".to_string(),
        "kimi" | "moonshot" => "kimi".to_string(),
        "openai" => "openai".to_string(),
        "siliconflow" => "siliconflow".to_string(),
        _ => "deepseek".to_string(),
    }
}

fn provider_config(provider: &str) -> ProviderConfig {
    match normalize_provider(provider).as_str() {
        "glm" => ProviderConfig {
            base_url: "https://open.bigmodel.cn/api/paas/v4",
            default_model: "glm-4-flash",
        },
        "kimi" => ProviderConfig {
            base_url: "https://api.moonshot.cn/v1",
            default_model: "moonshot-v1-8k",
        },
        "openai" => ProviderConfig {
            base_url: "https://api.openai.com/v1",
            default_model: "gpt-4o-mini",
        },
        "siliconflow" => ProviderConfig {
            base_url: "https://api.siliconflow.cn/v1",
            default_model: "Qwen/Qwen2.5-72B-Instruct",
        },
        _ => ProviderConfig {
            base_url: "https://api.deepseek.com",
            default_model: "deepseek-chat",
        },
    }
}

fn resolve_model(provider: &str, model_override: Option<&str>) -> String {
    if let Some(model) = model_override.map(str::trim).filter(|value| !value.is_empty()) {
        return model.to_string();
    }
    provider_config(provider).default_model.to_string()
}

fn chat_completions_url(provider: &str) -> String {
    let base = provider_config(provider).base_url.trim_end_matches('/');
    format!("{base}/chat/completions")
}

pub async fn validate_api_key(provider: &str, model_override: Option<&str>, api_key: &str) -> Result<()> {
    let trimmed_key = api_key.trim();
    if trimmed_key.is_empty() {
        return Err(anyhow!("missing api key"));
    }

    let model = resolve_model(provider, model_override);

    let body = json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": "Reply with OK."
            }
        ],
        "max_tokens": 8,
        "temperature": 0,
        "stream": false
    });

    let _ = send_chat_completion_with_retries(provider, trimmed_key, &body).await?;

    Ok(())
}

pub async fn summarize_and_score_batch(
    provider: &str,
    model_override: Option<&str>,
    api_key: &str,
    interest_context: &str,
    articles: &[FeedArticle],
) -> Result<Vec<LlmResult>> {
    let trimmed_key = api_key.trim();
    if trimmed_key.is_empty() {
        return Err(anyhow!("missing api key"));
    }
    if articles.is_empty() {
        return Ok(Vec::new());
    }
    let model = resolve_model(provider, model_override);

    let article_payload = articles
        .iter()
        .enumerate()
        .map(|(index, article)| {
            let module = policy::normalize_module(&article.module);
            let bucket = policy::normalize_bucket(&module, &article.bucket);
            let scoring_hint = scoring_focus_for_source(&module, &bucket, &article.source_kind);
            format!(
                "INDEX: {index}\nSOURCE: {}\nMODULE: {}\nBUCKET: {}\nDISCIPLINE: {:?}\nSOURCE_KIND: {:?}\nRESOURCE_TYPE: {:?}\nSCORING_HINT: {}\nTITLE: {}\nCONTENT: {}",
                article.source_name,
                module,
                bucket,
                article.discipline,
                article.source_kind,
                article.resource_type,
                scoring_hint,
                article.title,
                truncate(&article.content, 1600)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let prompt = format!(
        "You are Briefy-pet's content analyzer.\n\
Return exactly one JSON object with this shape:\n\
{{\"items\":[{{\"index\":0,\"summary\":\"...\",\"fit_level\":\"high|medium|low\",\"fit_score\":0,\"recommendation_reason\":\"...\"}}]}}\n\
Rules:\n\
- Return one item for every input article.\n\
- Preserve input order by index.\n\
- summary and recommendation_reason must be Simplified Chinese.\n\
- fit_level must be one of high, medium, low.\n\
- fit_score must be an integer from 0 to 100.\n\
- fit_score must prioritize relevance, reliability, and freshness.\n\
- Do not output markdown, code fences, explanation, or extra keys.\n\
\n\
Personalization context:\n{interest_context}\n\
\n\
Articles:\n{article_payload}"
    );

    let body = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "Return exactly one JSON object. No markdown, no code fences, no explanation."
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "response_format": {
            "type": "json_object"
        },
        "max_tokens": 1600,
        "temperature": 0,
        "stream": false
    });

    let mut last_error = None;

    for attempt in 1..=LLM_RETRIES {
        match score_batch_once(provider, trimmed_key, &body, articles.len()).await {
            Ok(results) => return Ok(results),
            Err(err) => {
                last_error = Some(err);
                if attempt < LLM_RETRIES {
                    sleep(Duration::from_millis(900 * attempt as u64)).await;
                    continue;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("unknown llm batch scoring failure")))
}

pub async fn summarize_and_score_single(
    provider: &str,
    model_override: Option<&str>,
    api_key: &str,
    interest_context: &str,
    article: &FeedArticle,
) -> Result<LlmResult> {
    let mut results =
        summarize_and_score_batch(
            provider,
            model_override,
            api_key,
            interest_context,
            std::slice::from_ref(article),
        )
            .await
            .with_context(|| format!("single article scoring failed: {}", article.title))?;
    results.pop().ok_or_else(|| {
        anyhow!(
            "single article scoring returned no result: {}",
            article.title
        )
    })
}

fn scoring_focus_for_source(module: &str, bucket: &str, source_kind: &SourceKind) -> &'static str {
    match (module, bucket) {
        ("technology", "research") | ("social_science", "academic_frontier") => {
            "Prefer evidence-backed insights, research novelty, and practical implications."
        }
        ("science", "physics") | ("science", "chemistry") | ("science", "biology") => {
            "Prefer rigorous methodology, reproducibility clues, and true scientific value."
        }
        ("medicine", "academic_frontier") => {
            "Prioritize clinical reliability, patient impact, and evidence hierarchy."
        }
        ("news_opinion", "news") => {
            "Prioritize factual density, timeliness, and low speculation."
        }
        ("news_opinion", "community_opinion")
        | ("news_opinion", "personal_opinion")
        | ("news_opinion", "streaming_opinion") => {
            "Reward viewpoint diversity but penalize noise, hype, and low-information opinions."
        }
        ("technology", "official") => {
            "Prioritize official updates with direct product or ecosystem impact."
        }
        ("entertainment", "lite_pool") => {
            "Keep high signal-to-noise and prefer durable quality over clickbait."
        }
        _ => match source_kind {
            SourceKind::AcademicJournal => {
                "Prefer authoritative sources, clear claims, and high informational density."
            }
            SourceKind::OfficialAnnouncement => {
                "Prefer official first-hand updates with concrete implications."
            }
            SourceKind::TechnicalBlog => {
                "Prefer practical depth, implementation details, and transferable insights."
            }
            SourceKind::CommunityHotspot => {
                "Prefer quality discussions and actionable insights over emotional noise."
            }
        },
    }
}

fn extract_message_content(payload: &Value) -> Option<String> {
    let content = &payload["choices"][0]["message"]["content"];
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }

    let items = content.as_array()?;
    let mut text = String::new();
    for item in items {
        if let Some(part) = item.get("text").and_then(Value::as_str) {
            text.push_str(part);
        }
    }

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

async fn score_batch_once(
    provider: &str,
    api_key: &str,
    body: &Value,
    expected_len: usize,
) -> Result<Vec<LlmResult>> {
    let payload = send_chat_completion_with_retries(provider, api_key, body).await?;
    let raw_content = extract_message_content(&payload).context("missing llm response content")?;
    let normalized = normalize_json_response(&raw_content).ok_or_else(|| {
        anyhow!(
            "no parseable JSON found in model response: {}",
            truncate(&raw_content, 200)
        )
    })?;
    let json_value: Value = serde_json::from_str(&normalized)
        .with_context(|| format!("model JSON parse failed: {}", truncate(&raw_content, 200)))?;
    parse_batch_result(&json_value, expected_len)
        .with_context(|| format!("model batch shape invalid: {}", truncate(&normalized, 200)))
}

async fn send_chat_completion_with_retries(
    provider: &str,
    api_key: &str,
    body: &Value,
) -> Result<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(LLM_REQUEST_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("failed to build llm client")?;
    let mut last_error = None;

    for attempt in 1..=LLM_RETRIES {
        match send_chat_completion_once(&client, provider, api_key, body).await {
            Ok(payload) => return Ok(payload),
            Err(err) => {
                let retryable = is_retryable_llm_error(&err);
                last_error = Some(err);
                if retryable && attempt < LLM_RETRIES {
                    sleep(Duration::from_millis(900 * attempt as u64)).await;
                    continue;
                }
                break;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("unknown llm request failure")))
}

async fn send_chat_completion_once(
    client: &reqwest::Client,
    provider: &str,
    api_key: &str,
    body: &Value,
) -> Result<Value> {
    let response = client
        .post(chat_completions_url(provider))
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json")
        .json(body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let message = response.text().await.unwrap_or_default();
        return Err(anyhow!("llm request failed with HTTP {status}: {message}"));
    }

    let payload: Value = response.json().await?;
    Ok(payload)
}

fn is_retryable_llm_error(err: &anyhow::Error) -> bool {
    let text = err.to_string();
    text.contains("HTTP 429")
        || text.contains("HTTP 502")
        || text.contains("HTTP 503")
        || text.contains("HTTP 504")
        || text.contains("operation timed out")
        || text.contains("timed out")
        || text.contains("connection")
        || text.contains("no parseable JSON found")
        || text.contains("model JSON parse failed")
        || text.contains("model batch shape invalid")
}

pub fn is_auth_failure(err: &anyhow::Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("http 401")
        || text.contains("http 403")
        || text.contains("unauthorized")
        || text.contains("invalid api key")
        || text.contains("incorrect api key")
        || text.contains("invalid key")
}

fn normalize_json_response(raw: &str) -> Option<String> {
    let stripped = strip_think_blocks(raw);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return None;
    }

    if serde_json::from_str::<Value>(trimmed).is_ok() {
        return Some(trimmed.to_string());
    }

    let unfenced_once = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim();
    let unfenced = unfenced_once
        .strip_suffix("```")
        .unwrap_or(unfenced_once)
        .trim()
        .to_string();

    if serde_json::from_str::<Value>(&unfenced).is_ok() {
        return Some(unfenced);
    }

    extract_first_json_block(trimmed)
}

fn strip_think_blocks(raw: &str) -> String {
    let mut remaining = raw;
    let mut output = String::new();

    loop {
        if let Some(start) = remaining.find("<think>") {
            output.push_str(&remaining[..start]);
            let after_start = &remaining[start + "<think>".len()..];
            if let Some(end) = after_start.find("</think>") {
                remaining = &after_start[end + "</think>".len()..];
            } else {
                break;
            }
        } else {
            output.push_str(remaining);
            break;
        }
    }

    output
}

fn extract_first_json_block(raw: &str) -> Option<String> {
    let chars: Vec<char> = raw.chars().collect();
    let mut depth = 0usize;
    let mut start_index = None;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in chars.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start_index = Some(index);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    let start = start_index?;
                    let candidate = chars[start..=index].iter().collect::<String>();
                    if serde_json::from_str::<Value>(&candidate).is_ok() {
                        return Some(candidate);
                    }
                    start_index = None;
                }
            }
            _ => {}
        }
    }

    None
}

fn parse_batch_result(value: &Value, expected_len: usize) -> Result<Vec<LlmResult>> {
    let items = extract_candidate_items(value, expected_len)?;

    let mut ordered = Vec::with_capacity(expected_len);
    ordered.resize_with(expected_len, || None);

    for (position, item) in items.iter().enumerate() {
        let index = item
            .get("index")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(position);
        if index >= expected_len {
            return Err(anyhow!("index out of range: {index}"));
        }
        ordered[index] = Some(parse_llm_result(item)?);
    }

    ordered
        .into_iter()
        .map(|entry| entry.ok_or_else(|| anyhow!("missing result for one or more indexes")))
        .collect()
}

fn parse_llm_result(value: &Value) -> Result<LlmResult> {
    let summary = value
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("该内容暂无可用摘要（模型输出缺失）")
        .to_string();
    let recommendation_reason = value
        .get("recommendation_reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("模型输出不完整，建议直接查看原文后再决定是否关注")
        .to_string();
    let fit_score = parse_fit_score(value.get("fit_score")).unwrap_or(40);
    let fit_level =
        parse_fit_level(value.get("fit_level")).unwrap_or_else(|| fit_level_from_score(fit_score));

    Ok(LlmResult {
        summary,
        fit_level,
        fit_score: fit_score.clamp(0, 100),
        recommendation_reason,
    })
}

fn extract_candidate_items<'a>(value: &'a Value, expected_len: usize) -> Result<Vec<&'a Value>> {
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        if items.len() == expected_len {
            return Ok(items.iter().collect());
        }
        if items.len() == 1 && expected_len == 1 {
            return Ok(items.iter().collect());
        }
    }

    if let Some(items) = value.get("results").and_then(Value::as_array) {
        if items.len() == expected_len {
            return Ok(items.iter().collect());
        }
    }

    if let Some(items) = value.as_array() {
        if items.len() == expected_len {
            return Ok(items.iter().collect());
        }
    }

    if expected_len == 1 {
        if value.get("summary").is_some() || value.get("fit_score").is_some() {
            return Ok(vec![value]);
        }
        if let Some(obj) = value.get("item") {
            return Ok(vec![obj]);
        }
    }

    Err(anyhow!(
        "missing usable items array: expected {expected_len} entries"
    ))
}

fn parse_fit_level(value: Option<&Value>) -> Option<FitLevel> {
    let raw = value?.as_str()?.trim().to_lowercase();
    match raw.as_str() {
        "high" | "高" => Some(FitLevel::High),
        "medium" | "mid" | "中" | "中等" => Some(FitLevel::Medium),
        "low" | "低" => Some(FitLevel::Low),
        _ => None,
    }
}

fn parse_fit_score(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                Some(int)
            } else {
                let float = number.as_f64()?;
                if float <= 1.0 {
                    Some((float * 100.0).round() as i64)
                } else {
                    Some(float.round() as i64)
                }
            }
        }
        Value::String(text) => {
            let trimmed = text.trim().trim_end_matches('%');
            if let Ok(int) = trimmed.parse::<i64>() {
                Some(int)
            } else if let Ok(float) = trimmed.parse::<f64>() {
                if float <= 1.0 {
                    Some((float * 100.0).round() as i64)
                } else {
                    Some(float.round() as i64)
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn fit_level_from_score(score: i64) -> FitLevel {
    if score >= 80 {
        FitLevel::High
    } else if score >= 60 {
        FitLevel::Medium
    } else {
        FitLevel::Low
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_json_response, parse_batch_result, parse_fit_level, parse_fit_score,
    };
    use crate::models::FitLevel;
    use serde_json::json;

    #[test]
    fn extracts_json_from_markdown_code_block() {
        let raw = "```json\n{\"items\":[{\"index\":0,\"summary\":\"ok\",\"fit_level\":\"high\",\"fit_score\":88,\"recommendation_reason\":\"ok\"}]}\n```";
        let normalized = normalize_json_response(raw).expect("json should be extracted");
        assert!(normalized.contains("\"items\""));
    }

    #[test]
    fn extracts_json_from_mixed_text_with_think_block() {
        let raw = "<think>hidden</think>\nprefix\n{\"items\":[{\"index\":0,\"summary\":\"ok\",\"fit_level\":\"medium\",\"fit_score\":\"72\",\"recommendation_reason\":\"ok\"}]}\nsuffix";
        let normalized = normalize_json_response(raw).expect("json should be extracted");
        assert!(normalized.starts_with('{'));
        assert!(normalized.ends_with('}'));
    }

    #[test]
    fn parses_non_standard_fit_fields() {
        assert_eq!(parse_fit_level(Some(&json!("高"))), Some(FitLevel::High));
        assert_eq!(parse_fit_score(Some(&json!(0.91))), Some(91));
        assert_eq!(parse_fit_score(Some(&json!("65%"))), Some(65));
    }

    #[test]
    fn parses_single_object_without_items_wrapper() {
        let value = json!({
            "summary": "ok",
            "fit_level": "high",
            "fit_score": 88,
            "recommendation_reason": "ok"
        });

        let parsed = parse_batch_result(&value, 1).expect("single object should parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].fit_level, FitLevel::High);
    }

    #[test]
    fn parses_items_without_index_by_position() {
        let value = json!({
            "items": [
                {
                    "summary": "first",
                    "fit_level": "low",
                    "fit_score": 35,
                    "recommendation_reason": "r1"
                },
                {
                    "summary": "second",
                    "fit_level": "medium",
                    "fit_score": 66,
                    "recommendation_reason": "r2"
                }
            ]
        });

        let parsed = parse_batch_result(&value, 2).expect("items should parse by position");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].summary, "first");
        assert_eq!(parsed[1].summary, "second");
    }
}
