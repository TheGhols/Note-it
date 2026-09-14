//! Every unit Note-it knows, and every spelling it answers to.
//!
//! A table rather than a chain of conditionals, for the reason every
//! conversion library eventually learns: `if km then m, if m then cm, …` is
//! O(n²) rules to write and O(n²) rules to get wrong, while a scale per unit is
//! one number per row and the conversion is arithmetic.
//!
//! **Every factor here is exact**, and the ones that look arbitrary are
//! definitions rather than measurements: an inch is 0.0254 m by international
//! agreement, a pound is 453.59237 g, a mile is 1609.344 m.
//!
//! What is deliberately absent matters as much. There is no `xícara`, no
//! `colher`, no `alqueire`: each has more than one real value, and a conversion
//! whose answer depends on which definition the reader had in mind is worse
//! than no conversion, because it is wrong silently. Currencies are absent for
//! a different reason — a rate has no answer without a network and is stale the
//! moment it is written down — and adding one to this table is the change that
//! must not be made quietly.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Length,
    Mass,
    Volume,
    Temperature,
    Time,
    Area,
    Digital,
    Speed,
}

/// How a unit converts to its dimension's base.
///
/// Most are linear. Temperature is not — its scales have different zeroes, so
/// `0 °C` is `32 °F` however hard you multiply — and those rows carry a pair of
/// conversions instead of a factor.
#[derive(Debug, Clone, Copy)]
pub enum Scale {
    Linear(f64),
    Celsius,
    Fahrenheit,
    Kelvin,
}

#[derive(Debug, Clone, Copy)]
pub struct Unit {
    pub id: &'static str,
    pub symbol: &'static str,
    /// The spelling used when the displayed value is not exactly one.
    pub plural: Option<&'static str>,
    pub dimension: Dimension,
    pub scale: Scale,
    pub aliases: &'static [&'static str],
}

use Dimension::*;
use Scale::Linear;

/// The base of each dimension is the row whose factor is 1: `m`, `g`, `mL`,
/// `K`, `s`, `m²`, `B`, `m/s`. Mass is based on the gram rather than the SI
/// kilogram so that `mg` is `0.001` and not `0.000001`.
pub const UNITS: &[Unit] = &[
    // length
    u(
        "mm",
        "mm",
        Length,
        Linear(0.001),
        &["milimetro", "milimetros"],
    ),
    u(
        "cm",
        "cm",
        Length,
        Linear(0.01),
        &["centimetro", "centimetros"],
    ),
    u("m", "m", Length, Linear(1.0), &["metro", "metros"]),
    u(
        "km",
        "km",
        Length,
        Linear(1000.0),
        &["quilometro", "quilometros"],
    ),
    u(
        "in",
        "in",
        Length,
        Linear(0.0254),
        &["polegada", "polegadas"],
    ),
    u("ft", "ft", Length, Linear(0.3048), &["pe", "pes"]),
    u("yd", "yd", Length, Linear(0.9144), &["jarda", "jardas"]),
    u("mi", "mi", Length, Linear(1609.344), &["milha", "milhas"]),
    // mass
    u(
        "mg",
        "mg",
        Mass,
        Linear(0.001),
        &["miligrama", "miligramas"],
    ),
    u("g", "g", Mass, Linear(1.0), &["grama", "gramas"]),
    u(
        "kg",
        "kg",
        Mass,
        Linear(1000.0),
        &["quilograma", "quilogramas", "quilo", "quilos"],
    ),
    u(
        "t",
        "t",
        Mass,
        Linear(1_000_000.0),
        &["tonelada", "toneladas"],
    ),
    u("oz", "oz", Mass, Linear(28.349523125), &["onca", "oncas"]),
    u("lb", "lb", Mass, Linear(453.59237), &["libra", "libras"]),
    // volume
    u(
        "mL",
        "mL",
        Volume,
        Linear(1.0),
        &["ml", "mililitro", "mililitros"],
    ),
    u(
        "cL",
        "cL",
        Volume,
        Linear(10.0),
        &["cl", "centilitro", "centilitros"],
    ),
    u(
        "dL",
        "dL",
        Volume,
        Linear(100.0),
        &["dl", "decilitro", "decilitros"],
    ),
    u("L", "L", Volume, Linear(1000.0), &["l", "litro", "litros"]),
    u("cm³", "cm³", Volume, Linear(1.0), &["cm3", "cc"]),
    u("m³", "m³", Volume, Linear(1_000_000.0), &["m3"]),
    // temperature
    u(
        "°C",
        "°C",
        Temperature,
        Scale::Celsius,
        &["C", "c", "celsius"],
    ),
    u(
        "°F",
        "°F",
        Temperature,
        Scale::Fahrenheit,
        &["F", "f", "fahrenheit"],
    ),
    u("K", "K", Temperature, Scale::Kelvin, &["kelvin"]),
    // time
    u(
        "ms",
        "ms",
        Time,
        Linear(0.001),
        &["milissegundo", "milissegundos"],
    ),
    u("s", "s", Time, Linear(1.0), &["seg", "segundo", "segundos"]),
    u("min", "min", Time, Linear(60.0), &["minuto", "minutos"]),
    u("h", "h", Time, Linear(3600.0), &["hora", "horas"]),
    Unit {
        id: "dia",
        symbol: "dia",
        plural: Some("dias"),
        dimension: Time,
        scale: Linear(86_400.0),
        aliases: &["dias", "d"],
    },
    Unit {
        id: "semana",
        symbol: "semana",
        plural: Some("semanas"),
        dimension: Time,
        scale: Linear(604_800.0),
        aliases: &["semanas"],
    },
    // area
    u("mm²", "mm²", Area, Linear(0.000001), &["mm2"]),
    u("cm²", "cm²", Area, Linear(0.0001), &["cm2"]),
    u("m²", "m²", Area, Linear(1.0), &["m2"]),
    u("km²", "km²", Area, Linear(1_000_000.0), &["km2"]),
    u("ha", "ha", Area, Linear(10_000.0), &["hectare", "hectares"]),
    // digital — SI prefixes are decimal and IEC prefixes are binary, and
    // Note-it does not blur them: `KB` is 1000 bytes and `KiB` is 1024, always.
    u("B", "B", Digital, Linear(1.0), &["byte", "bytes"]),
    u("KB", "KB", Digital, Linear(1000.0), &[]),
    u("MB", "MB", Digital, Linear(1_000_000.0), &[]),
    u("GB", "GB", Digital, Linear(1_000_000_000.0), &[]),
    u("TB", "TB", Digital, Linear(1_000_000_000_000.0), &[]),
    u("KiB", "KiB", Digital, Linear(1024.0), &[]),
    u("MiB", "MiB", Digital, Linear(1_048_576.0), &[]),
    u("GiB", "GiB", Digital, Linear(1_073_741_824.0), &[]),
    u("TiB", "TiB", Digital, Linear(1_099_511_627_776.0), &[]),
    // speed — three named rows, not `length / time` worked out at run time.
    u("m/s", "m/s", Speed, Linear(1.0), &[]),
    u("km/h", "km/h", Speed, Linear(1.0 / 3.6), &[]),
    u("mph", "mph", Speed, Linear(0.44704), &[]),
];

