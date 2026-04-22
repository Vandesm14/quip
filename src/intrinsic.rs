use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::Expr;
use crate::run::{CallError, CallErrorKind, Error, Runtime};

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
  pub handler:
    for<'a> fn(&mut Runtime<'a>, Vec<Expr<'a>>) -> Result<Expr<'a>, Error>,
}

impl Intrinsic {
  pub fn check_params<'a>(
    &self,
    runtime: &mut Runtime<'a>,
    list: &[Expr<'a>],
    symbol: &str,
  ) -> Result<Vec<Expr<'a>>, Error> {
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
            return Err(Error::CallError(CallError {
              symbol: symbol.to_owned(),
              kind: CallErrorKind::IncorrectArity {
                expected: self.params.len(),
                received: args.len(),
              },
            }));
          }
          self.check_type(&args[i], *kind, symbol)?;
          validated.push(args[i].clone());
        }
        Param::EvalTo(kind) => {
          if i >= args.len() {
            return Err(Error::CallError(CallError {
              symbol: symbol.to_owned(),
              kind: CallErrorKind::IncorrectArity {
                expected: self.params.len(),
                received: args.len(),
              },
            }));
          }
          let evaluated = runtime.eval_expr(&args[i])?;
          self.check_type(&evaluated, *kind, symbol)?;
          validated.push(evaluated);
        }
        Param::Many(kind) => {
          if i != self.params.len() - 1 {
            return Err(Error::Message(format!(
              "'{}': Many must be the last parameter",
              symbol
            )));
          }
          for (_j, arg) in args.iter().enumerate().skip(i) {
            self.check_type(arg, *kind, symbol)?;
            validated.push(arg.clone());
          }
          break;
        }
        Param::ManyEvalTo(kind) => {
          if i != self.params.len() - 1 {
            return Err(Error::Message(format!(
              "'{}': ManyEvalTo must be the last parameter",
              symbol
            )));
          }
          for (_j, arg) in args.iter().enumerate().skip(i) {
            let evaluated = runtime.eval_expr(arg)?;
            self.check_type(&evaluated, *kind, symbol)?;
            validated.push(evaluated);
          }
          break;
        }
      }
    }

    // Check for too many arguments (when no Many param)
    if !has_many && args.len() > self.params.len() {
      return Err(Error::CallError(CallError {
        symbol: symbol.to_owned(),
        kind: CallErrorKind::IncorrectArity {
          expected: self.params.len(),
          received: args.len(),
        },
      }));
    }

    Ok(validated)
  }

  /// Validate argument type against ExprKindVariant
  fn check_type(
    &self,
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
      Err(Error::CallError(CallError {
        symbol: symbol.to_owned(),
        kind: CallErrorKind::TypeMismatch {
          expected: vec![variant.to_string()],
          received: vec![expr.kind.type_name().to_owned()],
        },
      }))
    } else {
      Ok(())
    }
  }
}

// Arithmetic
pub fn intrinsic_add<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  match lhs + rhs {
    Ok(kind) => Ok(Expr {
      kind: kind.normalize_numeric(),
    }),
    Err(_) => Err(Error::Message("'+' requires numeric arguments".to_string())),
  }
}

pub fn intrinsic_sub<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  match lhs - rhs {
    Ok(kind) => Ok(Expr {
      kind: kind.normalize_numeric(),
    }),
    Err(_) => Err(Error::Message("'-' requires numeric arguments".to_string())),
  }
}

pub fn intrinsic_mul<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  match lhs * rhs {
    Ok(kind) => Ok(Expr {
      kind: kind.normalize_numeric(),
    }),
    Err(_) => Err(Error::Message("'*' requires numeric arguments".to_string())),
  }
}

pub fn intrinsic_div<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);

  match &rhs {
    ExprKind::Integer(0) => {
      return Err(Error::Message("'/' division by zero".to_string()));
    }
    ExprKind::Float(f) if *f == 0.0 => {
      return Err(Error::Message("'/' division by zero".to_string()));
    }
    _ => {}
  }

  match lhs / rhs {
    Ok(kind) => Ok(Expr {
      kind: kind.normalize_numeric(),
    }),
    Err(_) => Err(Error::Message("'/' requires numeric arguments".to_string())),
  }
}

