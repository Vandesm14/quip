use core::{cmp::Ordering, fmt, ops, ops::Range};
use std::{collections::HashMap, rc::Rc};

use itertools::Itertools;
use slotmap::DefaultKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
  pub kind: TokenKind,
  pub span: Span,
}

impl fmt::Display for Token {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.kind)
  }
}

impl Token {
  pub const fn new(
    kind: TokenKind,
    start: usize,
    end: usize,
    line: u32,
    column: u32,
  ) -> Self {
    Self {
      kind,
      span: Span::new(start, end, line, column),
    }
  }

  pub const fn begin(
    kind: TokenKind,
    start: usize,
    line: u32,
    column: u32,
  ) -> Self {
    Self::new(kind, start, start, line, column)
  }

  pub const fn end(mut self, end: usize) -> Self {
    self.span.end = end;
    self
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
  /// The lower byte bound (inclusive).
  pub start: usize,
  /// The upper byte bound (exclusive).
  pub end: usize,
  pub line: u32,
  pub column: u32,
}

impl Span {
  pub const fn new(start: usize, end: usize, line: u32, column: u32) -> Self {
    Self {
      start,
      end,
      line,
      column,
    }
  }

  /// Returns the <code>[Range]\<[usize]\></code> of this [`Span`].
  #[inline]
  pub const fn to_range(self) -> Range<usize> {
    Range {
      start: self.start,
      end: self.end,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
  Invalid,
  Eof,

  LeftParen,
  RightParen,

  Lazy,

  Integer,
  Float,
  String,
  Symbol,
  Keyword,

  Comment,
}

impl fmt::Display for TokenKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Invalid => write!(f, "invalid characters"),
      Self::Eof => write!(f, "end of file"),

      Self::LeftParen => write!(f, "'('"),
      Self::RightParen => write!(f, "')'"),

      Self::Integer => write!(f, "an integer literal"),
      Self::Float => write!(f, "a float literal"),
      Self::String => write!(f, "a string literal"),
      Self::Symbol => write!(f, "a symbol literal"),
      Self::Keyword => write!(f, "a keyword literal"),

      Self::Lazy => write!(f, "'"),

      Self::Comment => write!(f, "a comment"),
    }
  }
}

const SYMBOL_CHARS: &[char] = &[
  '~', '!', '@', '#', '$', '%', '^', '&', '*', '-', '_', '=', '+', '|', '\'',
  '<', '.', '>', '?', '/',
];

pub fn lex(source: impl AsRef<str>) -> Vec<Token> {
  let source = source.as_ref();

  // TODO: -1-2 will be two integers because we don't check that a space (or other delimiter such as parens) follow it.
  // This applies to strings and everything else.
  let mut tokens: Vec<Token> = Vec::new();
  let mut current: Option<Token> = None;
  let mut line = 1;
  let mut column = 1;

  for (i, char) in source.char_indices() {
    if char == '\n' {
      line += 1;
      column = 1;
    } else {
      column += 1;
    }

    if let Some(ref mut token) = current {
      match token.kind {
        TokenKind::Integer => {
          if char == '.' {
            token.kind = TokenKind::Float;
          } else if !char.is_ascii_digit() {
            tokens.push(token.end(i));
            current = None;
          }
        }
        TokenKind::Float => {
          if !char.is_ascii_digit() {
            tokens.push(token.end(i));
            current = None;
          }
        }
        TokenKind::String => {
          if char == '"' {
            tokens.push(token.end(i));
            current = None;
            continue;
          }
        }
        TokenKind::Symbol | TokenKind::Keyword => {
          if !(char.is_ascii_alphanumeric() || SYMBOL_CHARS.contains(&char)) {
            tokens.push(token.end(i));
            current = None;
          }
        }
        TokenKind::Comment => {
          if char == '\n' {
            current = None;
          }
        }

        TokenKind::Invalid => unreachable!("invalids are single chars"),
        TokenKind::Eof => unimplemented!("should never be EOF"),
        TokenKind::LeftParen => unreachable!("parens are single chars"),
        TokenKind::RightParen => unreachable!("parens are single chars"),
        TokenKind::Lazy => unreachable!("lazy is a single char"),
      }
    }

    if current.is_none() {
      if char == '\'' {
        tokens.push(Token::new(TokenKind::Lazy, i, i + 1, line, column));
      } else if char == ';' && source.chars().nth(i + 1) == Some(';') {
        current = Some(Token::begin(TokenKind::Comment, i + 1, line, column));
      } else if char == '"' {
        current = Some(Token::begin(TokenKind::String, i + 1, line, column));
      } else if char.is_ascii_digit() {
        current = Some(Token::begin(TokenKind::Integer, i, line, column));
      } else if char == '-' {
        // Lookahead to determine if this is a negative number or a symbol.
        let next_char = source.chars().nth(i + 1);
        if matches!(next_char, Some(c) if c.is_ascii_digit()) {
          current = Some(Token::begin(TokenKind::Integer, i, line, column));
        } else {
          current = Some(Token::begin(TokenKind::Symbol, i, line, column));
        }
      } else if char == '(' {
        tokens.push(Token::new(TokenKind::LeftParen, i, i + 1, line, column));
      } else if char == ')' {
        tokens.push(Token::new(TokenKind::RightParen, i, i + 1, line, column));
      } else if char == ':' {
        current = Some(Token::begin(TokenKind::Keyword, i + 1, line, column));
      } else if char.is_alphabetic() || SYMBOL_CHARS.contains(&char) {
        current = Some(Token::begin(TokenKind::Symbol, i, line, column));
      } else if !char.is_whitespace() {
        tokens.push(Token::new(TokenKind::Invalid, i, i + 1, line, column));
      }
    }
  }

  if let Some(token) = current {
    tokens.push(token.end(source.len()));
  }

  tokens
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
  pub kind: ExprKind,
  pub span: Option<Span>,
}

