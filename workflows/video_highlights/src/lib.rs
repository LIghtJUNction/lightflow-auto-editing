use std::env;
use std::io::Read;
use std::time::Duration;

use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value, json};
use ring::hmac;

pub const WORKFLOW_ID: &str = "lightflow.video_highlights";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const VIDEOSCORE_MODEL: &str = "TIGER-Lab/VideoScore-v1.1";

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "VideoScore Long-video Highlights",
        description: "Rank long-video windows with a configured VideoScore service and return non-overlapping highlight candidates.",
        input "source_path": "path" { description: "Long source video path, mounted read-only in the VideoScore service.", required: true, widget: "file_open", }
        input "brief": "text" { description: "What should count as a highlight; used only to contextualize scoring.", required: true, widget: "textarea", }
        input "window_seconds": "integer" { description: "Candidate window duration in seconds, from 3 through 60.", default: 12, }
        input "stride_seconds": "integer" { description: "Time between candidate windows in seconds, from 1 through 60.", default: 6, }
        input "max_highlights": "integer" { description: "Maximum number of returned candidates, from 1 through 30.", default: 8, }
        input "minimum_score": "number" { description: "Fail-closed lower bound for the VideoScore aggregate, from 1.0 through 4.0.", default: 2.5, }
        output "highlights": "json" { description: "Ranked non-overlapping highlight candidates with VideoScore dimensions and timecodes." }
        output "clips": "json" { description: "Direct auto-edit clip objects derived from signed highlights; pass this output unchanged as clips or sources." }
        output "summary": "text" { description: "Model-backed highlight detection summary." }
    }
    .builtin_runtime("runner", "lightflow.runner", "runner.v1")
    .build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, HighlightError> {
    let source_path = required_text(inputs, "source_path")?;
    let brief = required_text(inputs, "brief")?;
    let window_seconds = bounded_integer(inputs, "window_seconds", 12, 3, 60)?;
    let stride_seconds = bounded_integer(inputs, "stride_seconds", 6, 1, 60)?;
    let max_highlights = bounded_integer(inputs, "max_highlights", 8, 1, 30)?;
    let minimum_score = bounded_number(inputs, "minimum_score", 2.5, 1.0, 4.0)?;
    let evidence_key = evidence_key()?;
    let endpoint = endpoint(&env::var("LIGHTFLOW_VIDEOSCORE_API_URL").unwrap_or_default())?;
    let request = json!({
        "model": VIDEOSCORE_MODEL,
        "source_path": source_path,
        "brief": brief,
        "window_seconds": window_seconds,
        "stride_seconds": stride_seconds,
    });
    let response = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(300))
        .build()
        .post(&endpoint)
        .set("Content-Type", "application/json")
        .send_json(request)
        .map_err(|error| HighlightError::owned(format!("VideoScore request failed: {error}")))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| HighlightError::owned(format!("VideoScore response failed: {error}")))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(HighlightError::new("VideoScore response exceeds 2 MiB"));
    }
    let candidates = parse_candidates(&bytes, source_path, &evidence_key)?;
    let highlights = select(
        candidates,
        max_highlights as usize,
        minimum_score,
        window_seconds as f64,
    );
    if highlights.is_empty() {
        return Err(HighlightError::new(
            "no VideoScore candidate meets minimum_score",
        ));
    }
    let count = highlights.len();
    let clips = clips_from_highlights(&highlights)?;
    Ok(Response {
        outputs: Map::from_iter([
            ("highlights".to_owned(), Value::Array(highlights)),
            ("clips".to_owned(), Value::Array(clips)),
            (
                "summary".to_owned(),
                format!("VideoScore selected {count} non-overlapping highlight candidates.").into(),
            ),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::from_iter([(
            "implementation".to_owned(),
            implementation_identity().into(),
        )]),
    })
}

fn clips_from_highlights(highlights: &[Value]) -> Result<Vec<Value>, HighlightError> {
    highlights
        .iter()
        .enumerate()
        .map(|(index, highlight)| {
            let object = highlight.as_object().ok_or_else(|| {
                HighlightError::new("normalized VideoScore highlight must be an object")
            })?;
            let path = object
                .get("source_path")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    HighlightError::new("normalized VideoScore highlight source_path missing")
                })?;
            let start = object
                .get("start_seconds")
                .and_then(Value::as_f64)
                .ok_or_else(|| {
                    HighlightError::new("normalized VideoScore highlight start_seconds missing")
                })?;
            let end = object
                .get("end_seconds")
                .and_then(Value::as_f64)
                .ok_or_else(|| {
                    HighlightError::new("normalized VideoScore highlight end_seconds missing")
                })?;
            Ok(json!({
                "id": format!("highlight-{}", index + 1),
                "path": path,
                "start": start,
                "end": end,
                "highlight": highlight,
            }))
        })
        .collect()
}

