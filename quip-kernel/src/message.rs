//! Jupyter wire-protocol messages.
//!
//! On-the-wire, each message is a multipart ZeroMQ frame:
//!
//! ```text
//! [zmq ids...]
//! b"<IDS|MSG>"
//! <hex-encoded hmac-sha256 signature>
//! <header json>
//! <parent_header json>
//! <metadata json>
//! <content json>
//! <extra buffers...>
//! ```
//!
//! The signature is computed over the concatenation of header, parent_header,
//! metadata, and content (the four JSON byte strings, in order).

use bytes::Bytes;
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

pub const DELIMITER: &[u8] = b"<IDS|MSG>";
pub const PROTOCOL_VERSION: &str = "5.3";

type HmacSha256 = Hmac<Sha256>;

/// The unserialized JSON header for a Jupyter message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
  pub msg_id: String,
  pub session: String,
  #[serde(default)]
  pub username: String,
  pub date: String,
  pub msg_type: String,
  pub version: String,
}

impl Header {
  pub fn new(msg_type: impl Into<String>, session: impl Into<String>) -> Self {
    Self {
      msg_id: Uuid::new_v4().to_string(),
      session: session.into(),
      username: "kernel".to_string(),
      date: Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
      msg_type: msg_type.into(),
      version: PROTOCOL_VERSION.to_string(),
    }
  }
}

/// A fully parsed Jupyter message, including the ZeroMQ routing frames that
/// precede the `<IDS|MSG>` delimiter.
#[derive(Debug, Clone)]
pub struct Message {
  /// Opaque ZeroMQ routing identities (ROUTER socket prefix frames).
  pub identities: Vec<Vec<u8>>,
  pub header: Header,
  pub parent_header: Option<Header>,
  pub metadata: Value,
  pub content: Value,
  pub buffers: Vec<Vec<u8>>,
}

impl Message {
  /// Construct a new outgoing message with the given header and JSON content.
  pub fn new(header: Header, content: Value) -> Self {
    Self {
      identities: Vec::new(),
      header,
      parent_header: None,
      metadata: Value::Object(Default::default()),
      content,
      buffers: Vec::new(),
    }
  }

  /// Build a reply in response to this incoming message. Copies routing
  /// identities and sets `parent_header` appropriately.
  pub fn reply(&self, msg_type: impl Into<String>, content: Value) -> Message {
    Message {
      identities: self.identities.clone(),
      header: Header::new(msg_type, &self.header.session),
      parent_header: Some(self.header.clone()),
      metadata: Value::Object(Default::default()),
      content,
      buffers: Vec::new(),
    }
  }

  /// Serialize the message into the sequence of frames required for sending
  /// on a ZeroMQ socket. The returned vector includes the routing identities,
  /// the `<IDS|MSG>` delimiter, the signature, the four JSON parts, and any
  /// trailing buffers.
  pub fn into_frames(self, signing_key: &[u8]) -> Vec<Bytes> {
    let header =
      serde_json::to_vec(&self.header).unwrap_or_else(|_| b"{}".to_vec());
    let parent_header = match &self.parent_header {
      Some(p) => serde_json::to_vec(p).unwrap_or_else(|_| b"{}".to_vec()),
      None => b"{}".to_vec(),
    };
    let metadata =
      serde_json::to_vec(&self.metadata).unwrap_or_else(|_| b"{}".to_vec());
    let content =
      serde_json::to_vec(&self.content).unwrap_or_else(|_| b"{}".to_vec());

    let signature =
      sign(signing_key, &[&header, &parent_header, &metadata, &content]);

    let mut frames: Vec<Bytes> = Vec::new();
    for id in self.identities {
      frames.push(Bytes::from(id));
    }
    frames.push(Bytes::from_static(DELIMITER));
    frames.push(Bytes::from(signature.into_bytes()));
    frames.push(Bytes::from(header));
    frames.push(Bytes::from(parent_header));
    frames.push(Bytes::from(metadata));
    frames.push(Bytes::from(content));
    for buf in self.buffers {
      frames.push(Bytes::from(buf));
    }
    frames
  }

