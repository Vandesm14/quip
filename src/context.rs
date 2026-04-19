use core::cell::RefCell;
use std::{borrow::Cow, collections::HashMap, rc::Rc};

use crate::{
  ast::{Expr, ExprKindVariants},
  chain::Chain,
};

pub type Val<'a> = Rc<RefCell<Chain<Option<Expr<'a>>>>>;

#[derive(Debug, Clone, PartialEq)]
pub struct Function<'a> {
  args: Vec<ExprKindVariants>,
  body: Vec<Expr<'a>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Context<'a> {
  pub vars: HashMap<Cow<'a, str>, Val<'a>>,
  pub fns: HashMap<Cow<'a, str>, Vec<Expr<'a>>>,
}

impl<'a> Context<'a> {
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

  /// Merges another context's vars into this one, not overwriting existing variables.
  pub fn merge(&mut self, other: Context<'a>) {
    for (name, item) in other.vars {
      if !self.has(name.clone())
        || (self.get_val(name.clone()).is_none()
          && item.borrow().val().is_some())
      {
        self.vars.insert(name.clone(), item);
      }
    }
  }

  /// Creates a new context with vars linked to self's (for function call scoping).
  pub fn duplicate(&self) -> Self {
    let mut vars = HashMap::new();
    for (name, item) in self.vars.iter() {
      let mut item = RefCell::borrow_mut(item);
      vars.insert(name.clone(), item.link());
    }
    Self {
      vars,
      fns: self.fns.clone(),
    }
  }
}
