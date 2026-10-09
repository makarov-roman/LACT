use super::setting_value::SettingValue;
use relm4::binding::{Binding, BoolBinding};

#[derive(Clone, Debug)]
pub struct BooleanValue {
    binding: BoolBinding,
    default_value: Option<bool>,
    is_changed: BoolBinding,
    is_default: BoolBinding,
}

impl BooleanValue {
    pub fn new(value: bool, default_value: Option<bool>) -> Self {
        let model = Self {
            binding: BoolBinding::new(value),
            default_value,
            is_changed: BoolBinding::new(false),
            is_default: BoolBinding::new(default_value == Some(value)),
        };
        model.binding.connect_value_notify({
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
        &self.binding
    }
}

impl SettingValue for BooleanValue {
    type Value = bool;

    fn get_value(&self) -> bool {
        self.binding.get()
    }

    fn set_value(&self, value: bool) {
        if self.get_value() != value {
            self.binding.set(value);
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

    fn is_changed(&self) -> &BoolBinding {
        &self.is_changed
    }

    fn is_default(&self) -> &BoolBinding {
        &self.is_default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_resets_and_loads_share_state_across_clones() {
        let value = BooleanValue::new(true, Some(false));
        let shared = value.clone();
        value.set_value(true);
        assert_eq!(value.get_changed_value(), None);
        shared.set_value(false);
        assert!(value.is_default().get());
        shared.set_value(true);
        assert_eq!(value.get_changed_value(), Some(true));
        value.reset();
        assert!(!shared.get_value());
        assert_eq!(shared.get_changed_value(), Some(false));
        value.set_initial_value(false);
        value.reset();
        assert!(value.is_default().get());
        assert_eq!(shared.get_changed_value(), None);
    }

    #[test]
    fn unknown_default_does_not_reset_the_value() {
        let value = BooleanValue::new(true, None);
        value.reset();
        assert!(value.get_value());
        assert!(!value.is_default().get());
        assert_eq!(value.get_changed_value(), None);
    }
}
