use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::{Expr, ExprKind, lex, parse};
use crate::run::{CallError, CallErrorKind, Error, ErrorReason, Runtime};

/// Type variant for parameter validation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExprType {
  /// true|false
  Boolean,
  /// 1, 0, -1
  Integer,
  /// 1.0, 0.0, -1.0
  Float,
  /// int|float
  Numeric,
  /// string
  String,
  /// (...)
  List,
  /// any
  Any,
}

impl std::fmt::Display for ExprType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ExprType::Boolean => write!(f, "boolean"),
      ExprType::Integer => write!(f, "integer"),
      ExprType::Float => write!(f, "float"),
      ExprType::Numeric => write!(f, "numeric"),
      ExprType::String => write!(f, "string"),
      ExprType::List => write!(f, "list"),
      ExprType::Any => write!(f, "any"),
    }
  }
}

#[derive(Debug, Clone, Copy)]
pub enum Param {
  One(ExprType),
  Many(ExprType),
  EvalTo(ExprType),
  ManyEvalTo(ExprType),
}

pub struct Intrinsic {
  pub name: &'static str,
  pub params: &'static [Param],
  pub handler: fn(&mut Runtime, Vec<Expr>) -> Result<Expr, Error>,
}

impl Intrinsic {
  pub fn check_params(
    &self,
    runtime: &mut Runtime,
    list: &[Expr],
    symbol: &str,
  ) -> Result<Vec<Expr>, Error> {
    let args = list.get(1..).unwrap_or(&[]);
    let mut validated = Vec::new();
    let has_many = self
      .params
      .iter()
      .any(|p| matches!(p, Param::Many(_) | Param::ManyEvalTo(_)));

    for (i, param) in self.params.iter().enumerate() {
      match param {
        Param::One(kind) => {
          if i >= args.len() {
            return Err(runtime.error(ErrorReason::CallError(CallError {
              symbol: symbol.to_owned(),
              kind: CallErrorKind::IncorrectArity {
                expected: self.params.len(),
                received: args.len(),
              },
            })));
          }
          self.check_type(runtime, &args[i], *kind, symbol)?;
          validated.push(args[i].clone());
        }
        Param::EvalTo(kind) => {
          if i >= args.len() {
            return Err(runtime.error(ErrorReason::CallError(CallError {
              symbol: symbol.to_owned(),
              kind: CallErrorKind::IncorrectArity {
                expected: self.params.len(),
                received: args.len(),
              },
            })));
          }
          let evaluated = runtime.eval_expr(&args[i])?;
          self.check_type(runtime, &evaluated, *kind, symbol)?;
          validated.push(evaluated);
        }
        Param::Many(kind) => {
          if i != self.params.len() - 1 {
            return Err(runtime.error(ErrorReason::Message(format!(
              "'{}': Many must be the last parameter",
              symbol
            ))));
          }
          for (_j, arg) in args.iter().enumerate().skip(i) {
            self.check_type(runtime, arg, *kind, symbol)?;
            validated.push(arg.clone());
          }
          break;
        }
        Param::ManyEvalTo(kind) => {
          if i != self.params.len() - 1 {
            return Err(runtime.error(ErrorReason::Message(format!(
              "'{}': ManyEvalTo must be the last parameter",
              symbol
            ))));
          }
          for (_j, arg) in args.iter().enumerate().skip(i) {
            let evaluated = runtime.eval_expr(arg)?;
            self.check_type(runtime, &evaluated, *kind, symbol)?;
            validated.push(evaluated);
          }
          break;
        }
      }
    }

    // Check for too many arguments (when no Many param)
    if !has_many && args.len() > self.params.len() {
      return Err(runtime.error(ErrorReason::CallError(CallError {
        symbol: symbol.to_owned(),
        kind: CallErrorKind::IncorrectArity {
          expected: self.params.len(),
          received: args.len(),
        },
      })));
    }

    Ok(validated)
  }

