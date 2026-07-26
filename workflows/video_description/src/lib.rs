use std::env;
use std::io::Read;
use std::time::Duration;

use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value, json};

pub const WORKFLOW_ID: &str = "lightflow.video_description";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

pub fn define() -> WorkflowSpec {
    workflow! {
    name: "Video Description", description: "Model-generated, fact-checked account-specific video descriptions.",
    input "account_group": "text" { description: "zh for the Chinese account, overseas for the Russian account.", required: true, widget: "select", choices: ["zh", "overseas"], }
    input "facts": "json" { description: "Frozen verified facts only; model output is checked against them.", required: true, widget: "json", }
    input "transcript": "text" { description: "Approved edited transcript.", required: true, widget: "textarea", }
    output "description": "json" { description: "Fact-checked model title, body, and hashtags." }
    output "summary": "text" { description: "Rust-native description workflow summary." }
}.builtin_runtime("command", "lightflow.command.run", "runner.v1").build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, DescriptionError> {
    let group = required_text(inputs, "account_group")?;
    if !matches!(group, "zh" | "overseas") {
        return Err(DescriptionError::new(
            "account_group must be zh or overseas",
        ));
    }
    let facts = inputs
        .get("facts")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| DescriptionError::new("facts must be a JSON object"))?;
    let transcript = required_text(inputs, "transcript")?;
    let endpoint = endpoint(&env::var("LIGHTFLOW_DESCRIPTION_API_URL").unwrap_or_default())?;
    let token = env::var("LIGHTFLOW_DESCRIPTION_API_TOKEN").unwrap_or_default();
    let model = env::var("LIGHTFLOW_DESCRIPTION_MODEL").unwrap_or_default();
    if token.is_empty() || model.is_empty() {
        return Err(DescriptionError::new(
            "LIGHTFLOW_DESCRIPTION_API_TOKEN and LIGHTFLOW_DESCRIPTION_MODEL must be configured",
        ));
    }
    let language = if group == "zh" {
        "Chinese"
    } else {
        "Russian; English keywords are allowed"
    };
    let prompt = format!(
        "Write a concise {language} social-video title, description, and 3-8 hashtags. Use only these verified facts: {}. Approved transcript: {transcript}. Return JSON only: {{\"title\":string,\"description\":string,\"hashtags\":[string]}}.",
        Value::Object(facts.clone())
    );
    let request = json!({"model": model, "temperature": 0.2, "messages": [{"role":"system","content":"Never invent facts. Return JSON only."},{"role":"user","content":prompt}]});
    let response = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(90))
        .build()
        .post(&endpoint)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_json(request)
        .map_err(|error| {
            DescriptionError::owned(format!("description model request failed: {error}"))
        })?;
    let mut reader = response.into_reader();
    let mut raw = Vec::new();
    (&mut reader)
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|error| {
            DescriptionError::owned(format!("description model response failed: {error}"))
        })?;
    if raw.len() > MAX_RESPONSE_BYTES {
        return Err(DescriptionError::new(
            "description model response exceeds 2 MiB",
        ));
    }
    let envelope: Value = lightflow::serde_json::from_slice(&raw).map_err(|error| {
        DescriptionError::owned(format!(
            "description model response is invalid JSON: {error}"
        ))
    })?;
    let content = envelope
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| DescriptionError::new("description model response is missing content"))?;
    let description: Value = lightflow::serde_json::from_str(content.trim()).map_err(|error| {
        DescriptionError::owned(format!("description model content is not JSON: {error}"))
    })?;
    validate_description(&description, &facts)?;
    Ok(Response {
        outputs: Map::from_iter([
            ("description".to_owned(), description),
            (
                "summary".to_owned(),
                "Model-generated description passed fact validation.".into(),
            ),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::from_iter([(
            "implementation".to_owned(),
            implementation_identity().into(),
        )]),
    })
}

fn validate_description(value: &Value, facts: &Map<String, Value>) -> Result<(), DescriptionError> {
    let object = value
        .as_object()
        .ok_or_else(|| DescriptionError::new("description must be a JSON object"))?;
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DescriptionError::new("description.title must be non-empty"))?;
    let body = object
        .get("description")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DescriptionError::new("description.description must be non-empty"))?;
    let tags = object
        .get("hashtags")
        .and_then(Value::as_array)
        .filter(|tags| (3..=8).contains(&tags.len()) && tags.iter().all(Value::is_string))
        .ok_or_else(|| {
            DescriptionError::new("description.hashtags must contain 3 through 8 strings")
        })?;
    let allowed_numbers = numbers(&Value::Object(facts.clone()).to_string());
    for number in numbers(&format!(
        "{title} {body} {}",
        tags.iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    )) {
        if !allowed_numbers.contains(&number) {
            return Err(DescriptionError::owned(format!(
                "model introduced unverified number: {number}"
            )));
        }
    }
    Ok(())
}
fn numbers(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_digit() && character != '.')
        .filter(|part| !part.is_empty() && part.chars().any(|character| character.is_ascii_digit()))
        .map(str::to_owned)
        .collect()
}
fn endpoint(base: &str) -> Result<String, DescriptionError> {
    let value = base.trim().trim_end_matches('/');
    let url = url::Url::parse(value)
        .map_err(|_| DescriptionError::new("LIGHTFLOW_DESCRIPTION_API_URL must be an https URL"))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(DescriptionError::new(
            "LIGHTFLOW_DESCRIPTION_API_URL must be an https URL",
        ));
    }
    Ok(format!("{value}/v1/chat/completions"))
}
fn required_text<'a>(
    inputs: &'a Map<String, Value>,
    name: &'static str,
) -> Result<&'a str, DescriptionError> {
    inputs
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .ok_or_else(|| DescriptionError::new("missing required text input"))
}
fn implementation_identity() -> String {
    format!(
        "lightflow.video_description.rust.fnv1a64:{:016x}",
        digest(include_bytes!("lib.rs"))
    )
}
const fn digest(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
        index += 1;
    }
    hash
}
#[derive(Debug)]
pub struct DescriptionError(String);
impl DescriptionError {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    fn owned(value: String) -> Self {
        Self(value)
    }
}
impl std::fmt::Display for DescriptionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for DescriptionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn description(title: &str, body: &str) -> Value {
        json!({
            "title": title,
            "description": body,
            "hashtags": ["#pickup", "#usedcar", "#diesel"]
        })
    }

    #[test]
    fn accepts_numbers_present_in_verified_facts() {
        let facts = lightflow::serde_json::from_value(json!({
            "price_wan": 2.8,
            "year": 2020
        }))
        .expect("test facts are an object");

        assert!(validate_description(&description("2.8万皮卡", "2020年柴油车"), &facts).is_ok());
    }

    #[test]
    fn rejects_numbers_not_present_in_verified_facts() {
        let facts = lightflow::serde_json::from_value(json!({"price_wan": 2.8}))
            .expect("test facts are an object");

        let error = validate_description(&description("3.5万皮卡", "柴油车"), &facts)
            .expect_err("unverified price must be rejected");

        assert_eq!(error.to_string(), "model introduced unverified number: 3.5");
    }

    #[test]
    fn endpoint_requires_https() {
        assert!(endpoint("http://localhost:8080").is_err());
        assert_eq!(
            endpoint("https://models.example.test/api").expect("https endpoint is accepted"),
            "https://models.example.test/api/v1/chat/completions"
        );
    }
}
