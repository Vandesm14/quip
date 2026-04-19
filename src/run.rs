use std::{borrow::Cow, collections::HashMap};

use itertools::Itertools;

use crate::ast::{Expr, ExprKind};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Context<'a> {
  pub vars: HashMap<Cow<'a, str>, Expr<'a>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Runtime<'a> {
  pub context: Context<'a>,
}

impl<'a> Runtime<'a> {
  pub fn eval_expr(&mut self, expr: &Expr<'a>) -> Result<Expr<'a>, String> {
    if let ExprKind::List(list) = &expr.kind
      && let Some(ExprKind::Symbol(sym)) = list.first().map(|e| &e.kind)
    {
      match sym.to_string().as_str() {
        "defn" => {
          // if let Some([name, args]) = list.get(1..3)
          //   && let Some(body) = list.get(3..)
          // {
          //   let ExprKind::Symbol(name) = &name.kind else {
          //     todo!("invalid name")
          //   };
          //   let ExprKind::List(args) = &args.kind else {
          //     todo!("invalid args");
          //   };
          //   let arg_symbols = args
          //     .iter()
          //     .filter_map(|a| {
          //       if let ExprKind::Symbol(sym) = &a.kind {
          //         Some(sym)
          //       } else {
          //         None
          //       }
          //     })
          //     .collect::<Vec<_>>();
          //   if arg_symbols.len() != args.len() {
          //     todo!("invalid arguments")
          //   }
          // }

          todo!("defn")
        }
        "+" => {
          if let Some([lhs, rhs]) = list.get(1..3) {
            let Ok(lhs) = self.eval_expr(lhs).map(|expr| expr.kind) else {
              todo!("bad lhs");
            };
            let Ok(rhs) = self.eval_expr(rhs).map(|expr| expr.kind) else {
              todo!("bad rhs");
            };

            if let ExprKind::Integer(lhs) = lhs
              && let ExprKind::Integer(rhs) = rhs
            {
              Ok(Expr {
                kind: ExprKind::Integer(lhs + rhs),
              })
            } else if let ExprKind::Float(lhs) = lhs
              && let ExprKind::Float(rhs) = rhs
            {
              Ok(Expr {
                kind: ExprKind::Float(lhs + rhs),
              })
            } else {
              todo!("mismatched types")
            }
          } else {
            todo!("invalid add")
          }
        }

        "print" => {
          if let Some(args) = list.get(1..) {
            println!(
              "{}",
              args
                .iter()
                .map(|expr| format!("{:?}", self.eval_expr(expr)))
                .join(" ")
            );
            Ok(Expr {
              kind: ExprKind::Nil,
            })
          } else {
            todo!("bad args")
          }
        }

        _ => Err("bad fn call".to_string()),
      }
    } else {
      Ok(expr.clone())
    }
  }
}
