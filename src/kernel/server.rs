//! Main kernel event loop.
//!
//! Spawns one tokio task per ZeroMQ channel. The shell/control tasks read
//! requests, dispatch to [`handlers`], then forward any resulting iopub
//! messages through an mpsc queue to the iopub task.
//!
//! [`handlers`]: super::handlers

use std::sync::Arc;

use tokio::{
  sync::{Mutex, mpsc, oneshot},
  task::JoinHandle,
};
use zeromq::{SocketRecv, SocketSend};

use super::{
  connection::ConnectionInfo,
  handlers::{self, KernelState},
  message::Message,
  sockets::{KernelSockets, SocketError, recv_router, send_iopub, send_router},
};

/// Run the kernel until a `shutdown_request` is received.
pub async fn run(info: ConnectionInfo) -> Result<(), SocketError> {
  let sockets = KernelSockets::bind(&info).await?;
  let key = Arc::<[u8]>::from(info.key.as_bytes().to_vec().into_boxed_slice());
  let session = uuid::Uuid::new_v4().to_string();
  let state = Arc::new(Mutex::new(KernelState::new(session)));

  let (iopub_tx, iopub_rx) = mpsc::unbounded_channel::<Message>();
  let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
  let shutdown_tx = Arc::new(Mutex::new(Some(shutdown_tx)));

  // Send an initial `starting` status on iopub so frontends know we're alive.
  // (Jupyter frontends typically show this as "kernel starting".)
  let starting = handlers::status_iopub(
    &dummy_parent(&state.lock().await.session_id),
    "starting",
  );
  let _ = iopub_tx.send(starting);

  let iopub_handle = spawn_iopub(sockets.iopub, Arc::clone(&key), iopub_rx);

  let heartbeat_handle = spawn_heartbeat(sockets.heartbeat);

  let shell_handle = spawn_shell(
    sockets.shell,
    Arc::clone(&key),
    Arc::clone(&state),
    iopub_tx.clone(),
    Arc::clone(&shutdown_tx),
  );

  let control_handle = spawn_control(
    sockets.control,
    Arc::clone(&key),
    iopub_tx.clone(),
    Arc::clone(&shutdown_tx),
  );

  // Drop the original sender so the iopub task can terminate once all
  // per-channel senders are gone.
  drop(iopub_tx);

  // Wait for shutdown signal.
  let _ = shutdown_rx.await;

  eprintln!("[quip-kernel] shutting down...");

  shell_handle.abort();
  control_handle.abort();
  heartbeat_handle.abort();
  iopub_handle.abort();

  Ok(())
}

fn spawn_heartbeat(mut socket: zeromq::RepSocket) -> JoinHandle<()> {
  tokio::spawn(async move {
    while let Ok(msg) = socket.recv().await {
      if socket.send(msg).await.is_err() {
        break;
      }
    }
  })
}

fn spawn_iopub(
  mut socket: zeromq::PubSocket,
  key: Arc<[u8]>,
  mut rx: mpsc::UnboundedReceiver<Message>,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
      if let Err(e) = send_iopub(&mut socket, &key, msg).await {
        eprintln!("[quip-kernel] iopub send error: {}", e);
      }
    }
  })
}

fn spawn_shell(
  mut socket: zeromq::RouterSocket,
  key: Arc<[u8]>,
  state: Arc<Mutex<KernelState>>,
  iopub_tx: mpsc::UnboundedSender<Message>,
  shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    loop {
      let request = match recv_router(&mut socket, &key).await {
        Ok(m) => m,
        Err(e) => {
          eprintln!("[quip-kernel] shell recv error: {}", e);
          continue;
        }
      };

      // Announce busy on iopub.
      let _ = iopub_tx.send(handlers::status_iopub(&request, "busy"));

      let (reply, stop) =
        handle_shell_request(&state, &iopub_tx, &request).await;

      if let Err(e) = send_router(&mut socket, &key, reply).await {
        eprintln!("[quip-kernel] shell send error: {}", e);
      }

      // Announce idle on iopub.
      let _ = iopub_tx.send(handlers::status_iopub(&request, "idle"));

      if stop {
        if let Some(tx) = shutdown_tx.lock().await.take() {
          let _ = tx.send(());
        }
        break;
      }
    }
  })
}

fn spawn_control(
  mut socket: zeromq::RouterSocket,
  key: Arc<[u8]>,
  iopub_tx: mpsc::UnboundedSender<Message>,
  shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    loop {
      let request = match recv_router(&mut socket, &key).await {
        Ok(m) => m,
        Err(e) => {
          eprintln!("[quip-kernel] control recv error: {}", e);
          continue;
        }
      };

      let _ = iopub_tx.send(handlers::status_iopub(&request, "busy"));

      let (reply, stop) = match request.header.msg_type.as_str() {
        "shutdown_request" => (handlers::handle_shutdown(&request), true),
        "interrupt_request" => (
          request.reply("interrupt_reply", serde_json::json!({"status": "ok"})),
          false,
        ),
        other => {
          eprintln!("[quip-kernel] ignoring control message '{}'", other);
          (
            request.reply(
              format!("{}_reply", other),
              serde_json::json!({"status": "error"}),
            ),
            false,
          )
        }
      };

      if let Err(e) = send_router(&mut socket, &key, reply).await {
        eprintln!("[quip-kernel] control send error: {}", e);
      }

      let _ = iopub_tx.send(handlers::status_iopub(&request, "idle"));

      if stop {
        if let Some(tx) = shutdown_tx.lock().await.take() {
          let _ = tx.send(());
        }
        break;
      }
    }
  })
}

/// Dispatch a shell-channel request. Returns the reply message and a flag
/// indicating the kernel should shut down.
async fn handle_shell_request(
  state: &Arc<Mutex<KernelState>>,
  iopub_tx: &mpsc::UnboundedSender<Message>,
  request: &Message,
) -> (Message, bool) {
  match request.header.msg_type.as_str() {
    "kernel_info_request" => (handlers::handle_kernel_info(request), false),
    "is_complete_request" => (handlers::handle_is_complete(request), false),
    "comm_info_request" => (handlers::handle_comm_info(request), false),
    "complete_request" => (handlers::handle_complete(request), false),
    "shutdown_request" => (handlers::handle_shutdown(request), true),
    "execute_request" => {
      let outcome = {
        let mut guard = state.lock().await;
        handlers::handle_execute(&mut guard, request)
      };
      for msg in outcome.iopub {
        let _ = iopub_tx.send(msg);
      }
      (outcome.shell_reply, outcome.should_shutdown)
    }
    other => {
      eprintln!("[quip-kernel] unhandled shell message '{}'", other);
      (
        request.reply(
          format!("{}_reply", other),
          serde_json::json!({"status": "error"}),
        ),
        false,
      )
    }
  }
}

/// Sent alongside the initial `starting` iopub status before any real request
/// has arrived. Contains a synthetic header so the `parent_header` field is
/// non-empty.
fn dummy_parent(session: &str) -> Message {
  Message::new(
    super::message::Header::new("kernel_startup", session),
    serde_json::json!({}),
  )
}
