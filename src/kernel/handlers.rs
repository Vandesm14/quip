//! Handlers for each Jupyter message type the kernel understands.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::{
  ast::{Expr, ExprKind, lex, parse},
  run::{ErrorReason, Output, Runtime},
};

use super::message::{Header, Message};

/// Shared state between the shell/control handlers.
pub struct KernelState {
  pub runtime: Runtime,
  pub execution_count: u64,
  pub session_id: String,
  /// Where `(print ...)` accumulates lines during an execute_request.
  pub print_buffer: Arc<Mutex<Vec<String>>>,
}

impl KernelState {
  pub fn new(session_id: String) -> Self {
    let print_buffer = Arc::new(Mutex::new(Vec::new()));
    let runtime = Runtime {
      output: Output::Buffered(Arc::clone(&print_buffer)),
      ..Default::default()
    };
    Self {
      runtime,
      execution_count: 0,
      session_id,
      print_buffer,
    }
  }

  /// Drain any buffered `print` output lines, joining them with newlines.
  pub fn drain_print(&self) -> Option<String> {
    let mut guard = self.print_buffer.lock().ok()?;
    if guard.is_empty() {
      return None;
    }
    let mut out = guard.join("\n");
    out.push('\n');
    guard.clear();
    Some(out)
  }
}

/// Result of handling an `execute_request`. Each field may be `None`.
pub struct ExecuteOutcome {
  /// Reply to send back on shell.
  pub shell_reply: Message,
  /// Messages to broadcast on iopub (in order).
  pub iopub: Vec<Message>,
  /// Hint that the kernel should shut down after flushing.
  pub should_shutdown: bool,
}

pub fn handle_kernel_info(request: &Message) -> Message {
  let content = json!({
    "status": "ok",
    "protocol_version": "5.3",
    "implementation": "quip",
    "implementation_version": env!("CARGO_PKG_VERSION"),
    "language_info": {
      "name": "quip",
      "version": env!("CARGO_PKG_VERSION"),
      "mimetype": "text/x-clojure",
      "file_extension": ".quip",
      "pygments_lexer": "clojure",
      "codemirror_mode": "clojure",
    },
    "banner": "Quip kernel",
    "help_links": [],
  });
  request.reply("kernel_info_reply", content)
}

pub fn handle_is_complete(request: &Message) -> Message {
  // A lightweight heuristic: count unmatched parens. If balanced, code is
  // complete; if more `(` than `)`, it's incomplete; otherwise, invalid.
  let code = request
    .content
    .get("code")
    .and_then(Value::as_str)
    .unwrap_or("");
  let (mut opens, mut closes) = (0i64, 0i64);
  let mut in_string = false;
  let mut in_comment = false;
  let mut prev = '\0';
  for c in code.chars() {
    if in_comment {
      if c == '\n' {
        in_comment = false;
      }
      prev = c;
      continue;
    }
    if in_string {
      if c == '"' && prev != '\\' {
        in_string = false;
      }
      prev = c;
      continue;
    }
    match c {
      '"' => in_string = true,
      ';' => in_comment = true,
      '(' => opens += 1,
      ')' => closes += 1,
      _ => {}
    }
    prev = c;
  }
  let status = if opens == closes {
    "complete"
  } else if opens > closes {
    "incomplete"
  } else {
    "invalid"
  };
  let mut content = json!({ "status": status });
  if status == "incomplete" {
    content["indent"] = json!("  ");
  }
  request.reply("is_complete_reply", content)
}

pub fn handle_comm_info(request: &Message) -> Message {
  request.reply(
    "comm_info_reply",
    json!({
      "status": "ok",
      "comms": {},
    }),
  )
}

pub fn handle_complete(request: &Message) -> Message {
  // Minimal stub: no autocompletion.
  let code = request
    .content
    .get("code")
    .and_then(Value::as_str)
    .unwrap_or("");
  let cursor_pos = request
    .content
    .get("cursor_pos")
    .and_then(Value::as_u64)
    .unwrap_or(code.len() as u64);
  request.reply(
    "complete_reply",
    json!({
      "status": "ok",
      "matches": [],
      "cursor_start": cursor_pos,
      "cursor_end": cursor_pos,
      "metadata": {},
    }),
  )
}

pub fn handle_shutdown(request: &Message) -> Message {
  let restart = request
    .content
    .get("restart")
    .and_then(Value::as_bool)
    .unwrap_or(false);
  request.reply(
    "shutdown_reply",
    json!({
      "status": "ok",
      "restart": restart,
    }),
  )
}