  /// Validate argument type against ExprKindVariant
  fn check_type(
    &self,
    runtime: &mut Runtime,
    expr: &Expr,
    variant: ExprType,
    symbol: &str,
  ) -> Result<(), Error> {
    use crate::ast::ExprKind;

    let valid = match variant {
      ExprType::Boolean => matches!(expr.kind, ExprKind::Boolean(_)),
      ExprType::Integer => matches!(expr.kind, ExprKind::Integer(_)),
      ExprType::Float => matches!(expr.kind, ExprKind::Float(_)),
      ExprType::Numeric => {
        matches!(expr.kind, ExprKind::Integer(_) | ExprKind::Float(_))
      }
      ExprType::String => matches!(expr.kind, ExprKind::String(_)),
      ExprType::List => matches!(expr.kind, ExprKind::List(_)),
      ExprType::Any => true,
    };

    if !valid {
      Err(runtime.error(ErrorReason::CallError(CallError {
        symbol: symbol.to_owned(),
        kind: CallErrorKind::TypeMismatch {
          expected: vec![variant.to_string()],
          received: vec![expr.kind.type_name().to_owned()],
        },
      })))
    } else {
      Ok(())
    }
  }
}

pub fn arithmetic(map: &mut HashMap<&'static str, Intrinsic>) {
  const ADD: Intrinsic = Intrinsic {
    name: "+",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: |runtime, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      match lhs + rhs {
        Ok(kind) => Ok(Expr {
          kind: kind.normalize_numeric(),
          span: None,
        }),
        Err(_) => Err(runtime.error(ErrorReason::Message(
          "'+' requires numeric arguments".to_string(),
        ))),
      }
    },
  };
  const SUB: Intrinsic = Intrinsic {
    name: "-",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: |runtime, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      match lhs - rhs {
        Ok(kind) => Ok(Expr {
          kind: kind.normalize_numeric(),
          span: None,
        }),
        Err(_) => Err(runtime.error(ErrorReason::Message(
          "'-' requires numeric arguments".to_string(),
        ))),
      }
    },
  };
  const MUL: Intrinsic = Intrinsic {
    name: "*",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: |runtime, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      match lhs * rhs {
        Ok(kind) => Ok(Expr {
          kind: kind.normalize_numeric(),
          span: None,
        }),
        Err(_) => Err(runtime.error(ErrorReason::Message(
          "'*' requires numeric arguments".to_string(),
        ))),
      }
    },
  };
  const DIV: Intrinsic =
    Intrinsic {
      name: "/",
      params: &[
        Param::EvalTo(ExprType::Numeric),
        Param::EvalTo(ExprType::Numeric),
      ],
      handler: |runtime, args| {
        let lhs = args[0].kind.clone();
        let rhs = args[1].kind.clone();
        let (lhs, rhs) = lhs.coerce_numeric(rhs);

        match &rhs {
          ExprKind::Integer(0) => {
            return Err(runtime.error(ErrorReason::Message(
              "'/' division by zero".to_string(),
            )));
          }
          ExprKind::Float(f) if *f == 0.0 => {
            return Err(runtime.error(ErrorReason::Message(
              "'/' division by zero".to_string(),
            )));
          }
          _ => {}
        }

        match lhs / rhs {
          Ok(kind) => Ok(Expr {
            kind: kind.normalize_numeric(),
            span: None,
          }),
          Err(_) => Err(runtime.error(ErrorReason::Message(
            "'/' requires numeric arguments".to_string(),
          ))),
        }
      },
    };
  const MOD: Intrinsic = Intrinsic {
    name: "%",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: |runtime, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);

      match &rhs {
        ExprKind::Integer(0) => {
          return Err(
            runtime
              .error(ErrorReason::Message("'%' modulo by zero".to_string())),
          );
        }
        ExprKind::Float(f) if *f == 0.0 => {
          return Err(
            runtime
              .error(ErrorReason::Message("'%' modulo by zero".to_string())),
          );
        }
        _ => {}
      }

      match lhs % rhs {
        Ok(kind) => Ok(Expr {
          kind: kind.normalize_numeric(),
          span: None,
        }),
        Err(_) => Err(runtime.error(ErrorReason::Message(
          "'%' requires numeric arguments".to_string(),
        ))),
      }
    },
  };

  map.insert("+", ADD);
  map.insert("-", SUB);
  map.insert("*", MUL);
  map.insert("/", DIV);
  map.insert("%", MOD);
}

