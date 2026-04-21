//! End-to-end tests for the Jupyter kernel.
//!
//! These tests spin up the kernel server inside the test process and talk to
//! it over the loopback interface using the same `zeromq` crate the kernel
//! uses. They cover the full Jupyter wire format round-trip (HMAC signing,
//! `<IDS|MSG>` framing, ROUTER/DEALER identity handling).

use std::{net::TcpListener, time::Duration};

use bytes::Bytes;
use quip::kernel::{
  connection::ConnectionInfo,
  message::{Header, Message},
  server,
};
use tokio::time::timeout;
use zeromq::{
  DealerSocket, Socket, SocketRecv, SocketSend, SubSocket, ZmqMessage,
};

/// Pick five free ports on localhost and return a connection info struct.
fn make_connection_info() -> ConnectionInfo {
  let mut ports = Vec::with_capacity(5);
  let mut listeners = Vec::new();
  for _ in 0..5 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    ports.push(port);
    listeners.push(l);
  }
  // Drop the listeners; a small race window remains but is fine for tests.
  drop(listeners);

  ConnectionInfo {
    transport: "tcp".to_string(),
    ip: "127.0.0.1".to_string(),
    shell_port: ports[0],
    iopub_port: ports[1],
    stdin_port: ports[2],
    control_port: ports[3],
    hb_port: ports[4],
    key: "test-signing-key".to_string(),
    signature_scheme: "hmac-sha256".to_string(),
    kernel_name: "quip".to_string(),
  }
}

async fn send_shell(dealer: &mut DealerSocket, key: &[u8], msg: Message) {
  let frames = msg.into_frames(key);
  let zmsg = ZmqMessage::try_from(frames).unwrap();
  dealer.send(zmsg).await.unwrap();
}

async fn recv_shell(dealer: &mut DealerSocket, key: &[u8]) -> Message {
  let zmsg = timeout(Duration::from_secs(5), dealer.recv())
    .await
    .expect("shell recv timeout")
    .expect("shell recv error");
  let frames: Vec<Bytes> = zmsg.into_vec();
  Message::from_frames(frames, key).expect("parse shell reply")
}

async fn recv_iopub(sub: &mut SubSocket, key: &[u8]) -> Message {
  let zmsg = timeout(Duration::from_secs(5), sub.recv())
    .await
    .expect("iopub recv timeout")
    .expect("iopub recv error");
  let frames: Vec<Bytes> = zmsg.into_vec();
  Message::from_frames(frames, key).expect("parse iopub msg")
}

async fn connect_clients(info: &ConnectionInfo) -> (DealerSocket, SubSocket) {
  // Wait briefly for the kernel's sockets to actually bind.
  for _ in 0..20 {
    tokio::time::sleep(Duration::from_millis(50)).await;
    if std::net::TcpStream::connect(("127.0.0.1", info.shell_port)).is_ok() {
      break;
    }
  }

  let mut shell = DealerSocket::new();
  shell.connect(&info.shell_endpoint()).await.unwrap();

  let mut iopub = SubSocket::new();
  iopub.connect(&info.iopub_endpoint()).await.unwrap();
  iopub.subscribe("").await.unwrap();

  // Give the SUB socket a moment to actually register.
  tokio::time::sleep(Duration::from_millis(200)).await;

  (shell, iopub)
}

/// Spawn the kernel server as a background task. Returns the connection info.
fn spawn_kernel() -> ConnectionInfo {
  let info = make_connection_info();
  let info_clone = info.clone();
  tokio::spawn(async move {
    let _ = server::run(info_clone).await;
  });
  info
}

#[tokio::test]
async fn kernel_info_request_is_answered() {
  let info = spawn_kernel();
  let key = info.key.as_bytes().to_vec();
  let (mut shell, _iopub) = connect_clients(&info).await;

  let request = Message::new(
    Header::new("kernel_info_request", "test-session"),
    serde_json::json!({}),
  );
  send_shell(&mut shell, &key, request).await;

  let reply = recv_shell(&mut shell, &key).await;
  assert_eq!(reply.header.msg_type, "kernel_info_reply");
  assert_eq!(reply.content["status"], "ok");
  assert_eq!(reply.content["implementation"], "quip");
  assert_eq!(reply.content["language_info"]["name"], "quip");
}

