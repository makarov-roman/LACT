use super::setting_value::SettingValue;
use relm4::binding::{Binding, BoolBinding, U32Binding};

#[derive(Clone, Debug)]
pub struct SelectValue {
    value: U32Binding,
    option_count: u32,
    default_value: Option<u32>,
    is_changed: BoolBinding,
    is_default: BoolBinding,
}

impl SelectValue {
    pub fn new(value: Option<u32>, default_value: Option<u32>, option_count: u32) -> Self {
        // Match GtkDropDown's initial selection for a nonempty model.
        let value = value
            .filter(|value| *value < option_count)
            .or_else(|| (option_count > 0).then_some(0));
        let default_value = default_value.filter(|value| *value < option_count);
        let model = Self {
            value: U32Binding::new(value.unwrap_or(gtk::INVALID_LIST_POSITION)),
            option_count,
            default_value,
            is_changed: BoolBinding::new(false),
            is_default: BoolBinding::new(default_value == value),
        };
        model.value.connect_value_notify({
            let is_changed = model.is_changed.clone();
            let is_default = model.is_default.clone();
            move |binding| {
                let value = (binding.get() < option_count).then(|| binding.get());
                is_default.set(default_value == value);
                is_changed.set(true);
            }
        });
        model
    }

    pub fn binding(&self) -> &U32Binding {
        &self.value
    }
}

impl SettingValue for SelectValue {
    type Value = Option<u32>;

    fn get_value(&self) -> Self::Value {
        let value = self.value.get();
        (value < self.option_count).then_some(value)
    }

    fn set_value(&self, value: Self::Value) {
        if value.is_some_and(|value| value >= self.option_count) {
            return;
        }
        if self.get_value() != value {
            self.value.set(value.unwrap_or(gtk::INVALID_LIST_POSITION));
        }
    }

    fn set_initial_value(&self, value: Self::Value) {
        self.set_value(value);
        self.is_changed.set(false);
    }

    fn reset(&self) {
        self.set_value(self.default_value);
    }

    fn serialize(&self) -> Option<Self::Value> {
        self.get_value()
            .filter(|value| Some(*value) != self.default_value)
            .map(Some)
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
        assert_eq!(value.serialize(), Some(Some(1)));
        value.set_value(Some(1));
        assert!(!value.is_changed().get());
        shared.set_value(Some(0));
        assert!(value.is_default().get());
        shared.set_value(Some(1));
        assert_eq!(value.serialize(), Some(Some(1)));
        assert!(value.is_changed().get());
        value.set_value(Some(9));
        assert_eq!(value.get_value(), Some(1));
        value.reset();
        assert_eq!(shared.serialize(), None);
        value.set_initial_value(Some(0));
        value.reset();
        assert!(value.is_default().get());
        assert!(!shared.is_changed().get());

        let empty = SelectValue::new(Some(1), Some(0), 0);
        empty.reset();
        assert_eq!(empty.get_value(), None);
        assert_eq!(empty.serialize(), None);
        assert!(!empty.is_changed().get());

        let unknown_default = SelectValue::new(Some(1), None, 2);
        assert_eq!(unknown_default.serialize(), Some(Some(1)));
        unknown_default.reset();
        assert_eq!(unknown_default.serialize(), None);
        assert!(unknown_default.is_changed().get());
        unknown_default.set_value(Some(1));
        assert_eq!(unknown_default.serialize(), Some(Some(1)));
        unknown_default.set_value(None);
        assert_eq!(unknown_default.serialize(), None);
        assert_eq!(unknown_default.get_value(), None);
    }
}
