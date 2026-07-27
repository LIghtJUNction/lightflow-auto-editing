use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::protocol::GatewayError;

pub(crate) const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn request_sha256<T: Serialize>(value: &T) -> Result<String, GatewayError> {
    Ok(sha256_hex(&canonical_json(value)?))
}

pub(crate) fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, GatewayError> {
    let payload = canonical_json(value)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(GatewayError::new(
            "gateway frame exceeds the protocol limit",
        ));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| GatewayError::new("gateway frame length is not representable"))?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(crate) fn decode_frame<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, GatewayError> {
    if bytes.len() < 4 {
        return Err(GatewayError::new(
            "gateway response has no complete frame header",
        ));
    }
    let mut header = [0; 4];
    header.copy_from_slice(&bytes[..4]);
    let declared = u32::from_be_bytes(header) as usize;
    if declared == 0 || declared > MAX_FRAME_BYTES {
        return Err(GatewayError::new(
            "gateway response frame length is invalid",
        ));
    }
    let expected = 4_usize
        .checked_add(declared)
        .ok_or_else(|| GatewayError::new("gateway response frame length overflow"))?;
    if bytes.len() != expected {
        return Err(GatewayError::new(
            "gateway response must contain exactly one complete frame",
        ));
    }
    serde_json::from_slice(&bytes[4..])
        .map_err(|error| GatewayError::new(format!("gateway response JSON is invalid: {error}")))
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, GatewayError> {
    serde_json::to_vec(value).map_err(|error| {
        GatewayError::new(format!("gateway protocol serialization failed: {error}"))
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}
