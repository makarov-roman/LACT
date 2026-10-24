use super::setting_value::SettingValue;
use relm4::binding::{Binding, BoolBinding};

#[derive(Clone, Debug)]
pub struct BooleanValue {
    value: BoolBinding,
    default_value: Option<bool>,
    is_changed: BoolBinding,
    is_default: BoolBinding,
}

impl BooleanValue {
    pub fn new(value: bool, default_value: Option<bool>) -> Self {
        let model = Self {
            value: BoolBinding::new(value),
            default_value,
            is_changed: BoolBinding::new(false),
            is_default: BoolBinding::new(default_value == Some(value)),
        };
        model.value.connect_value_notify({
            let is_changed = model.is_changed.clone();
            let is_default = model.is_default.clone();
            move |binding| {
                is_default.set(default_value == Some(binding.get()));
                is_changed.set(true);
            }
        });
        model
    }

    pub fn binding(&self) -> &BoolBinding {
        &self.value
    }
}

impl SettingValue for BooleanValue {
    type Value = bool;

    fn get_value(&self) -> bool {
        self.value.get()
    }

    fn set_value(&self, value: bool) {
        if self.get_value() != value {
            self.value.set(value);
        }
    }

    fn set_initial_value(&self, value: bool) {
        self.set_value(value);
        self.is_changed.set(false);
    }

    fn reset(&self) {
        if let Some(value) = self.default_value {
            self.set_value(value);
        }
    }

    fn serialize(&self) -> Option<bool> {
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
