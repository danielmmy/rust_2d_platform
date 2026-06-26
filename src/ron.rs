//! A tiny, self-contained reader for the slice of [RON] our maps use.
//!
//! The map files are written in [Rusty Object Notation], but we only need a small
//! corner of it — named-field structs `( field: value, … )`, lists `[ … ]`, string
//! literals, and char literals. Rather than depend on the full `ron`/`serde`
//! stack, we parse that subset by hand here: [`from_str`] turns the text into a
//! [`Value`] tree, and [`world`](crate::world) walks it to build a map.
//!
//! Comments (`//` line and `/* … */` block) and trailing commas are tolerated, so
//! the `.map.ron` files stay comfortable to hand-edit.
//!
//! [RON]: https://github.com/ron-rs/ron
//! [Rusty Object Notation]: https://github.com/ron-rs/ron

use std::fmt;

/// A parsed value. Only the shapes our map files actually contain are modelled.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A string literal, `"like this"`.
    Str(String),
    /// A char literal, `'x'`.
    Char(char),
    /// A number, `0.25` or `-3` (always kept as `f64`).
    Num(f64),
    /// A list, `[a, b, c]`.
    List(Vec<Value>),
    /// A named-field struct, `(field: value, …)`.
    Map(Vec<(String, Value)>),
}

impl Value {
    /// Borrow this value as a string, or fail if it isn't one.
    pub fn as_str(&self) -> Result<&str, RonError> {
        match self {
            Value::Str(s) => Ok(s),
            _ => Err(RonError("expected a string".into())),
        }
    }

    /// Read this value as a char, or fail if it isn't one.
    #[allow(dead_code)] // part of the reader's API; exercised by tests
    pub fn as_char(&self) -> Result<char, RonError> {
        match self {
            Value::Char(c) => Ok(*c),
            _ => Err(RonError("expected a char".into())),
        }
    }

    /// Read this value as an `f64`, or fail if it isn't a number.
    pub fn as_f64(&self) -> Result<f64, RonError> {
        match self {
            Value::Num(n) => Ok(*n),
            _ => Err(RonError("expected a number".into())),
        }
    }

    /// Read this value as an `f32` (convenience over [`Value::as_f64`]).
    pub fn as_f32(&self) -> Result<f32, RonError> {
        self.as_f64().map(|n| n as f32)
    }

    /// Read this value as an `i32` (numbers are stored as `f64`).
    pub fn as_i32(&self) -> Result<i32, RonError> {
        self.as_f64().map(|n| n as i32)
    }

    /// Borrow this value as a list, or fail if it isn't one.
    pub fn as_list(&self) -> Result<&[Value], RonError> {
        match self {
            Value::List(items) => Ok(items),
            _ => Err(RonError("expected a list".into())),
        }
    }

    /// Look up a struct field, failing if it (or the struct) is missing.
    pub fn field(&self, name: &str) -> Result<&Value, RonError> {
        self.try_field(name)
            .ok_or_else(|| RonError(format!("missing field '{name}'")))
    }

    /// Look up an optional struct field; `None` if absent (or not a struct).
    pub fn try_field(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Map(fields) => fields.iter().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Anything that went wrong while reading RON, with a human-readable message.
#[derive(Debug, Clone, PartialEq)]
pub struct RonError(pub String);

impl fmt::Display for RonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RON error: {}", self.0)
    }
}

impl std::error::Error for RonError {}

