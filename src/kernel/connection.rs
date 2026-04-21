use std::path::Path;

use serde::{Deserialize, Serialize};

/// The JSON connection file passed to the kernel by the Jupyter client.
///
/// See <https://jupyter-client.readthedocs.io/en/stable/kernels.html#connection-files>.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
  pub transport: String,
  pub ip: String,
  pub shell_port: u16,
  pub iopub_port: u16,
  pub stdin_port: u16,
  pub control_port: u16,
  pub hb_port: u16,
  pub key: String,
  pub signature_scheme: String,
  #[serde(default)]
  pub kernel_name: String,
}

impl ConnectionInfo {
  pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConnectionError> {
    let bytes = std::fs::read(path.as_ref()).map_err(|e| {
      ConnectionError::Io(path.as_ref().display().to_string(), e)
    })?;
    Self::from_bytes(&bytes)
  }

  pub fn from_bytes(bytes: &[u8]) -> Result<Self, ConnectionError> {
    serde_json::from_slice(bytes).map_err(ConnectionError::Parse)
  }

  pub fn endpoint(&self, port: u16) -> String {
    format!("{}://{}:{}", self.transport, self.ip, port)
  }

  pub fn shell_endpoint(&self) -> String {
    self.endpoint(self.shell_port)
  }

  pub fn iopub_endpoint(&self) -> String {
    self.endpoint(self.iopub_port)
  }

  pub fn stdin_endpoint(&self) -> String {
    self.endpoint(self.stdin_port)
  }

  pub fn control_endpoint(&self) -> String {
    self.endpoint(self.control_port)
  }

  pub fn heartbeat_endpoint(&self) -> String {
    self.endpoint(self.hb_port)
  }
}

#[derive(thiserror::Error, Debug)]
pub enum ConnectionError {
  #[error("failed to read connection file '{0}': {1}")]
  Io(String, std::io::Error),
  #[error("failed to parse connection file: {0}")]
  Parse(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_example_connection_file() {
    let json = r#"{
      "transport": "tcp",
      "ip": "127.0.0.1",
      "shell_port": 50001,
      "iopub_port": 50002,
      "stdin_port": 50003,
      "control_port": 50004,
      "hb_port": 50005,
      "key": "abcdef",
      "signature_scheme": "hmac-sha256",
      "kernel_name": "quip"
    }"#;
    let info = ConnectionInfo::from_bytes(json.as_bytes()).unwrap();
    assert_eq!(info.shell_endpoint(), "tcp://127.0.0.1:50001");
    assert_eq!(info.key, "abcdef");
  }
}
