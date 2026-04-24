use std::io;
use std::net::TcpListener;

use quip_core::{
  ast::{Expr, ExprKind, lex, parse},
  run::Runtime,
};
use quip_notebook::{Request, Response, read_framed_json, write_framed_json};

fn main() {
  let listener = TcpListener::bind("127.0.0.1:7478").unwrap();
  let mut runtime = Runtime::default();
  runtime.context.use_intrinsics(quip_core::intrinsic::all());

  for stream in listener.incoming() {
    let mut stream = stream.unwrap();

    loop {
      let req = match read_framed_json(&mut stream) {
        Ok(r) => r,
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
        Err(_) => break,
      };

      println!("Request: {req:#?}");
      match req {
        Request::Init => {
          runtime = Runtime::default();
          runtime.context.use_intrinsics(quip_core::intrinsic::all());
        }
        Request::Eval { id, source } => {
          let result: Result<String, String> = (|| {
            let tokens = lex(&source);
            let exprs = parse(&source, tokens)?;
            let mut last = Expr {
              kind: ExprKind::Nil,
              span: None,
            };
            for expr in exprs {
              last = runtime.eval_expr(&expr).map_err(|e| e.to_string())?;
            }
            Ok(last.to_string())
          })();
          let response = Response::Eval { id, result };
          write_framed_json(&mut stream, &response).unwrap();
        }
      }
    }
  }
}
