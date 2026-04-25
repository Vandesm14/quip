use std::rc::Rc;

use crate::{
  ast::{Expr, ExprKind, Span},
  context::{Context, Scope},
};

// FIXME(leonskij): Rc'ing the fields is a hack to reduce the stack size of the
//                  Result's produced during evaluation.
#[derive(Debug, Clone)]
pub struct Error {
  pub reason: Rc<ErrorReason>,
  pub call_stack: Rc<[CallFrame]>,
  pub scope: Rc<Scope>,
}

impl core::fmt::Display for Error {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    const LOCATION_PADDING: usize = 8;

    writeln!(f, "{}", self.reason)?;

    writeln!(f)?;
    writeln!(f, "current scope:")?;

    for (k, v) in &self.scope.vars {
      writeln!(f, "    - {k:<6} : {v}")?;
    }

    writeln!(f)?;
    writeln!(f, "call stack:")?;

    for call_frame in self.call_stack.iter().rev() {
      write!(f, "    ")?;

      let location = if let Some(span) = call_frame.expr.span {
        format!("{}:{}:", span.line, span.column)
      } else {
        "??:??:".into()
      };

      write!(f, "{location} ")?;

      let location_padding = (location.len() < LOCATION_PADDING)
        .then(|| " ".repeat(LOCATION_PADDING - location.len()));

      if let Some(location_padding) = &location_padding {
        write!(f, "{location_padding}")?;
      }

      write!(f, "{}", call_frame.expr)?;

      if call_frame.recurs != 0 {
        writeln!(f)?;
        write!(f, "    {}", " ".repeat(location.len()))?;

        if let Some(location_padding) = &location_padding {
          write!(f, "{location_padding}")?;
        }

        let plural = if call_frame.recurs != 1 { "s" } else { "" };
        writeln!(f, " ^ recursed {} time{plural}", call_frame.recurs)?;
      } else {
        writeln!(f)?;
      }
    }

    Ok(())
  }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ErrorReason {
  #[error("{0}")]
  CallError(#[from] CallError),
  #[error("{0}")]
  Message(String),
}

impl From<String> for ErrorReason {
  fn from(msg: String) -> Self {
    Self::Message(msg)
  }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("call error on '{symbol}': {kind:?}")]
pub struct CallError {
  pub symbol: String,
  pub kind: CallErrorKind,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CallErrorKind {
  #[error(
    "incorrect arity: expected {expected} arguments, received {received} arguments"
  )]
  IncorrectArity { expected: usize, received: usize },
  #[error("type mismatch: expected {expected:?}, received {received:?}")]
  TypeMismatch {
    expected: Vec<String>,
    received: Vec<String>,
  },
  #[error("uncallable form")]
  UncallableForm,
}

#[derive(Debug, Clone, Default)]
pub struct Runtime {
  pub context: Context,
  pub recur: Option<Vec<Expr>>,
  pub call_stack: Vec<CallFrame>,
}

impl Runtime {
  pub fn call(
    &mut self,
    expr: &Expr,
    call_args: Vec<Expr>,
    name: &str,
  ) -> Result<Expr, Error> {
    let ExprKind::Function {
      ref params,
      ref body,
      env,
    } = expr.kind
    else {
      unreachable!(
        "only function expressions can be called but got {expr}; logic bug"
      );
    };

    if call_args.len() != params.len() {
      return Err(self.error(ErrorReason::CallError(CallError {
        symbol: name.to_owned(),
        kind: CallErrorKind::IncorrectArity {
          expected: params.len(),
          received: call_args.len(),
        },
      })));
    }

    let mut result = Expr {
      kind: ExprKind::Nil,
      span: None,
    };

    let call_frame_index = self.call_stack.len();
    self.call_stack.push(CallFrame {
      expr: expr.clone(),
      call_site: expr.span,
      recurs: 0,
    });

    let mut bound = Vec::new();
    for (param, arg_expr) in params.iter().zip(call_args.iter()) {
      let val = self.eval_expr(arg_expr)?;
      bound.push((param.clone(), val));
    }

    let saved = self.context.push_scope(env);

    self.recur = Some(call_args);
    let mut recurred = false;
    while let Some(args) = self.recur.take() {
      if recurred {
        for (param, arg_expr) in params.iter().zip(args.iter()) {
          let val = self.eval_expr(arg_expr)?;
          bound.push((param.clone(), val));
        }
      }

      for (param_name, val) in bound.drain(..) {
        self.context.define(param_name, val);
      }
      for body_expr in body {
        result = match self.eval_expr(body_expr) {
          Ok(v) => v,
          Err(e) => {
            self.context.restore_scope(saved);
            return Err(e);
          }
        };
      }

      self.call_stack[call_frame_index].recurs += 1;
      recurred = true;
    }

    self.context.restore_scope(saved);
    assert!(self.call_stack.pop().is_some());

    Ok(result)
  }

