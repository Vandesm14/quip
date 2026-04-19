use std::{
  borrow::Cow,
  collections::{HashMap, HashSet},
};

use slotmap::{DefaultKey, SlotMap};

use crate::ast::{Expr, ExprKind};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scope<'a> {
  pub vars: HashMap<Cow<'a, str>, Expr<'a>>,
  pub parent: Option<DefaultKey>,
}

impl<'a> Scope<'a> {
  pub fn new(parent: Option<DefaultKey>) -> Self {
    Self {
      vars: HashMap::new(),
      parent,
    }
  }
}

#[derive(Debug, Clone)]
pub struct Context<'a> {
  envs: SlotMap<DefaultKey, Scope<'a>>,
  current: DefaultKey,
}

impl<'a> Default for Context<'a> {
  fn default() -> Self {
    let mut envs = SlotMap::new();
    let current = envs.insert(Scope::new(None));
    Self { envs, current }
  }
}

impl<'a> Context<'a> {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn current(&self) -> DefaultKey {
    self.current
  }

  pub fn get(&self, name: &str) -> Option<&Expr<'a>> {
    let mut idx = self.current;
    loop {
      if let Some(val) = self.envs.get(idx).unwrap().vars.get(name) {
        return Some(val);
      }
      match self.envs.get(idx).unwrap().parent {
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

  pub fn push_scope(&mut self, parent_env: DefaultKey) -> DefaultKey {
    let saved = self.current;
    self.current = self.envs.insert(Scope::new(Some(parent_env)));
    saved
  }

  pub fn restore_scope(&mut self, saved: DefaultKey) {
    self.current = saved;
  }

  pub fn trigger_gc(&mut self) {
    let mut to_remove: HashSet<DefaultKey> = HashSet::from_iter(
      self.envs.keys().skip(1).filter(|e| *e != self.current()),
    );
    for env in self.envs.values() {
      for var in env.vars.values() {
        if let ExprKind::Function { env, .. } = var.kind {
          to_remove.remove(&env);
        }
      }
    }

    for key in to_remove.into_iter() {
      self.envs.remove(key);
    }
  }
}
