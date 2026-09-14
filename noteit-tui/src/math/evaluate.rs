//! Walking the tree and doing arithmetic. Nothing here interprets text.
//!
//! There are seven node shapes and no path from a note to a function call, a
//! property access or a global — those are not nodes this tree can hold. A
//! conversion is no different: its two units were resolved from the table
//! while parsing, so what reaches this function is a factor or a scale, never
//! a string from the note.

use super::errors::MathError;
use super::parser::{Aggregate, BinaryOp, Node};
use super::units::{convert, ConversionFailure};
use std::collections::HashMap;

/// Everything an expression is allowed to see.
///
/// `variables` is a map with no inherited keys, which is a safety property
/// rather than a style choice: a structure that answered `constructor` or
/// `__proto__` with something real would make `= constructor` a way into the
/// runtime. Here an unknown name is unknown whatever it is called.
pub struct Scope<'a> {
    pub variables: &'a HashMap<String, f64>,
    /// The block of values an aggregator on this line operates on.
    pub samples: &'a [f64],
}

pub fn evaluate(node: &Node, scope: &Scope) -> Result<f64, MathError> {
    finite(evaluate_node(node, scope)?)
}

/// Overflow and `0/0` are not results.
///
/// A number the reader cannot use is a failed calculation, not a calculation
/// that produced infinity.
fn finite(value: f64) -> Result<f64, MathError> {
    if !value.is_finite() {
        return Err(MathError::InvalidExpression);
    }
    Ok(if value == 0.0 { 0.0 } else { value })
}

fn evaluate_node(node: &Node, scope: &Scope) -> Result<f64, MathError> {
    match node {
        Node::Number(value) => Ok(*value),
        // A percentage is a hundredth. The contextual readings of `+`, `-` and
        // `de` are applied by their operators, which can see that the operand
        // was written with a `%`; on its own it is just the number it names.
        Node::Percent(operand) => Ok(evaluate_node(operand, scope)? / 100.0),
        Node::Negate(operand) => Ok(-evaluate_node(operand, scope)?),
        Node::Variable(name) => scope
            .variables
            .get(name)
            .copied()
            .ok_or(MathError::UnknownVariable),
        Node::Aggregate(kind) => aggregate(*kind, scope.samples),
        Node::Conversion { operand, from, to } => {
            // Whatever the expression came to is a quantity in the source unit.
            // The dimensions were checked while parsing, so what can still fail
            // is a conversion with no answer rather than one never allowed.
            convert(evaluate_node(operand, scope)?, from, to).map_err(|failure| match failure {
                ConversionFailure::Incompatible => MathError::IncompatibleUnits,
                ConversionFailure::Impossible => MathError::InvalidConversion,
            })
        }
        Node::Binary {
            operator,
            left,
            right,
        } => binary(*operator, left, right, scope),
    }
}

fn binary(operator: BinaryOp, left: &Node, right: &Node, scope: &Scope) -> Result<f64, MathError> {
    let left_value = evaluate_node(left, scope)?;

    // `X% de Y` is the only reading of `de`, and the parser has already refused
    // it for anything but a percentage on the left.
    if operator == BinaryOp::Of {
        return Ok(left_value * evaluate_node(right, scope)?);
    }

    let right_value = evaluate_node(right, scope)?;
    let right_is_percent = matches!(right, Node::Percent(_));

    match operator {
        // `200 + 10%` is a ten percent increase, because that is what the line
        // means to everyone who writes it. The rule is tied to a `%` written
        // right there, not to a value that happens to have come from one: a
        // variable holding `10%` holds `0.1`, and `200 + taxa` adds `0.1`.
        BinaryOp::Add if right_is_percent => Ok(left_value + left_value * right_value),
        BinaryOp::Add => Ok(left_value + right_value),
        BinaryOp::Subtract if right_is_percent => Ok(left_value - left_value * right_value),
        BinaryOp::Subtract => Ok(left_value - right_value),
        BinaryOp::Multiply => Ok(left_value * right_value),
        BinaryOp::Divide => {
            if right_value == 0.0 {
                return Err(MathError::DivisionByZero);
            }
            Ok(left_value / right_value)
        }
        BinaryOp::Of => unreachable!("handled above"),
    }
}

fn aggregate(kind: Aggregate, samples: &[f64]) -> Result<f64, MathError> {
    if kind == Aggregate::Count {
        return Ok(samples.len() as f64);
    }
    let total: f64 = samples.iter().sum();
    if kind == Aggregate::Sum {
        return Ok(total);
    }
    // The average of nothing is `0 / 0`, and saying so is more honest than
    // answering zero.
    if samples.is_empty() {
        return Err(MathError::DivisionByZero);
    }
    Ok(total / samples.len() as f64)
}
