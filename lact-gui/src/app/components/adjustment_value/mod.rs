use super::setting_value::SettingValue;
use gtk::{glib::SignalHandlerId, prelude::*};
use std::{cell::Cell, ops::Deref, rc::Rc};

/// GTK numeric adapter. Setting semantics live in `SettingValue`.
#[derive(Clone, Debug)]
pub struct AdjustmentValue {
    adjustment: gtk::Adjustment,
    pub setting: SettingValue<f64>,
    pub value_ratio: Rc<Cell<f64>>,
    edit_signal: Rc<SignalHandlerId>,
}

impl Default for AdjustmentValue {
    fn default() -> Self {
        Self::new(0.0, None, 0.0, 0.0, 1.0, 1.0)
    }
}

impl Deref for AdjustmentValue {
    type Target = gtk::Adjustment;
    fn deref(&self) -> &Self::Target {
        &self.adjustment
    }
}

impl AdjustmentValue {
    pub fn new(
        value: f64,
        default: Option<f64>,
        lower: f64,
        upper: f64,
        step: f64,
        page: f64,
    ) -> Self {
        let adjustment = gtk::Adjustment::new(value, lower, upper, step, page, 0.0);
        let setting = SettingValue::new(Some(adjustment.value()), default);
        let value_ratio = Rc::new(Cell::new(1.0));
        let edit_signal = adjustment.connect_value_changed({
            let setting = setting.clone();
            let ratio = value_ratio.clone();
            move |adj| setting.edit(Some(adj.value() / ratio.get()))
        });
        Self {
            adjustment,
            setting,
            value_ratio,
            edit_signal: Rc::new(edit_signal),
        }
    }

    pub fn without_edit_tracking(&self, update: impl FnOnce()) {
        self.adjustment.block_signal(&self.edit_signal);
        update();
        self.adjustment.unblock_signal(&self.edit_signal);
    }

    pub fn set_initial_value(&self, value: f64) {
        self.without_edit_tracking(|| self.adjustment.set_value(value));
        self.setting
            .load(Some(self.adjustment.value() / self.value_ratio.get()));
    }

    pub fn reset(&self) {
        self.setting.reset();
        self.without_edit_tracking(|| {
            if let Some(value) = self.setting.value() {
                self.adjustment.set_value(value * self.value_ratio.get());
            }
        });
    }

    pub fn get_changed_value(&self) -> Option<Option<f64>> {
        self.setting.get_changed_value()
    }

    pub fn get_nonzero_value(&self) -> Option<f64> {
        self.setting.value().filter(|value| *value != 0.0)
    }

    pub fn set_default_value(&self, value: Option<f64>) {
        self.setting.set_default_value(value);
    }

    pub fn default_value(&self) -> Option<f64> {
        self.setting.default_value()
    }
}