  /// Parse an incoming multipart message from the raw ZMQ frames. The key is
  /// the HMAC signing key from the connection file; if it is non-empty, the
  /// signature is validated.
  pub fn from_frames(
    frames: Vec<Bytes>,
    signing_key: &[u8],
  ) -> Result<Message, ParseError> {
    let delim_pos = frames
      .iter()
      .position(|f| f.as_ref() == DELIMITER)
      .ok_or(ParseError::MissingDelimiter)?;

    let identities: Vec<Vec<u8>> =
      frames[..delim_pos].iter().map(|b| b.to_vec()).collect();

    // Need at least: <IDS|MSG>, signature, header, parent, meta, content.
    if frames.len() < delim_pos + 6 {
      return Err(ParseError::TooFewFrames(frames.len()));
    }

    let signature = &frames[delim_pos + 1];
    let header_b = &frames[delim_pos + 2];
    let parent_b = &frames[delim_pos + 3];
    let metadata_b = &frames[delim_pos + 4];
    let content_b = &frames[delim_pos + 5];
    let buffers: Vec<Vec<u8>> =
      frames[delim_pos + 6..].iter().map(|b| b.to_vec()).collect();

    if !signing_key.is_empty() {
      verify(
        signing_key,
        signature,
        &[header_b, parent_b, metadata_b, content_b],
      )?;
    }

    let header: Header =
      serde_json::from_slice(header_b).map_err(ParseError::BadHeader)?;
    let parent_header: Option<Header> =
      parse_optional_header(parent_b).map_err(ParseError::BadHeader)?;
    let metadata: Value =
      serde_json::from_slice(metadata_b).unwrap_or(Value::Null);
    let content: Value =
      serde_json::from_slice(content_b).unwrap_or(Value::Null);

    Ok(Message {
      identities,
      header,
      parent_header,
      metadata,
      content,
      buffers,
    })
  }
}

fn parse_optional_header(
  bytes: &[u8],
) -> Result<Option<Header>, serde_json::Error> {
  // Jupyter sends an empty object `{}` when there is no parent.
  let val: Value = serde_json::from_slice(bytes)?;
  match val {
    Value::Object(ref map) if map.is_empty() => Ok(None),
    Value::Null => Ok(None),
    _ => Ok(Some(serde_json::from_value(val)?)),
  }
}

fn sign(key: &[u8], parts: &[&[u8]]) -> String {
  let mut mac =
    HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
  for p in parts {
    mac.update(p);
  }
  hex::encode(mac.finalize().into_bytes())
}

fn verify(
  key: &[u8],
  provided: &[u8],
  parts: &[&[u8]],
) -> Result<(), ParseError> {
  let expected = sign(key, parts);
  // Case-insensitive compare on hex digest.
  let provided_str =
    std::str::from_utf8(provided).map_err(|_| ParseError::BadSignature)?;
  if expected.eq_ignore_ascii_case(provided_str) {
    Ok(())
  } else {
    Err(ParseError::BadSignature)
  }
}

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
  #[error("message missing <IDS|MSG> delimiter")]
  MissingDelimiter,
  #[error("message has too few frames ({0})")]
  TooFewFrames(usize),
  #[error("invalid HMAC signature")]
  BadSignature,
  #[error("failed to parse message header: {0}")]
  BadHeader(serde_json::Error),
}

#[cfg(test)]
mod tests {
  use super::*;

  const KEY: &[u8] = b"secret-key";

  #[test]
  fn round_trip_signs_and_parses() {
    let mut msg = Message::new(
      Header::new("kernel_info_request", "sess-1"),
      serde_json::json!({}),
    );
    msg.identities = vec![b"abc".to_vec()];
    let frames = msg.clone().into_frames(KEY);
    let parsed = Message::from_frames(frames, KEY).unwrap();
    assert_eq!(parsed.header.msg_type, "kernel_info_request");
    assert_eq!(parsed.identities, vec![b"abc".to_vec()]);
  }

  #[test]
  fn bad_signature_is_rejected() {
    let msg = Message::new(
      Header::new("execute_request", "sess-1"),
      serde_json::json!({"code": "(+ 1 2)"}),
    );
    let frames = msg.into_frames(KEY);
    let err = Message::from_frames(frames, b"different-key").unwrap_err();
    matches!(err, ParseError::BadSignature);
  }

  #[test]
  fn empty_key_skips_verification() {
    let msg =
      Message::new(Header::new("execute_request", "s"), serde_json::json!({}));
    let frames = msg.into_frames(KEY);
    Message::from_frames(frames, b"").unwrap();
  }
}
