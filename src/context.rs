use core::cell::RefCell;
use std::{borrow::Cow, collections::HashMap, rc::Rc};

use crate::{ast::Expr, chain::Chain};

pub type Val<'a> = Rc<RefCell<Chain<Option<Expr<'a>>>>>;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scope<'a> {
  pub vars: HashMap<Cow<'a, str>, Val<'a>>,
}

impl<'a> Scope<'a> {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn define(&mut self, name: Cow<'a, str>, item: Expr<'a>) -> Val<'a> {
    if let Some(c) = self.vars.get(&name) {
      let mut chain = RefCell::borrow_mut(c);
      match chain.is_root() {
        true => chain.set(Some(item)),
        false => chain.unlink_with(Some(item)),
      }
      c.clone()
    } else {
      let val = Rc::new(RefCell::new(Chain::new(Some(item))));
      self.vars.insert(name, val.clone());
      val
    }
  }

  pub fn set(
    &mut self,
    name: Cow<'a, str>,
    item: Expr<'a>,
  ) -> Result<Val<'a>, String> {
    if let Some(c) = self.vars.get_mut(&name) {
      let mut chain = RefCell::borrow_mut(c);
      chain.set(Some(item));
      Ok(c.clone())
    } else {
      Err(format!("cannot set '{}' before it is defined", name))
    }
  }

  pub fn reserve(&mut self, name: Cow<'a, str>) {
    self
      .vars
      .entry(name)
      .or_insert_with(|| Rc::new(RefCell::new(Chain::new(None))));
  }

  pub fn has(&self, name: Cow<'a, str>) -> bool {
    self.vars.contains_key(&name)
  }

  pub fn get_val(&self, name: Cow<'a, str>) -> Option<Expr<'a>> {
    self.vars.get(&name).and_then(|item| item.borrow().val())
  }

  pub fn get_ref(&self, name: Cow<'a, str>) -> Option<&Val<'a>> {
    self.vars.get(&name)
  }

  pub fn remove(&mut self, name: Cow<'a, str>) {
    self.vars.remove(&name);
  }

  /// Merges another scope into this one, not overwriting existing variables.
  pub fn merge(&mut self, other: Scope<'a>) {
    for (name, item) in other.vars {
      if !self.has(name.clone())
        || (self.get_val(name.clone()).is_none()
          && item.borrow().val().is_some())
      {
        self.vars.insert(name, item);
      }
    }
  }

  /// Creates a new scope with all vars linked to self's (non-root), for function call isolation.
  pub fn duplicate(&self) -> Self {
    let mut vars = HashMap::new();
    for (name, item) in self.vars.iter() {
      let mut item = RefCell::borrow_mut(item);
      vars.insert(name.clone(), item.link());
    }
    Self { vars }
  }
}

/// A runtime context holding a stack of scopes.
#[derive(Debug, Clone, PartialEq)]
pub struct Context<'a> {
  scopes: Vec<Scope<'a>>,
}

impl<'a> Default for Context<'a> {
  fn default() -> Self {
    Self {
      scopes: vec![Scope::new()],
    }
  }
}

impl<'a> Context<'a> {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn scope(&self) -> &Scope<'a> {
    self.scopes.last().expect("scope stack is empty")
  }

  pub fn scope_mut(&mut self) -> &mut Scope<'a> {
    self.scopes.last_mut().expect("scope stack is empty")
  }

  pub fn push_scope(&mut self, scope: Scope<'a>) {
    self.scopes.push(scope);
  }

  pub fn pop_scope(&mut self) {
    if self.scopes.len() > 1 {
      self.scopes.pop();
    }
  }

  pub fn define(&mut self, name: Cow<'a, str>, item: Expr<'a>) -> Val<'a> {
    self.scope_mut().define(name, item)
  }

  pub fn set(
    &mut self,
    name: Cow<'a, str>,
    item: Expr<'a>,
  ) -> Result<Val<'a>, String> {
    self.scope_mut().set(name, item)
  }

  pub fn reserve(&mut self, name: Cow<'a, str>) {
    self.scope_mut().reserve(name);
  }

  pub fn has(&self, name: Cow<'a, str>) -> bool {
    self.scope().has(name)
  }

  pub fn get_val(&self, name: Cow<'a, str>) -> Option<Expr<'a>> {
    self.scope().get_val(name)
  }

  pub fn get_ref(&self, name: Cow<'a, str>) -> Option<&Val<'a>> {
    self.scope().get_ref(name)
  }

  pub fn remove(&mut self, name: Cow<'a, str>) {
    self.scope_mut().remove(name);
  }
}
