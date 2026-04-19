use std::borrow::Cow;

use slotmap::DefaultKey;

use crate::{
  ast::{Expr, ExprKind},
  context::Context,
};

#[derive(Debug, Clone, Default)]
pub struct Runtime<'a> {
  pub context: Context<'a>,
}

impl<'a> Runtime<'a> {
  fn call(
    &mut self,
    closure_env: DefaultKey,
    params: Vec<Cow<'a, str>>,
    body: Vec<Expr<'a>>,
    call_args: &[Expr<'a>],
    name: &str,
  ) -> Result<Expr<'a>, String> {
    if call_args.len() != params.len() {
      return Err(format!(
        "'{}' expects {} arg(s), got {}",
        name,
        params.len(),
        call_args.len()
      ));
    }

    let mut bound = Vec::new();
    for (param, arg_expr) in params.iter().zip(call_args.iter()) {
      let val = self.eval_expr(arg_expr)?;
      bound.push((param.clone(), val));
    }

    let saved = self.context.push_scope(closure_env);
    for (param_name, val) in bound {
      self.context.define(param_name, val);
    }

    let mut result = Expr {
      kind: ExprKind::Nil,
    };
    for body_expr in &body {
      result = match self.eval_expr(body_expr) {
        Ok(v) => v,
        Err(e) => {
          self.context.restore_scope(saved);
          return Err(e);
        }
      };
    }

    self.context.restore_scope(saved);
    Ok(result)
  }

  fn parse_params(
    param_list: &[Expr<'a>],
    ctx: &str,
  ) -> Result<Vec<Cow<'a, str>>, String> {
    param_list
      .iter()
      .map(|p| {
        if let ExprKind::Symbol(sym) = &p.kind {
          Ok(sym.clone())
        } else {
          Err(format!("{}: invalid param", ctx))
        }
      })
      .collect()
  }

  pub fn eval_expr(&mut self, expr: &Expr<'a>) -> Result<Expr<'a>, String> {
    if let ExprKind::List(list) = &expr.kind
      && let Some(ExprKind::Symbol(sym)) = list.first().map(|e| &e.kind)
    {
      match sym.to_string().as_str() {
        "fn" => {
          // (fn (params...) body...)
          let Some(params_expr) = list.get(1) else {
            return Err("fn: expected params list".to_string());
          };
          let ExprKind::List(param_list) = &params_expr.kind else {
            return Err("fn: expected params list".to_string());
          };
          let params = Self::parse_params(param_list, "fn")?;
          let body = list.get(2..).unwrap_or(&[]).to_vec();
          let env = self.context.current();
          Ok(Expr {
            kind: ExprKind::Function { params, body, env },
          })
        }

        "defn" => {
          // (defn name (params...) body...)  →  (def name (fn (params...) body...))
          let Some([name_expr, params_expr]) = list.get(1..3) else {
            return Err("defn: expected name and params".to_string());
          };
          let ExprKind::Symbol(name) = &name_expr.kind else {
            return Err("defn: invalid name".to_string());
          };
          let ExprKind::List(param_list) = &params_expr.kind else {
            return Err("defn: expected params list".to_string());
          };
          let params = Self::parse_params(param_list, "defn")?;
          let body = list.get(3..).unwrap_or(&[]).to_vec();
          let env = self.context.current();
          let func = Expr {
            kind: ExprKind::Function { params, body, env },
          };
          self.context.define(name.clone(), func);
          Ok(Expr {
            kind: ExprKind::Nil,
          })
        }

        "def" => {
          if let Some([name, val]) = list.get(1..3) {
            let ExprKind::Symbol(name) = &name.kind else {
              return Err("def: invalid name".to_string());
            };
            let val = self.eval_expr(val)?;
            self.context.define(name.clone(), val.clone());
            Ok(val)
          } else {
            Err("invalid def".to_string())
          }
        }

        "set" => {
          if let Some([name, val]) = list.get(1..3) {
            let ExprKind::Symbol(name) = &name.kind else {
              return Err("set: invalid name".to_string());
            };
            let val = self.eval_expr(val)?;
            self.context.set(name.clone(), val.clone())?;
            Ok(val)
          } else {
            Err("invalid set".to_string())
          }
        }

        "+" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            match lhs + rhs {
              Ok(kind) => Ok(Expr {
                kind: kind.normalize_numeric(),
              }),
              Err(_) => Err("'+' requires numeric arguments".to_string()),
            }
          } else {
            Err("'+' requires two arguments".to_string())
          }
        }

        "-" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            match lhs - rhs {
              Ok(kind) => Ok(Expr {
                kind: kind.normalize_numeric(),
              }),
              Err(_) => Err("'-' requires numeric arguments".to_string()),
            }
          } else {
            Err("'-' requires two arguments".to_string())
          }
        }

