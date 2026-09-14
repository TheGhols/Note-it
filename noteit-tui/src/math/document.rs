//! A note, evaluated top to bottom.
//!
//! Top-down is the entire dependency model. A variable exists from the line
//! that declares it onwards and nowhere else, which makes `= preco * 2` above
//! `preco := 100` an unknown variable rather than a puzzle, and makes cycles
//! impossible without a graph resolver to prevent them.

use super::errors::MathError;
use super::evaluate::{evaluate, Scope};
use super::format::format_number;
use super::lexer::is_valid_name;
use super::parser::{is_literal, parse, Node};
use std::collections::HashMap;

/// A line of a note, as the engine sees it.
///
/// `None` means "this line can never be a calculation" — it is inside a code
/// block, it carries an inline-code span, it is a heading, a list item, a
/// comment. The engine still receives it, because it has to know a line was
/// there: an opaque line breaks a block of values the same way prose does.
pub type Source<'a> = Option<&'a str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Calculation,
    Declaration,
    Prose,
}

impl LineKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Calculation => "calculation",
            Self::Declaration => "declaration",
            Self::Prose => "prose",
        }
    }
}

/// What, if anything, gets drawn beside a line.
#[derive(Debug, Clone, PartialEq)]
pub enum LineResult {
    None,
    Value { value: f64, text: String },
    Error(MathError),
}

/// A calculation: a line beginning with `=`.
///
/// The marker is required, and that is the whole point. Without it every
/// "2 + 2 = ?" written in prose, every date and every version number would be a
/// candidate. `==` is excluded so a line of emphasis is never a calculation.
fn calculation_of(source: &str) -> Option<&str> {
    let trimmed = source.trim_start_matches([' ', '\t']);
    let rest = trimmed.strip_prefix('=')?;
    if rest.starts_with('=') {
        return None;
    }
    Some(rest)
}

/// A declaration: one bare token, `:=`, and an expression.
///
/// `:=` rather than `=` because `=` already means "calculate this", and because
/// a line of ordinary prose almost never contains `:=`. The name is captured as
/// whatever was written before the operator, so an unusable one is reported
/// rather than silently read as prose — the reader typed `:=` and meant it.
fn declaration_of(source: &str) -> Option<(&str, &str)> {
    let trimmed = source.trim_start_matches([' ', '\t']);
    let operator = trimmed.find(":=")?;
    let name = trimmed[..operator].trim_end_matches([' ', '\t']);
    if name.is_empty() || name.contains([' ', '\t', ':', '=']) {
        return None;
    }
    Some((name, &trimmed[operator + 2..]))
}

pub fn classify_line(source: Source) -> LineKind {
    let Some(source) = source else {
        return LineKind::Prose;
    };
    if calculation_of(source).is_some() {
        return LineKind::Calculation;
    }
    if declaration_of(source).is_some() {
        return LineKind::Declaration;
    }
    LineKind::Prose
}

/// A result, spelled for the reader.
///
/// A conversion carries its target unit, because `10000` on its own answers a
/// different question from `10000 m`. Everything else is a bare number. The
/// unit is the table's own symbol and never anything the note wrote.
fn value_of(value: f64, node: &Node) -> LineResult {
    let Node::Conversion { to, .. } = node else {
        return LineResult::Value {
            value,
            text: format_number(value),
        };
    };
    let unit = match to.plural {
        Some(plural) if value.abs() != 1.0 => plural,
        _ => to.symbol,
    };
    LineResult::Value {
        value,
        text: format!("{} {unit}", format_number(value)),
    }
}

