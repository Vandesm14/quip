use std::collections::HashMap;
use std::rc::Rc;

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
  /// symbol
  Symbol,
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
      ExprType::Symbol => write!(f, "symbol"),
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
      ExprType::Symbol => matches!(expr.kind, ExprKind::Symbol(_)),
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
  const AND: Intrinsic = Intrinsic {
    params: &[Param::One(ExprType::Any), Param::One(ExprType::Any)],
    handler: |runtime, args| {
      let [lhs, rhs] = args.get(0..2).unwrap() else {
        unreachable!("handled by type-checker")
      };
      let lhs = runtime.eval_expr(lhs)?.kind;
      let ExprKind::Boolean(lhs) = lhs else {
        return Err(runtime.error(ErrorReason::Message(
          "'and' requires boolean arguments".to_string(),
        )));
      };
      if !lhs {
        return Ok(Expr {
          kind: ExprKind::Boolean(false),
          span: None,
        });
      }
      let rhs = runtime.eval_expr(rhs)?.kind;
      let ExprKind::Boolean(rhs) = rhs else {
        return Err(runtime.error(ErrorReason::Message(
          "'and' requires boolean arguments".to_string(),
        )));
      };
      Ok(Expr {
        kind: ExprKind::Boolean(rhs),
        span: None,
      })
    },
  };
  const OR: Intrinsic = Intrinsic {
    params: &[Param::One(ExprType::Any), Param::One(ExprType::Any)],
    handler: |runtime, args| {
      let [lhs, rhs] = args.get(0..2).unwrap() else {
        unreachable!("handled by type-checker")
      };
      let lhs = runtime.eval_expr(lhs)?.kind;
      let ExprKind::Boolean(lhs) = lhs else {
        return Err(runtime.error(ErrorReason::Message(
          "'or' requires boolean arguments".to_string(),
        )));
      };
      if lhs {
        return Ok(Expr {
          kind: ExprKind::Boolean(true),
          span: None,
        });
      }
      let rhs = runtime.eval_expr(rhs)?.kind;
      let ExprKind::Boolean(rhs) = rhs else {
        return Err(runtime.error(ErrorReason::Message(
          "'or' requires boolean arguments".to_string(),
        )));
      };
      Ok(Expr {
        kind: ExprKind::Boolean(rhs),
        span: None,
      })
    },
  };
  const NOT: Intrinsic = Intrinsic {
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

  map.insert("and", AND);
  map.insert("or", OR);
  map.insert("not", NOT);
}

