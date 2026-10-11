mod adjustment_control;
mod boolean_control;
mod select_control;

pub use self::{
    adjustment_control::{AdjustmentControl, AdjustmentControlMsg, AdjustmentInit},
    boolean_control::{BooleanControl, BooleanControlMsg, BooleanInit},
    select_control::{SelectControl, SelectControlMsg, SelectInit, SelectOption},
};
use super::setting_value::SettingValue;
use adw::prelude::*;
use relm4::{
    Component, ComponentController, Controller, FactorySender, RelmWidgetExt, css,
    factory::FactoryComponent, view,
};
use std::marker::PhantomData;

pub struct SettingRow<Key> {
    pub control: SettingControl,
    _key: PhantomData<Key>,
}

#[derive(Default)]
pub struct SettingRowInit {
    pub title: String,
    pub title_tooltip: String,
    pub info_text: String,
    pub label_group: Option<gtk::SizeGroup>,
    pub input_group: Option<gtk::SizeGroup>,
    pub control: SettingControlInit,
}

pub enum SettingControlInit {
    Adjustment(AdjustmentInit),
    Boolean(BooleanInit),
    Select(SelectInit),
}

impl Default for SettingControlInit {
    fn default() -> Self {
        Self::Adjustment(AdjustmentInit::default())
    }
}

impl From<AdjustmentInit> for SettingControlInit {
    fn from(init: AdjustmentInit) -> Self {
        Self::Adjustment(init)
    }
}

impl From<BooleanInit> for SettingControlInit {
    fn from(init: BooleanInit) -> Self {
        Self::Boolean(init)
    }
}

