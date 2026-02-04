use crate::ast::Quantity;
use crate::diagnostic::{Diagnostic, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Time,
    Rate,
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
}
