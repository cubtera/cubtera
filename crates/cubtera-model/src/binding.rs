//! `Binding`: desired state (section 5.4). A `Binding` names a unit and a
//! [`Selector`] expression over resolved dimension data; `expand` (in
//! `cubtera-app`'s `bindings` module - it needs `InventoryPort`/
//! `ResolveUseCase`, which this zero-I/O crate cannot depend on) walks the
//! inventory and turns it into the concrete set of `InstanceId`s that
//! *should* exist, minus `exclude`.
//!
//! `Selector` is deliberately a small, dependency-free boolean-expression
//! AST - not an external CEL crate - so `cubtera-model` stays at zero I/O
//! and zero third-party surface beyond `serde_json`/`jsonschema` (the
//! existing rule for this layer). It supports exactly what section 5.4/section 6
//! need: `<dim_type>.<field> == <literal>`, `<dim_type>.<field> in
//! [<literal>, ...]`, `&&`, `||`, `!`, and parens - evaluated against a
//! per-instance context (dim_type -> that dimension's own resolved
//! fields, `"name"` injected).

use crate::error::ModelError;
use cubtera_kernel::InstanceId;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

/// Everything a [`Selector`] can evaluate against for one candidate
/// instance: one flat object per dimension type actually resolved for
/// it (root through leaf), each with `"name"` injected alongside whatever
/// fields that dimension's own `meta` section carries. Building this is
/// `cubtera-app::bindings::BindingUseCase`'s job - it is the thing that
/// actually walks `ResolveUseCase` across a `key_path`.
pub type SelectorContext = BTreeMap<String, Value>;

/// A literal value in a selector expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Str(String),
    Bool(bool),
    Num(f64),
}

impl Literal {
    fn matches(&self, value: &Value) -> bool {
        match (self, value) {
            (Literal::Str(s), Value::String(v)) => s == v,
            (Literal::Bool(b), Value::Bool(v)) => b == v,
            (Literal::Num(n), Value::Number(v)) => v.as_f64() == Some(*n),
            _ => false,
        }
    }
}

/// `<dim_type>.<field>` - the only place a selector reaches into a
/// candidate's [`SelectorContext`].
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub dim_type: String,
    pub field: String,
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.dim_type, self.field)
    }
}

/// A small boolean-expression AST over a [`SelectorContext`] - see the
/// module doc comment for the supported grammar.
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    /// Matches everything - the empty/absent selector.
    All,
    Eq(Path, Literal),
    In(Path, Vec<Literal>),
    And(Box<Selector>, Box<Selector>),
    Or(Box<Selector>, Box<Selector>),
    Not(Box<Selector>),
}

impl Selector {
    /// Parse a selector expression, e.g. `env.name in ['prod', 'stg'] &&
    /// dc.status == 'active'`. Single or double quotes are both accepted
    /// for string literals.
    pub fn parse(input: &str) -> Result<Self, ModelError> {
        let tokens = tokenize(input)?;
        let mut parser = Parser { tokens, pos: 0 };
        let selector = parser.parse_or()?;
        if parser.pos != parser.tokens.len() {
            return Err(ModelError::Selector(format!(
                "unexpected trailing input at token {}",
                parser.pos
            )));
        }
        Ok(selector)
    }

    /// Evaluate against one candidate's context. A path referencing a
    /// dimension type or field the context doesn't have is `false`, never
    /// an error - a selector like `service.tier == 'edge'` is simply not
    /// satisfied for a candidate that never resolved a `service`
    /// dimension, exactly like a missing key in any other boolean
    /// expression language.
    pub fn evaluate(&self, ctx: &SelectorContext) -> bool {
        match self {
            Selector::All => true,
            Selector::Eq(path, lit) => lookup(ctx, path).is_some_and(|v| lit.matches(v)),
            Selector::In(path, lits) => {
                lookup(ctx, path).is_some_and(|v| lits.iter().any(|lit| lit.matches(v)))
            }
            Selector::And(a, b) => a.evaluate(ctx) && b.evaluate(ctx),
            Selector::Or(a, b) => a.evaluate(ctx) || b.evaluate(ctx),
            Selector::Not(a) => !a.evaluate(ctx),
        }
    }
}