impl From<SelectInit> for SettingControlInit {
    fn from(init: SelectInit) -> Self {
        Self::Select(init)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingRowValue {
    Numeric(f64),
    Boolean(bool),
    Selected(Option<u32>),
}

impl SettingRowValue {
    pub fn as_numeric(self) -> Option<f64> {
        match self {
            Self::Numeric(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_boolean(self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_selected(self) -> Option<u32> {
        match self {
            Self::Selected(value) => value,
            _ => None,
        }
    }
}

impl<Key> SettingRow<Key> {
    pub fn widget(&self) -> &gtk::Box {
        self.control.widget()
    }

    pub fn has_default(&self) -> bool {
        match &self.control {
            SettingControl::Adjustment(control) => control.model().value.has_default(),
            SettingControl::Boolean(control) => control.model().value.has_default(),
            SettingControl::Select(control) => control.model().value.has_default(),
        }
    }

    fn set_value(&self, value: SettingRowValue) {
        match (&self.control, value) {
            (SettingControl::Adjustment(control), SettingRowValue::Numeric(value)) => {
                control.model().value.set_value(value)
            }
            (SettingControl::Boolean(control), SettingRowValue::Boolean(value)) => {
                control.model().value.set_value(value)
            }
            (SettingControl::Select(control), SettingRowValue::Selected(value)) => {
                control.model().value.set_value(value)
            }
            _ => panic!("setting value does not match the row's control"),
        }
    }

    pub fn get_value(&self) -> SettingRowValue {
        match &self.control {
            SettingControl::Adjustment(control) => {
                SettingRowValue::Numeric(control.model().value.get_value())
            }
            SettingControl::Boolean(control) => {
                SettingRowValue::Boolean(control.model().value.get_value())
            }
            SettingControl::Select(control) => {
                SettingRowValue::Selected(control.model().value.get_value())
            }
        }
    }

    pub fn get_changed_value(&self) -> Option<SettingRowValue> {
        match &self.control {
            SettingControl::Adjustment(control) => control
                .model()
                .value
                .get_changed_value()
                .map(SettingRowValue::Numeric),
            SettingControl::Boolean(control) => control
                .model()
                .value
                .get_changed_value()
                .map(SettingRowValue::Boolean),
            SettingControl::Select(control) => control
                .model()
                .value
                .get_changed_value()
                .map(SettingRowValue::Selected),
        }
    }
}

pub enum SettingControl {
    Adjustment(Controller<AdjustmentControl>),
    Boolean(Controller<BooleanControl>),
    Select(Controller<SelectControl>),
}

impl SettingControl {
    fn new(init: SettingRowInit, title: gtk::Box, output: &relm4::Sender<()>) -> Self {
        let label_group = init.label_group;
        let input_group = init.input_group;
        match init.control {
            SettingControlInit::Adjustment(init) => Self::Adjustment(
                AdjustmentControl::builder()
                    .launch((init, title, label_group, input_group))
                    .forward(output, |()| ()),
            ),
            SettingControlInit::Boolean(init) => Self::Boolean(
                BooleanControl::builder()
                    .launch((init, title, label_group, input_group))
                    .forward(output, |()| ()),
            ),
            SettingControlInit::Select(init) => Self::Select(
                SelectControl::builder()
                    .launch((init, title, label_group, input_group))
                    .forward(output, |()| ()),
            ),
        }
    }

    fn widget(&self) -> &gtk::Box {
        match self {
            Self::Adjustment(control) => control.widget(),
            Self::Boolean(control) => control.widget(),
            Self::Select(control) => control.widget(),
        }
    }

    fn set_visible(&self, visible: bool) {
        match self {
            Self::Adjustment(control) => control.emit(AdjustmentControlMsg::SetVisible(visible)),
            Self::Boolean(control) => control.emit(BooleanControlMsg::SetVisible(visible)),
            Self::Select(control) => control.emit(SelectControlMsg::SetVisible(visible)),
        }
    }

    fn reset(&self) {
        match self {
            Self::Adjustment(control) => control.emit(AdjustmentControlMsg::Reset),
            Self::Boolean(control) => control.emit(BooleanControlMsg::Reset),
            Self::Select(control) => control.emit(SelectControlMsg::Reset),
        }
    }

    fn set_sensitive(&self, sensitive: bool) {
        match self {
            Self::Adjustment(control) => {
                control.emit(AdjustmentControlMsg::SetSensitive(sensitive))
            }
            Self::Boolean(control) => control.emit(BooleanControlMsg::SetSensitive(sensitive)),
            Self::Select(control) => control.emit(SelectControlMsg::SetSensitive(sensitive)),
        }
    }
}

#[derive(Debug)]
pub enum SettingRowMsg {
    Reset,
    SetValue(SettingRowValue),
    SetVisible(bool),
    /// Enable editing while leaving informational popovers accessible.
    SetSensitive(bool),
}

#[relm4::factory(pub)]
impl<Key: 'static> FactoryComponent for SettingRow<Key> {
    type ParentWidget = gtk::ListBox;
    type Index = Key;
    type Init = SettingRowInit;
    type Input = SettingRowMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        #[name = "root_row"]
        gtk::ListBoxRow {
            set_activatable: false,
            set_selectable: false,
            add_css_class: "setting-row",
            add_css_class: match &self.control {
                SettingControl::Adjustment(_) => "adjustment-row",
                SettingControl::Boolean(_) => "boolean-row",
                SettingControl::Select(_) => "select-row",
            },
            set_child: Some(self.control.widget()),
        },
    }

    fn init_model(init: Self::Init, _index: &Self::Index, sender: FactorySender<Self>) -> Self {
        let title = init.title_widget();
        Self {
            control: SettingControl::new(init, title, sender.output_sender()),
            _key: PhantomData,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &gtk::ListBoxRow,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();
        widgets
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        _sender: FactorySender<Self>,
    ) {
        match msg {
            SettingRowMsg::Reset => self.control.reset(),
            SettingRowMsg::SetValue(value) => self.set_value(value),
            SettingRowMsg::SetSensitive(sensitive) => self.control.set_sensitive(sensitive),
            SettingRowMsg::SetVisible(visible) => {
                self.control.set_visible(visible);
                widgets.root_row.set_visible(visible);
            }
        }
    }
}

impl SettingRowInit {
    fn title_widget(&self) -> gtk::Box {
        view! {
            title = gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_valign: gtk::Align::Center,
                set_hexpand: true,
                set_spacing: 5,

                gtk::Label {
                    set_valign: gtk::Align::Center,
                    set_xalign: 0.0,
                    set_markup: &self.title,
                    set_tooltip_text: (!self.title_tooltip.is_empty()).then_some(self.title_tooltip.as_str()),
                },
                gtk::Box {
                    add_css_class: css::CAPTION,
                    set_valign: gtk::Align::End,

                    gtk::MenuButton {
                        set_visible: !self.info_text.is_empty(),
                        add_css_class: css::FLAT,
                        #[wrap(Some)]
                        set_popover = &gtk::Popover {
                            gtk::Label {
                                set_label: &self.info_text,
                                set_margin_all: 5,
                                set_wrap: true,
                                set_wrap_mode: gtk::pango::WrapMode::Word,
                                set_max_width_chars: 55,
                            },
                        },
                        #[wrap(Some)]
                        set_child = &gtk::Image {
                            set_icon_name: Some("info-outline-symbolic"),
                            set_pixel_size: 12,
                        },
                    },
                },
            },
        }
        title
    }
}

#[cfg(all(test, feature = "gtk-tests"))]
mod tests;