pub fn intrinsic_mod<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);

  match &rhs {
    ExprKind::Integer(0) => {
      return Err(Error::Message("'%' modulo by zero".to_string()));
    }
    ExprKind::Float(f) if *f == 0.0 => {
      return Err(Error::Message("'%' modulo by zero".to_string()));
    }
    _ => {}
  }

  match lhs % rhs {
    Ok(kind) => Ok(Expr {
      kind: kind.normalize_numeric(),
    }),
    Err(_) => Err(Error::Message("'%' requires numeric arguments".to_string())),
  }
}

// Comparison

pub fn intrinsic_eq<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  Ok(Expr {
    kind: ExprKind::Boolean(lhs == rhs),
  })
}

pub fn intrinsic_neq<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  Ok(Expr {
    kind: ExprKind::Boolean(lhs != rhs),
  })
}

pub fn intrinsic_lt<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  match lhs.partial_cmp(&rhs) {
    Some(ord) => Ok(Expr {
      kind: ExprKind::Boolean(ord.is_lt()),
    }),
    None => Err(Error::Message(
      "'<' requires comparable arguments".to_string(),
    )),
  }
}

pub fn intrinsic_lte<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  match lhs.partial_cmp(&rhs) {
    Some(ord) => Ok(Expr {
      kind: ExprKind::Boolean(ord.is_le()),
    }),
    None => Err(Error::Message(
      "'<=' requires comparable arguments".to_string(),
    )),
  }
}

pub fn intrinsic_gt<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  match lhs.partial_cmp(&rhs) {
    Some(ord) => Ok(Expr {
      kind: ExprKind::Boolean(ord.is_gt()),
    }),
    None => Err(Error::Message(
      "'>' requires comparable arguments".to_string(),
    )),
  }
}

pub fn intrinsic_gte<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let lhs = args[0].kind.clone();
  let rhs = args[1].kind.clone();
  let (lhs, rhs) = lhs.coerce_numeric(rhs);
  match lhs.partial_cmp(&rhs) {
    Some(ord) => Ok(Expr {
      kind: ExprKind::Boolean(ord.is_ge()),
    }),
    None => Err(Error::Message(
      "'>=' requires comparable arguments".to_string(),
    )),
  }
}

// Boolean

pub fn intrinsic_not<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let val = args[0].kind.clone();
  let ExprKind::Boolean(b) = val else {
    return Err(Error::Message(
      "'not' requires a boolean argument".to_string(),
    ));
  };
  Ok(Expr {
    kind: ExprKind::Boolean(!b),
  })
}

// List Operations

pub fn intrinsic_list<'a>(
  runtime: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let evaluated = args
    .iter()
    .map(|expr| runtime.eval_expr(expr))
    .collect::<Result<Vec<_>, _>>()?;
  Ok(Expr {
    kind: ExprKind::List(Arc::new(evaluated)),
  })
}

pub fn intrinsic_nth<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let idx_val = args[0].clone();
  let ExprKind::Integer(idx) = idx_val.kind else {
    return Err(Error::CallError(CallError {
      symbol: "nth".to_owned(),
      kind: CallErrorKind::TypeMismatch {
        expected: vec!["integer".to_owned()],
        received: vec![idx_val.kind.type_name().to_owned()],
      },
    }));
  };
  if idx < 0 {
    return Err(Error::Message(
      "'nth' index must be non-negative".to_string(),
    ));
  }
  let idx = idx as usize;

  let col_val = args[1].clone();
  let col_type = col_val.kind.type_name().to_owned();
  match col_val.kind {
    ExprKind::List(items) => items.get(idx).cloned().ok_or_else(|| {
      Error::Message(format!("'nth' index {} out of bounds", idx))
    }),
    ExprKind::String(s) => s
      .chars()
      .nth(idx)
      .map(|ch| Expr {
        kind: ExprKind::String(ch.to_string()),
      })
      .ok_or_else(|| {
        Error::Message(format!("'nth' index {} out of bounds", idx))
      }),
    _ => Err(Error::CallError(CallError {
      symbol: "nth".to_owned(),
      kind: CallErrorKind::TypeMismatch {
        expected: vec!["list".to_owned(), "string".to_owned()],
        received: vec![col_type],
      },
    })),
  }
}

