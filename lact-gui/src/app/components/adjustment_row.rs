use super::adjustment_value::AdjustmentValue;
use crate::app::utils::formatting::fmt_value_with_unit;
use crate::{I18N, app::utils::ext::make_event_controller_no_scroll};
use adw::prelude::*;
use i18n_embed_fl::fl;
use relm4::binding::Binding;
use relm4::{FactorySender, RelmWidgetExt, css, factory::FactoryComponent};
use std::marker::PhantomData;

pub struct AdjustmentRow<Key> {
    title: String,
    title_tooltip: String,
    info_text: String,
    unit: String,
    _key: PhantomData<Key>,
    adjustment: AdjustmentValue,
    value_ratio: f64,
    size_group_widgets: Vec<(gtk::SizeGroup, gtk::Widget)>,
}

pub struct AdjustmentRowInit {
    pub title: String,
    pub title_tooltip: String,
    pub info_text: String,
    pub unit: String,
    pub value: f64,
    pub default_value: Option<f64>,
    pub lower: f64,
    pub upper: f64,
    pub step_increment: f64,
    pub page_increment: f64,
}

impl Default for AdjustmentRowInit {
    fn default() -> Self {
        Self {
            title: String::new(),
            title_tooltip: String::new(),
            info_text: String::new(),
            unit: String::new(),
            value: 0.0,
            default_value: None,
            lower: 0.0,
            upper: 0.0,
            step_increment: 1.0,
            page_increment: 10.0,
        }
    }
}

#[derive(Debug)]
pub enum AdjustmentRowMsg {
    /// Change display units while preserving the edit state.
    ValueRatio(f64),
    Reset,
    Refresh,
    SetVisible(bool),
    AddSizeGroup {
        label_group: gtk::SizeGroup,
        input_group: gtk::SizeGroup,
        lower_label_group: gtk::SizeGroup,
        upper_label_group: gtk::SizeGroup,
    },
}

