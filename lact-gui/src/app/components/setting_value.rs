use relm4::binding::{Binding, BoolBinding};
use std::{cell::RefCell, rc::Rc};

/// A setting's value and edit state, independent of its widget representation.
#[derive(Clone, Debug)]
pub struct SettingValue<T> {
    value: Rc<RefCell<Option<T>>>,
    default_value: Rc<RefCell<Option<T>>>,
    pub is_changed: BoolBinding,
    pub is_default: BoolBinding,
}

impl<T: Clone + PartialEq> SettingValue<T> {
    pub fn new(value: Option<T>, default_value: Option<T>) -> Self {
        let is_default = BoolBinding::new(value == default_value);
        Self {
            value: Rc::new(RefCell::new(value)),
            default_value: Rc::new(RefCell::new(default_value)),
            is_changed: BoolBinding::new(false),
            is_default,
        }
    }

    pub fn value(&self) -> Option<T> {
        self.value.borrow().clone()
    }

    pub fn default_value(&self) -> Option<T> {
        self.default_value.borrow().clone()
    }

    pub fn load(&self, value: Option<T>) {
        self.update(value, false);
    }

    pub fn edit(&self, value: Option<T>) {
        self.update(value, true);
    }

    pub fn reset(&self) {
        self.edit(self.default_value());
    }

    pub fn set_default_value(&self, value: Option<T>) {
        *self.default_value.borrow_mut() = value;
        self.is_default.set(self.value() == self.default_value());
    }

    fn update(&self, value: Option<T>, changed: bool) {
        *self.value.borrow_mut() = value;
        self.is_default.set(self.value() == self.default_value());
        self.is_changed.set(changed);
    }

    /// The outer `None` means no edit. Default values remain explicit values;
    /// callers decide how to represent them in configuration.
    pub fn get_changed_value(&self) -> Option<Option<T>> {
        self.is_changed.get().then(|| self.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn keeps_edits_until_loaded_even_at_the_original_or_default_value() {
        let value = SettingValue::new(Some(50.0), Some(0.0));
        let notified = Rc::new(Cell::new(false));
        value.is_changed.connect_value_notify({
            let notified = notified.clone();
            move |binding| notified.set(binding.get())
        });
        assert_eq!(value.get_changed_value(), None);

        let shared = value.clone();
        shared.edit(Some(25.0));
        assert!(notified.get());
        assert_eq!(value.get_changed_value(), Some(Some(25.0)));
        value.edit(Some(50.0));
        assert_eq!(value.get_changed_value(), Some(Some(50.0)));

        value.reset();
        assert!(value.is_default.get());
        assert_eq!(value.get_changed_value(), Some(Some(0.0)));
        value.load(Some(0.0));
        assert!(!notified.get());
        assert!(value.is_default.get());
        assert_eq!(value.get_changed_value(), None);
    }

    #[test]
    fn tracks_unknown_defaults_and_non_numeric_values() {
        let unknown = SettingValue::new(Some(2000), None);
        unknown.reset();
        assert_eq!(unknown.value(), None);
        assert!(unknown.is_default.get());
        assert_eq!(unknown.get_changed_value(), Some(None));
        unknown.edit(Some(2000));
        assert!(!unknown.is_default.get());

        let boolean = SettingValue::new(Some(true), Some(false));
        boolean.reset();
        assert_eq!(boolean.get_changed_value(), Some(Some(false)));

        let text = SettingValue::new(Some(String::from("a")), None);
        text.set_default_value(Some(String::from("a")));
        assert!(text.is_default.get());
        assert!(!text.is_changed.get());
        assert_eq!(text.get_changed_value(), None);
    }
}