pub fn intrinsic_set_nth<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let idx_val = args[0].clone();
  let ExprKind::Integer(idx) = idx_val.kind else {
    return Err(Error::CallError(CallError {
      symbol: "set-nth".to_owned(),
      kind: CallErrorKind::TypeMismatch {
        expected: vec!["integer".to_owned()],
        received: vec![idx_val.kind.type_name().to_owned()],
      },
    }));
  };
  if idx < 0 {
    return Err(Error::Message(
      "'set-nth' index must be non-negative".to_string(),
    ));
  }
  let idx = idx as usize;

  let list_val = args[1].clone();
  let new_val = args[2].clone();
  let list_type = list_val.kind.type_name().to_owned();

  let ExprKind::List(items) = list_val.kind else {
    return Err(Error::CallError(CallError {
      symbol: "set-nth".to_owned(),
      kind: CallErrorKind::TypeMismatch {
        expected: vec!["list".to_owned()],
        received: vec![list_type],
      },
    }));
  };

  if idx >= items.len() {
    return Err(Error::Message(format!(
      "'set-nth' index {} out of bounds",
      idx
    )));
  }

  let mut new_items = (*items).clone();
  new_items[idx] = new_val;
  Ok(Expr {
    kind: ExprKind::List(Arc::new(new_items)),
  })
}

pub fn intrinsic_push<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let list_val = args[0].clone();
  let new_val = args[1].clone();
  let list_type = list_val.kind.type_name().to_owned();

  let ExprKind::List(items) = list_val.kind else {
    return Err(Error::CallError(CallError {
      symbol: "push".to_owned(),
      kind: CallErrorKind::TypeMismatch {
        expected: vec!["list".to_owned()],
        received: vec![list_type],
      },
    }));
  };

  let mut new_items = (*items).clone();
  new_items.push(new_val);
  Ok(Expr {
    kind: ExprKind::List(Arc::new(new_items)),
  })
}

pub fn intrinsic_pop<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let list_val = args[0].clone();
  let list_type = list_val.kind.type_name().to_owned();

  let ExprKind::List(items) = list_val.kind else {
    return Err(Error::CallError(CallError {
      symbol: "pop".to_owned(),
      kind: CallErrorKind::TypeMismatch {
        expected: vec!["list".to_owned()],
        received: vec![list_type],
      },
    }));
  };

  if items.is_empty() {
    return Err(Error::Message(
      "'pop' requires a non-empty list".to_string(),
    ));
  }

  let new_items = items[..items.len() - 1].to_vec();
  Ok(Expr {
    kind: ExprKind::List(Arc::new(new_items)),
  })
}

// Type & I/O

pub fn intrinsic_len<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let val = args[0].kind.clone();
  match val {
    ExprKind::String(s) => Ok(Expr {
      kind: ExprKind::Integer(s.len() as i64),
    }),
    ExprKind::List(list) => Ok(Expr {
      kind: ExprKind::Integer(list.len() as i64),
    }),
    _ => Err(Error::Message(
      "'len' requires one string or list argument".to_string(),
    )),
  }
}

pub fn intrinsic_typeof<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let val = args[0].kind.clone();
  Ok(Expr {
    kind: ExprKind::String(val.type_name().to_string()),
  })
}

pub fn intrinsic_print<'a>(
  runtime: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let parts = args
    .iter()
    .map(|expr| runtime.eval_expr(expr).map(|e| e.to_string()))
    .collect::<Result<Vec<_>, _>>()?;
  println!("{}", parts.join(" "));
  Ok(Expr {
    kind: ExprKind::Nil,
  })
}

pub fn intrinsic_dbg<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  let val = args[0].clone();
  println!("{:?}", val);
  Ok(val)
}

pub fn intrinsic_to_string<'a>(
  _runtime: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  Ok(Expr {
    kind: ExprKind::String(args[0].to_string()),
  })
}

// Meta/Error

pub fn intrinsic_lazy<'a>(
  _runtime: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  Ok(args[0].clone())
}