const fn u(
    id: &'static str,
    symbol: &'static str,
    dimension: Dimension,
    scale: Scale,
    aliases: &'static [&'static str],
) -> Unit {
    Unit {
        id,
        symbol,
        plural: None,
        dimension,
        scale,
        aliases,
    }
}

/// The unit written as `text`, or `None` when nothing in the table is spelled
/// that way.
///
/// Lookup is **exact and case-sensitive**. `m` is a metre and `M` is nothing;
/// `mL` and `ml` are both the millilitre because both are listed, and `mb` is
/// not the megabyte because it is not. There is no case folding and no
/// normalisation anywhere, which is what makes "is this a unit?" a question
/// with one answer.
pub fn find_unit(text: &str) -> Option<&'static Unit> {
    UNITS
        .iter()
        .find(|unit| unit.id == text || unit.aliases.contains(&text))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionFailure {
    Incompatible,
    Impossible,
}

/// Absolute zero, the floor of the temperature dimension.
const ABSOLUTE_ZERO_KELVIN: f64 = 0.0;

fn to_base(unit: &Unit, value: f64) -> f64 {
    match unit.scale {
        Linear(factor) => value * factor,
        Scale::Celsius => value + 273.15,
        Scale::Fahrenheit => (value + 459.67) * 5.0 / 9.0,
        Scale::Kelvin => value,
    }
}

fn from_base(unit: &Unit, base: f64) -> f64 {
    match unit.scale {
        Linear(factor) => base / factor,
        Scale::Celsius => base - 273.15,
        Scale::Fahrenheit => base * 9.0 / 5.0 - 459.67,
        Scale::Kelvin => base,
    }
}

/// Converts a value from one unit to another.
///
/// Two units, one base, and arithmetic. Nothing here looks anything up by a
/// string the note supplied: the units arrive already resolved from the table.
pub fn convert(value: f64, from: &Unit, to: &Unit) -> Result<f64, ConversionFailure> {
    // Dimensions are static, so this is decided before any arithmetic happens.
    // A kilogram is not a kilometre and no amount of context makes it one.
    if from.dimension != to.dimension {
        return Err(ConversionFailure::Incompatible);
    }

    let base = to_base(from, value);

    // A temperature under absolute zero is not a reading with an unusual sign;
    // it is not a temperature. Converting it would hand the reader a number
    // that cannot exist, dressed as an answer.
    if from.dimension == Temperature && base < ABSOLUTE_ZERO_KELVIN {
        return Err(ConversionFailure::Impossible);
    }

    let converted = from_base(to, base);
    if !converted.is_finite() {
        return Err(ConversionFailure::Impossible);
    }
    Ok(if converted == 0.0 { 0.0 } else { converted })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_belongs_to_exactly_one_unit() {
        // The graphical registry refuses to build when two rows claim the same
        // spelling; this is the same check, made once.
        let mut seen: Vec<&str> = Vec::new();
        for unit in UNITS {
            for spelling in std::iter::once(&unit.id).chain(unit.aliases.iter()) {
                assert!(!seen.contains(spelling), "{spelling:?} is claimed twice");
                seen.push(spelling);
            }
        }
    }

    #[test]
    fn no_currency_and_no_ambiguous_kitchen_measure_is_present() {
        for absent in [
            "USD", "BRL", "EUR", "xicara", "colher", "cup", "tsp", "alqueire",
        ] {
            assert!(find_unit(absent).is_none(), "{absent} must not be a unit");
        }
    }
}
