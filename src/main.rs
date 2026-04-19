use quip::{
  ast::{lex, parse},
  run::Runtime,
};

fn main() {
  let source = include_str!("../example.quip");
  let tokens = lex(source);
  let exprs = parse(source, tokens).unwrap();

  let mut runtime = Runtime::default();
  for expr in exprs.iter() {
    runtime.eval_expr(expr).unwrap();
  }
}