  pub(crate) fn parse_params(
    param_list: &[Expr],
    ctx: &str,
  ) -> Result<Vec<Rc<str>>, String> {
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

  pub fn eval_expr(&mut self, expr: &Expr) -> Result<Expr, Error> {
    if let ExprKind::Form(list) = &expr.kind {
      if let Some(first) = list.first() {
        match &first.kind {
          ExprKind::Symbol(symbol) => {
            let symbol = symbol.to_string();

            // Check for intrinsics first.
            if let Some(intrinsic) =
              self.context.get_intrinsic(symbol.as_str()).cloned()
            {
              let args = intrinsic.check_params(self, list, &symbol)?;
              self.call_stack.push(CallFrame {
                expr: expr.clone(),
                call_site: expr.span,
                recurs: 0,
              });
              let result = (intrinsic.handler)(self, args);
              assert!(self.call_stack.pop().is_some());
              return result;
            }

            // Symbol look-up.
            let val =
              self.context.get(symbol.as_str()).cloned().ok_or_else(|| {
                self.error(ErrorReason::Message(format!(
                  "undefined '{}'",
                  symbol
                )))
              })?;
            if let ExprKind::Function { .. } = val.kind {
              let call_args = list.get(1..).unwrap_or(&[]);
              self.call(&val, call_args.to_vec(), symbol.as_ref())
            } else if let ExprKind::Form(..) = val.kind {
              self.eval_expr(&val)
            } else {
              Err(self.error(ErrorReason::Message(format!(
                "'{}' is not a function",
                symbol
              ))))
            }
          }
          ExprKind::Error(_) => todo!("call errors = throw them?"),
          // ExprKind::List(exprs) => todo!(),
          ExprKind::Form(exprs) => {
            let mut exprs = exprs.to_vec();
            for arg in list.get(1..).unwrap_or_default() {
              exprs.push(arg.clone());
            }
            self.eval_expr(&Expr {
              kind: ExprKind::Form(Rc::new(exprs)),
              span: None,
            })
          }
          ExprKind::Function { .. } => self.call(
            first,
            list.get(1..).unwrap_or_default().to_vec(),
            &format!("{expr}"),
          ),
          _ => Err(self.error(ErrorReason::CallError(CallError {
            symbol: format!("{expr}"),
            kind: CallErrorKind::UncallableForm,
          }))),
        }
      } else {
        // TODO: decide whether to error if there's an empty form.
        Ok(expr.clone())
      }
    } else if let ExprKind::Symbol(sym) = &expr.kind {
      // Get vars.
      self.context.get(sym).cloned().ok_or_else(|| {
        self.error(ErrorReason::Message(format!("undefined var '{}'", sym)))
      })
    } else {
      Ok(expr.clone())
    }
  }

  #[inline]
  pub(crate) fn error(&self, reason: ErrorReason) -> Error {
    Error {
      reason: reason.into(),
      call_stack: self.call_stack.clone().into(),
      scope: self.context.scope().clone().into(),
    }
  }
}

#[derive(Debug, Clone)]
pub struct CallFrame {
  pub expr: Expr,
  pub call_site: Option<Span>,
  pub recurs: usize,
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ast::{lex, parse};

  fn run(source: &str) -> Result<Expr, String> {
    let tokens = lex(source, Default::default());
    let exprs = parse(source, tokens)?;

    let mut runtime = Runtime::default();
    runtime.context.use_intrinsics(crate::intrinsic::all());

    let mut last = Expr {
      kind: ExprKind::Nil,
      span: None,
    };
    for expr in &exprs {
      last = runtime.eval_expr(expr).map_err(|e| e.to_string())?;
    }
    Ok(last)
  }