/// Parse a complete RON document into a [`Value`].
pub fn from_str(input: &str) -> Result<Value, RonError> {
    let mut parser = Parser {
        chars: input.chars().collect(),
        pos: 0,
    };
    let value = parser.parse_value()?;
    parser.skip_trivia();
    if parser.pos != parser.chars.len() {
        return Err(parser.err("unexpected trailing input"));
    }
    Ok(value)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn err(&self, msg: &str) -> RonError {
        RonError(format!("at char {}: {msg}", self.pos))
    }

    /// Skip whitespace and comments.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => self.pos += 1,
                Some('/') if self.peek2() == Some('/') => {
                    while let Some(c) = self.bump() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                Some('/') if self.peek2() == Some('*') => {
                    self.pos += 2;
                    while self.pos < self.chars.len()
                        && !(self.peek() == Some('*') && self.peek2() == Some('/'))
                    {
                        self.pos += 1;
                    }
                    self.pos = (self.pos + 2).min(self.chars.len());
                }
                _ => break,
            }
        }
    }

    fn expect(&mut self, ch: char) -> Result<(), RonError> {
        if self.peek() == Some(ch) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected '{ch}'")))
        }
    }

    fn parse_value(&mut self) -> Result<Value, RonError> {
        self.skip_trivia();
        match self.peek() {
            Some('"') => self.parse_string().map(Value::Str),
            Some('\'') => self.parse_char().map(Value::Char),
            Some('[') => self.parse_list(),
            Some('(') => self.parse_paren(),
            Some(c) if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() => {
                self.parse_number()
            }
            // A named struct like `Foo( … )`: skip the name, then read the body.
            Some(c) if is_ident_start(c) => {
                self.parse_ident();
                self.skip_trivia();
                if self.peek() == Some('(') {
                    self.parse_paren()
                } else {
                    Err(self.err("unexpected identifier"))
                }
            }
            _ => Err(self.err("expected a value")),
        }
    }

    /// Parse a `( … )` group: either a named-field struct (`(field: value, …)` →
    /// [`Value::Map`]) or a positional tuple (`(a, b, …)` → [`Value::List`], the same
    /// shape as `[ … ]`). They're told apart by looking ahead for `ident :`.
    fn parse_paren(&mut self) -> Result<Value, RonError> {
        self.expect('(')?;
        self.skip_trivia();
        if self.peek() == Some(')') {
            self.bump();
            return Ok(Value::List(Vec::new())); // an empty tuple `()`
        }
        let checkpoint = self.pos;
        let named = self.parse_ident().is_some() && {
            self.skip_trivia();
            self.peek() == Some(':')
        };
        self.pos = checkpoint;
        if named {
            self.parse_struct_fields()
        } else {
            self.parse_tuple_items()
        }
    }

    /// The body of a named struct (the opening `(` already consumed).
    fn parse_struct_fields(&mut self) -> Result<Value, RonError> {
        let mut fields = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(')') => {
                    self.bump();
                    break;
                }
                None => return Err(self.err("unterminated struct")),
                _ => {}
            }
            let name = self
                .parse_ident()
                .ok_or_else(|| self.err("expected a field name"))?;
            self.skip_trivia();
            self.expect(':')?;
            let value = self.parse_value()?;
            fields.push((name, value));
            self.skip_trivia();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(')') => {
                    self.bump();
                    break;
                }
                _ => return Err(self.err("expected ',' or ')'")),
            }
        }
        Ok(Value::Map(fields))
    }

    /// The body of a positional tuple (the opening `(` already consumed).
    fn parse_tuple_items(&mut self) -> Result<Value, RonError> {
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(')') => {
                    self.bump();
                    break;
                }
                None => return Err(self.err("unterminated tuple")),
                _ => {}
            }
            items.push(self.parse_value()?);
            self.skip_trivia();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(')') => {
                    self.bump();
                    break;
                }
                _ => return Err(self.err("expected ',' or ')'")),
            }
        }
        Ok(Value::List(items))
    }

    fn parse_list(&mut self) -> Result<Value, RonError> {
        self.expect('[')?;
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(']') => {
                    self.bump();
                    break;
                }
                None => return Err(self.err("unterminated list")),
                _ => {}
            }
            items.push(self.parse_value()?);
            self.skip_trivia();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    break;
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
        Ok(Value::List(items))
    }

    fn parse_string(&mut self) -> Result<String, RonError> {
        self.expect('"')?;
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err(self.err("unterminated string")),
                Some('"') => break,
                Some('\\') => s.push(self.parse_escape()?),
                Some(c) => s.push(c),
            }
        }
        Ok(s)
    }

    fn parse_number(&mut self) -> Result<Value, RonError> {
        let start = self.pos;
        if matches!(self.peek(), Some('-' | '+')) {
            self.pos += 1;
        }
        let mut saw_digit = false;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
            saw_digit = true;
        }
        if self.peek() == Some('.') {
            self.pos += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
                saw_digit = true;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some('-' | '+')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if !saw_digit {
            return Err(self.err("invalid number"));
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>()
            .map(Value::Num)
            .map_err(|_| self.err("invalid number"))
    }

    fn parse_char(&mut self) -> Result<char, RonError> {
        self.expect('\'')?;
        let c = match self.bump() {
            None => return Err(self.err("unterminated char")),
            Some('\\') => self.parse_escape()?,
            Some(c) => c,
        };
        self.expect('\'')?;
        Ok(c)
    }

    fn parse_escape(&mut self) -> Result<char, RonError> {
        match self.bump() {
            Some('"') => Ok('"'),
            Some('\'') => Ok('\''),
            Some('\\') => Ok('\\'),
            Some('n') => Ok('\n'),
            Some('t') => Ok('\t'),
            Some('r') => Ok('\r'),
            Some('0') => Ok('\0'),
            Some('u') => self.parse_unicode_escape(),
            _ => Err(self.err("invalid escape sequence")),
        }
    }

    /// Parse the `{XXXX}` body of a `\u{…}` escape (already past the `u`).
    fn parse_unicode_escape(&mut self) -> Result<char, RonError> {
        self.expect('{')?;
        let mut code = 0u32;
        let mut digits = 0;
        while let Some(c) = self.peek() {
            let Some(d) = c.to_digit(16) else { break };
            code = code * 16 + d;
            digits += 1;
            self.pos += 1;
        }
        self.expect('}')?;
        if digits == 0 {
            return Err(self.err("empty unicode escape"));
        }
        char::from_u32(code).ok_or_else(|| self.err("invalid unicode scalar value"))
    }

    fn parse_ident(&mut self) -> Option<String> {
        if !matches!(self.peek(), Some(c) if is_ident_start(c)) {
            return None;
        }
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_ident_continue(c)) {
            self.pos += 1;
        }
        Some(self.chars[start..self.pos].iter().collect())
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_nested_struct() {
        let value = from_str(
            r###"
            (
                solid: "#",
                spawns: [
                    (marker: 'S', name: "start"),
                ],
                tiles: [ "##", "#." ],
            )
            "###,
        )
        .unwrap();

        assert_eq!(value.field("solid").unwrap().as_str().unwrap(), "#");

        let spawns = value.field("spawns").unwrap().as_list().unwrap();
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].field("marker").unwrap().as_char().unwrap(), 'S');
        assert_eq!(spawns[0].field("name").unwrap().as_str().unwrap(), "start");

        let tiles = value.field("tiles").unwrap().as_list().unwrap();
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[1].as_str().unwrap(), "#.");
    }

    #[test]
    fn skips_comments_and_trailing_commas() {
        let value = from_str(
            r#"
            ( // a leading comment
                a: "x", /* inline */ b: "y",
            )
            "#,
        )
        .unwrap();
        assert_eq!(value.field("a").unwrap().as_str().unwrap(), "x");
        assert_eq!(value.field("b").unwrap().as_str().unwrap(), "y");
    }

    #[test]
    fn parses_numbers() {
        let value = from_str("( bg: [0.1, 0.25, -3], n: 5, e: 1.5e2 )").unwrap();
        let bg = value.field("bg").unwrap().as_list().unwrap();
        assert_eq!(bg[0].as_f32().unwrap(), 0.1);
        assert_eq!(bg[1].as_f32().unwrap(), 0.25);
        assert_eq!(bg[2].as_f32().unwrap(), -3.0);
        assert_eq!(value.field("n").unwrap().as_f64().unwrap(), 5.0);
        assert_eq!(value.field("e").unwrap().as_f64().unwrap(), 150.0);
    }

    #[test]
    fn handles_escapes() {
        let value = from_str(r#"( s: "a\tb\n", c: '\'' )"#).unwrap();
        assert_eq!(value.field("s").unwrap().as_str().unwrap(), "a\tb\n");
        assert_eq!(value.field("c").unwrap().as_char().unwrap(), '\'');
    }

    #[test]
    fn parses_positional_tuples() {
        // A tuple `(a, b, …)` reads as a list, distinct from a named struct.
        let value = from_str(r#"( doors: [((1, 2), "r1_0", (3, 4))], n: (a: 5) )"#).unwrap();
        let doors = value.field("doors").unwrap().as_list().unwrap();
        let door = doors[0].as_list().unwrap();
        assert_eq!(door.len(), 3);
        assert_eq!(door[0].as_list().unwrap()[0].as_i32().unwrap(), 1);
        assert_eq!(door[0].as_list().unwrap()[1].as_i32().unwrap(), 2);
        assert_eq!(door[1].as_str().unwrap(), "r1_0");
        assert_eq!(door[2].as_list().unwrap()[0].as_i32().unwrap(), 3);
        // A named struct in the same document still parses as a map.
        assert_eq!(
            value
                .field("n")
                .unwrap()
                .field("a")
                .unwrap()
                .as_i32()
                .unwrap(),
            5
        );
    }

    #[test]
    fn optional_field_is_none_when_absent() {
        let value = from_str(r#"( a: "x" )"#).unwrap();
        assert!(value.try_field("missing").is_none());
        assert!(value.field("missing").is_err());
    }

    #[test]
    fn reports_errors_on_garbage() {
        assert!(from_str("(a: )").is_err());
        assert!(from_str("[1, 2").is_err());
        assert!(from_str(r#"( a: "unterminated )"#).is_err());
    }
}
