use std::borrow::Cow;

use crate::{
  ast::{Expr, ExprKind},
  context::{Context, Scope},
};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Runtime<'a> {
  pub context: Context<'a>,
}

impl<'a> Runtime<'a> {
  fn call(
    &mut self,
    fn_scope: Scope<'a>,
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

    self.context.push_scope(fn_scope.duplicate());
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
          self.context.pop_scope();
          return Err(e);
        }
      };
    }

    self.context.pop_scope();
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
          let scope = self.context.scope().clone();
          Ok(Expr {
            kind: ExprKind::Function {
              params,
              body,
              scope,
            },
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
          let scope = self.context.scope().clone();
          let func = Expr {
            kind: ExprKind::Function {
              params,
              body,
              scope,
            },
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
            match (lhs, rhs) {
              (ExprKind::Integer(l), ExprKind::Integer(r)) => Ok(Expr {
                kind: ExprKind::Integer(l + r),
              }),
              (ExprKind::Float(l), ExprKind::Float(r)) => Ok(Expr {
                kind: ExprKind::Float(l + r),
              }),
              _ => Err("'+' requires matching numeric types".to_string()),
            }
          } else {
            Err("'+' requires two arguments".to_string())
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
            .get_val(sym.clone())
            .ok_or_else(|| format!("undefined '{}'", sym))?;
          if let ExprKind::Function {
            params,
            body,
            scope,
          } = val.kind
          {
            let call_args = list.get(1..).unwrap_or(&[]);
            self.call(scope, params, body, call_args, sym.as_ref())
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
      if let ExprKind::Function {
        params,
        body,
        scope,
      } = callee.kind
      {
        let call_args = list.get(1..).unwrap_or(&[]);
        self.call(scope, params, body, call_args, "<anonymous>")
      } else {
        Ok(expr.clone())
      }
    } else if let ExprKind::Symbol(sym) = &expr.kind {
      // Get vars.
      self
        .context
        .get_val(sym.clone())
        .ok_or_else(|| format!("undefined '{}'", sym))
    } else {
      Ok(expr.clone())
    }
  }
}