pub fn intrinsic_eval<'a>(
  runtime: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  let result = args[0].clone();
  runtime.eval_expr(&result)
}

pub fn intrinsic_call<'a>(
  runtime: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  // First arg is unevaluated - evaluate it to get the function
  let fn_expr = runtime.eval_expr(&args[0])?;
  if let ExprKind::Function { params, body, env } = fn_expr.kind {
    // It's a function - call it with the rest of the arguments
    runtime.call(
      env,
      params,
      body,
      args.get(1..).unwrap_or_default().to_vec(),
      "(call ...)",
    )
  } else if let ExprKind::List(list) = &fn_expr.kind {
    // It's a list - try to evaluate it as a function call
    // Add the rest of the arguments and evaluate
    let mut new_list = (**list).clone();
    for arg in args.get(1..).unwrap_or_default() {
      new_list.push(arg.clone());
    }
    runtime.eval_expr(&Expr {
      kind: ExprKind::List(Arc::new(new_list)),
    })
  } else {
    Ok(fn_expr)
  }
}

pub fn intrinsic_try<'a>(
  runtime: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  match runtime.eval_expr(&args[0]) {
    Ok(result) => Ok(result),
    Err(err) => Ok(Expr {
      kind: ExprKind::Error(err.to_string().into()),
    }),
  }
}

pub fn intrinsic_error<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let inner = args[0].clone();
  Ok(Expr {
    kind: ExprKind::Error(Error::Message(inner.to_string())),
  })
}

pub fn intrinsic_throw<'a>(
  _: &mut Runtime<'a>,
  args: Vec<Expr<'a>>,
) -> Result<Expr<'a>, Error> {
  use crate::ast::ExprKind;

  let inner = args[0].clone();
  if let ExprKind::Error(ref err) = inner.kind {
    Err(err.clone())
  } else {
    Err(Error::CallError(CallError {
      symbol: "throw".to_owned(),
      kind: CallErrorKind::TypeMismatch {
        expected: vec!["error".to_owned()],
        received: vec![inner.kind.type_name().to_owned()],
      },
    }))
  }
}

pub fn arithmetic(map: &mut HashMap<&'static str, &'static Intrinsic>) {
  const ADD: Intrinsic = Intrinsic {
    name: "+",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: intrinsic_add,
  };
  const SUB: Intrinsic = Intrinsic {
    name: "-",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: intrinsic_sub,
  };
  const MUL: Intrinsic = Intrinsic {
    name: "*",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: intrinsic_mul,
  };
  const DIV: Intrinsic = Intrinsic {
    name: "/",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: intrinsic_div,
  };
  const MOD: Intrinsic = Intrinsic {
    name: "%",
    params: &[
      Param::EvalTo(ExprType::Numeric),
      Param::EvalTo(ExprType::Numeric),
    ],
    handler: intrinsic_mod,
  };

  map.insert("+", &ADD);
  map.insert("-", &SUB);
  map.insert("*", &MUL);
  map.insert("/", &DIV);
  map.insert("%", &MOD);
}

pub fn comparison(map: &mut HashMap<&'static str, &'static Intrinsic>) {
  const EQ: Intrinsic = Intrinsic {
    name: "=",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: intrinsic_eq,
  };
  const NEQ: Intrinsic = Intrinsic {
    name: "!=",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: intrinsic_neq,
  };
  const LT: Intrinsic = Intrinsic {
    name: "<",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: intrinsic_lt,
  };
  const LTE: Intrinsic = Intrinsic {
    name: "<=",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: intrinsic_lte,
  };
  const GT: Intrinsic = Intrinsic {
    name: ">",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: intrinsic_gt,
  };
  const GTE: Intrinsic = Intrinsic {
    name: ">=",
    params: &[Param::EvalTo(ExprType::Any), Param::EvalTo(ExprType::Any)],
    handler: intrinsic_gte,
  };

  map.insert("=", &EQ);
  map.insert("!=", &NEQ);
  map.insert("<", &LT);
  map.insert("<=", &LTE);
  map.insert(">", &GT);
  map.insert(">=", &GTE);
}

