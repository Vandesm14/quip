use itertools::Itertools;

use crate::{
  ast::{Expr, ExprKind},
  context::{Context, FnParam, Function},
};

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
          if let Some([name, args]) = list.get(1..3)
            && let Some(body) = list.get(3..)
          {
            let ExprKind::Symbol(name) = &name.kind else {
              todo!("invalid name")
            };
            let ExprKind::List(args) = &args.kind else {
              todo!("invalid args");
            };

            let mut params = Vec::new();
            for arg in args {
              let ExprKind::Symbol(sym) = &arg.kind else {
                return Err("invalid argument in defn".to_string());
              };
              params.push(FnParam { name: sym.clone() });
            }

            self.context.fns.insert(
              name.clone(),
              Function {
                params,
                body: body.to_vec(),
              },
            );

            Ok(Expr {
              kind: ExprKind::Nil,
            })
          } else {
            todo!("invalid defn")
          }
        }
        "def" => {
          if let Some([name, val]) = list.get(1..3) {
            let ExprKind::Symbol(name) = &name.kind else {
              todo!("invalid name")
            };
            let Ok(val) = self.eval_expr(val) else {
              todo!("bad eval")
            };
            let val = self.eval_expr(&val)?;

            self.context.define(name.clone(), val.clone());

            Ok(val)
          } else {
            todo!("invalid def")
          }
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

        name => {
          if let Some(func) = self.context.fns.get(sym).cloned() {
            let call_args = list.get(1..).unwrap_or(&[]);
            if call_args.len() != func.params.len() {
              return Err(format!(
                "'{}' expects {} arg(s), got {}",
                name,
                func.params.len(),
                call_args.len()
              ));
            }

            let mut bound = Vec::new();
            for (param, arg_expr) in func.params.iter().zip(call_args.iter()) {
              let val = self.eval_expr(arg_expr)?;
              bound.push((param.name.clone(), val));
            }

            let mut child = self.context.duplicate();
            for (param_name, val) in bound {
              child.define(param_name, val);
            }

            let mut child_rt = Runtime { context: child };
            let mut result = Expr {
              kind: ExprKind::Nil,
            };
            for body_expr in &func.body {
              result = child_rt.eval_expr(body_expr)?;
            }
            Ok(result)
          } else {
            Err("bad fn call".to_string())
          }
        }
      }
    } else if let ExprKind::Symbol(sym) = &expr.kind {
      if let Some(val) = self.context.get_val(sym.clone()) {
        Ok(val)
      } else {
        todo!("invalid symbol")
      }
    } else {
      Ok(expr.clone())
    }
  }
}