pub fn comparison(map: &mut HashMap<&'static str, Intrinsic>) {
  const EQ: Intrinsic = Intrinsic {
    name: "=",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: |_, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      Ok(Expr {
        kind: ExprKind::Boolean(lhs == rhs),
        span: None,
      })
    },
  };
  const NEQ: Intrinsic = Intrinsic {
    name: "!=",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: |_, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      Ok(Expr {
        kind: ExprKind::Boolean(lhs != rhs),
        span: None,
      })
    },
  };
  const LT: Intrinsic = Intrinsic {
    name: "<",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      match lhs.partial_cmp(&rhs) {
        Some(ord) => Ok(Expr {
          kind: ExprKind::Boolean(ord.is_lt()),
          span: None,
        }),
        None => Err(runtime.error(ErrorReason::Message(
          "'<' requires comparable arguments".to_string(),
        ))),
      }
    },
  };
  const LTE: Intrinsic = Intrinsic {
    name: "<=",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      match lhs.partial_cmp(&rhs) {
        Some(ord) => Ok(Expr {
          kind: ExprKind::Boolean(ord.is_le()),
          span: None,
        }),
        None => Err(runtime.error(ErrorReason::Message(
          "'<=' requires comparable arguments".to_string(),
        ))),
      }
    },
  };
  const GT: Intrinsic = Intrinsic {
    name: ">",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      match lhs.partial_cmp(&rhs) {
        Some(ord) => Ok(Expr {
          kind: ExprKind::Boolean(ord.is_gt()),
          span: None,
        }),
        None => Err(runtime.error(ErrorReason::Message(
          "'>' requires comparable arguments".to_string(),
        ))),
      }
    },
  };
  const GTE: Intrinsic = Intrinsic {
    name: ">=",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      let lhs = args[0].kind.clone();
      let rhs = args[1].kind.clone();
      let (lhs, rhs) = lhs.coerce_numeric(rhs);
      match lhs.partial_cmp(&rhs) {
        Some(ord) => Ok(Expr {
          kind: ExprKind::Boolean(ord.is_ge()),
          span: None,
        }),
        None => Err(runtime.error(ErrorReason::Message(
          "'>=' requires comparable arguments".to_string(),
        ))),
      }
    },
  };

  map.insert("=", EQ);
  map.insert("!=", NEQ);
  map.insert("<", LT);
  map.insert("<=", LTE);
  map.insert(">", GT);
  map.insert(">=", GTE);
}

pub fn boolean(map: &mut HashMap<&'static str, Intrinsic>) {
  const NOT: Intrinsic = Intrinsic {
    name: "not",
    params: &[Param::EvalTo(ExprType::Boolean)],
    handler: |runtime, args| {
      let val = args[0].kind.clone();
      let ExprKind::Boolean(b) = val else {
        return Err(runtime.error(ErrorReason::Message(
          "'not' requires a boolean argument".to_string(),
        )));
      };
      Ok(Expr {
        kind: ExprKind::Boolean(!b),
        span: None,
      })
    },
  };

  map.insert("not", NOT);
}

