//! Jupyter kernel implementation for Quip.
//!
//! The kernel is a separate binary (`quip-kernel`) that speaks the Jupyter
//! wire protocol over ZeroMQ. It embeds the regular Quip [`Runtime`] and runs
//! it across multiple `execute_request` messages, preserving state (defined
//! variables, functions) between evaluations.
//!
//! [`Runtime`]: crate::run::Runtime
//!
//! # Lifetime note
//!
//! The CLI REPL leaks its input via `Box::leak` to obtain the `&'static str`
//! lifetimes that `Expr<'a>` / `Runtime<'a>` require. That is bounded by the
//! length of a single REPL session.
//!
//! The kernel instead relies on [`Expr::into_owned`] to convert each parsed
//! expression into an `Expr<'static>` that does not borrow from the request's
//! source string. The source buffer can then be freed after parsing, and the
//! long-lived `Runtime<'static>` never accumulates references to per-request
//! buffers.
//!
//! [`Expr::into_owned`]: crate::ast::Expr::into_owned

pub mod connection;
pub mod handlers;
pub mod message;
pub mod server;
pub mod sockets;