        "*" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            match lhs * rhs {
              Ok(kind) => Ok(Expr {
                kind: kind.normalize_numeric(),
              }),
              Err(_) => Err("'*' requires numeric arguments".to_string()),
            }
          } else {
            Err("'*' requires two arguments".to_string())
          }
        }

        "/" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);

            match &rhs {
              ExprKind::Integer(0) => {
                return Err("'/' division by zero".to_string());
              }
              ExprKind::Float(f) if *f == 0.0 => {
                return Err("'/' division by zero".to_string());
              }
              _ => {}
            }

            match lhs / rhs {
              Ok(kind) => Ok(Expr {
                kind: kind.normalize_numeric(),
              }),
              Err(_) => Err("'/' requires numeric arguments".to_string()),
            }
          } else {
            Err("'/' requires two arguments".to_string())
          }
        }

        "%" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);

            match &rhs {
              ExprKind::Integer(0) => {
                return Err("'%' modulo by zero".to_string());
              }
              ExprKind::Float(f) if *f == 0.0 => {
                return Err("'%' modulo by zero".to_string());
              }
              _ => {}
            }

            match lhs % rhs {
              Ok(kind) => Ok(Expr {
                kind: kind.normalize_numeric(),
              }),
              Err(_) => Err("'%' requires numeric arguments".to_string()),
            }
          } else {
            Err("'%' requires two arguments".to_string())
          }
        }

        "=" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            Ok(Expr {
              kind: ExprKind::Boolean(lhs == rhs),
            })
          } else {
            Err("'=' requires two arguments".to_string())
          }
        }

        "!=" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            Ok(Expr {
              kind: ExprKind::Boolean(lhs != rhs),
            })
          } else {
            Err("'!=' requires two arguments".to_string())
          }
        }

        "<" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            match lhs.partial_cmp(&rhs) {
              Some(ord) => Ok(Expr {
                kind: ExprKind::Boolean(ord.is_lt()),
              }),
              None => Err("'<' requires comparable arguments".to_string()),
            }
          } else {
            Err("'<' requires two arguments".to_string())
          }
        }

        "<=" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            match lhs.partial_cmp(&rhs) {
              Some(ord) => Ok(Expr {
                kind: ExprKind::Boolean(ord.is_le()),
              }),
              None => Err("'<=' requires comparable arguments".to_string()),
            }
          } else {
            Err("'<=' requires two arguments".to_string())
          }
        }

        ">" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            match lhs.partial_cmp(&rhs) {
              Some(ord) => Ok(Expr {
                kind: ExprKind::Boolean(ord.is_gt()),
              }),
              None => Err("'>' requires comparable arguments".to_string()),
            }
          } else {
            Err("'>' requires two arguments".to_string())
          }
        }

        ">=" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let lhs = self.eval_expr(lhs)?.kind;
            let rhs = self.eval_expr(rhs)?.kind;
            let (lhs, rhs) = lhs.coerce_numeric(rhs);
            match lhs.partial_cmp(&rhs) {
              Some(ord) => Ok(Expr {
                kind: ExprKind::Boolean(ord.is_ge()),
              }),
              None => Err("'>=' requires comparable arguments".to_string()),
            }
          } else {
            Err("'>=' requires two arguments".to_string())
          }
        }

        "print" => {
          if let Some(args) = list.get(1..) {
            let parts = args
              .iter()
              .map(|expr| self.eval_expr(expr).map(|e| e.to_string()))
              .collect::<Result<Vec<_>, _>>()?;
            println!("{}", parts.join(" "));
            Ok(Expr {
              kind: ExprKind::Nil,
            })
          } else {
            Err("'print' requires at least one argument".to_string())
          }
        }

        _ => {
          // Symbol look-up.
          let val = self
            .context
            .get(sym)
            .cloned()
            .ok_or_else(|| format!("undefined '{}'", sym))?;
          if let ExprKind::Function { params, body, env } = val.kind {
            let call_args = list.get(1..).unwrap_or(&[]);
            self.call(env, params, body, call_args, sym.as_ref())
          } else {
            Err(format!("'{}' is not a function", sym))
          }
        }
      }
    } else if let ExprKind::List(list) = &expr.kind {
      // Behavior for calling anon functions ((fn ...) args...).
      let Some(head) = list.first() else {
        return Ok(expr.clone());
      };
      let callee = self.eval_expr(head)?;
      if let ExprKind::Function { params, body, env } = callee.kind {
        let call_args = list.get(1..).unwrap_or(&[]);
        self.call(env, params, body, call_args, "<anonymous>")
      } else {
        Ok(expr.clone())
      }
    } else if let ExprKind::Symbol(sym) = &expr.kind {
      // Get vars.
      self
        .context
        .get(sym)
        .cloned()
        .ok_or_else(|| format!("undefined fn '{}'", sym))
    } else {
      Ok(expr.clone())
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ast::{lex, parse};

  fn run(source: &str) -> Result<Expr<'_>, String> {
    let tokens = lex(source);
    let exprs = parse(source, tokens)?;
    let mut runtime = Runtime::default();
    let mut last = Expr {
      kind: ExprKind::Nil,
    };
    for expr in &exprs {
      last = runtime.eval_expr(expr)?;
    }
    Ok(last)
  }

  fn run_runtime(source: &str) -> Runtime<'_> {
    let tokens = lex(source);
    let exprs = parse(source, tokens).unwrap();
    let mut runtime = Runtime::default();
    for expr in &exprs {
      runtime.eval_expr(expr).unwrap();
    }
    runtime
  }

  fn eval_source<'a>(runtime: &mut Runtime<'a>, source: &'a str) -> Expr<'a> {
    let tokens = lex(source);
    let exprs = parse(source, tokens).unwrap();
    let mut last = Expr {
      kind: ExprKind::Nil,
    };
    for expr in &exprs {
      last = runtime.eval_expr(expr).unwrap();
    }
    last
  }

  mod operations {
    use super::*;
    const FLOAT_THRESHOLD: f64 = 0.0001;

    mod arithmetic {
      use super::*;

      mod addition {
        use super::*;

        #[test]
        fn addition_integers() {
          let result = run("(+ 5 3)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(8));
        }

        #[test]
        fn addition_floats_normalizes_to_integer() {
          let result = run("(+ 5.5 2.5)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(8));
        }

        #[test]
        fn addition_floats_stays_float_when_not_whole() {
          let result = run("(+ 1.5 2.0)").unwrap();
          if let ExprKind::Float(f) = result.kind {
            assert!((f - 3.5).abs() < FLOAT_THRESHOLD);
          } else {
            panic!("Expected float");
          }
        }

        #[test]
        fn addition_coerces_mixed_numerics() {
          let result = run("(+ 5 2.5)").unwrap();
          if let ExprKind::Float(f) = result.kind {
            assert!((f - 7.5).abs() < FLOAT_THRESHOLD);
          } else {
            panic!("Expected float");
          }
        }

        #[test]
        fn addition_type_mismatch() {
          let result = run("(+ 5 \"hello\")");
          assert!(result.is_err());
        }
      }

      mod subtraction {
        use super::*;

        #[test]
        fn subtraction_integers() {
          let result = run("(- 10 3)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(7));
        }

        #[test]
        fn subtraction_floats() {
          let result = run("(- 10.5 3.2)").unwrap();
          if let ExprKind::Float(f) = result.kind {
            assert!((f - 7.3).abs() < FLOAT_THRESHOLD);
          } else {
            panic!("Expected float");
          }
        }

        #[test]
        fn subtraction_coerces_mixed_numerics() {
          let result = run("(- 10 3.2)").unwrap();
          if let ExprKind::Float(f) = result.kind {
            assert!((f - 6.8).abs() < FLOAT_THRESHOLD);
          } else {
            panic!("Expected float");
          }
        }

        #[test]
        fn subtraction_type_mismatch() {
          let result = run("(- 10 \"hello\")");
          assert!(result.is_err());
        }

        #[test]
        fn subtraction_negative_result() {
          let result = run("(- 3 10)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(-7));
        }
      }

      mod multiplication {
        use super::*;

        #[test]
        fn multiplication_integers() {
          let result = run("(* 5 3)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(15));
        }

        #[test]
        fn multiplication_floats_normalizes_to_integer() {
          let result = run("(* 5.5 2.0)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(11));
        }

        #[test]
        fn multiplication_floats_stays_float_when_not_whole() {
          let result = run("(* 2.5 2.0)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(5));
        }

        #[test]
        fn multiplication_by_zero() {
          let result = run("(* 5 0)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(0));
        }

        #[test]
        fn multiplication_coerces_mixed_numerics() {
          let result = run("(* 5 3.5)").unwrap();
          if let ExprKind::Float(f) = result.kind {
            assert!((f - 17.5).abs() < FLOAT_THRESHOLD);
          } else {
            panic!("Expected float");
          }
        }

        #[test]
        fn multiplication_type_mismatch() {
          let result = run("(* 5 \"hello\")");
          assert!(result.is_err());
        }
      }

      mod division {
        use super::*;

        #[test]
        fn division_integers() {
          let result = run("(/ 15 3)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(5));
        }

        #[test]
        fn division_floats_normalizes_to_integer() {
          let result = run("(/ 15.0 3.0)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(5));
        }

        #[test]
        fn division_floats_stays_float_when_not_whole() {
          let result = run("(/ 7.0 2.0)").unwrap();
          if let ExprKind::Float(f) = result.kind {
            assert!((f - 3.5).abs() < FLOAT_THRESHOLD);
          } else {
            panic!("Expected float");
          }
        }

        #[test]
        fn division_by_zero_integers() {
          let result = run("(/ 10 0)");
          assert!(result.is_err());
        }

        #[test]
        fn division_by_zero_floats() {
          let result = run("(/ 10.0 0.0)");
          assert!(result.is_err());
        }

        #[test]
        fn division_coerces_mixed_numerics() {
          let result = run("(/ 15 3.0)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(5));
        }

        #[test]
        fn division_type_mismatch() {
          let result = run("(/ 10 \"hello\")");
          assert!(result.is_err());
        }

        #[test]
        fn division_integer_truncation() {
          let result = run("(/ 7 2)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(3));
        }
      }

      mod modulo {
        use super::*;

        #[test]
        fn modulo_integers() {
          let result = run("(% 10 3)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(1));
        }

        #[test]
        fn modulo_floats() {
          let result = run("(% 10.5 3.0)").unwrap();
          if let ExprKind::Float(f) = result.kind {
            assert!((f - 1.5).abs() < FLOAT_THRESHOLD);
          } else {
            panic!("Expected float");
          }
        }

        #[test]
        fn modulo_by_zero_integers() {
          let result = run("(% 10 0)");
          assert!(result.is_err());
        }

        #[test]
        fn modulo_by_zero_floats() {
          let result = run("(% 10.0 0.0)");
          assert!(result.is_err());
        }

        #[test]
        fn modulo_coerces_mixed_numerics() {
          let result = run("(% 10 3.0)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(1));
        }

        #[test]
        fn modulo_type_mismatch() {
          let result = run("(% 10 \"hello\")");
          assert!(result.is_err());
        }

        #[test]
        fn modulo_negative_dividend() {
          let result = run("(% -10 3)").unwrap();
          assert_eq!(result.kind, ExprKind::Integer(-1));
        }
      }
    }

    mod comparison {
      use super::*;

      mod equality {
        use super::*;

        #[test]
        fn equal_integers() {
          assert_eq!(run("(= 1 1)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn unequal_integers() {
          assert_eq!(run("(= 1 2)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn equal_after_numeric_coercion() {
          assert_eq!(run("(= 1 1.0)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn equal_floats() {
          assert_eq!(run("(= 1.5 1.5)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn equal_strings() {
          assert_eq!(
            run("(= \"hello\" \"hello\")").unwrap().kind,
            ExprKind::Boolean(true)
          );
        }

        #[test]
        fn unequal_strings() {
          assert_eq!(
            run("(= \"hello\" \"world\")").unwrap().kind,
            ExprKind::Boolean(false)
          );
        }

        #[test]
        fn equal_nil() {
          assert_eq!(run("(= nil nil)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn equal_booleans() {
          assert_eq!(
            run("(= true true)").unwrap().kind,
            ExprKind::Boolean(true)
          );
          assert_eq!(
            run("(= false false)").unwrap().kind,
            ExprKind::Boolean(true)
          );
        }

        #[test]
        fn unequal_booleans() {
          assert_eq!(
            run("(= true false)").unwrap().kind,
            ExprKind::Boolean(false)
          );
        }

        #[test]
        fn different_types_are_not_equal() {
          assert_eq!(run("(= 1 \"1\")").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn missing_argument_is_an_error() {
          assert!(run("(= 1)").is_err());
        }
      }

      mod inequality {
        use super::*;

        #[test]
        fn equal_values_are_not_unequal() {
          assert_eq!(run("(!= 1 1)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn unequal_values_are_unequal() {
          assert_eq!(run("(!= 1 2)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn coerced_equal_values_are_not_unequal() {
          assert_eq!(run("(!= 1 1.0)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn different_types_are_unequal() {
          assert_eq!(
            run("(!= 1 \"1\")").unwrap().kind,
            ExprKind::Boolean(true)
          );
        }
      }

      mod less_than {
        use super::*;

        #[test]
        fn integer_less_than() {
          assert_eq!(run("(< 1 2)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn integer_not_less_than_equal() {
          assert_eq!(run("(< 1 1)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn integer_not_less_than_greater() {
          assert_eq!(run("(< 2 1)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn float_less_than() {
          assert_eq!(run("(< 1.5 2.5)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn mixed_numeric_less_than() {
          assert_eq!(run("(< 1 2.0)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn incomparable_types_are_an_error() {
          assert!(run("(< \"a\" \"b\")").is_err());
        }
      }

      mod less_than_or_equal {
        use super::*;

        #[test]
        fn integer_less_than_or_equal_less() {
          assert_eq!(run("(<= 1 2)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn integer_less_than_or_equal_equal() {
          assert_eq!(run("(<= 1 1)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn integer_not_less_than_or_equal_greater() {
          assert_eq!(run("(<= 2 1)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn mixed_numeric_less_than_or_equal() {
          assert_eq!(run("(<= 1 1.0)").unwrap().kind, ExprKind::Boolean(true));
        }
      }

      mod greater_than {
        use super::*;

        #[test]
        fn integer_greater_than() {
          assert_eq!(run("(> 2 1)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn integer_not_greater_than_equal() {
          assert_eq!(run("(> 1 1)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn integer_not_greater_than_less() {
          assert_eq!(run("(> 1 2)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn mixed_numeric_greater_than() {
          assert_eq!(run("(> 2.0 1)").unwrap().kind, ExprKind::Boolean(true));
        }
      }

      mod greater_than_or_equal {
        use super::*;

        #[test]
        fn integer_greater_than_or_equal_greater() {
          assert_eq!(run("(>= 2 1)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn integer_greater_than_or_equal_equal() {
          assert_eq!(run("(>= 1 1)").unwrap().kind, ExprKind::Boolean(true));
        }

        #[test]
        fn integer_not_greater_than_or_equal_less() {
          assert_eq!(run("(>= 1 2)").unwrap().kind, ExprKind::Boolean(false));
        }

        #[test]
        fn mixed_numeric_greater_than_or_equal() {
          assert_eq!(run("(>= 1 1.0)").unwrap().kind, ExprKind::Boolean(true));
        }
      }
    }
  }

  mod scopes {
    use super::*;

    #[test]
    fn top_level_scopes() {
      let result = run("(def a 0) a").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(0));
    }

    #[test]
    fn function_scopes_are_isolated() {
      let source: &'static str = "((fn () (def a 0)))";
      let runtime = run_runtime(source);
      assert!(runtime.context.get("a").is_none());
    }

    #[test]
    fn nested_function_scopes_are_isolated() {
      let result = run(
        "
      (def a 0)
      ((fn ()
        (def a 1)
        ((fn () (def a 2)))))
      a
    ",
      )
      .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(0));
    }

    #[test]
    fn functions_can_set_to_outer() {
      let result = run(
        "
      (def a 0)
      (defn f () (set a 1))
      (f)
      a
    ",
      )
      .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(1));
    }

    #[test]
    fn closures_can_access_vars() {
      let result = run(
        "
      (def a 0)
      (defn outer ()
        (def a 1)
        (fn () a))
      ((outer))
    ",
      )
      .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(1));
    }

    #[test]
    fn closures_can_mutate_vars() {
      let result = run(
        "
      (def a 0)
      (defn outer ()
        (def a 1)
        (fn () (set a 2) a))
      ((outer))
    ",
      )
      .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(2));
    }

    #[test]
    fn closures_use_lexical_scope_not_call_site() {
      let result = run(
        "
      (def a 0)
      (defn f () a)
      (defn shadow () (def a 1) (f))
      (shadow)
    ",
      )
      .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(0));
    }

    #[test]
    fn calling_function_with_same_var_preserves_scope() {
      let result = run(
        "
      (def a 0)
      (defn f () a)
      (defn caller () (def a 1) (f))
      (caller)
    ",
      )
      .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(0));
    }

    #[test]
    fn closures_share_mutable_state_across_calls() {
      let result = run(
        "
      (defn make-counter ()
        (def n 0)
        (fn () (set n (+ n 1)) n))
      (def c (make-counter))
      (c)
      (c)
      (c)
    ",
      )
      .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(3));
    }

    #[test]
    fn for_each_pattern_uses_lexical_scope() {
      let result = run(
        "
      (defn for-test (each)
        (def el 999)
        (each 1))
      (def el 0)
      (for-test (fn (x) (set el x)))
      el
    ",
      )
      .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(1));
    }
  }

  mod garbage_collection {
    use super::*;

    #[test]
    fn gc_should_not_trigger_below_threshold() {
      let mut runtime: Runtime<'static> = Runtime {
        context: Context::new(10),
      };
      assert!(!runtime.context.should_gc());

      eval_source(&mut runtime, "((fn () nil)) ((fn () nil))");
      assert!(!runtime.context.should_gc());
    }

    #[test]
    fn gc_should_trigger_at_or_above_threshold() {
      let mut runtime: Runtime<'static> = Runtime {
        context: Context::new(3),
      };
      assert!(!runtime.context.should_gc());

      eval_source(&mut runtime, "((fn () nil))");
      assert!(!runtime.context.should_gc());

      eval_source(&mut runtime, "((fn () nil))");
      assert!(runtime.context.should_gc());

      eval_source(&mut runtime, "((fn () nil))");
      assert!(runtime.context.should_gc());
    }

    #[test]
    fn gc_removes_orphaned_call_scopes() {
      let mut runtime: Runtime<'static> = Runtime::default();

      eval_source(
        &mut runtime,
        "((fn () nil)) ((fn () nil)) ((fn () nil)) ((fn () nil))",
      );
      assert_eq!(runtime.context.envs_len(), 5);

      runtime.context.trigger_gc();
      assert_eq!(runtime.context.envs_len(), 1);
    }

    #[test]
    fn gc_removes_scope_of_overwritten_closure() {
      let mut runtime: Runtime<'static> = Runtime::default();

      eval_source(
        &mut runtime,
        "
      (defn make-counter ()
        (def n 0)
        (fn () (set n (+ n 1)) n))
      (def c (make-counter))
      ",
      );
      let before = runtime.context.envs_len();
      assert!(before > 1);

      eval_source(&mut runtime, "(def c nil)");
      runtime.context.trigger_gc();
      assert!(runtime.context.envs_len() < before);
    }

    #[test]
    fn gc_preserves_root_scope() {
      let mut runtime: Runtime<'static> = Runtime::default();

      eval_source(&mut runtime, "(def x 42) ((fn () nil)) ((fn () nil))");
      runtime.context.trigger_gc();

      let result = eval_source(&mut runtime, "x");
      assert_eq!(result.kind, ExprKind::Integer(42));
    }

    #[test]
    fn gc_preserves_live_closure() {
      let mut runtime: Runtime<'static> = Runtime::default();

      eval_source(
        &mut runtime,
        "
      (defn make-counter ()
        (def n 0)
        (fn () (set n (+ n 1)) n))
      (def c (make-counter))
      ",
      );

      // Fill envs with unrelated call scopes that should be collectible.
      eval_source(
        &mut runtime,
        "((fn () nil)) ((fn () nil)) ((fn () nil)) ((fn () nil))",
      );

      runtime.context.trigger_gc();

      let r1 = eval_source(&mut runtime, "(c)");
      assert_eq!(r1.kind, ExprKind::Integer(1));

      let r2 = eval_source(&mut runtime, "(c)");
      assert_eq!(r2.kind, ExprKind::Integer(2));

      let r3 = eval_source(&mut runtime, "(c)");
      assert_eq!(r3.kind, ExprKind::Integer(3));
    }

    #[test]
    fn gc_preserves_closure_parent_chain() {
      let mut runtime: Runtime<'static> = Runtime::default();

      eval_source(
        &mut runtime,
        "
      (defn outer ()
        (def a 10)
        (defn middle ()
          (def b 20)
          (fn () (+ a b)))
        (middle))
      (def f (outer))
      ",
      );

      eval_source(&mut runtime, "((fn () nil)) ((fn () nil))");
      runtime.context.trigger_gc();

      let r = eval_source(&mut runtime, "(f)");
      assert_eq!(r.kind, ExprKind::Integer(30));
    }

    #[test]
    fn gc_preserves_multiple_closures_sharing_state() {
      let mut runtime: Runtime<'static> = Runtime::default();

      eval_source(
        &mut runtime,
        "
      (def pair-inc nil)
      (def pair-get nil)
      (defn make-pair ()
        (def n 0)
        (set pair-inc (fn () (set n (+ n 1)) n))
        (set pair-get (fn () n)))
      (make-pair)
      ",
      );

      runtime.context.trigger_gc();

      eval_source(&mut runtime, "(pair-inc)");
      eval_source(&mut runtime, "(pair-inc)");
      let r = eval_source(&mut runtime, "(pair-get)");
      assert_eq!(r.kind, ExprKind::Integer(2));
    }

    #[test]
    fn gc_is_deterministic() {
      let mut runtime: Runtime<'static> = Runtime::default();

      eval_source(
        &mut runtime,
        "
      (defn make-counter ()
        (def n 0)
        (fn () (set n (+ n 1)) n))
      (def c (make-counter))
      ",
      );

      runtime.context.trigger_gc();
      let after_first = runtime.context.envs_len();
      runtime.context.trigger_gc();
      runtime.context.trigger_gc();
      assert_eq!(runtime.context.envs_len(), after_first);

      let r = eval_source(&mut runtime, "(c)");
      assert_eq!(r.kind, ExprKind::Integer(1));
    }
  }
}