fn lookup<'a>(ctx: &'a SelectorContext, path: &Path) -> Option<&'a Value> {
    ctx.get(&path.dim_type).and_then(|v| v.get(&path.field))
}

/// A `Binding`: `unit` should exist for every candidate `selector`
/// matches, except `exclude`. `wave` groups bindings for ordered batch
/// application (`cubtera-app::bindings::group_by_wave`) - waves are a
/// static ordering hint the operator assigns, not something inferred from
/// a dependency graph (section "No auto-DAG", decisions log section 13 - preserved here
/// unchanged: nothing about `Binding` infers ordering from `[inputs]`).
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub id: String,
    pub unit: cubtera_kernel::Ident,
    pub selector: Selector,
    pub exclude: Vec<InstanceId>,
    pub wave: u32,
}

impl Binding {
    /// Whether `id` should exist under this binding: matches `selector`
    /// and isn't in `exclude`.
    pub fn matches(&self, id: &InstanceId, ctx: &SelectorContext) -> bool {
        !self.exclude.contains(id) && self.selector.evaluate(ctx)
    }
}

// ---------------------------------------------------------------------------
// Tokenizer + recursive-descent parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(f64),
    Bool(bool),
    Dot,
    Eq,
    In,
    And,
    Or,
    Not,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
}

