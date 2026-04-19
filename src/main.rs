use quip::lexer::lex;

fn main() {
  let source = include_str!("../example.quip");

  println!("{:#?}", lex(source));
}