/// Execute code from an `execute_request`, producing the shell reply and any
/// iopub messages (stream output, execute_result, error).
pub fn handle_execute(
  state: &mut KernelState,
  request: &Message,
) -> ExecuteOutcome {
  let code = request
    .content
    .get("code")
    .and_then(Value::as_str)
    .unwrap_or("")
    .to_string();
  let silent = request
    .content
    .get("silent")
    .and_then(Value::as_bool)
    .unwrap_or(false);
  let store_history = request
    .content
    .get("store_history")
    .and_then(Value::as_bool)
    .unwrap_or(true);

  if !silent && store_history {
    state.execution_count += 1;
  }
  let execution_count = state.execution_count;

  let mut iopub = Vec::new();

  // Broadcast execute_input so all frontends see the code being run.
  if !silent {
    let input_msg = reply_on_iopub(
      request,
      "execute_input",
      json!({
        "code": code,
        "execution_count": execution_count,
      }),
    );
    iopub.push(input_msg);
  }

  // Tokenize and parse. Converting to owned Exprs lets us drop `code` after
  // parsing without violating the runtime's lifetime invariants.
  let tokens = lex(&code);
  let owned_exprs: Result<Vec<Expr>, String> =
    parse(&code, tokens).map(|exprs| exprs.into_iter().collect());

  match owned_exprs {
    Err(err) => {
      let ename = "ParseError".to_string();
      let evalue = err.clone();
      let traceback = vec![format!("{}: {}", ename, evalue)];
      if !silent {
        iopub.push(flush_stream_iopub(state, request));
        iopub.push(reply_on_iopub(
          request,
          "error",
          json!({
            "ename": ename,
            "evalue": evalue,
            "traceback": traceback,
          }),
        ));
      }
      let reply = request.reply(
        "execute_reply",
        json!({
          "status": "error",
          "execution_count": execution_count,
          "ename": ename,
          "evalue": evalue,
          "traceback": traceback,
        }),
      );
      ExecuteOutcome {
        shell_reply: reply,
        iopub,
        should_shutdown: false,
      }
    }
    Ok(exprs) => {
      let mut last_value: Option<Expr> = None;
      let mut runtime_error: Option<crate::run::Error> = None;

      for expr in &exprs {
        match state.runtime.eval_expr(expr) {
          Ok(value) => last_value = Some(value),
          Err(err) => {
            runtime_error = Some(err);
            break;
          }
        }
        state.runtime.context.do_gc_if_over();
      }

      if !silent && let Some(stream_msg) = build_stream_iopub(state, request) {
        iopub.push(stream_msg);
      }

      if let Some(err) = runtime_error {
        let ename = error_ename(&err);
        let evalue = err.to_string();
        let traceback = vec![format!("{}: {}", ename, evalue)];
        if !silent {
          iopub.push(reply_on_iopub(
            request,
            "error",
            json!({
              "ename": ename,
              "evalue": evalue,
              "traceback": traceback,
            }),
          ));
        }
        let reply = request.reply(
          "execute_reply",
          json!({
            "status": "error",
            "execution_count": execution_count,
            "ename": ename,
            "evalue": evalue,
            "traceback": traceback,
          }),
        );
        ExecuteOutcome {
          shell_reply: reply,
          iopub,
          should_shutdown: false,
        }
      } else {
        if !silent
          && let Some(val) = last_value
          && let Some(text) = render_result(&val)
        {
          iopub.push(reply_on_iopub(
            request,
            "execute_result",
            json!({
              "execution_count": execution_count,
              "data": {"text/plain": text},
              "metadata": {},
            }),
          ));
        }
        let reply = request.reply(
          "execute_reply",
          json!({
            "status": "ok",
            "execution_count": execution_count,
            "user_expressions": {},
            "payload": [],
          }),
        );
        ExecuteOutcome {
          shell_reply: reply,
          iopub,
          should_shutdown: false,
        }
      }
    }
  }
}

/// Produce a `status` iopub message (`busy` or `idle`) parented to `request`.
pub fn status_iopub(request: &Message, state: &str) -> Message {
  reply_on_iopub(request, "status", json!({ "execution_state": state }))
}

/// Construct a new iopub message parented to `request` with the given header
/// type and content.
fn reply_on_iopub(
  request: &Message,
  msg_type: &str,
  content: Value,
) -> Message {
  let header = Header::new(msg_type, &request.header.session);
  let mut msg = Message::new(header, content);
  msg.parent_header = Some(request.header.clone());
  msg
}

/// Take any buffered `print` output and emit it as a `stream` iopub message.
/// Returns `None` if there is nothing to emit.
fn build_stream_iopub(
  state: &mut KernelState,
  request: &Message,
) -> Option<Message> {
  let text = state.drain_print()?;
  Some(reply_on_iopub(
    request,
    "stream",
    json!({
      "name": "stdout",
      "text": text,
    }),
  ))
}

/// Always produce a stream message even if empty. Used when we must emit
/// something before an error message to keep output ordering intuitive.
fn flush_stream_iopub(state: &mut KernelState, request: &Message) -> Message {
  let text = state.drain_print().unwrap_or_default();
  reply_on_iopub(
    request,
    "stream",
    json!({
      "name": "stdout",
      "text": text,
    }),
  )
}

/// Choose how to render an evaluated expression for `text/plain` display.
/// `Nil` results are suppressed (no `execute_result` emitted).
fn render_result(expr: &Expr) -> Option<String> {
  match &expr.kind {
    ExprKind::Nil => None,
    _ => Some(expr.to_string()),
  }
}

fn error_ename(err: &crate::run::Error) -> String {
  match err.reason.as_ref() {
    ErrorReason::CallError(_) => "CallError".to_string(),
    ErrorReason::Message(_) => "RuntimeError".to_string(),
  }
}