impl core::fmt::Display for Expr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", &self.kind)
  }
}

#[derive(Debug, Clone, Default)]
pub enum ExprKind {
  #[default]
  Nil,

  String(String),
  Keyword(Rc<str>),
  Symbol(Rc<str>),
  Error(Rc<str>),

  Float(f64),
  Integer(i64),

  Boolean(bool),

  List(Rc<Vec<Expr>>),
  Map(HashMap<Rc<str>, Expr>),

  Function {
    params: Vec<Rc<str>>,
    body: Vec<Expr>,
    env: DefaultKey,
  },
}

impl ExprKind {
  /// Promotes two numeric operands to a common type. If either operand is a
  /// [`Float`], both are returned as [`Float`]s. Non-numeric operands are
  /// returned unchanged.
  pub fn coerce_numeric(self, other: Self) -> (Self, Self) {
    match (self, other) {
      (Self::Integer(l), Self::Float(r)) => {
        (Self::Float(l as f64), Self::Float(r))
      }
      (Self::Float(l), Self::Integer(r)) => {
        (Self::Float(l), Self::Float(r as f64))
      }
      pair => pair,
    }
  }

  /// Demotes a [`Float`] back to an [`Integer`] if it is a finite whole number
  /// within the [`i64`] range. Other values are returned unchanged.
  pub fn normalize_numeric(self) -> Self {
    match self {
      Self::Float(f)
        if f.is_finite()
          && f.fract() == 0.0
          && f >= i64::MIN as f64
          && f <= i64::MAX as f64 =>
      {
        Self::Integer(f as i64)
      }
      other => other,
    }
  }

  pub fn type_name(&self) -> &'static str {
    match self {
      ExprKind::Nil => "nil",
      ExprKind::String(..) => "string",
      ExprKind::Keyword(..) => "keyword",
      ExprKind::Symbol(..) => "symbol",
      ExprKind::Error(..) => "error",
      ExprKind::Float(..) => "float",
      ExprKind::Integer(..) => "integer",
      ExprKind::Boolean(..) => "boolean",
      ExprKind::List(..) => "list",
      ExprKind::Map(..) => "map",
      ExprKind::Function { .. } => "function",
    }
  }
}

impl core::fmt::Display for ExprKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ExprKind::Nil => write!(f, "nil"),
      ExprKind::Boolean(b) => write!(f, "{}", b),
      ExprKind::String(string) => write!(f, "{}", string),
      ExprKind::Keyword(keyword) => write!(f, ":{}", keyword),
      ExprKind::Symbol(symbol) => write!(f, "{}", symbol),
      ExprKind::Error(error) => write!(f, "Error({})", error),
      ExprKind::Float(float) => write!(f, "{}", float),
      ExprKind::Integer(integer) => write!(f, "{}", integer),
      ExprKind::List(exprs) => {
        write!(f, "({})", exprs.iter().map(|e| e.to_string()).join(" "))
      }
      ExprKind::Map(_) => todo!(),
      ExprKind::Function { params, body, .. } => {
        write!(
          f,
          "(fn ({}) {})",
          params.iter().join(" "),
          body.iter().join(" ")
        )
      }
    }
  }
}