pub fn list_ops(map: &mut HashMap<&'static str, Intrinsic>) {
  const LIST: Intrinsic = Intrinsic {
    params: &[Param::Many(ExprType::Any)],
    handler: |runtime, args| {
      let evaluated = args
        .iter()
        .map(|expr| runtime.eval_expr(expr))
        .collect::<Result<Vec<_>, _>>()?;
      Ok(Expr {
        kind: ExprKind::List(Rc::new(evaluated)),
        span: None,
      })
    },
  };
  const NTH: Intrinsic = Intrinsic {
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
        kind: ExprKind::List(Rc::new(new_items)),
        span: None,
      })
    },
  };
  const PUSH: Intrinsic = Intrinsic {
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
        kind: ExprKind::List(Rc::new(new_items)),
        span: None,
      })
    },
  };
  const POP: Intrinsic = Intrinsic {
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
        kind: ExprKind::List(Rc::new(new_items)),
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
    params: &[Param::EvalTo(ExprType::Any)],
    handler: |_, args| {
      let val = args[0].clone();
      println!("{:?}", val);
      Ok(val)
    },
  };
  const TO_STRING: Intrinsic = Intrinsic {
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
  const FN: Intrinsic = Intrinsic {
    params: &[Param::One(ExprType::List), Param::Many(ExprType::Any)],
    handler: |runtime, args| {
      let Some(params_expr) = args.first() else {
        return Err(runtime.error(ErrorReason::Message(
          "fn: expected params list".to_string(),
        )));
      };
      let ExprKind::List(param_list) = &params_expr.kind else {
        return Err(runtime.error(ErrorReason::Message(
          "fn: expected params list".to_string(),
        )));
      };
      let params = Runtime::parse_params(param_list, "fn")
        .map_err(|e| runtime.error(e.into()))?;
      let body = args.get(1..).unwrap_or_default().to_vec();
      let env = runtime.context.current();
      Ok(Expr {
        kind: ExprKind::Function { params, body, env },
        // TODO: give it the span of the expr. handlers need access to the expr.
        span: None,
      })
    },
  };
  const DEFN: Intrinsic = Intrinsic {
    params: &[
      Param::One(ExprType::Symbol),
      Param::One(ExprType::List),
      Param::Many(ExprType::Any),
    ],
    handler: |runtime, args| {
      // (defn name [params...] body...)  →  (def name (fn [params...] body...))
      let Some([name, params_expr]) = args.get(0..2) else {
        return Err(runtime.error(ErrorReason::Message(
          "defn: expected name and params".to_string(),
        )));
      };
      let ExprKind::Symbol(sym) = &name.kind else {
        return Err(
          runtime.error(ErrorReason::Message("defn: invalid name".to_string())),
        );
      };
      // TODO: reimplement by passing intrinsics key into the handler.
      // if intrinsics.contains_key(sym.as_ref()) {
      //   return Err(runtime.error(ErrorReason::Message(format!(
      //     "'{sym}' is an intrinsic and cannot be redefined"
      //   ))));
      // }
      let ExprKind::List(param_list) = &params_expr.kind else {
        return Err(runtime.error(ErrorReason::Message(
          "defn: expected params list".to_string(),
        )));
      };
      let params = Runtime::parse_params(param_list, "defn")
        .map_err(|e| runtime.error(e.into()))?;
      let body = args.get(2..).unwrap_or(&[]).to_vec();
      let env = runtime.context.current();
      let func = Expr {
        kind: ExprKind::Function { params, body, env },
        // TODO: give it the span of the expr. handlers need access to the expr.
        span: None,
      };
      runtime.context.define(sym.clone(), func);
      Ok(name.clone())
    },
  };
  const DEF: Intrinsic = Intrinsic {
    params: &[Param::One(ExprType::Symbol), Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      if let Some([name, val]) = args.get(0..2) {
        let ExprKind::Symbol(sym) = &name.kind else {
          return Err(
            runtime
              .error(ErrorReason::Message("def: invalid name".to_string())),
          );
        };
        // TODO: reimplement by passing intrinsics key into the handler.
        // if intrinsics.contains_key(sym.as_ref()) {
        //   return Err(runtime.error(ErrorReason::Message(format!(
        //     "'{sym}' is an intrinsic and cannot be redefined"
        //   ))));
        // }
        runtime.context.define(sym.clone(), val.clone());
        Ok(name.clone())
      } else {
        Err(runtime.error(ErrorReason::Message("invalid def".to_string())))
      }
    },
  };
  const SET: Intrinsic = Intrinsic {
    params: &[Param::One(ExprType::Symbol), Param::EvalTo(ExprType::Any)],
    handler: |runtime, args| {
      let [
        Expr {
          kind: ExprKind::Symbol(name),
          ..
        },
        val,
      ] = args.get(0..2).unwrap()
      else {
        unreachable!("handled by type-checker");
      };
      runtime
        .context
        .set(name.clone(), val.clone())
        .map_err(ErrorReason::Message)
        .map_err(|e| runtime.error(e))?;
      Ok(val.clone())
    },
  };
  const LAZY: Intrinsic = Intrinsic {
    params: &[Param::One(ExprType::Any)],
    handler: |_, args| Ok(args[0].clone()),
  };
  const EVAL: Intrinsic = Intrinsic {
    params: &[Param::One(ExprType::Any)],
    handler: |runtime, args| {
      let result = args[0].clone();
      runtime.eval_expr(&result)
    },
  };
  const CALL: Intrinsic = Intrinsic {
    params: &[Param::One(ExprType::Any), Param::Many(ExprType::Any)],
    handler: |runtime, args| {
      let expr = runtime.eval_expr(&args[0])?;
      if let ExprKind::Function { .. } = &expr.kind {
        runtime.call(
          &expr,
          args.get(1..).unwrap_or_default().to_vec(),
          "(call ...)",
        )
      } else {
        let expr = runtime.eval_expr(&expr)?;
        Ok(expr)
      }
    },
  };
  const DO: Intrinsic = Intrinsic {
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
  const RECUR: Intrinsic = Intrinsic {
    params: &[Param::Many(ExprType::Any)],
    handler: |runtime, args| {
      runtime.recur = Some(args.to_vec());
      Ok(Expr {
        kind: ExprKind::Nil,
        span: None,
      })
    },
  };
  const IF: Intrinsic = Intrinsic {
    params: &[Param::EvalTo(ExprType::Boolean), Param::One(ExprType::Any)],
    handler: |runtime, args| {
      let [cond, body_expr] = args.get(0..2).unwrap() else {
        unreachable!("handled by type-checker");
      };
      if let ExprKind::Boolean(true) = cond.kind {
        let body = runtime.eval_expr(body_expr)?;
        if let ExprKind::Function { .. } = body.kind {
          runtime.call(
            &body,
            Vec::new(),
            &format!("(if {} {})", cond, body_expr),
          )
        } else {
          Ok(body)
        }
      } else {
        Ok(Expr {
          kind: ExprKind::Nil,
          span: None,
        })
      }
    },
  };

  map.insert("fn", FN);
  map.insert("defn", DEFN);
  map.insert("def", DEF);
  map.insert("set", SET);
  map.insert("lazy", LAZY);
  map.insert("eval", EVAL);
  map.insert("call", CALL);
  map.insert("do", DO);
  map.insert("parse", PARSE);
  map.insert("recur", RECUR);
  map.insert("if", IF);
}

pub fn error_ops(map: &mut HashMap<&'static str, Intrinsic>) {
  const TRY: Intrinsic = Intrinsic {
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
    params: &[Param::EvalTo(ExprType::Any)],
    handler: |_, args| {
      let inner = args[0].clone();
      Ok(Expr {
        kind: ExprKind::Error(Rc::from(inner.to_string())),
        span: None,
      })
    },
  };
  const THROW: Intrinsic = Intrinsic {
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
