//! The smallest JSON reader the conformance fixtures need.
//!
//! Hand-written rather than pulled in with a dependency: the fixtures are
//! objects of arrays of strings and numbers, the TUI takes no serialisation
//! crate, and adding one so that a test can read a file would put it in the
//! shipped binary's dependency tree.

#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(fields) => fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn array(&self) -> &[Value] {
        match self {
            Value::Array(items) => items,
            _ => &[],
        }
    }

    pub fn string(&self) -> Option<&str> {
        match self {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Number(value) => Some(*value),
            _ => None,
        }
    }
}

pub fn parse(text: &str) -> Value {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    parse_value(&chars, &mut index)
}

fn skip_whitespace(chars: &[char], index: &mut usize) {
    while *index < chars.len() && chars[*index].is_whitespace() {
        *index += 1;
    }
}

fn parse_value(chars: &[char], index: &mut usize) -> Value {
    skip_whitespace(chars, index);
    match chars.get(*index) {
        Some('{') => parse_object(chars, index),
        Some('[') => parse_array(chars, index),
        Some('"') => Value::String(parse_string(chars, index)),
        Some('t') => {
            *index += 4;
            Value::Bool(true)
        }
        Some('f') => {
            *index += 5;
            Value::Bool(false)
        }
        Some('n') => {
            *index += 4;
            Value::Null
        }
        _ => parse_number(chars, index),
    }
}

fn parse_object(chars: &[char], index: &mut usize) -> Value {
    let mut fields = Vec::new();
    *index += 1;
    loop {
        skip_whitespace(chars, index);
        if chars.get(*index) == Some(&'}') {
            *index += 1;
            return Value::Object(fields);
        }
        let key = parse_string(chars, index);
        skip_whitespace(chars, index);
        *index += 1;
        let value = parse_value(chars, index);
        fields.push((key, value));
        skip_whitespace(chars, index);
        if chars.get(*index) == Some(&',') {
            *index += 1;
        }
    }
}

fn parse_array(chars: &[char], index: &mut usize) -> Value {
    let mut items = Vec::new();
    *index += 1;
    loop {
        skip_whitespace(chars, index);
        if chars.get(*index) == Some(&']') {
            *index += 1;
            return Value::Array(items);
        }
        items.push(parse_value(chars, index));
        skip_whitespace(chars, index);
        if chars.get(*index) == Some(&',') {
            *index += 1;
        }
    }
}

fn parse_string(chars: &[char], index: &mut usize) -> String {
    let mut text = String::new();
    *index += 1;
    while let Some(&character) = chars.get(*index) {
        *index += 1;
        match character {
            '"' => return text,
            '\\' => {
                let escaped = chars.get(*index).copied().unwrap_or('"');
                *index += 1;
                text.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'u' => {
                        let hex: String = chars[*index..*index + 4].iter().collect();
                        *index += 4;
                        char::from_u32(u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD))
                            .unwrap_or('\u{FFFD}')
                    }
                    other => other,
                });
            }
            other => text.push(other),
        }
    }
    text
}

fn parse_number(chars: &[char], index: &mut usize) -> Value {
    let start = *index;
    while chars
        .get(*index)
        .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E'))
    {
        *index += 1;
    }
    let text: String = chars[start..*index].iter().collect();
    Value::Number(text.parse().unwrap_or(f64::NAN))
}
