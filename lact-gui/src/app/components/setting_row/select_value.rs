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