  fn eval_source(runtime: &mut Runtime, source: &str) -> Expr {
    let tokens = lex(source, Default::default());
    let exprs = parse(source, tokens).unwrap();
    let mut last = Expr {
      kind: ExprKind::Nil,
      span: None,
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
          assert_eq!(
            run("(= 1 \"1\")").unwrap().kind,
            ExprKind::Boolean(false)
          );
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

      mod boolean {
        use super::*;

        mod and {
          use super::*;

          #[test]
          fn true_and_true() {
            assert_eq!(
              run("(and true true)").unwrap().kind,
              ExprKind::Boolean(true)
            );
          }

          #[test]
          fn true_and_false() {
            assert_eq!(
              run("(and true false)").unwrap().kind,
              ExprKind::Boolean(false)
            );
          }

          #[test]
          fn false_and_true() {
            assert_eq!(
              run("(and false true)").unwrap().kind,
              ExprKind::Boolean(false)
            );
          }

          #[test]
          fn false_and_false() {
            assert_eq!(
              run("(and false false)").unwrap().kind,
              ExprKind::Boolean(false)
            );
          }

          #[test]
          fn non_boolean_lhs_is_an_error() {
            assert!(run("(and 1 true)").is_err());
          }

          #[test]
          fn non_boolean_rhs_is_an_error() {
            assert!(run("(and true 1)").is_err());
          }

          #[test]
          fn short_circuits_on_false_lhs() {
            assert_eq!(
              run("(and false (/ 1 0))").unwrap().kind,
              ExprKind::Boolean(false)
            );
          }

          #[test]
          fn missing_argument_is_an_error() {
            assert!(run("(and true)").is_err());
          }
        }

        mod or {
          use super::*;

          #[test]
          fn true_or_true() {
            assert_eq!(
              run("(or true true)").unwrap().kind,
              ExprKind::Boolean(true)
            );
          }

          #[test]
          fn true_or_false() {
            assert_eq!(
              run("(or true false)").unwrap().kind,
              ExprKind::Boolean(true)
            );
          }

          #[test]
          fn false_or_true() {
            assert_eq!(
              run("(or false true)").unwrap().kind,
              ExprKind::Boolean(true)
            );
          }

          #[test]
          fn false_or_false() {
            assert_eq!(
              run("(or false false)").unwrap().kind,
              ExprKind::Boolean(false)
            );
          }

          #[test]
          fn non_boolean_lhs_is_an_error() {
            assert!(run("(or 1 false)").is_err());
          }

          #[test]
          fn non_boolean_rhs_is_an_error() {
            assert!(run("(or false 1)").is_err());
          }

          #[test]
          fn short_circuits_on_true_lhs() {
            assert_eq!(
              run("(or true (/ 1 0))").unwrap().kind,
              ExprKind::Boolean(true)
            );
          }

          #[test]
          fn missing_argument_is_an_error() {
            assert!(run("(or true)").is_err());
          }
        }

        mod not {
          use super::*;

          #[test]
          fn not_true() {
            assert_eq!(
              run("(not true)").unwrap().kind,
              ExprKind::Boolean(false)
            );
          }

          #[test]
          fn not_false() {
            assert_eq!(
              run("(not false)").unwrap().kind,
              ExprKind::Boolean(true)
            );
          }

          #[test]
          fn non_boolean_argument_is_an_error() {
            assert!(run("(not 1)").is_err());
          }

          #[test]
          fn double_not() {
            assert_eq!(
              run("(not (not true))").unwrap().kind,
              ExprKind::Boolean(true)
            );
          }

          #[test]
          fn missing_argument_is_an_error() {
            assert!(run("(not)").is_err());
          }
        }
      }
    }

    mod string_list {
      use super::*;

      mod len {
        use super::*;

        #[test]
        fn len_of_empty_string() {
          assert_eq!(run("(len \"\")").unwrap().kind, ExprKind::Integer(0));
        }

        #[test]
        fn len_of_string() {
          assert_eq!(
            run("(len \"hello\")").unwrap().kind,
            ExprKind::Integer(5)
          );
        }

        #[test]
        fn len_of_empty_list() {
          assert_eq!(run("(len [])").unwrap().kind, ExprKind::Integer(0));
        }

        #[test]
        fn len_of_list_with_integers() {
          assert_eq!(run("(len [1 2 3])").unwrap().kind, ExprKind::Integer(3));
        }

        #[test]
        fn len_of_list_with_mixed_types() {
          assert_eq!(
            run("(len [1 \"hello\" true])").unwrap().kind,
            ExprKind::Integer(3)
          );
        }

        #[test]
        fn len_requires_one_argument() {
          assert!(run("(len)").is_err());
        }

        #[test]
        fn len_requires_string_or_list() {
          assert!(run("(len 42)").is_err());
        }

        #[test]
        fn len_of_boolean_fails() {
          assert!(run("(len true)").is_err());
        }

        #[test]
        fn len_of_nil_fails() {
          assert!(run("(len nil)").is_err());
        }
      }
    }

    mod r#typeof {
      use super::*;

      #[test]
      fn typeof_integer() {
        let result = run("(typeof 42)").unwrap();
        assert_eq!(result.kind, ExprKind::String("integer".to_string()));
      }

      #[test]
      fn typeof_float() {
        let result = run("(typeof 3.14)").unwrap();
        assert_eq!(result.kind, ExprKind::String("float".to_string()));
      }

      #[test]
      fn typeof_string() {
        let result = run("(typeof \"hello\")").unwrap();
        assert_eq!(result.kind, ExprKind::String("string".to_string()));
      }

      #[test]
      fn typeof_empty_string() {
        let result = run("(typeof \"\")").unwrap();
        assert_eq!(result.kind, ExprKind::String("string".to_string()));
      }

      #[test]
      fn typeof_true_boolean() {
        let result = run("(typeof true)").unwrap();
        assert_eq!(result.kind, ExprKind::String("boolean".to_string()));
      }

      #[test]
      fn typeof_false_boolean() {
        let result = run("(typeof false)").unwrap();
        assert_eq!(result.kind, ExprKind::String("boolean".to_string()));
      }

      #[test]
      fn typeof_nil() {
        let result = run("(typeof nil)").unwrap();
        assert_eq!(result.kind, ExprKind::String("nil".to_string()));
      }

      #[test]
      fn typeof_form() {
        let result = run("(typeof ())").unwrap();
        assert_eq!(result.kind, ExprKind::String("form".to_string()));
      }

      #[test]
      fn typeof_list() {
        let result = run("(typeof [])").unwrap();
        assert_eq!(result.kind, ExprKind::String("list".to_string()));
      }

      #[test]
      fn typeof_function() {
        let result = run("(typeof (fn [] 42))").unwrap();
        assert_eq!(result.kind, ExprKind::String("function".to_string()));
      }

      #[test]
      fn typeof_of_defined_function() {
        let result = run("(defn foo [] 42) (typeof foo)").unwrap();
        assert_eq!(result.kind, ExprKind::String("function".to_string()));
      }

      #[test]
      fn typeof_of_defined_var() {
        let result = run("(def foo 42) (typeof foo)").unwrap();
        assert_eq!(result.kind, ExprKind::String("integer".to_string()));
      }

      #[test]
      fn typeof_requires_one_argument() {
        assert!(run("(typeof)").is_err());
      }

      #[test]
      fn typeof_returns_correct_type_in_expression() {
        let result = run("(= (typeof \"test\") \"string\")").unwrap();
        assert_eq!(result.kind, ExprKind::Boolean(true));
      }

      #[test]
      fn typeof_returns_different_types_are_not_equal() {
        let result = run("(= (typeof 42) (typeof \"42\"))").unwrap();
        assert_eq!(result.kind, ExprKind::Boolean(false));
      }
    }