fn parse_candidates(
    bytes: &[u8],
    source_path: &str,
    key: &[u8],
) -> Result<Vec<Value>, HighlightError> {
    let response: Value = lightflow::serde_json::from_slice(bytes).map_err(|error| {
        HighlightError::owned(format!("VideoScore response is invalid JSON: {error}"))
    })?;
    let segments = response
        .get("segments")
        .and_then(Value::as_array)
        .ok_or_else(|| HighlightError::new("VideoScore response must contain segments array"))?;
    segments
        .iter()
        .map(|segment| normalize_candidate(segment, source_path, key))
        .collect()
}

fn normalize_candidate(
    value: &Value,
    source_path: &str,
    key: &[u8],
) -> Result<Value, HighlightError> {
    let object = value
        .as_object()
        .ok_or_else(|| HighlightError::new("VideoScore segment must be an object"))?;
    let start = required_number(object, "start_seconds")?;
    let end = required_number(object, "end_seconds")?;
    let score = required_number(object, "aggregate_score")?;
    if !(start >= 0.0 && end > start && (1.0..=4.0).contains(&score)) {
        return Err(HighlightError::new(
            "VideoScore segment bounds or aggregate_score are invalid",
        ));
    }
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .unwrap_or("VideoScore-ranked candidate");
    let evidence = sign_videoscore_evidence(key, source_path, start, end, score, reason);
    Ok(json!({
        "workflow": WORKFLOW_ID,
        "source_path": source_path,
        "start_seconds": start,
        "end_seconds": end,
        "score": score,
        "model": VIDEOSCORE_MODEL,
        "evidence": evidence,
        "dimensions": object.get("dimensions").cloned().unwrap_or_else(|| json!({})),
        "reason": reason,
    }))
}

fn select(
    mut candidates: Vec<Value>,
    limit: usize,
    minimum: f64,
    default_window: f64,
) -> Vec<Value> {
    candidates.sort_by(|left, right| score(right).total_cmp(&score(left)));
    let mut selected = Vec::new();
    for candidate in candidates {
        if score(&candidate) < minimum || selected.len() == limit {
            continue;
        }
        let start = candidate["start_seconds"].as_f64().unwrap_or_default();
        let end = candidate["end_seconds"]
            .as_f64()
            .unwrap_or(start + default_window);
        if selected
            .iter()
            .any(|other: &Value| overlaps(start, end, other))
        {
            continue;
        }
        let mut candidate = candidate;
        candidate["rank"] = (selected.len() + 1).into();
        selected.push(candidate);
    }
    selected
}

fn score(candidate: &Value) -> f64 {
    candidate["score"].as_f64().unwrap_or_default()
}
fn overlaps(start: f64, end: f64, other: &Value) -> bool {
    start < other["end_seconds"].as_f64().unwrap_or_default()
        && end > other["start_seconds"].as_f64().unwrap_or_default()
}
fn endpoint(base: &str) -> Result<String, HighlightError> {
    let base = base.trim().trim_end_matches('/');
    let url = url::Url::parse(base)
        .map_err(|_| HighlightError::new("LIGHTFLOW_VIDEOSCORE_API_URL must be an https URL"))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(HighlightError::new(
            "LIGHTFLOW_VIDEOSCORE_API_URL must be an https URL",
        ));
    }
    Ok(format!("{base}/v1/highlights"))
}
fn required_text<'a>(
    inputs: &'a Map<String, Value>,
    name: &'static str,
) -> Result<&'a str, HighlightError> {
    inputs
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HighlightError::new("missing required text input"))
}
fn bounded_integer(
    inputs: &Map<String, Value>,
    name: &'static str,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, HighlightError> {
    let value = inputs.get(name).and_then(Value::as_i64).unwrap_or(default);
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(HighlightError::new(
            "integer input is outside its allowed range",
        ))
    }
}
fn bounded_number(
    inputs: &Map<String, Value>,
    name: &'static str,
    default: f64,
    minimum: f64,
    maximum: f64,
) -> Result<f64, HighlightError> {
    let value = inputs.get(name).and_then(Value::as_f64).unwrap_or(default);
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(HighlightError::new(
            "number input is outside its allowed range",
        ))
    }
}
fn required_number(object: &Map<String, Value>, name: &'static str) -> Result<f64, HighlightError> {
    object
        .get(name)
        .and_then(Value::as_f64)
        .ok_or_else(|| HighlightError::new("VideoScore segment is missing a numeric field"))
}

fn evidence_key() -> Result<Vec<u8>, HighlightError> {
    let key = env::var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY")
        .map_err(|_| HighlightError::new("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY is required"))?;
    if key
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .count()
        < 32
    {
        return Err(HighlightError::new(
            "LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY requires at least 32 non-whitespace bytes",
        ));
    }
    Ok(key.into_bytes())
}