impl PartialEq for ExprKind {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Nil, Self::Nil) => true,

      (Self::Boolean(lhs), Self::Boolean(rhs)) => lhs == rhs,

      (Self::String(lhs), Self::String(rhs)) => lhs == rhs,
      (Self::Keyword(lhs), Self::Keyword(rhs)) => lhs == rhs,
      (Self::Symbol(lhs), Self::Symbol(rhs)) => lhs == rhs,
      (Self::Error(lhs), Self::Error(rhs)) => lhs == rhs,

      (Self::Float(lhs), Self::Float(rhs)) => lhs == rhs,
      (Self::Integer(lhs), Self::Integer(rhs)) => lhs == rhs,

      (Self::List(lhs), Self::List(rhs)) => lhs == rhs,
      (Self::Map(lhs), Self::Map(rhs)) => lhs == rhs,

      (
        Self::Function {
          params: lhs_params,
          body: lhs_body,
          env: lhs_env,
        },
        Self::Function {
          params: rhs_params,
          body: rhs_body,
          env: rhs_env,
        },
      ) => {
        lhs_params == rhs_params && lhs_body == rhs_body && lhs_env == rhs_env
      }

      _ => false,
    }
  }
}

impl PartialOrd for ExprKind {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    match (self, other) {
      (Self::Nil, Self::Nil) => Some(Ordering::Equal),

      (Self::Boolean(lhs), Self::Boolean(rhs)) => lhs.partial_cmp(rhs),

      (Self::String(lhs), Self::String(rhs)) => {
        lhs.eq(rhs).then_some(Ordering::Equal)
      }
      (Self::Keyword(lhs), Self::Keyword(rhs)) => {
        lhs.eq(rhs).then_some(Ordering::Equal)
      }
      (Self::Symbol(lhs), Self::Symbol(rhs)) => {
        lhs.eq(rhs).then_some(Ordering::Equal)
      }
      (Self::Error(lhs), Self::Error(rhs)) => {
        lhs.eq(rhs).then_some(Ordering::Equal)
      }

      (Self::Float(lhs), Self::Float(rhs)) => lhs.partial_cmp(rhs),
      (Self::Integer(lhs), Self::Integer(rhs)) => lhs.partial_cmp(rhs),

      (Self::List(lhs), Self::List(rhs)) => {
        lhs.eq(rhs).then_some(Ordering::Equal)
      }

      _ => None,
    }
  }
}

impl ops::Add for ExprKind {
  type Output = Result<Self, (Self, Self)>;

  fn add(self, rhs: Self) -> Self::Output {
    match (self, rhs) {
      (Self::Integer(lhs), Self::Integer(rhs)) => {
        Ok(Self::Integer(lhs.saturating_add(rhs)))
      }
      (Self::Float(lhs), Self::Float(rhs)) => Ok(Self::Float(lhs + rhs)),

      (lhs, rhs) => Err((lhs, rhs)),
    }
  }
}

impl ops::Sub for ExprKind {
  type Output = Result<Self, (Self, Self)>;

  fn sub(self, rhs: Self) -> Self::Output {
    match (self, rhs) {
      (Self::Integer(lhs), Self::Integer(rhs)) => {
        Ok(Self::Integer(lhs.saturating_sub(rhs)))
      }
      (Self::Float(lhs), Self::Float(rhs)) => Ok(Self::Float(lhs - rhs)),

      (lhs, rhs) => Err((lhs, rhs)),
    }
  }
}

impl ops::Mul for ExprKind {
  type Output = Result<Self, (Self, Self)>;

  fn mul(self, rhs: Self) -> Self::Output {
    match (self, rhs) {
      (Self::Integer(lhs), Self::Integer(rhs)) => {
        Ok(Self::Integer(lhs.saturating_mul(rhs)))
      }
      (Self::Float(lhs), Self::Float(rhs)) => Ok(Self::Float(lhs * rhs)),

      (lhs, rhs) => Err((lhs, rhs)),
    }
  }
}

impl ops::Div for ExprKind {
  type Output = Result<Self, (Self, Self)>;

  fn div(self, rhs: Self) -> Self::Output {
    match (self, rhs) {
      (Self::Integer(lhs), Self::Integer(rhs)) => {
        Ok(Self::Integer(lhs.saturating_div(rhs)))
      }
      (Self::Float(lhs), Self::Float(rhs)) => Ok(Self::Float(lhs / rhs)),

      (lhs, rhs) => Err((lhs, rhs)),
    }
  }
}

impl ops::Rem for ExprKind {
  type Output = Result<Self, (Self, Self)>;

