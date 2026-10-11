use super::setting_value::SettingValue;
use relm4::binding::{Binding, BoolBinding, U32Binding};

#[derive(Clone, Debug)]
pub struct SelectValue {
    binding: U32Binding,
    option_count: u32,
    default_value: Option<u32>,
    is_changed: BoolBinding,
    is_default: BoolBinding,
}

impl SelectValue {
    pub fn new(value: Option<u32>, default_value: Option<u32>, option_count: u32) -> Self {
        // GtkDropDown always selects an item when its model is nonempty.
        let value = value
            .filter(|value| *value < option_count)
            .or_else(|| (option_count > 0).then_some(0));
        let default_value = default_value.filter(|value| *value < option_count);
        let model = Self {
            binding: U32Binding::new(value.unwrap_or(gtk::INVALID_LIST_POSITION)),
            option_count,
            default_value,
            is_changed: BoolBinding::new(false),
            is_default: BoolBinding::new(default_value.is_some() && default_value == value),
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

    pub fn binding(&self) -> &U32Binding {
        &self.binding
    }
}

impl SettingValue for SelectValue {
    type Value = Option<u32>;

    fn get_value(&self) -> Self::Value {
        let value = self.binding.get();
        (value < self.option_count).then_some(value)
    }

    fn set_value(&self, value: Self::Value) {
        let value = value.filter(|value| *value < self.option_count);
        if value.is_none() && self.option_count > 0 {
            return;
        }
        if self.get_value() != value {
            self.binding
                .set(value.unwrap_or(gtk::INVALID_LIST_POSITION));
        }
    }

    fn set_initial_value(&self, value: Self::Value) {
        self.set_value(value);
        self.is_changed.set(false);
    }

    fn reset(&self) {
        if let Some(value) = self.default_value {
            self.set_value(Some(value));
        }
    }

    fn has_default(&self) -> bool {
        self.default_value.is_some()
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
    fn selection_edits_resets_and_loads_share_state() {
        let value = SelectValue::new(Some(1), Some(0), 2);
        let shared = value.clone();
        value.set_value(Some(1));
        assert_eq!(value.get_changed_value(), None);
        shared.set_value(Some(0));
        assert!(value.is_default().get());
        shared.set_value(Some(1));
        assert_eq!(value.get_changed_value(), Some(Some(1)));
        value.set_value(Some(9));
        assert_eq!(value.get_value(), Some(1));
        value.reset();
        assert_eq!(shared.get_changed_value(), Some(Some(0)));
        value.set_initial_value(Some(0));
        value.reset();
        assert!(value.is_default().get());
        assert_eq!(shared.get_changed_value(), None);

        let empty = SelectValue::new(Some(1), Some(0), 0);
        empty.reset();
        assert_eq!(empty.get_value(), None);
        assert_eq!(empty.get_changed_value(), None);
        assert!(!empty.is_default().get());
    }
}
