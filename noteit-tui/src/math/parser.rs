//! The grammar, written out once.
//!
//! ```text
//! line           := aggregate | conversion | expression
//! conversion     := expression unitRef 'em' unitRef
//! unitRef        := name ('/' name)?
//!
//! expression     := additive
//! additive       := multiplicative (('+' | '-') multiplicative)*
//! multiplicative := unary (('*' | '/' | 'de') unary)*
//! unary          := ('-' | '+') unary | postfix
//! postfix        := primary '%'*
//! primary        := number | name | '(' expression ')'
//! ```
//!
//! `de` sits at the multiplicative level and is accepted only when what stands
//! to its left is a percentage — `10% de 200` reads, `200 de 10` does not.
//!
//! `em` sits at the line level, and that placement is what makes conversion
//! cost the expression grammar nothing: the expression parser runs first and
//! stops of its own accord at the source unit, so whatever it leaves behind is
//! where the units are read from. The unit therefore applies to the **whole**
//! left-hand expression: `= 10 + 5 km em m` is fifteen kilometres.

use super::errors::MathError;
use super::lexer::{is_aggregate_name, tokenize, Token, TokenType};
use super::units::{find_unit, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    Sum,
    Avg,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    /// `X% de Y`, the percentage preposition.
    Of,
}

#[derive(Debug, Clone)]
pub enum Node {
    Number(f64),
    Variable(String),
    Percent(Box<Node>),
    Negate(Box<Node>),
    Binary {
        operator: BinaryOp,
        left: Box<Node>,
        right: Box<Node>,
    },
    Aggregate(Aggregate),
    Conversion {
        operand: Box<Node>,
        from: &'static Unit,
        to: &'static Unit,
    },
}

/// Deep enough for any expression a person writes, shallow enough to be safe.
pub const MAX_DEPTH: usize = 64;

pub fn parse(source: &str) -> Result<Node, MathError> {
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Err(MathError::InvalidExpression);
    }

    // An aggregator is the whole expression or it is not an expression.
    // Allowing `sum * 2` would mean deciding what the aggregated set is when
    // the line also does arithmetic, and there is no obvious reading of that.
    if tokens.len() == 1 && tokens[0].kind == TokenType::Identifier {
        let name = tokens[0].text.to_lowercase();
        if let Some(aggregate) = aggregate_of(&name) {
            return Ok(Node::Aggregate(aggregate));
        }
    }

    let mut parser = Parser { tokens, index: 0 };
    let node = parser.expression(0)?;
    if parser.at_end() {
        return Ok(node);
    }
    parser.conversion_tail(node)
}

