use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use lightflow::serde_json::{Map, Value, json};

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOKEN_FILE_BYTES: usize = 4 * 1024;
const JSON_RPC_VERSION: &str = "2.0";
const REQUEST_ID: &str = "lightflow-video-work-api";

pub fn call(action: &str, mut arguments: Map<String, Value>) -> Result<Map<String, Value>, Error> {
    let base_url = env::var("LIGHTFLOW_VIDEO_WORK_API_URL").unwrap_or_default();
    let endpoint = endpoint(&base_url)?;
    let token = configured_token(
        env::var("LIGHTFLOW_VIDEO_WORK_API_TOKEN").ok(),
        env::var("LIGHTFLOW_VIDEO_WORK_API_TOKEN_FILE").ok(),
    )?;
    arguments.insert("action".to_owned(), action.into());
    let payload = json!({
        "jsonrpc": JSON_RPC_VERSION,
        "id": REQUEST_ID,
        "method": "tools/call",
        "params": {"name": "video_editor", "arguments": arguments},
    });
    let response = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(120))
        .build()
        .post(&endpoint)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_json(payload)
        .map_err(|error| Error::owned(format!("Video Work API MCP request failed: {error}")))?;
    let mut reader = response.into_reader();
    let mut body = Vec::new();
    (&mut reader)
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| Error::owned(format!("Video Work API MCP response failed: {error}")))?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(Error::new("Video Work API MCP response exceeds 8 MiB"));
    }
    let response: Value = lightflow::serde_json::from_slice(&body).map_err(|error| {
        Error::owned(format!(
            "Video Work API MCP response is invalid JSON: {error}"
        ))
    })?;
    decode_response(&response)
}

fn configured_token(
    environment_token: Option<String>,
    token_file: Option<String>,
) -> Result<String, Error> {
    if let Some(token) = environment_token.filter(|token| !token.trim().is_empty()) {
        return Ok(token);
    }
    let Some(token_file) = token_file.filter(|path| !path.trim().is_empty()) else {
        return Err(Error::new("Video Work API MCP token is not configured"));
    };
    read_token_file(Path::new(&token_file))
}

fn read_token_file(path: &Path) -> Result<String, Error> {
    let mut file = open_token_file(path)?;
    let metadata = file
        .metadata()
        .map_err(|_| Error::new("Video Work API MCP token file is unavailable"))?;
    if !metadata.is_file() {
        return Err(Error::new(
            "Video Work API MCP token file must be a regular file",
        ));
    }

    let mut bytes = Vec::new();
    (&mut file)
        .take((MAX_TOKEN_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::new("Video Work API MCP token file is unavailable"))?;
    if bytes.len() > MAX_TOKEN_FILE_BYTES {
        return Err(Error::new("Video Work API MCP token file exceeds 4 KiB"));
    }
    let token = String::from_utf8(bytes)
        .map_err(|_| Error::new("Video Work API MCP token file must be valid UTF-8"))?;
    let token = token.trim();
    if token.is_empty() {
        return Err(Error::new(
            "Video Work API MCP token file must not be empty",
        ));
    }
    Ok(token.to_owned())
}

fn open_token_file(path: &Path) -> Result<fs::File, Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| Error::new("Video Work API MCP token file is unavailable"))
    }
    #[cfg(not(unix))]
    {
        fs::File::open(path).map_err(|_| Error::new("Video Work API MCP token file is unavailable"))
    }
}

fn decode_response(response: &Value) -> Result<Map<String, Value>, Error> {
    if response.get("error").is_some() {
        return Err(Error::new("Video Work API MCP returned an error"));
    }
    if response.get("jsonrpc").and_then(Value::as_str) != Some(JSON_RPC_VERSION) {
        return Err(Error::new(
            "Video Work API MCP response has an invalid jsonrpc version",
        ));
    }
    if response.get("id").and_then(Value::as_str) != Some(REQUEST_ID) {
        return Err(Error::new(
            "Video Work API MCP response has an unexpected id",
        ));
    }
    let result = response
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::new("Video Work API MCP response is missing object result"))?;
    result
        .get("structuredContent")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| {
            Error::new("Video Work API MCP response is missing object result.structuredContent")
        })
}

fn endpoint(base_url: &str) -> Result<String, Error> {
    let value = base_url.trim().trim_end_matches('/');
    let url = url::Url::parse(value)
        .map_err(|_| Error::new("LIGHTFLOW_VIDEO_WORK_API_URL must be an http(s) URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(Error::new(
            "LIGHTFLOW_VIDEO_WORK_API_URL must be an http(s) URL",
        ));
    }
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if url.scheme() == "http" && !loopback {
        return Err(Error::new(
            "LIGHTFLOW_VIDEO_WORK_API_URL must use HTTPS unless it targets loopback",
        ));
    }
    Ok(format!("{value}/mcp"))
}

