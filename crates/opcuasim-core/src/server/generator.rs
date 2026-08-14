use evalexpr::ContextWithMutableVariables;
use rand::Rng;

use super::models::{DataType, SimulationMode};

/// Generate the next f64 value for a simulation mode.
/// Returns None for Static mode (no automatic generation).
pub fn generate_value(mode: &SimulationMode, elapsed_secs: f64, iteration: u64) -> Option<f64> {
    match mode {
        SimulationMode::Static { .. } => None,
        SimulationMode::Random { min, max, .. } => Some(rand::thread_rng().gen_range(*min..=*max)),
        SimulationMode::Sine {
            amplitude,
            offset,
            period_ms,
            ..
        } => {
            let period_secs = *period_ms as f64 / 1000.0;
            Some(
                *offset
                    + *amplitude * (2.0 * std::f64::consts::PI * elapsed_secs / period_secs).sin(),
            )
        }
        SimulationMode::Linear {
            start,
            step,
            min,
            max,
            mode,
            ..
        } => {
            let range = max - min;
            if range <= 0.0 {
                return Some(*start);
            }
            let raw = start + step * iteration as f64;
            match mode {
                super::models::LinearMode::Repeat => Some(min + (raw - min).rem_euclid(range)),
                super::models::LinearMode::Bounce => {
                    let pos = (raw - min) / range;
                    let frac = pos - pos.floor();
                    // Use f64 modulo to avoid i64 overflow on extreme iterations.
                    let cycle = pos.floor().rem_euclid(2.0);
                    if cycle < 1.0 {
                        Some(min + frac * range)
                    } else {
                        Some(max - frac * range)
                    }
                }
            }
        }
        SimulationMode::Script { expression, .. } => {
            let mut context = evalexpr::HashMapContext::new();
            let _ = context.set_value("t".into(), evalexpr::Value::Float(elapsed_secs));
            let _ = context.set_value("iteration".into(), evalexpr::Value::Float(iteration as f64));
            match evalexpr::eval_number_with_context(expression, &context) {
                Ok(v) => Some(v),
                Err(e) => {
                    log::warn!("Script expression '{}' eval failed: {}", expression, e);
                    None
                }
            }
        }
    }
}

/// Convert an f64 value to a string representation appropriate for the data type.
pub fn f64_to_string(value: f64, data_type: &DataType) -> String {
    match data_type {
        DataType::Boolean => if value > 0.5 { "true" } else { "false" }.to_string(),
        DataType::Int16 | DataType::Enum { .. } => {
            (value.clamp(i32::MIN as f64, i32::MAX as f64) as i32).to_string()
        }
        DataType::Int32 => (value.clamp(i32::MIN as f64, i32::MAX as f64) as i32).to_string(),
        DataType::Int64 => (value.clamp(i64::MIN as f64, i64::MAX as f64) as i64).to_string(),
        DataType::UInt16 => (value.clamp(0.0, u16::MAX as f64) as u16).to_string(),
        DataType::UInt32 => (value.clamp(0.0, u32::MAX as f64) as u32).to_string(),
        DataType::UInt64 => (value.clamp(0.0, u64::MAX as f64) as u64).to_string(),
        DataType::Float => format!("{:.6}", value as f32),
        DataType::Double => format!("{:.6}", value),
        DataType::String => format!("{:.2}", value),
        DataType::DateTime | DataType::ByteString => format!("{:.2}", value),
        DataType::Array { element_type, .. } | DataType::Array2D { element_type, .. } => {
            f64_to_string(value, element_type)
        }
        // Structures have no single f64 scalar representation (Task 7 will
        // build structured ExtensionObject values); fall back to the raw f64
        // so callers still receive a deterministic string.
        DataType::Structure { .. } => format!("{:.6}", value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(expression: &str) -> SimulationMode {
        SimulationMode::Script {
            expression: expression.into(),
            interval_ms: 1000,
        }
    }

    #[test]
    fn script_uses_t_and_iteration_variables() {
        let v = generate_value(&script("t * 2 + iteration"), 3.0, 1).unwrap();
        assert!((v - 7.0).abs() < 1e-9, "t*2+iteration = 7.0, got {v}");
    }

    #[test]
    fn script_invalid_expression_returns_none() {
        assert!(generate_value(&script("not valid ++"), 1.0, 0).is_none());
    }

    #[test]
    fn linear_bounce_survives_extreme_iterations() {
        let mode = SimulationMode::Linear {
            start: 0.0,
            step: 1.0,
            min: 0.0,
            max: 10.0,
            mode: super::super::models::LinearMode::Bounce,
            interval_ms: 100,
        };
        // i64::MAX iterations would overflow `cycle as i64`; the f64 modulo
        // must stay within [0, 10].
        let v = generate_value(&mode, 0.0, i64::MAX as u64).unwrap();
        assert!((0.0..=10.0).contains(&v), "bounce value out of range: {v}");
    }
}