fn aggregate_of(name: &str) -> Option<Aggregate> {
    match name {
        "sum" => Some(Aggregate::Sum),
        "avg" => Some(Aggregate::Avg),
        "count" => Some(Aggregate::Count),
        _ => None,
    }
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn peek_kind(&self) -> Option<TokenType> {
        self.peek().map(|token| token.kind)
    }

    fn take(&mut self) -> Result<Token, MathError> {
        let token = self
            .tokens
            .get(self.index)
            .cloned()
            .ok_or(MathError::InvalidExpression)?;
        self.index += 1;
        Ok(token)
    }

    fn at_end(&self) -> bool {
        self.index == self.tokens.len()
    }

    /// Reads `unitRef 'em' unitRef` off the end of a line whose expression is
    /// already parsed, and resolves both units.
    ///
    /// Resolution happens here rather than during evaluation: a dimension is a
    /// static property of a spelling, so `= 10 kg em km` cannot become valid
    /// for some value of the expression.
    fn conversion_tail(&mut self, operand: Node) -> Result<Node, MathError> {
        let from = self.unit_ref()?;

        // Anything other than `em` here is trailing text the grammar has no
        // rule for — `= 10 km` among them. There is no conversion without a
        // target, and guessing one would be inventing the reader's intent.
        if self.peek_kind() != Some(TokenType::Em) {
            return Err(MathError::InvalidExpression);
        }
        self.index += 1;

        let to = self.unit_ref()?;
        if !self.at_end() {
            return Err(MathError::InvalidExpression);
        }
        if from.dimension != to.dimension {
            return Err(MathError::IncompatibleUnits);
        }
        Ok(Node::Conversion {
            operand: Box::new(operand),
            from,
            to,
        })
    }

    /// One unit reference: a name, optionally over another name.
    ///
    /// The `/` form exists for `km/h` and `m/s`, which are single rows rather
    /// than a length divided by a time. The slash is only consumed when a name
    /// follows it, so a division that happens to sit where a unit could has
    /// already been taken by the expression parser.
    fn unit_ref(&mut self) -> Result<&'static Unit, MathError> {
        let first = self.peek().ok_or(MathError::InvalidExpression)?;
        if first.kind != TokenType::Identifier {
            return Err(MathError::InvalidExpression);
        }
        let mut text = first.text.clone();
        self.index += 1;

        if self.peek_kind() == Some(TokenType::Slash)
            && self.tokens.get(self.index + 1).map(|token| token.kind)
                == Some(TokenType::Identifier)
        {
            text = format!("{text}/{}", self.tokens[self.index + 1].text);
            self.index += 2;
        }

        find_unit(&text).ok_or(MathError::UnknownUnit)
    }

    fn expression(&mut self, depth: usize) -> Result<Node, MathError> {
        if depth > MAX_DEPTH {
            return Err(MathError::InvalidExpression);
        }
        self.additive(depth)
    }

    fn additive(&mut self, depth: usize) -> Result<Node, MathError> {
        let mut left = self.multiplicative(depth)?;
        loop {
            let operator = match self.peek_kind() {
                Some(TokenType::Plus) => BinaryOp::Add,
                Some(TokenType::Minus) => BinaryOp::Subtract,
                _ => return Ok(left),
            };
            self.index += 1;
            let right = self.multiplicative(depth)?;
            left = Node::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
    }

    fn multiplicative(&mut self, depth: usize) -> Result<Node, MathError> {
        let mut left = self.unary(depth)?;
        loop {
            match self.peek_kind() {
                Some(TokenType::Star) | Some(TokenType::Slash) => {
                    let operator = if self.peek_kind() == Some(TokenType::Star) {
                        BinaryOp::Multiply
                    } else {
                        BinaryOp::Divide
                    };
                    self.index += 1;
                    let right = self.unary(depth)?;
                    left = Node::Binary {
                        operator,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                Some(TokenType::De) => {
                    // `de` reads "of", and only a percentage is "of" something.
                    if !matches!(left, Node::Percent(_)) {
                        return Err(MathError::InvalidExpression);
                    }
                    self.index += 1;
                    let right = self.unary(depth)?;
                    left = Node::Binary {
                        operator: BinaryOp::Of,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                _ => return Ok(left),
            }
        }
    }

    fn unary(&mut self, depth: usize) -> Result<Node, MathError> {
        match self.peek_kind() {
            Some(TokenType::Minus) => {
                self.index += 1;
                Ok(Node::Negate(Box::new(self.unary(depth + 1)?)))
            }
            Some(TokenType::Plus) => {
                self.index += 1;
                self.unary(depth + 1)
            }
            _ => self.postfix(depth),
        }
    }

    fn postfix(&mut self, depth: usize) -> Result<Node, MathError> {
        let mut node = self.primary(depth)?;
        while self.peek_kind() == Some(TokenType::Percent) {
            self.index += 1;
            node = Node::Percent(Box::new(node));
        }
        Ok(node)
    }

    fn primary(&mut self, depth: usize) -> Result<Node, MathError> {
        let next = self.take()?;
        match next.kind {
            TokenType::Number => Ok(Node::Number(next.value)),
            TokenType::Identifier => {
                // Reached only when an aggregator is used as part of a larger
                // expression, which the whole-expression rule has ruled out.
                if is_aggregate_name(&next.text) {
                    return Err(MathError::InvalidExpression);
                }
                Ok(Node::Variable(next.text))
            }
            TokenType::LParen => {
                let inner = self.expression(depth + 1)?;
                if self.take()?.kind != TokenType::RParen {
                    return Err(MathError::InvalidExpression);
                }
                Ok(inner)
            }
            _ => Err(MathError::InvalidExpression),
        }
    }
}

/// Whether an expression is a bare literal, so its value is already on screen.
///
/// `preco := 120` needs no result beside it; `subtotal := preco * 3` does. The
/// rule is about the shape of what was written and not about the value, so the
/// same line always decides the same way.
pub fn is_literal(node: &Node) -> bool {
    match node {
        Node::Number(_) => true,
        Node::Percent(operand) | Node::Negate(operand) => is_literal(operand),
        _ => false,
    }
}
