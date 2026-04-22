use std::{
  collections::{HashMap, HashSet},
  panic,
  rc::Rc,
};

use slotmap::{DefaultKey, SlotMap};

use crate::ast::{Expr, ExprKind};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scope {
  pub vars: HashMap<Rc<str>, Expr>,
  pub parent: Option<DefaultKey>,
}

impl Scope {
  pub fn new(parent: Option<DefaultKey>) -> Self {
    Self {
      vars: HashMap::new(),
      parent,
    }
  }
}

#[derive(Debug, Clone)]
pub struct Context {
  envs: SlotMap<DefaultKey, Scope>,
  current: DefaultKey,
  gc_threshold: usize,
  root_id: DefaultKey,
}

impl Default for Context {
  fn default() -> Self {
    let mut envs = SlotMap::new();
    let current = envs.insert(Scope::new(None));
    Self {
      envs,
      current,
      root_id: current,
      // TODO: use a proper value from experimentation, instead of 500 (a guess).
      gc_threshold: 500,
    }
  }
}

impl Context {
  pub fn new(gc_threshold: usize) -> Self {
    Self {
      gc_threshold,
      ..Default::default()
    }
  }

  pub fn current(&self) -> DefaultKey {
    self.current
  }

  pub fn get(&self, name: &str) -> Option<&Expr> {
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

  pub fn define(&mut self, name: Rc<str>, val: Expr) {
    self.envs[self.current].vars.insert(name, val);
  }

  pub fn set(&mut self, name: Rc<str>, val: Expr) -> Result<(), String> {
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

  /// Walks the parents of a scope and returns them in ascending order [self, 1st order parent, 2nd order parent, ...].
  fn parents(&self, key: DefaultKey) -> Option<Vec<(DefaultKey, Scope)>> {
    let mut result = Vec::new();
    let mut idx = key;
    loop {
      let scope = self.envs.get(idx)?;
      result.push((idx, scope.clone()));
      match scope.parent {
        Some(parent) => idx = parent,
        None => return Some(result),
      }
    }
  }

  pub fn trigger_gc(&mut self) {
    let current_keys = self
      .parents(self.current())
      .map(|keys| keys.iter().map(|(k, _)| *k).collect::<Vec<_>>());
    if let Some(current_keys) = current_keys {
      let mut to_remove: HashSet<DefaultKey> = HashSet::from_iter(
        // Protect the root and current scopes.
        self
          .envs
          .keys()
          .filter(|k| *k != self.root_id && !current_keys.contains(k)),
      );
      for env in self.envs.values() {
        for var in env.vars.values() {
          // If a scope is referenced in a function, keep it.
          if let ExprKind::Function { env, .. } = var.kind {
            if let Some(parents) = self.parents(env) {
              for (key, _) in parents {
                to_remove.remove(&key);
              }
            } else {
              panic!("failed to find parent scope");
            }
          }
        }
      }

      for key in to_remove.into_iter() {
        self.envs.remove(key);
      }
    } else {
      panic!("no current scope found");
    }
  }

  pub fn should_gc(&self) -> bool {
    self.envs.len() >= self.gc_threshold
  }

  pub fn envs_len(&self) -> usize {
    self.envs.len()
  }

  pub fn do_gc_if_over(&mut self) {
    if self.should_gc() {
      self.trigger_gc();
    }
  }
}
