use relm4::binding::BoolBinding;

/// Common operations for settings backed by different controls.
pub trait SettingValue {
    type Value;

    fn get_value(&self) -> Self::Value;

    /// Sets the value as an edit. Setting the same effective value is a no-op.
    fn set_value(&self, value: Self::Value);

    /// Sets the value and clears the edit state.
    fn set_initial_value(&self, value: Self::Value);

    /// Restores the default if the control can represent it.
    fn reset(&self);

    /// Convert state to value we'll persist in config
    fn serialize(&self) -> Option<Self::Value>;

    fn is_changed(&self) -> &BoolBinding;
    fn is_default(&self) -> &BoolBinding;
}
