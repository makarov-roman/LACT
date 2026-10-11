use super::OcPageMsg;
use crate::{
    APP_BROKER, I18N,
    app::{
        components::{
            adjustment_card::AdjustmentCard,
            page_section::PageSection,
            setting_row::{SelectInit, SelectOption, SettingRow, SettingRowInit, SettingRowValue},
        },
        msg::AppMsg,
        utils::formatting,
    },
};
use gtk::prelude::*;
use i18n_embed_fl::fl;
use lact_schema::AmdUmaCarveout;
use relm4::{ComponentParts, ComponentSender, WidgetTemplate, factory::FactoryHashMap};

pub struct IGpuFrame {
    rows: FactoryHashMap<(), SettingRow<()>>,
}

#[derive(Debug)]
pub enum IGpuFrameMsg {
    UmaCarveout(Option<AmdUmaCarveout>),
}

#[relm4::component(pub)]
impl relm4::Component for IGpuFrame {
    type Init = ();
    type Input = IGpuFrameMsg;
    type Output = OcPageMsg;
    type CommandOutput = ();

    view! {
        PageSection {
            set_hide_visible_container: true,
            set_name: fl!(I18N, "igpu-section"),
            #[watch]
            set_visible: !model.rows.is_empty(),

            append_child = model.rows.widget(),
        }
    }

    fn init(
        _: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            rows: FactoryHashMap::builder()
                .launch(AdjustmentCard::init(()).content.clone())
                .forward(APP_BROKER.sender(), |()| AppMsg::SettingsChanged),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            IGpuFrameMsg::UmaCarveout(carveout) => {
                self.rows.clear();
                if let Some(carveout) = carveout.filter(|carveout| !carveout.options.is_empty()) {
                    let options = carveout
                        .options
                        .into_iter()
                        .map(|option| {
                            let size = formatting::fmt_human_bytes(option.size_bytes, None);
                            SelectOption {
                                label: match option.name {
                                    Some(name) => format!("{size} ({name})"),
                                    None => size,
                                },
                                description: fl!(I18N, "uma-carveout-caption"),
                            }
                        })
                        .collect();
                    self.rows.insert(
                        (),
                        SettingRowInit {
                            title: fl!(I18N, "uma-carveout"),
                            control: SelectInit {
                                options,
                                selected: Some(carveout.current as u32),
                                ..Default::default()
                            }
                            .into(),
                            ..Default::default()
                        },
                    );
                }
            }
        }
    }
}

impl IGpuFrame {
    pub fn serialize(&self) -> Option<usize> {
        self.rows
            .get(&())
            .and_then(SettingRow::serialize)
            .and_then(SettingRowValue::as_selected)
            .map(|value| value as usize)
    }
}
