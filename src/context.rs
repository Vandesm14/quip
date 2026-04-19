use std::{borrow::Cow, collections::HashMap};

use crate::ast::Expr;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scope<'a> {
  pub vars: HashMap<Cow<'a, str>, Expr<'a>>,
  pub parent: Option<usize>,
}

impl<'a> Scope<'a> {
  pub fn new(parent: Option<usize>) -> Self {
    Self {
      vars: HashMap::new(),
      parent,
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Context<'a> {
  envs: Vec<Scope<'a>>,
  current: usize,
}

impl<'a> Default for Context<'a> {
  fn default() -> Self {
    Self {
      envs: vec![Scope::new(None)],
      current: 0,
    }
  }
}

impl<'a> Context<'a> {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn current(&self) -> usize {
    self.current
  }

  pub fn get(&self, name: &str) -> Option<&Expr<'a>> {
    let mut idx = self.current;
    loop {
      if let Some(val) = self.envs[idx].vars.get(name) {
        return Some(val);
      }
      match self.envs[idx].parent {
        Some(p) => idx = p,
        None => return None,
      }
    }
  }

  pub fn define(&mut self, name: Cow<'a, str>, val: Expr<'a>) {
    self.envs[self.current].vars.insert(name, val);
  }

  pub fn set(
    &mut self,
    name: Cow<'a, str>,
    val: Expr<'a>,
  ) -> Result<(), String> {
    let mut idx = self.current;
    loop {
      #[allow(clippy::map_entry)]
      if self.envs[idx].vars.contains_key(&name) {
        self.envs[idx].vars.insert(name, val);
        return Ok(());
      }
      match self.envs[idx].parent {
        Some(p) => idx = p,
        None => {
          return Err(format!("cannot set '{}' before it is defined", name));
        }
      }
    }
  }

  pub fn push_scope(&mut self, parent_env: usize) -> usize {
    let saved = self.current;
    self.envs.push(Scope::new(Some(parent_env)));
    self.current = self.envs.len() - 1;
    saved
  }

  pub fn restore_scope(&mut self, saved: usize) {
    self.current = saved;
  }
}
