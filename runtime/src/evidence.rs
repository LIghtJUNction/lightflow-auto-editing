use lightflow::serde_json::{Map, Value};
use ring::hmac;

use crate::RuntimeError;
use crate::media::{number, text};

pub(crate) const VIDEOSCORE_WORKFLOW: &str = "lightflow.video_highlights";
pub(crate) const VIDEOSCORE_MODEL: &str = "TIGER-Lab/VideoScore-v1.1";

pub(crate) struct EvidenceFields<'a> {
    pub(crate) source_path: &'a str,
    pub(crate) model: &'a str,
    pub(crate) start: f64,
    pub(crate) end: f64,
    pub(crate) score: f64,
    pub(crate) reason: &'a str,
}

pub(crate) fn evidence_key() -> Result<Vec<u8>, RuntimeError> {
    let key = std::env::var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY")
        .map_err(|_| RuntimeError::new("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY is required"))?;
    if key
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .count()
        < 32
    {
        return Err(RuntimeError::new(
            "LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY requires at least 32 non-whitespace bytes",
        ));
    }
    Ok(key.into_bytes())
}

pub(crate) fn verify_evidence(
    key: &[u8],
    fields: &EvidenceFields<'_>,
    evidence: &str,
) -> Result<(), RuntimeError> {
    if evidence.len() != 64
        || !evidence
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(RuntimeError::new(
            "VideoScore evidence must be lowercase hexadecimal HMAC-SHA256",
        ));
    }
    let expected = decode_lowercase_hex(evidence)?;
    let signing_key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hmac::verify(&signing_key, &evidence_message(fields), &expected)
        .map_err(|_| RuntimeError::new("VideoScore evidence HMAC verification failed"))
}

pub(crate) fn verify_highlight_signature(
    clip: &Map<String, Value>,
    index: usize,
    key: &[u8],
) -> Result<(), RuntimeError> {
    let highlight = clip
        .get("highlight")
        .and_then(Value::as_object)
        .ok_or_else(|| RuntimeError::new(format!("clips[{index}].highlight must be an object")))?;
    let source_path = text(
        highlight.get("source_path").ok_or_else(|| {
            RuntimeError::new(format!("clips[{index}].highlight.source_path missing"))
        })?,
        &format!("clips[{index}].highlight.source_path"),
    )?;
    let model = text(
        highlight
            .get("model")
            .ok_or_else(|| RuntimeError::new(format!("clips[{index}].highlight.model missing")))?,
        &format!("clips[{index}].highlight.model"),
    )?;
    if model != VIDEOSCORE_MODEL {
        return Err(RuntimeError::new(format!(
            "clips[{index}].highlight.model must be {VIDEOSCORE_MODEL}"
        )));
    }
    let workflow = text(
        highlight.get("workflow").ok_or_else(|| {
            RuntimeError::new(format!("clips[{index}].highlight.workflow missing"))
        })?,
        &format!("clips[{index}].highlight.workflow"),
    )?;
    if workflow != VIDEOSCORE_WORKFLOW {
        return Err(RuntimeError::new(format!(
            "clips[{index}].highlight.workflow must be {VIDEOSCORE_WORKFLOW}"
        )));
    }
    let evidence = text(
        highlight.get("evidence").ok_or_else(|| {
            RuntimeError::new(format!("clips[{index}].highlight.evidence missing"))
        })?,
        &format!("clips[{index}].highlight.evidence"),
    )?;
    let fields = EvidenceFields {
        source_path,
        model,
        start: number(
            highlight.get("start_seconds"),
            &format!("clips[{index}].highlight.start_seconds"),
        )?,
        end: number(
            highlight.get("end_seconds"),
            &format!("clips[{index}].highlight.end_seconds"),
        )?,
        score: number(
            highlight.get("score"),
            &format!("clips[{index}].highlight.score"),
        )?,
        reason: text(
            highlight.get("reason").ok_or_else(|| {
                RuntimeError::new(format!("clips[{index}].highlight.reason missing"))
            })?,
            &format!("clips[{index}].highlight.reason"),
        )?,
    };
    verify_evidence(key, &fields, evidence)
}

pub(crate) fn evidence_message(fields: &EvidenceFields<'_>) -> Vec<u8> {
    let mut message = b"lightflow.videoscore.evidence.v1\0".to_vec();
    for value in [
        VIDEOSCORE_WORKFLOW,
        fields.source_path,
        fields.model,
        fields.reason,
    ] {
        message.extend_from_slice(&(value.len() as u64).to_be_bytes());
        message.extend_from_slice(value.as_bytes());
    }
    for value in [fields.start, fields.end, fields.score] {
        message.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    message
}

fn decode_lowercase_hex(value: &str) -> Result<Vec<u8>, RuntimeError> {
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_nibble(value: u8) -> Result<u8, RuntimeError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(RuntimeError::new(
            "VideoScore evidence must be lowercase hexadecimal HMAC-SHA256",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_round_trips_and_rejects_tampering() {
        let key = b"0123456789abcdef0123456789abcdef";
        let fields = EvidenceFields {
            source_path: "media/source.mp4",
            model: VIDEOSCORE_MODEL,
            start: 1.0,
            end: 4.0,
            score: 3.4,
            reason: "Clear vehicle shot.",
        };
        let signing_key = hmac::Key::new(hmac::HMAC_SHA256, key);
        let tag = hmac::sign(&signing_key, &evidence_message(&fields));
        let evidence = tag
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert!(verify_evidence(key, &fields, &evidence).is_ok());
        let tampered = EvidenceFields {
            reason: "Different reason.",
            ..fields
        };
        assert!(verify_evidence(key, &tampered, &evidence).is_err());
    }
}