pub fn list_ops(map: &mut HashMap<&'static str, Intrinsic>) {
  const LIST: Intrinsic = Intrinsic {
    name: "list",
    params: &[Param::Many(ExprType::Any)],
    handler: |runtime, args| {
      let evaluated = args
        .iter()
        .map(|expr| runtime.eval_expr(expr))
        .collect::<Result<Vec<_>, _>>()?;
      Ok(Expr {
        kind: ExprKind::List(Arc::new(evaluated)),
        span: None,
      })
    },
  };
  const NTH: Intrinsic = Intrinsic {
    name: "nth",
    params: &[
      Param::EvalTo(ExprType::Integer),
      Param::EvalTo(ExprType::Any),
    ],
    handler: |runtime, args| {
      let idx_val = args[0].clone();
      let ExprKind::Integer(idx) = idx_val.kind else {
        return Err(runtime.error(ErrorReason::CallError(CallError {
          symbol: "nth".to_owned(),
          kind: CallErrorKind::TypeMismatch {
            expected: vec!["integer".to_owned()],
            received: vec![idx_val.kind.type_name().to_owned()],
          },
        })));
      };
      if idx < 0 {
        return Err(runtime.error(ErrorReason::Message(
          "'nth' index must be non-negative".to_string(),
        )));
      }
      let idx = idx as usize;

      let col_val = args[1].clone();
      let col_type = col_val.kind.type_name().to_owned();
      match col_val.kind {
        ExprKind::List(items) => items.get(idx).cloned().ok_or_else(|| {
          runtime.error(ErrorReason::Message(format!(
            "'nth' index {} out of bounds",
            idx
          )))
        }),
        ExprKind::String(s) => s
          .chars()
          .nth(idx)
          .map(|ch| Expr {
            kind: ExprKind::String(ch.to_string()),
            span: None,
          })
          .ok_or_else(|| {
            runtime.error(ErrorReason::Message(format!(
              "'nth' index {} out of bounds",
              idx
            )))
          }),
        _ => Err(runtime.error(ErrorReason::CallError(CallError {
          symbol: "nth".to_owned(),
          kind: CallErrorKind::TypeMismatch {
            expected: vec!["list".to_owned(), "string".to_owned()],
            received: vec![col_type],
          },
        }))),
      }
    },
  };
  const SET_NTH: Intrinsic = Intrinsic {
    name: "set-nth",
    params: &[
      Param::EvalTo(ExprType::Integer),
      Param::EvalTo(ExprType::List),
      Param::EvalTo(ExprType::Any),
    ],
    handler: |runtime, args| {
      let idx_val = args[0].clone();
      let ExprKind::Integer(idx) = idx_val.kind else {
        return Err(runtime.error(ErrorReason::CallError(CallError {
          symbol: "set-nth".to_owned(),
          kind: CallErrorKind::TypeMismatch {
            expected: vec!["integer".to_owned()],
            received: vec![idx_val.kind.type_name().to_owned()],
          },
        })));
      };
      if idx < 0 {
        return Err(runtime.error(ErrorReason::Message(
          "'set-nth' index must be non-negative".to_string(),
        )));
      }
      let idx = idx as usize;

      let list_val = args[1].clone();
      let new_val = args[2].clone();
      let list_type = list_val.kind.type_name().to_owned();

      let ExprKind::List(items) = list_val.kind else {
        return Err(runtime.error(ErrorReason::CallError(CallError {
          symbol: "set-nth".to_owned(),
          kind: CallErrorKind::TypeMismatch {
            expected: vec!["list".to_owned()],
            received: vec![list_type],
          },
        })));
      };

      if idx >= items.len() {
        return Err(runtime.error(ErrorReason::Message(format!(
          "'set-nth' index {} out of bounds",
          idx
        ))));
      }

      let mut new_items = (*items).clone();
      new_items[idx] = new_val;
      Ok(Expr {
        kind: ExprKind::List(Arc::new(new_items)),
        span: None,
      })
    },
  };
  const PUSH: Intrinsic = Intrinsic {
    name: "push",
    params: &[Param::EvalTo(ExprType::List), Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      let list_val = args[0].clone();
      let new_val = args[1].clone();
      let list_type = list_val.kind.type_name().to_owned();

      let ExprKind::List(items) = list_val.kind else {
        return Err(runtime.error(ErrorReason::CallError(CallError {
          symbol: "push".to_owned(),
          kind: CallErrorKind::TypeMismatch {
            expected: vec!["list".to_owned()],
            received: vec![list_type],
          },
        })));
      };

      let mut new_items = (*items).clone();
      new_items.push(new_val);
      Ok(Expr {
        kind: ExprKind::List(Arc::new(new_items)),
        span: None,
      })
    },
  };
  const POP: Intrinsic = Intrinsic {
    name: "pop",
    params: &[Param::EvalTo(ExprType::List)],
    handler: |runtime, args| {
      let list_val = args[0].clone();
      let list_type = list_val.kind.type_name().to_owned();

      let ExprKind::List(items) = list_val.kind else {
        return Err(runtime.error(ErrorReason::CallError(CallError {
          symbol: "pop".to_owned(),
          kind: CallErrorKind::TypeMismatch {
            expected: vec!["list".to_owned()],
            received: vec![list_type],
          },
        })));
      };

      if items.is_empty() {
        return Err(runtime.error(ErrorReason::Message(
          "'pop' requires a non-empty list".to_string(),
        )));
      }

      let new_items = items[..items.len() - 1].to_vec();
      Ok(Expr {
        kind: ExprKind::List(Arc::new(new_items)),
        span: None,
      })
    },
  };

  map.insert("list", LIST);
  map.insert("nth", NTH);
  map.insert("set-nth", SET_NTH);
  map.insert("push", PUSH);
  map.insert("pop", POP);
}

