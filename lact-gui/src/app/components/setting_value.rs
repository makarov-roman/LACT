use relm4::binding::{Binding, BoolBinding};

/// Common operations for settings backed by different controls.
pub trait SettingValue {
    type Value;

    fn get_value(&self) -> Self::Value;

    /// Sets the value as an edit. Setting the same effective value is a no-op.
    fn set_value(&self, value: Self::Value);

    /// Sets the value and clears the edit state.
    fn set_initial_value(&self, value: Self::Value);

    /// Sets the stored default as an edit, if a default is available.
    fn reset(&self);

    fn is_changed(&self) -> &BoolBinding;
    fn is_default(&self) -> &BoolBinding;

    /// Edits remain pending until loaded again, even when back at the initial value.
    /// Defaults remain explicit values; callers decide how to serialize them.
    fn get_changed_value(&self) -> Option<Self::Value> {
        self.is_changed().get().then(|| self.get_value())
    }
}