#[relm4::factory(pub)]
impl<Key: 'static> FactoryComponent for AdjustmentRow<Key> {
    type ParentWidget = gtk::ListBox;
    type Index = Key;
    type Init = AdjustmentRowInit;
    type Input = AdjustmentRowMsg;
    // Both slider changes and uncommitted spin-button text edits notify the parent.
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        #[name = "root_row"]
        gtk::ListBoxRow {
            set_activatable: false,
            set_selectable: false,
            add_css_class: "adjustment-row",

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                gtk::Box {
                    set_spacing: 12,

                    #[name = "title_box"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_valign: gtk::Align::Center,
                        set_hexpand: true,
                        set_spacing: 5,

                        #[name = "label"]
                        gtk::Label {
                            set_valign: gtk::Align::Center,
                            set_xalign: 0.0,
                            set_markup: &self.title,
                            set_tooltip_text: (!self.title_tooltip.is_empty()).then_some(self.title_tooltip.as_str()),
                        },

                        gtk::Box {
                            add_css_class: css::CAPTION,
                            set_valign: gtk::Align::End,

                            #[name = "details_button"]
                            gtk::MenuButton {
                                set_valign: gtk::Align::Center,
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

                    #[name = "spinbutton"]
                    gtk::SpinButton {
                        set_adjustment: &*self.adjustment,
                        set_valign: gtk::Align::Center,
                        set_update_policy: gtk::SpinButtonUpdatePolicy::IfValid,
                        connect_output[adjustment = self.adjustment.clone()] => move |spin| {
                            if adjustment.setting.value().is_none() {
                                spin.set_text(&fl!(I18N, "default-button"));
                                gtk::glib::Propagation::Stop
                            } else {
                                gtk::glib::Propagation::Proceed
                            }
                        },
                        add_controller = make_event_controller_no_scroll(),
                        connect_changed[adjustment = self.adjustment.clone()] => move |spin| {
                            if let Ok(value) = spin.text().parse::<f64>() {
                                adjustment.setting.edit(Some(value / adjustment.value_ratio.get()));
                            }
                        } @ text_change_signal,
                    },
                },

                gtk::Box {
                    set_spacing: 12,
                    #[watch]
                    set_visible: !self.unknown_default(),

                    #[name = "lower_label"]
                    gtk::Label {
                        set_label: &fmt_value_with_unit(self.adjustment.lower(), &self.unit),
                        add_css_class: css::CAPTION,
                        add_css_class: css::DIM_LABEL,
                    },

                    #[name = "scale"]
                    gtk::Scale {
                        set_adjustment: &*self.adjustment,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_hexpand: true,
                        set_digits: 0,
                        set_round_digits: 0,
                        set_value_pos: gtk::PositionType::Right,
                        set_width_request: 100,
                        add_controller = make_event_controller_no_scroll(),
                    },

                    #[name = "upper_label"]
                    gtk::Label {
                        set_label: &fmt_value_with_unit(self.adjustment.upper(), &self.unit),
                        add_css_class: css::CAPTION,
                        add_css_class: css::DIM_LABEL,
                    },
                },
            },
        },

        #[local_ref]
        changed -> relm4::binding::BoolBinding {
            connect_value_notify[sender] => move |changed| {
                if changed.get() {
                    let _ = sender.output(());
                }
                sender.input(AdjustmentRowMsg::Refresh);
            },
        },
        #[local_ref]
        is_default -> relm4::binding::BoolBinding {
            connect_value_notify[sender] => move |_| {
                sender.input(AdjustmentRowMsg::Refresh);
            },
        },
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        let adjustment = AdjustmentValue::new(
            init.value,
            init.default_value,
            init.lower,
            init.upper,
            init.step_increment,
            init.page_increment,
        );
        Self {
            title: init.title,
            title_tooltip: init.title_tooltip,
            info_text: init.info_text,
            unit: init.unit,
            _key: PhantomData,
            adjustment,
            value_ratio: 1.0,
            size_group_widgets: Vec::new(),
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &gtk::ListBoxRow,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let changed = &self.adjustment.setting.is_changed;
        let is_default = &self.adjustment.setting.is_default;
        let widgets = view_output!();

        widgets
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: FactorySender<Self>,
    ) {
        match msg {
            AdjustmentRowMsg::ValueRatio(ratio) => {
                // Changing display units must not emit an edit notification.
                widgets.spinbutton.block_signal(&widgets.text_change_signal);
                let factor = ratio / self.value_ratio;
                self.adjustment.without_edit_tracking(|| {
                    let value = self.adjustment.value() * factor;
                    self.adjustment.set_lower(self.adjustment.lower() * factor);
                    self.adjustment.set_upper(self.adjustment.upper() * factor);
                    self.adjustment.set_value(value);
                });
                self.value_ratio = ratio;
                self.adjustment.value_ratio.set(ratio);
                widgets
                    .lower_label
                    .set_label(&fmt_value_with_unit(self.adjustment.lower(), &self.unit));
                widgets
                    .upper_label
                    .set_label(&fmt_value_with_unit(self.adjustment.upper(), &self.unit));

                widgets
                    .spinbutton
                    .unblock_signal(&widgets.text_change_signal);
            }
            AdjustmentRowMsg::Reset => {
                widgets.spinbutton.block_signal(&widgets.text_change_signal);
                self.adjustment.reset();
                widgets.spinbutton.set_value(self.adjustment.value());
                widgets
                    .spinbutton
                    .unblock_signal(&widgets.text_change_signal);
            }
            AdjustmentRowMsg::Refresh => (),
            AdjustmentRowMsg::SetVisible(visible) => {
                if widgets.root_row.get_visible() != visible {
                    for (group, widget) in &self.size_group_widgets {
                        if visible {
                            group.add_widget(widget);
                        } else {
                            group.remove_widget(widget);
                        }
                    }
                    widgets.root_row.set_visible(visible);
                }
            }
            AdjustmentRowMsg::AddSizeGroup {
                label_group,
                input_group,
                lower_label_group,
                upper_label_group,
            } => {
                for (group, widget) in [
                    (label_group, widgets.title_box.clone().upcast()),
                    (input_group, widgets.spinbutton.clone().upcast()),
                    (lower_label_group, widgets.lower_label.clone().upcast()),
                    (upper_label_group, widgets.upper_label.clone().upcast()),
                ] {
                    if widgets.root_row.get_visible() {
                        group.add_widget(&widget);
                    }
                    self.size_group_widgets.push((group, widget));
                }
            }
        }
        self.update_view(widgets, sender);
    }
}

impl<Key> AdjustmentRow<Key> {
    pub fn get_value(&self) -> f64 {
        self.adjustment.value() / self.value_ratio
    }

    pub fn get_changed_value(&self) -> Option<Option<f64>> {
        self.adjustment.get_changed_value()
    }

    fn unknown_default(&self) -> bool {
        self.adjustment.setting.value().is_none()
    }
}