fn tokenize(input: &str) -> Result<Vec<Token>, ModelError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '.' => {
                tokens.push(Token::Dot);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '[' => {
                tokens.push(Token::LBracket);
                i += 1;
            }
            ']' => {
                tokens.push(Token::RBracket);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '!' => {
                tokens.push(Token::Not);
                i += 1;
            }
            '=' if chars.get(i + 1) == Some(&'=') => {
                tokens.push(Token::Eq);
                i += 2;
            }
            '&' if chars.get(i + 1) == Some(&'&') => {
                tokens.push(Token::And);
                i += 2;
            }
            '|' if chars.get(i + 1) == Some(&'|') => {
                tokens.push(Token::Or);
                i += 2;
            }
            '\'' | '"' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    if chars[i] == quote {
                        closed = true;
                        i += 1;
                        break;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                if !closed {
                    return Err(ModelError::Selector(format!(
                        "unterminated string literal starting at {i}"
                    )));
                }
                tokens.push(Token::Str(s));
            }
            _ if c.is_ascii_digit()
                || (c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) =>
            {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let raw: String = chars[start..i].iter().collect();
                let num = raw
                    .parse::<f64>()
                    .map_err(|_| ModelError::Selector(format!("invalid number {raw:?}")))?;
                tokens.push(Token::Num(num));
            }
            _ if c.is_alphanumeric() || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                tokens.push(match word.as_str() {
                    "in" => Token::In,
                    "true" => Token::Bool(true),
                    "false" => Token::Bool(false),
                    _ => Token::Ident(word),
                });
            }
            other => {
                return Err(ModelError::Selector(format!(
                    "unexpected character {other:?} at position {i}"
                )));
            }
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<(), ModelError> {
        match self.advance() {
            Some(tok) if tok == expected => Ok(()),
            other => Err(ModelError::Selector(format!(
                "expected {expected:?}, found {other:?}"
            ))),
        }
    }

    fn parse_or(&mut self) -> Result<Selector, ModelError> {
        let mut lhs = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.advance();
            let rhs = self.parse_and()?;
            lhs = Selector::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Selector, ModelError> {
        let mut lhs = self.parse_unary()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Selector::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Selector, ModelError> {
        if matches!(self.peek(), Some(Token::Not)) {
            self.advance();
            let inner = self.parse_unary()?;
            return Ok(Selector::Not(Box::new(inner)));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<Selector, ModelError> {
        if matches!(self.peek(), Some(Token::LParen)) {
            self.advance();
            let inner = self.parse_or()?;
            self.expect(&Token::RParen)?;
            return Ok(inner);
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Selector, ModelError> {
        let path = self.parse_path()?;
        match self.advance() {
            Some(Token::Eq) => {
                let lit = self.parse_literal()?;
                Ok(Selector::Eq(path, lit))
            }
            Some(Token::In) => {
                self.expect(&Token::LBracket)?;
                let mut lits = vec![self.parse_literal()?];
                while matches!(self.peek(), Some(Token::Comma)) {
                    self.advance();
                    lits.push(self.parse_literal()?);
                }
                self.expect(&Token::RBracket)?;
                Ok(Selector::In(path, lits))
            }
            other => Err(ModelError::Selector(format!(
                "expected '==' or 'in' after {path}, found {other:?}"
            ))),
        }
    }

    fn parse_path(&mut self) -> Result<Path, ModelError> {
        let dim_type = match self.advance() {
            Some(Token::Ident(s)) => s.clone(),
            other => {
                return Err(ModelError::Selector(format!(
                    "expected a dimension type, found {other:?}"
                )))
            }
        };
        self.expect(&Token::Dot)?;
        let field = match self.advance() {
            Some(Token::Ident(s)) => s.clone(),
            other => {
                return Err(ModelError::Selector(format!(
                    "expected a field name after '{dim_type}.', found {other:?}"
                )))
            }
        };
        Ok(Path { dim_type, field })
    }

    fn parse_literal(&mut self) -> Result<Literal, ModelError> {
        match self.advance() {
            Some(Token::Str(s)) => Ok(Literal::Str(s.clone())),
            Some(Token::Num(n)) => Ok(Literal::Num(*n)),
            Some(Token::Bool(b)) => Ok(Literal::Bool(*b)),
            other => Err(ModelError::Selector(format!(
                "expected a literal, found {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> SelectorContext {
        let mut ctx = SelectorContext::new();
        ctx.insert(
            "env".to_string(),
            json!({"name": "prod", "owner_team": "platform"}),
        );
        ctx.insert(
            "dc".to_string(),
            json!({"name": "prod-use1", "status": "active"}),
        );
        ctx
    }

    #[test]
    fn parses_and_evaluates_simple_equality() {
        let sel = Selector::parse("dc.status == 'active'").unwrap();
        assert!(sel.evaluate(&ctx()));
        let sel = Selector::parse("dc.status == 'draining'").unwrap();
        assert!(!sel.evaluate(&ctx()));
    }

    #[test]
    fn parses_and_evaluates_in_list() {
        let sel = Selector::parse("env.name in ['prod', 'stg']").unwrap();
        assert!(sel.evaluate(&ctx()));
        let sel = Selector::parse("env.name in ['stg', 'dev']").unwrap();
        assert!(!sel.evaluate(&ctx()));
    }

    #[test]
    fn parses_and_and_or_and_not_with_correct_precedence() {
        // `&&` binds tighter than `||`.
        let sel = Selector::parse(
            "dc.status == 'draining' || env.name == 'prod' && dc.status == 'active'",
        )
        .unwrap();
        assert!(sel.evaluate(&ctx()));

        let sel = Selector::parse("!(dc.status == 'draining')").unwrap();
        assert!(sel.evaluate(&ctx()));
    }

    #[test]
    fn missing_dimension_type_or_field_is_false_not_an_error() {
        let sel = Selector::parse("service.tier == 'edge'").unwrap();
        assert!(!sel.evaluate(&ctx()));
        let sel = Selector::parse("env.missing_field == 'x'").unwrap();
        assert!(!sel.evaluate(&ctx()));
    }

    #[test]
    fn rejects_malformed_expressions() {
        assert!(Selector::parse("dc.status ==").is_err());
        assert!(Selector::parse("dc.status active").is_err());
        assert!(Selector::parse("dc.status == 'active").is_err()); // unterminated string
    }

    #[test]
    fn binding_matches_respects_exclude_regardless_of_selector() {
        use cubtera_kernel::{DimRef, Ident, InstanceId};

        let id = InstanceId::try_new(
            Ident::parse("cubtera").unwrap(),
            Ident::parse("network").unwrap(),
            [DimRef::parse("dc:prod-use1").unwrap()],
            [],
        )
        .unwrap();

        let binding = Binding {
            id: "b1".to_string(),
            unit: Ident::parse("network").unwrap(),
            selector: Selector::parse("dc.status == 'active'").unwrap(),
            exclude: vec![id.clone()],
            wave: 0,
        };
        assert!(!binding.matches(&id, &ctx()));

        let binding_no_exclude = Binding {
            exclude: vec![],
            ..binding
        };
        assert!(binding_no_exclude.matches(&id, &ctx()));
    }

    #[test]
    fn all_selector_matches_every_context() {
        assert!(Selector::All.evaluate(&SelectorContext::new()));
    }
}
