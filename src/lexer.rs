use core::{fmt, ops::Range};

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
  pub const fn new(kind: TokenKind, start: usize, end: usize) -> Self {
    Self {
      kind,
      span: Span::new(start, end),
    }
  }

  pub const fn begin(kind: TokenKind, start: usize) -> Self {
    Self::new(kind, start, start)
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
}

impl Span {
  pub const fn new(start: usize, end: usize) -> Self {
    Self { start, end }
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

      Self::Comment => write!(f, "a comment"),
    }
  }
}

const SYMBOL_CHARS: [char; 11] =
  ['!', '@', '#', '$', '%', '^', '&', '*', '/', '-', '_'];

pub fn lex(source: impl AsRef<str>) -> Vec<Token> {
  let source = source.as_ref();

  // TODO: -1-2 will be two integers because we don't check that a space (or other delimiter such as parens) follow it.
  // This applies to strings and everything else.
  let mut tokens: Vec<Token> = Vec::new();
  let mut current: Option<Token> = None;
  for (i, char) in source.char_indices() {
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
          }
        }
        TokenKind::Symbol | TokenKind::Keyword => {
          if !(char.is_alphabetic() || SYMBOL_CHARS.contains(&char)) {
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
      }
    } else {
      // TODO: comments should be double ":"?
      if char == ';' {
        current = Some(Token::begin(TokenKind::Comment, i + 1));
      } else if char == '"' {
        current = Some(Token::begin(TokenKind::String, i + 1));
      } else if char.is_ascii_digit() || char == '-' {
        current = Some(Token::begin(TokenKind::Integer, i));
      } else if char == '(' {
        tokens.push(Token::new(TokenKind::LeftParen, i, i + 1));
      } else if char == ')' {
        tokens.push(Token::new(TokenKind::RightParen, i, i + 1));
      } else if char == ':' {
        current = Some(Token::begin(TokenKind::Keyword, i + 1));
      } else if char.is_alphabetic() || SYMBOL_CHARS.contains(&char) {
        current = Some(Token::begin(TokenKind::Symbol, i));
      } else if !char.is_whitespace() {
        tokens.push(Token::new(TokenKind::Invalid, i, i + 1));
      }
    }
  }

  if let Some(token) = current {
    tokens.push(token.end(source.len()));
  }

  tokens
}