/// Evaluates a whole note and returns one result per line.
///
/// Variables are note-wide and cross anything — headings, code blocks, lists.
/// The only thing contiguity decides is which values an aggregator adds up.
pub fn evaluate_note(lines: &[Source]) -> Vec<LineResult> {
    let mut variables: HashMap<String, f64> = HashMap::new();
    let mut results = Vec::with_capacity(lines.len());

    // The run of consecutive calculation lines directly above the current one,
    // which is what `sum`, `avg` and `count` operate on.
    //
    // Only `= …` lines that produced a value are in it. Prose, a heading, a
    // failed calculation or a declaration of anything but an aggregate all end
    // the run, so a number sitting in a sentence is never added to anything.
    let mut run: Vec<f64> = Vec::new();

    // Whether the line just above was an aggregator.
    //
    // An aggregator reads the block and leaves it where it is, so three of them
    // stacked under one block each answer about that same block. A value under
    // a `= sum` starts a new one, which keeps two totals two totals.
    let mut after_aggregate = false;

    for source in lines {
        let kind = classify_line(*source);

        let Some(text) = source else {
            results.push(LineResult::None);
            run.clear();
            after_aggregate = false;
            continue;
        };

        match kind {
            LineKind::Prose => {
                results.push(LineResult::None);
                run.clear();
                after_aggregate = false;
            }
            LineKind::Calculation => {
                let expression = calculation_of(text).unwrap_or_default();
                let parsed = parse(expression);
                let (result, node) = match parsed {
                    Ok(node) => {
                        let scope = Scope {
                            variables: &variables,
                            samples: &run,
                        };
                        match evaluate(&node, &scope) {
                            Ok(value) => (value_of(value, &node), Some(node)),
                            Err(error) => (LineResult::Error(error), Some(node)),
                        }
                    }
                    Err(error) => (LineResult::Error(error), None),
                };

                let is_aggregate = matches!(node, Some(Node::Aggregate(_)));
                let is_conversion = matches!(node, Some(Node::Conversion { .. }));
                let value = match &result {
                    LineResult::Value { value, .. } => Some(*value),
                    _ => None,
                };
                results.push(result);

                if is_aggregate {
                    after_aggregate = true;
                } else if is_conversion {
                    // A converted quantity ends the block. `sum`, `avg` and
                    // `count` add up plain numbers and know nothing about
                    // units, so letting `10 km em m` into a block would total
                    // ten thousand of something against five of something else
                    // and present the answer as a fact.
                    run.clear();
                    after_aggregate = false;
                } else if let Some(value) = value {
                    if after_aggregate {
                        run = vec![value];
                        after_aggregate = false;
                    } else {
                        run.push(value);
                    }
                } else {
                    run.clear();
                    after_aggregate = false;
                }
            }
            LineKind::Declaration => {
                let (name, expression) = declaration_of(text).expect("classified as one");
                let samples = run.clone();

                if !is_valid_name(name) {
                    // The name is unusable, so whatever it used to hold is not
                    // this. Keeping the old value would answer with a variable
                    // whose definition the reader can no longer see.
                    variables.remove(name);
                    results.push(LineResult::Error(MathError::InvalidName));
                    run.clear();
                    after_aggregate = false;
                    continue;
                }

                let declared = parse(expression).and_then(|node| {
                    let scope = Scope {
                        variables: &variables,
                        samples: &samples,
                    };
                    evaluate(&node, &scope).map(|value| (node, value))
                });

                match declared {
                    Ok((node, value)) => {
                        // A variable holds a number and only a number.
                        // `metros := 10 km em m` stores 10000, not "10000
                        // metres": carrying a unit through a variable would
                        // mean every value becoming a quantity, and with it
                        // percentages, aggregation and every existing rule.
                        variables.insert(name.to_owned(), value);
                        // The value of `preco := 120` is already on the line.
                        // Only a declaration that computes something gets a
                        // result drawn beside it.
                        results.push(if is_literal(&node) {
                            LineResult::None
                        } else {
                            value_of(value, &node)
                        });

                        // A declaration reads the block above it the way
                        // `= sum` does, and leaves it alone the same way;
                        // anything else it declares ends the block.
                        if matches!(node, Node::Aggregate(_)) {
                            after_aggregate = true;
                        } else {
                            run.clear();
                            after_aggregate = false;
                        }
                    }
                    Err(error) => {
                        // A declaration that failed declares nothing.
                        // Everything below that used the name reports an
                        // unknown variable, which is the truth.
                        variables.remove(name);
                        results.push(LineResult::Error(error));
                        run.clear();
                        after_aggregate = false;
                    }
                }
            }
        }
    }

    results
}
