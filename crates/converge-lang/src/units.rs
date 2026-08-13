use crate::ast::Quantity;
use crate::diagnostic::{Diagnostic, Span};

/// The kinds of quantity the checker knows. A unit belongs to exactly one kind and kinds
/// don't convert into each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Time,
    Rate,
    Voltage,
}

impl UnitKind {
    pub fn label(self) -> &'static str {
        match self {
            UnitKind::Time => "time",
            UnitKind::Rate => "rate",
            UnitKind::Voltage => "voltage",
        }
    }
}

/// The kind a unit token belongs to, or `None` if it isn't a unit Converge knows.
pub fn kind_of(unit: &str) -> Option<UnitKind> {
    match unit {
        "s" | "ms" | "us" | "ns" => Some(UnitKind::Time),
        "Hz" | "kHz" => Some(UnitKind::Rate),
        "V" | "mV" | "uV" => Some(UnitKind::Voltage),
        _ => None,
    }
}

/// Convert to volts. The volt is the canonical membrane unit, so a bare number in a voltage
/// position is volts. That keeps `1000 mV` and `1 V` behaving identically instead of making
/// the meaning of an unmarked weight depend on how the threshold next to it was spelled.
pub fn volts(q: &Quantity, context: &str) -> Result<f64, Diagnostic> {
    let factor = match q.unit.as_ref() {
        None => 1.0,
        Some(unit) => match unit.name.as_str() {
            "V" => 1.0,
            "mV" => 1e-3,
            "uV" => 1e-6,
            _ => {
                return Err(Diagnostic::new(format!(
                    "unsupported voltage unit `{}` for {context}",
                    unit.name
                ))
                .with_span(unit.span.clone()));
            }
        },
    };
    let v = q.value * factor;
    if !v.is_finite() {
        return Err(
            Diagnostic::new(format!("invalid voltage value for {context}"))
                .with_span(q.span.clone()),
        );
    }
    Ok(v)
}

pub fn expect_volts(q: &Quantity, context: &str) -> Result<(), Diagnostic> {
    let _ = volts(q, context)?;
    Ok(())
}

pub fn time_to_nanos(q: &Quantity, context: &str) -> Result<i64, Diagnostic> {
    let unit = q
        .unit
        .as_ref()
        .ok_or_else(|| missing_unit(context, &q.span))?;
    let factor = match unit.name.as_str() {
        "s" => 1_000_000_000.0,
        "ms" => 1_000_000.0,
        "us" => 1_000.0,
        "ns" => 1.0,
        _ => {
            return Err(Diagnostic::new(format!(
                "unsupported time unit `{}` for {context}",
                unit.name
            ))
            .with_span(unit.span.clone()));
        }
    };
    let nanos = q.value * factor;
    if !nanos.is_finite() {
        return Err(
            Diagnostic::new(format!("invalid time value for {context}")).with_span(q.span.clone())
        );
    }
    Ok(nanos.round() as i64)
}

pub fn rate_to_hz(q: &Quantity, context: &str) -> Result<f64, Diagnostic> {
    let unit = q
        .unit
        .as_ref()
        .ok_or_else(|| missing_unit(context, &q.span))?;
    let factor = match unit.name.as_str() {
        "Hz" => 1.0,
        "kHz" => 1_000.0,
        _ => {
            return Err(Diagnostic::new(format!(
                "unsupported rate unit `{}` for {context}",
                unit.name
            ))
            .with_span(unit.span.clone()));
        }
    };
    let hz = q.value * factor;
    if !hz.is_finite() {
        return Err(
            Diagnostic::new(format!("invalid rate value for {context}")).with_span(q.span.clone())
        );
    }
    Ok(hz)
}

pub fn expect_time(q: &Quantity, context: &str) -> Result<(), Diagnostic> {
    let _ = time_to_nanos(q, context)?;
    Ok(())
}

pub fn expect_rate(q: &Quantity, context: &str) -> Result<(), Diagnostic> {
    let _ = rate_to_hz(q, context)?;
    Ok(())
}

fn missing_unit(context: &str, span: &Span) -> Diagnostic {
    Diagnostic::new(format!("missing unit for {context}")).with_span(span.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Ident;

    fn q(value: f64, unit: &str) -> Quantity {
        Quantity {
            value,
            unit: Some(Ident::new(unit, Span::new(0, 0))),
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn time_units_convert() {
        assert_eq!(time_to_nanos(&q(1.0, "s"), "t").unwrap(), 1_000_000_000);
        assert_eq!(time_to_nanos(&q(2.0, "ms"), "t").unwrap(), 2_000_000);
        assert_eq!(time_to_nanos(&q(3.0, "us"), "t").unwrap(), 3_000);
        assert_eq!(time_to_nanos(&q(4.0, "ns"), "t").unwrap(), 4);
    }

    #[test]
    fn rate_units_convert() {
        assert_eq!(rate_to_hz(&q(1.0, "Hz"), "r").unwrap(), 1.0);
        assert_eq!(rate_to_hz(&q(2.0, "kHz"), "r").unwrap(), 2000.0);
    }

    fn bare(value: f64) -> Quantity {
        Quantity {
            value,
            unit: None,
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn voltage_units_convert() {
        assert_eq!(volts(&q(1.0, "V"), "v").unwrap(), 1.0);
        assert_eq!(volts(&q(1.0, "mV"), "v").unwrap(), 1e-3);
        assert_eq!(volts(&q(1.0, "uV"), "v").unwrap(), 1e-6);
    }

    // The unit used to be read and thrown away, so these two were the same threshold for the
    // wrong reason. They're the same threshold now because the conversion happens.
    #[test]
    fn millivolts_and_volts_agree() {
        assert_eq!(
            volts(&q(1000.0, "mV"), "v").unwrap(),
            volts(&q(1.0, "V"), "v").unwrap()
        );
    }

    // The volt is the canonical membrane unit, so a bare number is volts.
    #[test]
    fn bare_number_is_volts() {
        assert_eq!(volts(&bare(1.0), "v").unwrap(), 1.0);
    }

    #[test]
    fn voltage_rejects_other_kinds() {
        assert!(volts(&q(1.0, "ms"), "v").is_err());
        assert!(volts(&q(1.0, "Hz"), "v").is_err());
        assert!(volts(&q(1.0, "furlong"), "v").is_err());
    }

    #[test]
    fn kinds_are_disjoint() {
        assert_eq!(kind_of("ms"), Some(UnitKind::Time));
        assert_eq!(kind_of("kHz"), Some(UnitKind::Rate));
        assert_eq!(kind_of("mV"), Some(UnitKind::Voltage));
        assert_eq!(kind_of("nope"), None);
    }
}
