use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DapMessage {
    Request {
        seq: u64,
        command: String,
        #[serde(default)]
        arguments: Value,
    },
    Response {
        seq: u64,
        request_seq: u64,
        success: bool,
        command: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        body: Value,
    },
    Event {
        seq: u64,
        event: String,
        #[serde(default)]
        body: Value,
    },
}

#[derive(Clone, Debug)]
pub struct DapEnvelope {
    pub content_length: usize,
    pub message: DapMessage,
}

pub fn encode_envelope(message: &DapMessage) -> Result<Vec<u8>, serde_json::Error> {
    let payload = serde_json::to_vec(message)?;
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    let mut out = header.into_bytes();
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_envelope(buffer: &[u8]) -> Result<Option<(DapEnvelope, usize)>, serde_json::Error> {
    let Some(header_end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Ok(None);
    };
    let header_bytes = &buffer[..header_end];
    let header_text = String::from_utf8_lossy(header_bytes);
    let mut len: Option<usize> = None;
    for line in header_text.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:")
            && let Ok(parsed) = value.trim().parse::<usize>()
        {
            len = Some(parsed);
            break;
        }
    }
    let Some(content_length) = len else {
        return Ok(None);
    };
    let payload_start = header_end + 4;
    if buffer.len() < payload_start + content_length {
        return Ok(None);
    }
    let payload = &buffer[payload_start..payload_start + content_length];
    let message = serde_json::from_slice::<DapMessage>(payload)?;
    Ok(Some((
        DapEnvelope {
            content_length,
            message,
        },
        payload_start + content_length,
    )))
}