#[derive(Debug)]
pub struct Error(String);
impl Error {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    fn owned(value: String) -> Self {
        Self(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_video_work_json_rpc_wrapper() {
        let response = json!({
            "jsonrpc": JSON_RPC_VERSION,
            "id": REQUEST_ID,
            "result": {
                "content": [{"type": "text", "text": "ignored"}],
                "structuredContent": {
                    "id": "profile-1",
                    "speaker_id": "speaker-1",
                    "style_name": "Narration",
                    "duration_seconds": 4.2
                }
            }
        });

        let payload = decode_response(&response).unwrap();
        assert_eq!(payload.get("id"), Some(&Value::String("profile-1".into())));
    }

    #[test]
    fn rejects_invalid_json_rpc_envelopes_and_contract_layers() {
        for response in [
            json!({"id": REQUEST_ID, "result": {"structuredContent": {}}}),
            json!({"jsonrpc": "1.0", "id": REQUEST_ID, "result": {"structuredContent": {}}}),
            json!({"jsonrpc": JSON_RPC_VERSION, "result": {"structuredContent": {}}}),
            json!({"jsonrpc": JSON_RPC_VERSION, "id": "other", "result": {"structuredContent": {}}}),
            json!({"jsonrpc": JSON_RPC_VERSION, "id": REQUEST_ID, "error": {"code": -32000, "message": "failed"}}),
            json!({"jsonrpc": JSON_RPC_VERSION, "id": REQUEST_ID}),
            json!({"jsonrpc": JSON_RPC_VERSION, "id": REQUEST_ID, "result": []}),
            json!({"jsonrpc": JSON_RPC_VERSION, "id": REQUEST_ID, "result": {}}),
            json!({"jsonrpc": JSON_RPC_VERSION, "id": REQUEST_ID, "result": {"structuredContent": []}}),
        ] {
            assert!(decode_response(&response).is_err());
        }
    }

    #[test]
    fn environment_token_takes_precedence_over_the_token_file() {
        let token = configured_token(
            Some("environment-token".to_owned()),
            Some("/this/path/is-not-read".to_owned()),
        )
        .unwrap();

        assert_eq!(token, "environment-token");
    }

    #[test]
    fn token_file_is_trimmed_before_use() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token");
        fs::write(&path, "  file-token\n").unwrap();

        let token = configured_token(None, Some(path.display().to_string())).unwrap();

        assert_eq!(token, "file-token");
    }

    #[test]
    fn blank_environment_token_falls_back_to_the_token_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token");
        fs::write(&path, "file-token").unwrap();

        let token =
            configured_token(Some("   ".to_owned()), Some(path.display().to_string())).unwrap();

        assert_eq!(token, "file-token");
    }

    #[cfg(unix)]
    #[test]
    fn token_file_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("token-target");
        let path = directory.path().join("token-link");
        fs::write(&target, "file-token").unwrap();
        symlink(&target, &path).unwrap();

        let error = configured_token(None, Some(path.display().to_string()))
            .unwrap_err()
            .to_string();

        assert_eq!(error, "Video Work API MCP token file is unavailable");
        assert!(!error.contains("token-link"));
    }

    #[test]
    fn token_file_failures_do_not_expose_file_contents() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing-secret-file");
        let oversized = directory.path().join("oversized-secret-file");
        let invalid_utf8 = directory.path().join("invalid-utf8-secret-file");
        fs::write(&oversized, vec![b'x'; MAX_TOKEN_FILE_BYTES + 1]).unwrap();
        fs::write(&invalid_utf8, [0xff, 0xfe]).unwrap();

        for path in [&missing, &oversized, &invalid_utf8] {
            let error = configured_token(None, Some(path.display().to_string()))
                .unwrap_err()
                .to_string();
            assert!(!error.contains("secret-file"));
        }

        assert_eq!(
            configured_token(None, Some(missing.display().to_string()))
                .unwrap_err()
                .to_string(),
            "Video Work API MCP token file is unavailable"
        );
        assert_eq!(
            configured_token(None, Some(oversized.display().to_string()))
                .unwrap_err()
                .to_string(),
            "Video Work API MCP token file exceeds 4 KiB"
        );
        assert_eq!(
            configured_token(None, Some(invalid_utf8.display().to_string()))
                .unwrap_err()
                .to_string(),
            "Video Work API MCP token file must be valid UTF-8"
        );
    }
}
