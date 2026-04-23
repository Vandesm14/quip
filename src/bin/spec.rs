use quip::run::Runtime;
use std::path::Path;

fn main() {
  let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec");

  let mut ok: usize = 0;
  let mut failed: usize = 0;
  let mut invalid: usize = 0;

  walk_dir(&manifest_dir, |entry| {
    let path = entry.path();
    let source = std::fs::read_to_string(&path).unwrap();

    let mut expected = None;

    for line in source.split('\n') {
      const EXPECTED: &str = ";; expected:";
      if let Some(source) = line.strip_prefix(EXPECTED) {
        if expected.is_some() {
          eprintln!("{}", path.display());
          eprintln!("    expected is provided multiple times");
          eprintln!();
          return;
        }

        let source = source.trim();

        let tokens = quip::ast::lex(source);
        let exprs = quip::ast::parse(source, tokens).unwrap();
        let mut runtime = Runtime::default();

        let result = match runtime.eval_expr(&exprs[0]) {
          Ok(result) => result,
          Err(e) => {
            eprintln!("{}", path.display());
            eprintln!("    expected is invalid");
            eprintln!("    {e}");
            eprintln!();
            invalid += 1;
            return;
          }
        };

        expected = Some(result);
      }
    }

    let Some(expected) = expected else {
      eprintln!("{}", path.display());
      eprintln!("    expected an 'expected: ...' key-value");
      eprintln!();
      invalid += 1;
      return;
    };

    let tokens = quip::ast::lex(&source);
    let exprs = quip::ast::parse(&source, tokens).unwrap();
    let mut runtime = Runtime::default();

    for expr in &exprs {
      match runtime.eval_expr(expr) {
        Ok(result) => {
          if result.kind == expected.kind {
            ok += 1;
          } else {
            eprintln!("{}", path.display());
            eprintln!("    expected {expected}");
            eprintln!("    found    {result}");
            eprintln!();
            failed += 1;
          }
        }
        Err(e) => {
          eprintln!("{}", path.display());
          eprintln!("    {e}");
          eprintln!();
        }
      }
    }
  });

  eprintln!("ok: {ok}");
  eprintln!("failed: {failed}");
  eprintln!("invalid: {invalid}");
}

fn walk_dir<F>(path: &Path, mut f: F) -> F::Output
where
  F: FnMut(std::fs::DirEntry),
{
  let mut stack = vec![std::fs::read_dir(path).unwrap()];

  while let Some(dir) = stack.pop() {
    for entry in dir.map(Result::unwrap) {
      let entry_type = entry.file_type().unwrap();

      if entry_type.is_dir() {
        stack.push(std::fs::read_dir(entry.path()).unwrap());
        continue;
      }

      if entry_type.is_file() {
        f(entry);
      }
    }
  }
}