#[tokio::test]
async fn execute_request_evaluates_code() {
  let info = spawn_kernel();
  let key = info.key.as_bytes().to_vec();
  let (mut shell, mut iopub) = connect_clients(&info).await;

  let request = Message::new(
    Header::new("execute_request", "test-session"),
    serde_json::json!({
      "code": "(+ 40 2)",
      "silent": false,
      "store_history": true,
      "user_expressions": {},
      "allow_stdin": false,
      "stop_on_error": true,
    }),
  );
  send_shell(&mut shell, &key, request).await;

  let reply = recv_shell(&mut shell, &key).await;
  assert_eq!(reply.header.msg_type, "execute_reply");
  assert_eq!(reply.content["status"], "ok");
  assert_eq!(reply.content["execution_count"], 1);

  // The matching execute_result should appear on iopub. Drain messages until
  // we find it (we ignore the status / starting / execute_input events).
  let mut found_result = false;
  for _ in 0..10 {
    let msg = recv_iopub(&mut iopub, &key).await;
    if msg.header.msg_type == "execute_result" {
      assert_eq!(msg.content["data"]["text/plain"], "42");
      found_result = true;
      break;
    }
  }
  assert!(found_result, "did not receive execute_result on iopub");
}

#[tokio::test]
async fn print_output_is_streamed_on_iopub() {
  let info = spawn_kernel();
  let key = info.key.as_bytes().to_vec();
  let (mut shell, mut iopub) = connect_clients(&info).await;

  let request = Message::new(
    Header::new("execute_request", "test-session"),
    serde_json::json!({
      "code": "(print \"hello\" \"world\")",
      "silent": false,
      "store_history": true,
    }),
  );
  send_shell(&mut shell, &key, request).await;

  let reply = recv_shell(&mut shell, &key).await;
  assert_eq!(reply.content["status"], "ok");

  let mut saw_stream = false;
  for _ in 0..10 {
    let msg = recv_iopub(&mut iopub, &key).await;
    if msg.header.msg_type == "stream" {
      assert_eq!(msg.content["name"], "stdout");
      assert!(
        msg.content["text"]
          .as_str()
          .unwrap()
          .contains("hello world"),
        "expected stream text to contain 'hello world', got {:?}",
        msg.content["text"]
      );
      saw_stream = true;
      break;
    }
  }
  assert!(saw_stream, "did not receive stream message on iopub");
}

#[tokio::test]
async fn state_persists_across_requests() {
  let info = spawn_kernel();
  let key = info.key.as_bytes().to_vec();
  let (mut shell, _iopub) = connect_clients(&info).await;

  // First request: define a variable.
  send_shell(
    &mut shell,
    &key,
    Message::new(
      Header::new("execute_request", "sess"),
      serde_json::json!({"code": "(def x 100)", "silent": false}),
    ),
  )
  .await;
  let reply = recv_shell(&mut shell, &key).await;
  assert_eq!(reply.content["status"], "ok");

  // Second request: read it back.
  send_shell(
    &mut shell,
    &key,
    Message::new(
      Header::new("execute_request", "sess"),
      serde_json::json!({"code": "(+ x 1)", "silent": false}),
    ),
  )
  .await;
  let reply = recv_shell(&mut shell, &key).await;
  assert_eq!(reply.content["status"], "ok");
  assert_eq!(reply.content["execution_count"], 2);
}

#[tokio::test]
async fn parse_errors_are_reported() {
  let info = spawn_kernel();
  let key = info.key.as_bytes().to_vec();
  let (mut shell, _iopub) = connect_clients(&info).await;

  send_shell(
    &mut shell,
    &key,
    Message::new(
      Header::new("execute_request", "sess"),
      serde_json::json!({"code": "(+ 1 2", "silent": false}),
    ),
  )
  .await;
  let reply = recv_shell(&mut shell, &key).await;
  assert_eq!(reply.content["status"], "error");
  assert_eq!(reply.content["ename"], "ParseError");
}

#[tokio::test]
async fn is_complete_request_detects_unbalanced_parens() {
  let info = spawn_kernel();
  let key = info.key.as_bytes().to_vec();
  let (mut shell, _iopub) = connect_clients(&info).await;

  send_shell(
    &mut shell,
    &key,
    Message::new(
      Header::new("is_complete_request", "sess"),
      serde_json::json!({"code": "(+ 1"}),
    ),
  )
  .await;
  let reply = recv_shell(&mut shell, &key).await;
  assert_eq!(reply.content["status"], "incomplete");

  send_shell(
    &mut shell,
    &key,
    Message::new(
      Header::new("is_complete_request", "sess"),
      serde_json::json!({"code": "(+ 1 2)"}),
    ),
  )
  .await;
  let reply = recv_shell(&mut shell, &key).await;
  assert_eq!(reply.content["status"], "complete");
}
