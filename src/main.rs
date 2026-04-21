use std::{io::Read, path::PathBuf};

use clap::Parser;
use quip::{
  ast::{lex, parse},
  run::Runtime,
};
use reedline::{
  DefaultPrompt, DefaultPromptSegment, DefaultValidator, Reedline, Signal,
};

#[derive(Debug, clap::Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
  #[command(subcommand)]
  subcommand: Option<Subcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
  /// Start an interactive REPL (default).
  #[command(alias = ">")]
  Repl,
  /// Run a .quip source file.
  Run {
    /// Path to the source file.
    input: PathBuf,
  },
  /// Run source code supplied via stdin.
  #[command(alias = "-")]
  Stdin,
}

fn main() {
  let cli = Cli::parse();

  match cli.subcommand.unwrap_or(Subcommand::Repl) {
    Subcommand::Repl => run_repl(),
    Subcommand::Run { input } => run_file(&input),
    Subcommand::Stdin => run_stdin(),
  }
}

fn run_repl() {
  let mut repl = Reedline::create().with_validator(Box::new(DefaultValidator));
  let prompt = DefaultPrompt::new(
    DefaultPromptSegment::Basic("quip".to_string()),
    DefaultPromptSegment::Empty,
  );

  let mut runtime: Runtime<'static> = Runtime::default();
  loop {
    match repl.read_line(&prompt) {
      Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => {
        println!("bye");
        break;
      }
      Ok(Signal::Success(line)) => {
        if line.trim().is_empty() {
          continue;
        }

        match line.trim() {
          ":exit" => break,
          ":reset" => {
            runtime = Runtime::default();
            println!("context reset");
            continue;
          }
          cmd if cmd.starts_with(':') => {
            eprintln!("error: unknown command '{cmd}'");
            continue;
          }
          _ => {}
        }

        let tokens = lex(&line);

        match parse(&line, tokens) {
          Ok(exprs) => {
            let exprs: Vec<_> =
              exprs.into_iter().map(|e| e.into_owned()).collect();
            for expr in &exprs {
              match runtime.eval_expr(expr) {
                Ok(result) => println!("{result}"),
                Err(e) => {
                  eprintln!("error: {e}");
                  break;
                }
              }
              runtime.context.do_gc_if_over();
            }
          }
          Err(e) => eprintln!("error: {e}"),
        }
      }
      Ok(_) => {
        eprintln!("unknown signal");
      }
      Err(e) => {
        eprintln!("error: {e}");
        break;
      }
    }
  }
}

fn run_stdin() {
  let mut source = String::new();
  if let Err(e) = std::io::stdin().read_to_string(&mut source) {
    eprintln!("error: {e}");
    std::process::exit(1);
  }
  run_source(&source);
}

fn run_file(input: &PathBuf) {
  let source = match std::fs::read_to_string(input) {
    Ok(s) => s,
    Err(e) => {
      eprintln!("error: {e}");
      std::process::exit(1);
    }
  };
  run_source(&source);
}

fn run_source(source: &str) {
  let tokens = lex(source);
  let exprs = match parse(source, tokens) {
    Ok(e) => e,
    Err(e) => {
      eprintln!("error: {e}");
      std::process::exit(1);
    }
  };

  let mut runtime = Runtime::default();
  for expr in &exprs {
    if let Err(e) = runtime.eval_expr(expr) {
      eprintln!("error: {e}");
      std::process::exit(1);
    }
    runtime.context.do_gc_if_over();
  }
}
