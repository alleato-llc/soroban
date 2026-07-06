//! Interactive control cells: checkbox / dropdown / slider / stepper rewrites,
//! each committed as an undoable edit through the storage literal.

use super::*;

impl Session {
    // MARK: Controls (slice ④)

    /// Rewrite a control cell's storage literal in place and commit it as an
    /// undoable edit. `Control::rewriting` preserves spacing, the 𝑖 name, and
    /// any trailing `# comment`. A no-op when the cell isn't a control.
    fn rewrite_control(&mut self, address: CellAddress, literal: &str) {
        let raw = self.cell_raw(address);
        if let Some(new_raw) = Control::rewriting(&raw, literal) {
            self.set_cell_raw(address, &new_raw);
        }
    }

    /// Flip a checkbox cell's stored `true`/`false`.
    pub fn toggle_checkbox(&mut self, address: CellAddress) {
        if let CellDisplay::Checkbox(info) = self.display_at(address) {
            self.rewrite_control(address, if info.is_on { "false" } else { "true" });
        }
    }

    /// Select a dropdown option by index, rewriting to its literal source.
    pub fn set_dropdown_index(&mut self, address: CellAddress, index: usize) {
        if let CellDisplay::Dropdown(info) = self.display_at(address) {
            if let Some(option) = info.options.get(index) {
                let literal = option_literal(option);
                self.rewrite_control(address, &literal);
            }
        }
    }

    /// Set a slider to the value nearest `target` on its step lattice, exactly
    /// (the position comes from the drag as `f64`; the stored value is snapped
    /// in `BigDecimal` so it stays a clean multiple of the step).
    pub fn set_slider(&mut self, address: CellAddress, target: f64) {
        if let CellDisplay::Slider(info) = self.display_at(address) {
            let minimum = info.minimum.to_f64();
            let step = info.step.to_f64();
            let value = if step > 0.0 {
                let steps = ((target - minimum) / step).round().max(0.0);
                let count = BigDecimal::from_f64(steps).unwrap_or_else(BigDecimal::zero);
                &info.minimum + &(&info.step * &count)
            } else {
                info.value.clone()
            };
            let value = clamp(value, &info.minimum, &info.maximum);
            self.rewrite_control(address, &value.to_string());
        }
    }

    /// Nudge a stepper (or slider) by one step, clamped to its range.
    pub fn step_control(&mut self, address: CellAddress, up: bool) {
        let info = match self.display_at(address) {
            CellDisplay::Stepper(info) | CellDisplay::Slider(info) => info,
            _ => return,
        };
        let delta = if up { info.step.clone() } else { -&info.step };
        let next = clamp(&info.value + &delta, &info.minimum, &info.maximum);
        self.rewrite_control(address, &next.to_string());
    }
}

/// A dropdown option's re-parseable literal source: numbers as-is, strings
/// quoted with the language's `\" \\ \n \t` escapes.
fn option_literal(value: &Value) -> String {
    match value {
        Value::String(text) => {
            let mut out = String::with_capacity(text.len() + 2);
            out.push('"');
            for character in text.chars() {
                match character {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    other => out.push(other),
                }
            }
            out.push('"');
            out
        }
        other => other.to_string(),
    }
}

/// Clamp a value into `[minimum, maximum]` (exact, `BigDecimal` ordering).
fn clamp(value: BigDecimal, minimum: &BigDecimal, maximum: &BigDecimal) -> BigDecimal {
    if value < *minimum {
        minimum.clone()
    } else if value > *maximum {
        maximum.clone()
    } else {
        value
    }
}
