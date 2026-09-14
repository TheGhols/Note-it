//! How a result is spelled for the reader.
//!
//! The value the engine carries and the value it shows are two different
//! things. Chained calculations use the full binary double — rounding a
//! variable to make it look nice would make everything below it wrong — and
//! only what is drawn beside the line goes through here.

/// Enough to be exact for anything a note computes, few enough to hide the
/// noise binary floating point leaves behind: `0.1 + 0.2` shows as `0,3`.
const SIGNIFICANT_DIGITS: usize = 12;

/// A result is a number to read, not a measurement to report.
const MAX_DECIMALS: usize = 10;

/// Above this, the graphical engine's `toFixed` switches to exponent form.
const FIXED_LIMIT: f64 = 1e21;

/// Rounds to `digits` significant digits, the way `Number.prototype.toPrecision`
/// does, by formatting and reading back.
fn to_precision(value: f64, digits: usize) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    format!("{:.*e}", digits - 1, value)
        .parse::<f64>()
        .unwrap_or(value)
}

/// A very large number, spelled the way the graphical engine spells it.
///
/// Above `FIXED_LIMIT` the canonical implementation falls back to the
/// platform's own number-to-string, which switches to exponent form and writes
/// the sign of the exponent: `1e+40`, not `1e40` and not the full forty-one
/// digits. Rust's `{:e}` agrees about the mantissa and omits the `+`, so that
/// is the whole of the difference — and it is the kind of difference only a
/// cross-conformance fixture finds, because both spellings are "right" and
/// only one of them matches the other window.
fn exponential(value: f64) -> String {
    let formatted = format!("{value:e}");
    match formatted.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') => {
            format!("{mantissa}e+{exponent}")
        }
        _ => formatted,
    }
}

/// Formats a result in pt-BR, with a comma for the decimal separator and no
/// thousands separator at all.
///
/// The missing grouping is deliberate. `.` and `,` are both accepted as decimal
/// separators when reading a number, so printing `109.876.463` would produce a
/// result this same engine reads back as something else. A result you can copy
/// into the next line is worth more than one that is easier on the eye.
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "—".to_owned();
    }

    let precise = to_precision(value, SIGNIFICANT_DIGITS);
    let normalized = if precise == 0.0 { 0.0 } else { precise };

    if normalized.abs() >= FIXED_LIMIT {
        return exponential(normalized).replace('.', ",");
    }

    if normalized.fract() == 0.0 && normalized.abs() < 1e15 {
        return format!("{normalized:.0}");
    }

    let mut text = format!("{normalized:.*}", MAX_DECIMALS);
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text.replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_floating_point_noise_is_hidden() {
        assert_eq!(format_number(0.1 + 0.2), "0,3");
    }

    #[test]
    fn an_integer_has_no_decimal_part() {
        assert_eq!(format_number(4.0), "4");
        assert_eq!(format_number(-3.0), "-3");
    }

    #[test]
    fn a_result_uses_a_comma_and_never_a_thousands_separator() {
        assert_eq!(format_number(2.5), "2,5");
        assert_eq!(format_number(10000.0), "10000");
    }

    #[test]
    fn a_very_large_number_uses_the_exponent_form_the_other_window_uses() {
        assert_eq!(format_number(1e40), "1e+40");
        assert_eq!(format_number(-1e40), "-1e+40");
        assert_eq!(format_number(1.5e22), "1,5e+22");
    }

    #[test]
    fn negative_zero_is_zero() {
        assert_eq!(format_number(-0.0), "0");
    }
}