    mod list {
      use super::*;

      #[test]
      fn list_with_no_arguments() {
        let result = run("(list)").unwrap();
        assert_eq!(result.kind, ExprKind::List(Rc::new(vec![])));
      }

      #[test]
      fn list_with_single_integer() {
        let result = run("(list 1)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 1);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_with_multiple_integers() {
        let result = run("(list 1 2 3)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
          assert_eq!(items[1].kind, ExprKind::Integer(2));
          assert_eq!(items[2].kind, ExprKind::Integer(3));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_with_mixed_types() {
        let result = run("(list 42 \"hello\" true)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::Integer(42));
          assert_eq!(items[1].kind, ExprKind::String("hello".to_string()));
          assert_eq!(items[2].kind, ExprKind::Boolean(true));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_with_nil() {
        let result = run("(list nil)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 1);
          assert_eq!(items[0].kind, ExprKind::Nil);
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_evaluates_expressions() {
        let result = run("(list (+ 1 2) (* 3 4))").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 2);
          assert_eq!(items[0].kind, ExprKind::Integer(3));
          assert_eq!(items[1].kind, ExprKind::Integer(12));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_with_strings() {
        let result = run("(list \"a\" \"b\" \"c\")").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::String("a".to_string()));
          assert_eq!(items[1].kind, ExprKind::String("b".to_string()));
          assert_eq!(items[2].kind, ExprKind::String("c".to_string()));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_with_symbols() {
        let result = run("(list (lazy a) (lazy b) (lazy c))").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::Symbol("a".into()));
          assert_eq!(items[1].kind, ExprKind::Symbol("b".into()));
          assert_eq!(items[2].kind, ExprKind::Symbol("c".into()));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_with_variables() {
        let result = run("(def x 10) (list x (+ x 5))").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 2);
          assert_eq!(items[0].kind, ExprKind::Integer(10));
          assert_eq!(items[1].kind, ExprKind::Integer(15));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_with_nested_lists() {
        let result = run("(list (list 1 2) (list 3 4))").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 2);
          if let ExprKind::List(inner1) = &items[0].kind {
            assert_eq!(inner1.len(), 2);
            assert_eq!(inner1[0].kind, ExprKind::Integer(1));
            assert_eq!(inner1[1].kind, ExprKind::Integer(2));
          } else {
            panic!("Expected nested list");
          }
          if let ExprKind::List(inner2) = &items[1].kind {
            assert_eq!(inner2.len(), 2);
            assert_eq!(inner2[0].kind, ExprKind::Integer(3));
            assert_eq!(inner2[1].kind, ExprKind::Integer(4));
          } else {
            panic!("Expected nested list");
          }
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn list_can_be_used_with_len() {
        let result = run("(len (list 1 2 3 4))").unwrap();
        assert_eq!(result.kind, ExprKind::Integer(4));
      }

      #[test]
      fn list_returns_type_list() {
        let result = run("(typeof (list 1 2 3))").unwrap();
        assert_eq!(result.kind, ExprKind::String("list".to_string()));
      }
    }

    mod nth {
      use super::*;

      #[test]
      fn nth_first_element_of_list() {
        let result = run("(nth 0 [1 2 3])").unwrap();
        assert_eq!(result.kind, ExprKind::Integer(1));
      }

      #[test]
      fn nth_middle_element_of_list() {
        let result = run("(nth 1 [1 2 3])").unwrap();
        assert_eq!(result.kind, ExprKind::Integer(2));
      }

      #[test]
      fn nth_last_element_of_list() {
        let result = run("(nth 2 [1 2 3])").unwrap();
        assert_eq!(result.kind, ExprKind::Integer(3));
      }

      #[test]
      fn nth_out_of_bounds() {
        let result = run("(nth 3 [1 2 3])");
        assert!(result.is_err());
      }

      #[test]
      fn nth_negative_index() {
        let result = run("(nth -1 [1 2 3])");
        assert!(result.is_err());
      }

      #[test]
      fn nth_of_string() {
        let result = run("(nth 0 \"hello\")").unwrap();
        assert_eq!(result.kind, ExprKind::String("h".to_string()));
      }

      #[test]
      fn nth_of_string_middle() {
        let result = run("(nth 2 \"hello\")").unwrap();
        assert_eq!(result.kind, ExprKind::String("l".to_string()));
      }

      #[test]
      fn nth_of_string_out_of_bounds() {
        let result = run("(nth 10 \"hello\")");
        assert!(result.is_err());
      }

      #[test]
      fn nth_requires_two_arguments() {
        let result = run("(nth 0)");
        assert!(result.is_err());
      }

      #[test]
      fn nth_requires_integer_index() {
        let result = run("(nth \"a\" [1 2 3])");
        assert!(result.is_err());
      }

      #[test]
      fn nth_requires_list_or_string() {
        let result = run("(nth 0 1)");
        assert!(result.is_err());
      }
    }

    mod set_nth {
      use super::*;

      #[test]
      fn set_nth_first_element() {
        let result = run("(set-nth 0 [1 2 3] 0)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::Integer(0));
          assert_eq!(items[1].kind, ExprKind::Integer(2));
          assert_eq!(items[2].kind, ExprKind::Integer(3));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn set_nth_middle_element() {
        let result = run("(set-nth 1 [1 2 3] 0)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
          assert_eq!(items[1].kind, ExprKind::Integer(0));
          assert_eq!(items[2].kind, ExprKind::Integer(3));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn set_nth_last_element() {
        let result = run("(set-nth 2 [1 2 3] 0)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
          assert_eq!(items[1].kind, ExprKind::Integer(2));
          assert_eq!(items[2].kind, ExprKind::Integer(0));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn set_nth_out_of_bounds() {
        let result = run("(set-nth 5 [1 2 3] 0)");
        assert!(result.is_err());
      }

      #[test]
      fn set_nth_negative_index() {
        let result = run("(set-nth -1 [1 2 3] 0)");
        assert!(result.is_err());
      }

      #[test]
      fn set_nth_with_different_type() {
        let result = run("(set-nth 0 [1 2 3] \"hello\")").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items[0].kind, ExprKind::String("hello".to_string()));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn set_nth_requires_three_arguments() {
        let result = run("(set-nth 0 [1 2 3])");
        assert!(result.is_err());
      }

      #[test]
      fn set_nth_requires_list() {
        let result = run("(set-nth 0 1 2)");
        assert!(result.is_err());
      }

      #[test]
      fn set_nth_requires_integer_index() {
        let result = run("(set-nth \"a\" [1 2 3] 0)");
        assert!(result.is_err());
      }
    }

    mod push {
      use super::*;

      #[test]
      fn push_to_empty_list() {
        let result = run("(push [] 42)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 1);
          assert_eq!(items[0].kind, ExprKind::Integer(42));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn push_to_non_empty_list() {
        let result = run("(push [1 2 3] 42)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 4);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
          assert_eq!(items[1].kind, ExprKind::Integer(2));
          assert_eq!(items[2].kind, ExprKind::Integer(3));
          assert_eq!(items[3].kind, ExprKind::Integer(42));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn push_multiple_values() {
        let result = run("(push (push [1] 2) 3)").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
          assert_eq!(items[1].kind, ExprKind::Integer(2));
          assert_eq!(items[2].kind, ExprKind::Integer(3));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn push_different_type() {
        let result = run("(push [1 2] \"hello\")").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
          assert_eq!(items[1].kind, ExprKind::Integer(2));
          assert_eq!(items[2].kind, ExprKind::String("hello".to_string()));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn push_requires_two_arguments() {
        let result = run("(push [1])");
        assert!(result.is_err());
      }

      #[test]
      fn push_requires_list() {
        let result = run("(push 1 2)");
        assert!(result.is_err());
      }

      #[test]
      fn push_with_expression() {
        let result = run("(push [1 2] (+ 3 4))").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 3);
          assert_eq!(items[2].kind, ExprKind::Integer(7));
        } else {
          panic!("Expected list");
        }
      }
    }

    mod pop {
      use super::*;

      #[test]
      fn pop_from_single_element_list() {
        let result = run("(pop [1])").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 0);
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn pop_from_multi_element_list() {
        let result = run("(pop [1 2 3])").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 2);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
          assert_eq!(items[1].kind, ExprKind::Integer(2));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn pop_multiple_times() {
        let result = run("(pop (pop [1 2 3]))").unwrap();
        if let ExprKind::List(items) = result.kind {
          assert_eq!(items.len(), 1);
          assert_eq!(items[0].kind, ExprKind::Integer(1));
        } else {
          panic!("Expected list");
        }
      }

      #[test]
      fn pop_empty_list() {
        let result = run("(pop (list))");
        assert!(result.is_err());
      }

      #[test]
      fn pop_requires_list() {
        let result = run("(pop 1)");
        assert!(result.is_err());
      }

      #[test]
      fn pop_requires_one_argument() {
        let result = run("(pop)");
        assert!(result.is_err());
      }
    }
  }

  mod call {
    use super::*;

    #[test]
    fn call_thunk_calls_zero_arg_fn() {
      let result = run("(call (fn [] 42))").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(42));
    }

    #[test]
    fn call_thunk_calls_one_arg_fn() {
      let result = run("(call (fn [a] a) 2)").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(2));
    }

    #[test]
    fn call_thunk_calls_one_arg_fn_err() {
      let result = run("(call (fn [a] a))");
      assert!(result.is_err());
    }

    #[test]
    fn call_non_fn_returns_value() {
      let result = run("(call 99)").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(99));
    }

    #[test]
    fn call_fn_from_var() {
      let result = run("(def t (fn [] 1)) (call t)").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(1));
    }

    #[test]
    fn call_expr_from_var() {
      let result = run("(def t (lazy (+ 1 2))) (call t)").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(3));
    }

    #[test]
    fn call_lazy_expr() {
      let result = run("(call (lazy (+ 1 2)))").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(3));
    }

    #[test]
    fn call_double_fn() {
      let result =
        run("(def a 0) (call (fn [] (set a 1) (fn [] (set a 2)))) a").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(1));
    }

    #[test]
    fn call_double_fn_from_var() {
      let result =
        run("(def a 0) (def t (fn [] (set a 1) (fn [] (set a 2)))) (call t) a")
          .unwrap();
      assert_eq!(result.kind, ExprKind::Integer(1));
    }

    #[test]
    fn call_missing_arg_errors() {
      let result = run("(call)");
      assert!(result.is_err());
    }
  }

  mod lazy_parsing {
    use super::*;

    #[test]
    fn test_lazy_symbols() {
      let result = run("'abc").unwrap();
      assert_eq!(result.kind, ExprKind::Symbol("abc".into()));
    }

    #[test]
    fn test_lazy_lists() {
      let result = run("'(+ 1 2)").unwrap();
      if let ExprKind::Form(items) = result.kind {
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, ExprKind::Symbol("+".into()));
        assert_eq!(items[1].kind, ExprKind::Integer(1));
        assert_eq!(items[2].kind, ExprKind::Integer(2));
      } else {
        panic!("Expected list");
      };
    }

    #[test]
    fn test_lazy_nested_lists() {
      let result = run("'(+ [1 2])").unwrap();
      if let ExprKind::Form(items) = result.kind {
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, ExprKind::Symbol("+".into()));
        if let ExprKind::List(ref items) =
          items.get(1).expect("Expected second item in list").kind
        {
          assert_eq!(items[0].kind, ExprKind::Integer(1));
          assert_eq!(items[1].kind, ExprKind::Integer(2));
        } else {
          panic!("Expected second list");
        }
      } else {
        panic!("Expected list");
      };
    }

    #[test]
    fn test_lazy_list_itemss() {
      let result = run("(list '+ 1 2)").unwrap();
      if let ExprKind::List(items) = result.kind {
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, ExprKind::Symbol("+".into()));
        assert_eq!(items[1].kind, ExprKind::Integer(1));
        assert_eq!(items[2].kind, ExprKind::Integer(2));
      } else {
        panic!("Expected list");
      };
    }

    #[test]
    fn test_lazy_and_lazy_fn() {
      let result = run("(= 'a (lazy a))").unwrap();
      assert_eq!(result.kind, ExprKind::Boolean(true));
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
      let result = run("(def a 0) ((fn [] (def a 1))) a").unwrap();
      assert_eq!(result.kind, ExprKind::Integer(0));
    }

    #[test]
    fn nested_function_scopes_are_isolated() {
      let result = run(
        "
      (def a 0)
      ((fn []
        (def a 1)
        ((fn [] (def a 2)))))
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
      (defn f [] (set a 1))
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
      (defn outer []
        (def a 1)
        (fn [] a))
      (call (outer))
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
      (defn outer []
        (def a 1)
        (fn [] (set a 2) a))
      (call (outer))
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
      (defn f [] a)
      (defn shadow [] (def a 1) (f))
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
      (defn f [] a)
      (defn caller [] (def a 1) (f))
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
      (defn make-counter []
        (def n 0)
        (fn [] (set n (+ n 1)) n))
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
      (defn for-test [each]
        (def el 999)
        (each 1))
      (def el 0)
      (for-test (fn [x] (set el x)))
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
      let mut runtime = Runtime {
        context: Context::new(10),
        ..Default::default()
      };
      runtime.context.use_intrinsics(crate::intrinsic::all());

      assert!(!runtime.context.should_gc());

      eval_source(&mut runtime, "((fn [] nil)) ((fn [] nil))");
      assert!(!runtime.context.should_gc());
    }

    #[test]
    fn gc_should_trigger_at_or_above_threshold() {
      let mut runtime = Runtime {
        context: Context::new(3),
        ..Default::default()
      };
      runtime.context.use_intrinsics(crate::intrinsic::all());

      assert!(!runtime.context.should_gc());

      eval_source(&mut runtime, "(call (fn [] nil))");
      assert!(!runtime.context.should_gc());

      eval_source(&mut runtime, "(call (fn [] nil))");
      assert!(runtime.context.should_gc());

      eval_source(&mut runtime, "(call (fn [] nil))");
      assert!(runtime.context.should_gc());
    }

    #[test]
    fn gc_removes_orphaned_call_scopes() {
      let mut runtime = Runtime::default();
      runtime.context.use_intrinsics(crate::intrinsic::all());

      eval_source(
        &mut runtime,
        "(call (fn [] nil)) (call (fn [] nil)) (call (fn [] nil)) (call (fn [] nil))",
      );
      assert_eq!(runtime.context.envs_len(), 5);

      runtime.context.trigger_gc();
      assert_eq!(runtime.context.envs_len(), 1);
    }

    #[test]
    fn gc_removes_scope_of_overwritten_closure() {
      let mut runtime = Runtime::default();
      runtime.context.use_intrinsics(crate::intrinsic::all());

      eval_source(
        &mut runtime,
        "
      (defn make-counter []
        (def n 0)
        (fn [] (set n (+ n 1)) n))
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
      let mut runtime = Runtime::default();
      runtime.context.use_intrinsics(crate::intrinsic::all());

      eval_source(&mut runtime, "(def x 42) ((fn [] nil)) ((fn [] nil))");
      runtime.context.trigger_gc();

      let result = eval_source(&mut runtime, "x");
      assert_eq!(result.kind, ExprKind::Integer(42));
    }

    #[test]
    fn gc_preserves_live_closure() {
      let mut runtime = Runtime::default();
      runtime.context.use_intrinsics(crate::intrinsic::all());

      eval_source(
        &mut runtime,
        "
      (defn make-counter []
        (def n 0)
        (fn [] (set n (+ n 1)) n))
      (def c (make-counter))
      ",
      );

      // Fill envs with unrelated call scopes that should be collectible.
      eval_source(
        &mut runtime,
        "((fn [] nil)) ((fn [] nil)) ((fn [] nil)) ((fn [] nil))",
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
      let mut runtime = Runtime::default();
      runtime.context.use_intrinsics(crate::intrinsic::all());

      eval_source(
        &mut runtime,
        "
      (defn outer []
        (def a 10)
        (defn middle []
          (def b 20)
          (fn [] (+ a b)))
        (middle))
      (def f (outer))
      ",
      );

      eval_source(&mut runtime, "((fn [] nil)) ((fn [] nil))");
      runtime.context.trigger_gc();

      let r = eval_source(&mut runtime, "(f)");
      assert_eq!(r.kind, ExprKind::Integer(30));
    }

    #[test]
    fn gc_preserves_multiple_closures_sharing_state() {
      let mut runtime = Runtime::default();
      runtime.context.use_intrinsics(crate::intrinsic::all());

      eval_source(
        &mut runtime,
        "
      (def pair-inc nil)
      (def pair-get nil)
      (defn make-pair []
        (def n 0)
        (set pair-inc (fn [] (set n (+ n 1)) n))
        (set pair-get (fn [] n)))
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
      let mut runtime = Runtime::default();
      runtime.context.use_intrinsics(crate::intrinsic::all());

      eval_source(
        &mut runtime,
        "
      (defn make-counter []
        (def n 0)
        (fn [] (set n (+ n 1)) n))
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
