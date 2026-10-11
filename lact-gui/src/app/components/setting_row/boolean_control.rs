use super::{boolean_value::BooleanValue, setting_value::SettingValue};
use gtk::prelude::*;
use relm4::{Component, ComponentParts, ComponentSender, binding::ConnectBinding};

#[derive(Default)]
pub struct BooleanInit {
    pub value: bool,
    pub default_value: Option<bool>,
}

pub struct BooleanControl {
    pub(super) value: BooleanValue,
    title: gtk::Box,
    size_group_widgets: Vec<(gtk::SizeGroup, gtk::Widget)>,
}

#[derive(Debug)]
pub enum BooleanControlMsg {
    Changed,
    Reset,
    SetVisible(bool),
}

#[relm4::component(pub)]
impl Component for BooleanControl {
    type Init = (
        BooleanInit,
        gtk::Box,
        Option<gtk::SizeGroup>,
        Option<gtk::SizeGroup>,
    );
    type Input = BooleanControlMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_spacing: 12,
            append: &model.title,

            #[name = "switch"]
            gtk::Switch {
                set_valign: gtk::Align::Center,
                set_halign: gtk::Align::End,
                bind: model.value.binding(),
            },
        },
    }

    fn init(
        (init, title, label_group, input_group): Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = Self {
            value: BooleanValue::new(init.value, init.default_value),
            title,
            size_group_widgets: Vec::new(),
        };
        let widgets = view_output!();
        for (group, widget) in [
            (label_group, model.title.clone().upcast::<gtk::Widget>()),
            (input_group, widgets.switch.clone().upcast()),
        ] {
            if let Some(group) = group {
                group.add_widget(&widget);
                model.size_group_widgets.push((group, widget));
            }
        }
        model.value.binding().connect_value_notify(move |_| {
            sender.input(BooleanControlMsg::Changed);
        });
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        _widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            BooleanControlMsg::Changed => {
                let _ = sender.output(());
            }
            BooleanControlMsg::Reset => self.value.reset(),
            BooleanControlMsg::SetVisible(visible) => {
                if root.get_visible() != visible {
                    for (group, widget) in &self.size_group_widgets {
                        if visible {
                            group.add_widget(widget);
                        } else {
                            group.remove_widget(widget);
                        }
                    }
                }
                root.set_visible(visible);
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        for (group, widget) in self.size_group_widgets.drain(..) {
            if group.widgets().contains(&widget) {
                group.remove_widget(&widget);
            }
        }
    }
}
