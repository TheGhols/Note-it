//! The whole vocabulary of the engine.
//!
//! There is no token for anything that could reach a runtime value: no dot, no
//! bracket, no string, no call syntax. An expression is a sequence of the
//! shapes below and nothing else.

use super::errors::MathError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Number,
    Identifier,
    De,
    Em,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenType,
    /// Numbers carry their value; identifiers carry their name.
    pub value: f64,
    pub text: String,
}

/// The names the grammar keeps for itself, matched without regard to case.
pub const RESERVED_NAMES: &[&str] = &["de", "em", "sum", "avg", "count"];
pub const AGGREGATE_NAMES: &[&str] = &["sum", "avg", "count"];

pub fn is_reserved_name(name: &str) -> bool {
    RESERVED_NAMES.contains(&name.to_lowercase().as_str())
}

pub fn is_aggregate_name(name: &str) -> bool {
    AGGREGATE_NAMES.contains(&name.to_lowercase().as_str())
}

/// A name a declaration may use: ASCII letters, digits and `_`, never starting
/// with a digit.
///
/// ASCII only, on purpose. Accepting Unicode would mean deciding whether two
/// visually identical names are the same variable, and every answer to that is
/// either a normalisation policy nobody asked for or a note where two names
/// quietly disagree.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_') && !is_reserved_name(name)
}

/// Ceilings so that a hostile or accidental paste costs a fixed amount.
pub const MAX_EXPRESSION_LENGTH: usize = 1000;
pub const MAX_TOKENS: usize = 512;

/// What a name may start with. `°` is here so `°C` and `°F` can be typed as
/// they are written elsewhere; it is not a step towards Unicode identifiers.
fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '°'
}

/// What a name may continue with. `²` and `³` close `m²` and `cm³`.
fn is_name_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '²' || c == '³'
}

/// Reads a number, and refuses the one spelling that could mean two things.
///
/// `.` is the canonical decimal separator and `,` is accepted as one, because
/// a pt-BR keyboard writes `10,5` without thinking about it. A *second*
/// separator is never accepted: `1.234.567` is thousands-grouped in one
/// convention and nonsense in the other, and guessing between them is exactly
/// the silent reinterpretation this engine must not do.
fn read_number(chars: &[char], start: usize) -> Result<(Token, usize), MathError> {
    let mut index = start;
    while index < chars.len() && chars[index].is_ascii_digit() {
        index += 1;
    }
    let mut text: String = chars[start..index].iter().collect();

    let separator = chars.get(index).copied();
    let next_is_digit = chars
        .get(index + 1)
        .copied()
        .is_some_and(|c| c.is_ascii_digit());
    if matches!(separator, Some('.') | Some(',')) && next_is_digit {
        index += 1;
        let fraction_start = index;
        while index < chars.len() && chars[index].is_ascii_digit() {
            index += 1;
        }
        text.push('.');
        text.extend(chars[fraction_start..index].iter());
    }

    // A separator still ahead is a second one: `1.234.567`, or `1,5.2`.
    let trailing = chars.get(index).copied();
    let trailing_is_digit = chars
        .get(index + 1)
        .copied()
        .is_some_and(|c| c.is_ascii_digit());
    if matches!(trailing, Some('.') | Some(',')) && trailing_is_digit {
        return Err(MathError::InvalidExpression);
    }

    let value = text
        .parse::<f64>()
        .map_err(|_| MathError::InvalidExpression)?;
    Ok((
        Token {
            kind: TokenType::Number,
            value,
            text,
        },
        index,
    ))
}

/// Turns an expression into tokens, or refuses it.
///
/// Every character has to be one the grammar knows. There is no "skip what I
/// do not understand" path, because that is how a parser starts accepting
/// things nobody designed.
pub fn tokenize(source: &str) -> Result<Vec<Token>, MathError> {
    if source.chars().count() > MAX_EXPRESSION_LENGTH {
        return Err(MathError::InvalidExpression);
    }

    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        let character = chars[index];

        if character == ' ' || character == '\t' {
            index += 1;
            continue;
        }
        if tokens.len() >= MAX_TOKENS {
            return Err(MathError::InvalidExpression);
        }

        let single = match character {
            '+' => Some(TokenType::Plus),
            '-' => Some(TokenType::Minus),
            '*' => Some(TokenType::Star),
            '/' => Some(TokenType::Slash),
            '%' => Some(TokenType::Percent),
            '(' => Some(TokenType::LParen),
            ')' => Some(TokenType::RParen),
            _ => None,
        };
        if let Some(kind) = single {
            tokens.push(Token {
                kind,
                value: 0.0,
                text: character.to_string(),
            });
            index += 1;
            continue;
        }

        if character.is_ascii_digit() {
            let (token, next) = read_number(&chars, index)?;
            tokens.push(token);
            index = next;
            continue;
        }

        if is_name_start(character) {
            let start = index;
            // The opening character is consumed on its own, because `°` may
            // start a name without being able to continue one.
            index += 1;
            while index < chars.len() && is_name_part(chars[index]) {
                index += 1;
            }
            let name: String = chars[start..index].iter().collect();
            let keyword = name.to_lowercase();
            let kind = match keyword.as_str() {
                "de" => TokenType::De,
                "em" => TokenType::Em,
                _ => TokenType::Identifier,
            };
            tokens.push(Token {
                kind,
                value: 0.0,
                text: name,
            });
            continue;
        }

        return Err(MathError::InvalidExpression);
    }

    Ok(tokens)
}
