use super::setting_value::SettingValue;
use gtk::prelude::*;
use relm4::binding::{Binding, BoolBinding};
use std::{cell::Cell, ops::Deref, rc::Rc};

/// A numeric setting stored in a shared GTK adjustment, with raw/display unit conversion.
#[derive(Clone, Debug)]
pub struct AdjustmentValue {
    value: gtk::Adjustment,
    default_value: Option<f64>,
    value_ratio: Rc<Cell<f64>>,
    is_changed: BoolBinding,
    is_default: BoolBinding,
}

impl Deref for AdjustmentValue {
    type Target = gtk::Adjustment;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl AdjustmentValue {
    pub fn new(
        value: f64,
        default_value: Option<f64>,
        lower: f64,
        upper: f64,
        step_increment: f64,
        page_increment: f64,
    ) -> Self {
        let adjustment =
            gtk::Adjustment::new(value, lower, upper, step_increment, page_increment, 0.0);
        let model = Self {
            is_default: BoolBinding::new(default_value == Some(adjustment.value())),
            is_changed: BoolBinding::new(false),
            default_value,
            value: adjustment,
            value_ratio: Rc::new(Cell::new(1.0)),
        };
        model.value.connect_value_changed({
            let is_changed = model.is_changed().clone();
            let is_default = model.is_default().clone();
            let value_ratio = model.value_ratio.clone();
            move |adjustment| {
                is_default.set(default_value == Some(adjustment.value() / value_ratio.get()));
                is_changed.set(true);
            }
        });

        model
    }

    /// Changes display units, retaining the raw value and edit state.
    pub fn set_value_ratio(&self, ratio: f64) {
        let value = self.get_value();
        let changed = self.is_changed.get();
        let lower = self.value.lower() / self.value_ratio.get();
        let upper = self.value.upper() / self.value_ratio.get();
        self.value_ratio.set(ratio);
        self.value.set_lower(lower * ratio);
        self.value.set_upper(upper * ratio);
        self.set_initial_value(value);
        self.is_changed.set(changed);
    }
}

impl SettingValue for AdjustmentValue {
    type Value = f64;

    fn get_value(&self) -> f64 {
        self.value.value() / self.value_ratio.get()
    }

    fn set_value(&self, value: f64) {
        self.value.set_value(value * self.value_ratio.get());
    }

    fn set_initial_value(&self, value: f64) {
        self.set_value(value);
        // Keep the refresh notification even when the numeric value is unchanged.
        self.value.emit_by_name::<()>("value_changed", &[]);
        self.is_changed.set(false);
    }

    fn reset(&self) {
        if let Some(value) = self.default_value {
            self.set_value(value);
        }
    }

    fn serialize(&self) -> Option<f64> {
        let value = self.get_value();
        (Some(value) != self.default_value).then_some(value)
    }

    fn is_changed(&self) -> &BoolBinding {
        &self.is_changed
    }

    fn is_default(&self) -> &BoolBinding {
        &self.is_default
    }
}
