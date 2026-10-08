use super::setting_value::SettingValue;
use gtk::prelude::*;
use std::ops::Deref;
use tracing::debug;

/// GTK numeric adapter backed by shared setting state.
#[derive(Clone, Debug)]
pub struct AdjustmentValue {
    adjustment: gtk::Adjustment,
    pub setting: SettingValue<f64>,
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
        lower: f64,
        upper: f64,
        step_increment: f64,
        page_increment: f64,
    ) -> Self {
        let adjustment =
            gtk::Adjustment::new(value, lower, upper, step_increment, page_increment, 0.0);
        let setting = SettingValue::new(Some(adjustment.value()), None);
        adjustment.connect_value_changed({
            let setting = setting.clone();
            move |adjustment| setting.edit(Some(adjustment.value()))
        });

        Self {
            adjustment,
            setting,
        }
    }

    pub fn get_changed_value(&self, filter_zero: bool) -> Option<f64> {
        if let Some(Some(value)) = self.setting.get_changed_value() {
            if filter_zero && value == 0.0 {
                None
            } else {
                debug!("Value was changed, returning {value}");
                Some(value)
            }
        } else {
            debug!("Value is unchanged, returning None");
            None
        }
    }

    pub fn set_initial_value(&self, value: f64) {
        self.adjustment.set_value(value);
        // Keep the refresh notification even when the numeric value is unchanged.
        self.adjustment.emit_by_name::<()>("value_changed", &[]);
        self.setting.load(Some(self.adjustment.value()));
    }
}