  fn rem(self, rhs: Self) -> Self::Output {
    match (self, rhs) {
      (Self::Integer(lhs), Self::Integer(rhs)) => Ok(Self::Integer(lhs % rhs)),
      (Self::Float(lhs), Self::Float(rhs)) => Ok(Self::Float(lhs % rhs)),

      (lhs, rhs) => Err((lhs, rhs)),
    }
  }
}

pub fn parse(source: &str, tokens: Vec<Token>) -> Result<Vec<Expr>, String> {
  let mut stack: Vec<Vec<Expr>> = vec![Vec::new()];
  let mut spans = vec![];
  // Whether the next expr should be lazy. (<span of the lazy `'`> <applies to list>).
  let mut lazy_span: Option<(Span, bool)> = None;

  let make_lazy = |expr: Expr, span: Span| Expr {
    kind: ExprKind::List(Rc::new(vec![
      Expr {
        kind: ExprKind::Symbol("lazy".into()),
        span: Some(span),
      },
      expr,
    ])),
    span: Some(span),
  };

  for token in tokens.into_iter() {
    let span = source
      .get(token.span.to_range())
      .ok_or_else(|| "bad span".to_string())?;

    match token.kind {
      TokenKind::Invalid => {}
      TokenKind::Eof => {}

      TokenKind::LeftParen => {
        stack.push(Vec::new());
        spans.push(token.span);

        if let Some((_, ref mut is_list)) = lazy_span {
          *is_list = true;
        }
      }
      TokenKind::RightParen => {
        let current = stack.pop();
        let start_span = spans.pop();

        if let Some(current) = current
          && let Some(start_span) = start_span
          && let Some(last) = stack.last_mut()
        {
          let expr = Expr {
            kind: ExprKind::List(Rc::new(current)),
            span: Some(Span {
              start: start_span.start,
              end: token.span.end,
              line: start_span.line,
              column: start_span.column,
            }),
          };
          last.push(
            lazy_span
              .take()
              .map(|(span, _)| make_lazy(expr.clone(), span))
              .unwrap_or(expr),
          );
        } else {
          return Err("unmatched '('".to_string());
        }
      }

      TokenKind::Lazy => {
        lazy_span = Some((token.span, false));
      }

      TokenKind::Integer => {
        let parsed = span
          .parse::<i64>()
          .map_err(|_| "invalid integer".to_string())?;
        if let Some(last) = stack.last_mut() {
          last.push(Expr {
            kind: ExprKind::Integer(parsed),
            span: Some(token.span),
          });
        }
      }
      TokenKind::Float => {
        let parsed = span
          .parse::<f64>()
          .map_err(|_| "invalid float".to_string())?;
        if let Some(last) = stack.last_mut() {
          last.push(Expr {
            kind: ExprKind::Float(parsed),
            span: Some(token.span),
          });
        }
      }

      TokenKind::String => {
        if let Some(last) = stack.last_mut() {
          last.push(Expr {
            kind: ExprKind::String(span.to_string()),
            span: Some(Span {
              start: token.span.start - 1,
              end: token.span.end + 1,
              ..token.span
            }),
          });
        }
      }
      TokenKind::Symbol => {
        if let Some(last) = stack.last_mut() {
          if span == "nil" {
            last.push(Expr {
              kind: ExprKind::Nil,
              span: Some(token.span),
            });
          } else if span == "true" {
            last.push(Expr {
              kind: ExprKind::Boolean(true),
              span: Some(token.span),
            });
          } else if span == "false" {
            last.push(Expr {
              kind: ExprKind::Boolean(false),
              span: Some(token.span),
            });
          } else {
            let expr = Expr {
              kind: ExprKind::Symbol(Rc::from(span)),
              span: Some(token.span),
            };
            last.push(
              if let Some((span, is_list)) = lazy_span
                && !is_list
              {
                lazy_span = None;
                make_lazy(expr, span)
              } else {
                expr
              },
            );
          }
        }
      }
      TokenKind::Keyword => {
        if let Some(last) = stack.last_mut() {
          last.push(Expr {
            kind: ExprKind::Keyword(Rc::from(span)),
            span: Some(token.span),
          });
        }
      }

      // TODO(leonskij): It would nice to include comments in debug information.
      TokenKind::Comment => {}
    }
  }

  if stack.len() > 1 {
    Err("unmatched ')'".to_owned())
  } else if let Some(first) = stack.first() {
    Ok(first.clone())
  } else {
    Err("err, idk".to_owned())
  }
}