/// Sign one normalized candidate for downstream LightFlow workflow-contract tests.
/// The caller supplies the runtime-only key; this function never reads or exposes it.
pub fn sign_videoscore_evidence(
    key: &[u8],
    source_path: &str,
    start: f64,
    end: f64,
    score: f64,
    reason: &str,
) -> String {
    let signing_key = hmac::Key::new(hmac::HMAC_SHA256, key);
    let tag = hmac::sign(
        &signing_key,
        &evidence_message(source_path, VIDEOSCORE_MODEL, start, end, score, reason),
    );
    lowercase_hex(tag.as_ref())
}

fn evidence_message(
    source_path: &str,
    model: &str,
    start: f64,
    end: f64,
    score: f64,
    reason: &str,
) -> Vec<u8> {
    let mut message = b"lightflow.videoscore.evidence.v1\0".to_vec();
    for value in [WORKFLOW_ID, source_path, model, reason] {
        message.extend_from_slice(&(value.len() as u64).to_be_bytes());
        message.extend_from_slice(value.as_bytes());
    }
    for value in [start, end, score] {
        message.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    message
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}
fn implementation_identity() -> String {
    format!(
        "lightflow.video_highlights.rust.fnv1a64:{:016x}",
        digest(include_bytes!("lib.rs"))
    )
}
const fn digest(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
        i += 1;
    }
    hash
}
#[derive(Debug)]
pub struct HighlightError(String);
impl HighlightError {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    fn owned(value: String) -> Self {
        Self(value)
    }
}
impl std::fmt::Display for HighlightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for HighlightError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_is_ranked_and_non_overlapping() {
        let chosen = select(
            vec![
                json!({"start_seconds": 0.0, "end_seconds": 12.0, "score": 3.7}),
                json!({"start_seconds": 6.0, "end_seconds": 18.0, "score": 3.8}),
                json!({"start_seconds": 20.0, "end_seconds": 32.0, "score": 3.6}),
            ],
            2,
            2.5,
            12.0,
        );
        assert_eq!(chosen.len(), 2);
        assert_eq!(chosen[0]["start_seconds"], 6.0);
        assert_eq!(chosen[1]["start_seconds"], 20.0);
    }

    #[test]
    fn clips_adapter_preserves_signed_highlight_and_clip_range() {
        let highlight = json!({
            "workflow": WORKFLOW_ID,
            "source_path": "media/source.mp4",
            "start_seconds": 4.0,
            "end_seconds": 12.0,
            "score": 3.6,
            "model": VIDEOSCORE_MODEL,
            "reason": "Clear vehicle view.",
            "evidence": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        });
        let clips = clips_from_highlights(std::slice::from_ref(&highlight)).expect("clip adapter");
        assert_eq!(clips[0]["id"], "highlight-1");
        assert_eq!(clips[0]["path"], highlight["source_path"]);
        assert_eq!(clips[0]["start"], highlight["start_seconds"]);
        assert_eq!(clips[0]["end"], highlight["end_seconds"]);
        assert_eq!(clips[0]["highlight"], highlight);
    }
    #[test]
    fn endpoint_requires_https() {
        assert!(endpoint("http://localhost:8080").is_err());
        assert_eq!(
            endpoint("https://score.example.test/api").expect("https endpoint"),
            "https://score.example.test/api/v1/highlights"
        );
    }

    #[test]
    fn evidence_tag_round_trips_and_rejects_tampering() {
        let key = b"0123456789abcdef0123456789abcdef";
        let evidence = sign_videoscore_evidence(
            key,
            "media/source.mp4",
            1.0,
            4.0,
            3.4,
            "Clear vehicle shot.",
        );
        assert_eq!(evidence.len(), 64);
        assert!(
            evidence
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        );
        let signing_key = hmac::Key::new(hmac::HMAC_SHA256, key);
        let message = evidence_message(
            "media/source.mp4",
            VIDEOSCORE_MODEL,
            1.0,
            4.0,
            3.4,
            "Clear vehicle shot.",
        );
        let tag = hmac::sign(&signing_key, &message);
        assert!(hmac::verify(&signing_key, &message, tag.as_ref()).is_ok());
        assert!(
            hmac::verify(
                &signing_key,
                &evidence_message(
                    "media/source.mp4",
                    VIDEOSCORE_MODEL,
                    1.0,
                    4.0,
                    3.4,
                    "Different reason.",
                ),
                tag.as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn evidence_key_requires_32_non_whitespace_bytes() {
        let previous = env::var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY").ok();
        unsafe { env::set_var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY", "too-short") };
        assert!(evidence_key().is_err());
        unsafe {
            env::set_var(
                "LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY",
                "0123456789abcdef0123456789abcdef",
            )
        };
        assert!(evidence_key().is_ok());
        if let Some(previous) = previous {
            unsafe { env::set_var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY", previous) };
        } else {
            unsafe { env::remove_var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY") };
        }
    }
}