pub fn boolean(map: &mut HashMap<&'static str, &'static Intrinsic>) {
  const NOT: Intrinsic = Intrinsic {
    name: "not",
    params: &[Param::EvalTo(ExprType::Boolean)],
    handler: intrinsic_not,
  };

  map.insert("not", &NOT);
}

pub fn list_ops(map: &mut HashMap<&'static str, &'static Intrinsic>) {
  const LIST: Intrinsic = Intrinsic {
    name: "list",
    params: &[Param::Many(ExprType::Any)],
    handler: intrinsic_list,
  };
  const NTH: Intrinsic = Intrinsic {
    name: "nth",
    params: &[
      Param::EvalTo(ExprType::Integer),
      Param::EvalTo(ExprType::Any),
    ],
    handler: intrinsic_nth,
  };
  const SET_NTH: Intrinsic = Intrinsic {
    name: "set-nth",
    params: &[
      Param::EvalTo(ExprType::Integer),
      Param::EvalTo(ExprType::List),
      Param::EvalTo(ExprType::Any),
    ],
    handler: intrinsic_set_nth,
  };
  const PUSH: Intrinsic = Intrinsic {
    name: "push",
    params: &[Param::EvalTo(ExprType::List), Param::EvalTo(ExprType::Any)],
    handler: intrinsic_push,
  };
  const POP: Intrinsic = Intrinsic {
    name: "pop",
    params: &[Param::EvalTo(ExprType::List)],
    handler: intrinsic_pop,
  };

  map.insert("list", &LIST);
  map.insert("nth", &NTH);
  map.insert("set-nth", &SET_NTH);
  map.insert("push", &PUSH);
  map.insert("pop", &POP);
}

pub fn type_ops(map: &mut HashMap<&'static str, &'static Intrinsic>) {
  const LEN: Intrinsic = Intrinsic {
    name: "len",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: intrinsic_len,
  };
  const TYPEOF: Intrinsic = Intrinsic {
    name: "typeof",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: intrinsic_typeof,
  };

  map.insert("len", &LEN);
  map.insert("typeof", &TYPEOF);
}

pub fn io_ops(map: &mut HashMap<&'static str, &'static Intrinsic>) {
  const PRINT: Intrinsic = Intrinsic {
    name: "print",
    params: &[Param::Many(ExprType::Any)],
    handler: intrinsic_print,
  };
  const DBG: Intrinsic = Intrinsic {
    name: "dbg",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: intrinsic_dbg,
  };
  const TO_STRING: Intrinsic = Intrinsic {
    name: "to-string",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: intrinsic_to_string,
  };

  map.insert("print", &PRINT);
  map.insert("dbg", &DBG);
  map.insert("to-string", &TO_STRING);
}

pub fn meta_ops(map: &mut HashMap<&'static str, &'static Intrinsic>) {
  const LAZY: Intrinsic = Intrinsic {
    name: "lazy",
    params: &[Param::One(ExprType::Any)],
    handler: intrinsic_lazy,
  };
  const EVAL: Intrinsic = Intrinsic {
    name: "eval",
    params: &[Param::One(ExprType::Any)],
    handler: intrinsic_eval,
  };
  const CALL: Intrinsic = Intrinsic {
    name: "call",
    params: &[Param::One(ExprType::Any), Param::Many(ExprType::Any)],
    handler: intrinsic_call,
  };

  map.insert("lazy", &LAZY);
  map.insert("eval", &EVAL);
  map.insert("call", &CALL);
}

pub fn error_ops(map: &mut HashMap<&'static str, &'static Intrinsic>) {
  const TRY: Intrinsic = Intrinsic {
    name: "try",
    params: &[Param::One(ExprType::Any)],
    handler: intrinsic_try,
  };
  const ERROR: Intrinsic = Intrinsic {
    name: "error",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: intrinsic_error,
  };
  const THROW: Intrinsic = Intrinsic {
    name: "throw",
    params: &[Param::EvalTo(ExprType::Any)],
    handler: intrinsic_throw,
  };

  map.insert("try", &TRY);
  map.insert("error", &ERROR);
  map.insert("throw", &THROW);
}

pub fn all() -> HashMap<&'static str, &'static Intrinsic> {
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
