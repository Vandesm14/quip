use quip::ast::{lex, parse};

fn main() {
  let source = include_str!("../example.quip");
  let tokens = lex(source);

  println!("{:#?}", tokens);
  println!();

  let exprs = parse(source, tokens);
  println!("{:#?}", exprs);
}
