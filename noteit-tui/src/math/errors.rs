//! Everything the engine can refuse to answer, and the words a note shows.
//!
//! Seven codes, and each tells the reader which line is not producing a number
//! and roughly why. Nothing else: no internal detail, no offending token, no
//! stack trace. The strings are constants, so no note content can be echoed
//! back into the document through an error message.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathError {
    InvalidExpression,
    UnknownVariable,
    DivisionByZero,
    InvalidName,
    UnknownUnit,
    IncompatibleUnits,
    InvalidConversion,
}

impl MathError {
    /// The identifier the cross-conformance fixture uses.
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidExpression => "invalid-expression",
            Self::UnknownVariable => "unknown-variable",
            Self::DivisionByZero => "division-by-zero",
            Self::InvalidName => "invalid-name",
            Self::UnknownUnit => "unknown-unit",
            Self::IncompatibleUnits => "incompatible-units",
            Self::InvalidConversion => "invalid-conversion",
        }
    }
}

/// The sentence shown beside the line, in the same words as the graphical
/// editor.
pub fn message_for(error: MathError) -> &'static str {
    match error {
        MathError::InvalidExpression => "expressão inválida",
        MathError::UnknownVariable => "variável desconhecida",
        MathError::DivisionByZero => "divisão por zero",
        MathError::InvalidName => "nome inválido",
        MathError::UnknownUnit => "unidade desconhecida",
        MathError::IncompatibleUnits => "unidades incompatíveis",
        MathError::InvalidConversion => "conversão inválida",
    }
}
