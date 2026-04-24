//! Thin wrappers around the raw `zeromq` sockets for Jupyter's 5-channel model.
//!
//! The channels are:
//!
//! - **heartbeat** (REP): trivial echo of incoming payloads
//! - **shell** (ROUTER): request/reply for `execute_request`, `kernel_info_request`, etc.
//! - **control** (ROUTER): higher priority channel; `shutdown_request`, `interrupt_request`
//! - **stdin** (ROUTER): kernel → frontend prompts; unused for Quip right now
//! - **iopub** (PUB): broadcasted kernel-side output (`status`, `stream`, `execute_result`, ...)

use bytes::Bytes;
use zeromq::{
  PubSocket, RepSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqError,
  ZmqMessage,
};

use super::{
  connection::ConnectionInfo,
  message::{Message, ParseError},
};

pub struct KernelSockets {
  pub shell: RouterSocket,
  pub control: RouterSocket,
  pub iopub: PubSocket,
  pub stdin: RouterSocket,
  pub heartbeat: RepSocket,
}

impl KernelSockets {
  pub async fn bind(info: &ConnectionInfo) -> Result<Self, SocketError> {
    let mut shell = RouterSocket::new();
    shell.bind(&info.shell_endpoint()).await?;

    let mut control = RouterSocket::new();
    control.bind(&info.control_endpoint()).await?;

    let mut iopub = PubSocket::new();
    iopub.bind(&info.iopub_endpoint()).await?;

    let mut stdin = RouterSocket::new();
    stdin.bind(&info.stdin_endpoint()).await?;

    let mut heartbeat = RepSocket::new();
    heartbeat.bind(&info.heartbeat_endpoint()).await?;

    Ok(KernelSockets {
      shell,
      control,
      iopub,
      stdin,
      heartbeat,
    })
  }
}

/// Receive a Jupyter message from a ROUTER socket (shell or control).
pub async fn recv_router(
  socket: &mut RouterSocket,
  key: &[u8],
) -> Result<Message, SocketError> {
  let raw = socket.recv().await?;
  let frames: Vec<Bytes> = raw.into_vec();
  Ok(Message::from_frames(frames, key)?)
}

/// Send a reply on a ROUTER socket. The message must already have its
/// `identities` populated (typically copied from an incoming request).
pub async fn send_router(
  socket: &mut RouterSocket,
  key: &[u8],
  msg: Message,
) -> Result<(), SocketError> {
  let frames = msg.into_frames(key);
  let zmsg = ZmqMessage::try_from(frames)
    .map_err(|_| SocketError::Other("empty zmq message".to_string()))?;
  socket.send(zmsg).await?;
  Ok(())
}

/// Broadcast an iopub message. The `identities` field is ignored for PUB;
/// only the `<IDS|MSG>` delimiter and JSON frames are sent.
pub async fn send_iopub(
  socket: &mut PubSocket,
  key: &[u8],
  mut msg: Message,
) -> Result<(), SocketError> {
  msg.identities.clear();
  let frames = msg.into_frames(key);
  let zmsg = ZmqMessage::try_from(frames)
    .map_err(|_| SocketError::Other("empty zmq message".to_string()))?;
  socket.send(zmsg).await?;
  Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum SocketError {
  #[error("zmq error: {0}")]
  Zmq(#[from] ZmqError),
  #[error("message parse error: {0}")]
  Parse(#[from] ParseError),
  #[error("{0}")]
  Other(String),
}