pub fn type_ops(map: &mut HashMap<&'static str, Intrinsic>) {
  const LEN: Intrinsic = Intrinsic {
    name: "len",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      let val = args[0].kind.clone();
      match val {
        ExprKind::String(s) => Ok(Expr {
          kind: ExprKind::Integer(s.len() as i64),
          span: None,
        }),
        ExprKind::List(list) => Ok(Expr {
          kind: ExprKind::Integer(list.len() as i64),
          span: None,
        }),
        _ => Err(runtime.error(ErrorReason::Message(
          "'len' requires one string or list argument".to_string(),
        ))),
      }
    },
  };
  const TYPEOF: Intrinsic = Intrinsic {
    name: "typeof",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: |_, args| {
      let val = args[0].kind.clone();
      Ok(Expr {
        kind: ExprKind::String(val.type_name().to_string()),
        span: None,
      })
    },
  };

  map.insert("len", LEN);
  map.insert("typeof", TYPEOF);
}

pub fn io_ops(map: &mut HashMap<&'static str, Intrinsic>) {
  const PRINT: Intrinsic = Intrinsic {
    name: "print",
    params: &[Param::Many(ExprType::Any)],
    handler: |runtime, args| {
      let parts = args
        .iter()
        .map(|expr| runtime.eval_expr(expr).map(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
      println!("{}", parts.join(" "));
      Ok(Expr {
        kind: ExprKind::Nil,
        span: None,
      })
    },
  };
  const DBG: Intrinsic = Intrinsic {
    name: "dbg",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: |_, args| {
      let val = args[0].clone();
      println!("{:?}", val);
      Ok(val)
    },
  };
  const TO_STRING: Intrinsic = Intrinsic {
    name: "to-string",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: |_, args| {
      Ok(Expr {
        kind: ExprKind::String(args[0].to_string()),
        span: None,
      })
    },
  };

  map.insert("print", PRINT);
  map.insert("dbg", DBG);
  map.insert("to-string", TO_STRING);
}

pub fn meta_ops(map: &mut HashMap<&'static str, Intrinsic>) {
  const LAZY: Intrinsic = Intrinsic {
    name: "lazy",
    params: &[Param::One(ExprType::Any)],
    handler: |_, args| Ok(args[0].clone()),
  };
  const EVAL: Intrinsic = Intrinsic {
    name: "eval",
    params: &[Param::One(ExprType::Any)],
    handler: |runtime, args| {
      let result = args[0].clone();
      runtime.eval_expr(&result)
    },
  };
  const CALL: Intrinsic = Intrinsic {
    name: "call",
    params: &[Param::One(ExprType::Any), Param::Many(ExprType::Any)],
    handler: |runtime, args| {
      // First arg is unevaluated - evaluate it to get the function
      let expr = runtime.eval_expr(&args[0])?;
      if let ExprKind::Function { .. } = &expr.kind {
        // It's a function - call it with the rest of the arguments
        runtime.call(
          &expr,
          args.get(1..).unwrap_or_default().to_vec(),
          "(call ...)",
        )
      // TODO(thedevbird): This is a cool behavior, currying exists through it:
      //                   (call (+) 1 2) ;; -> 3
      //                   but it makes call do more than one or two things.
      //                   This should be implemented by separating lists and
      //                   forms where calling is just (<form/symbol/function>).
      // } else if let ExprKind::List(list) = &fn_expr.kind {
      //   // It's a list - try to evaluate it as a function call
      //   // Add the rest of the arguments and evaluate
      //   let mut new_list = (**list).clone();
      //   for arg in args.get(1..).unwrap_or_default() {
      //     new_list.push(arg.clone());
      //   }
      //   runtime.eval_expr(&Expr {
      //     kind: ExprKind::List(Arc::new(new_list)),
      //     span: None,
      //   })
      } else {
        let expr = runtime.eval_expr(&expr)?;
        Ok(expr)
      }
    },
  };
  const DO: Intrinsic = Intrinsic {
    name: "do",
    params: &[Param::Many(ExprType::Any)],
    handler: |runtime, args| {
      let result = args.iter().try_fold(
        Expr {
          kind: ExprKind::Nil,
          span: None,
        },
        |_, expr| runtime.eval_expr(expr),
      )?;
      Ok(result)
    },
  };
  const PARSE: Intrinsic = Intrinsic {
    name: "parse",
    params: &[Param::EvalTo(ExprType::String)],
    handler: |runtime, args| {
      let ExprKind::String(ref str) = args.first().unwrap().kind else {
        unreachable!("type-checker validates types");
      };
      let tokens = lex(str);
      let exprs = parse(str, tokens).map_err(|err| {
        runtime.error(ErrorReason::Message(format!("parse error: {err}")))
      })?;
      Ok(Expr {
        kind: ExprKind::List(Rc::new(exprs)),
        span: None,
      })
    },
  };

  map.insert("lazy", LAZY);
  map.insert("eval", EVAL);
  map.insert("call", CALL);
  map.insert("do", DO);
  map.insert("parse", PARSE);
}

pub fn error_ops(map: &mut HashMap<&'static str, Intrinsic>) {
  const TRY: Intrinsic = Intrinsic {
    name: "try",
    params: &[Param::One(ExprType::Any)],
    handler: |runtime, args| match runtime.eval_expr(&args[0]) {
      Ok(result) => Ok(result),
      Err(err) => Ok(Expr {
        kind: ExprKind::Error(err.to_string().into()),
        span: None,
      }),
    },
  };
  const ERROR: Intrinsic = Intrinsic {
    name: "error",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: |_, args| {
      let inner = args[0].clone();
      Ok(Expr {
        kind: ExprKind::Error(Arc::from(inner.to_string())),
        span: None,
      })
    },
  };
  const THROW: Intrinsic = Intrinsic {
    name: "throw",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      let inner = args[0].clone();
      if let ExprKind::Error(ref err) = inner.kind {
        Err(runtime.error(ErrorReason::Message(err.to_string())))
      } else {
        Err(runtime.error(ErrorReason::CallError(CallError {
          symbol: "throw".to_owned(),
          kind: CallErrorKind::TypeMismatch {
            expected: vec!["error".to_owned()],
            received: vec![inner.kind.type_name().to_owned()],
          },
        })))
      }
    },
  };

  map.insert("try", TRY);
  map.insert("error", ERROR);
  map.insert("throw", THROW);
}

pub fn all() -> HashMap<&'static str, Intrinsic> {
  let mut map = HashMap::new();
  arithmetic(&mut map);
  comparison(&mut map);
  boolean(&mut map);
  list_ops(&mut map);
  type_ops(&mut map);
  io_ops(&mut map);
  meta_ops(&mut map);
  error_ops(&mut map);
  map
}
