mod heuristics_list;

use crate::{
    APP_BROKER, I18N,
    app::{
        components::{
            adjustment_card::AdjustmentCard,
            page_section::PageSection,
            setting_row::{
                AdjustmentInit, SelectInit, SelectOption, SettingRow, SettingRowInit,
                SettingRowMsg, SettingRowValue,
            },
        },
        msg::AppMsg,
        pages::oc_page::OcPageMsg,
    },
};
use adw::prelude::*;
use amdgpu_sysfs::gpu_handle::{PerformanceLevel, power_profile_mode::PowerProfileModesTable};
use heuristics_list::PowerProfileHeuristicsList;
use i18n_embed_fl::fl;
use lact_schema::PowerStats;
use nvml_wrapper::enums::device::PowerMizerMode;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, WidgetTemplate,
    factory::FactoryHashMap,
};

const PERFORMANCE_LEVELS: [PerformanceLevel; 8] = [
    PerformanceLevel::Auto,
    PerformanceLevel::High,
    PerformanceLevel::Low,
    PerformanceLevel::Manual,
    PerformanceLevel::ProfileStandard,
    PerformanceLevel::ProfileMinSclk,
    PerformanceLevel::ProfileMinMclk,
    PerformanceLevel::ProfilePeak,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PowerSetting {
    PowerCap,
    PerformanceLevel,
    PowerProfile,
    PowerMizer,
}

pub struct PowerFrame {
    rows: FactoryHashMap<PowerSetting, SettingRow<PowerSetting>>,
    performance_levels: Vec<PerformanceLevel>,
    power_profile_modes_table: Option<PowerProfileModesTable>,
    supported_power_mizer_modes: Vec<PowerMizerMode>,
    heuristics_notebook: gtk::Notebook,
    heuristics_components: Vec<relm4::Controller<PowerProfileHeuristicsList>>,
}

#[derive(Debug)]
pub enum PowerFrameMsg {
    PowerStats(PowerStats),
    PerformanceLevel(Option<PerformanceLevel>),
    SetPerformanceLevel(PerformanceLevel),
    PowerProfileModes(Option<PowerProfileModesTable>),
    PowerMizerInfo {
        active: Option<PowerMizerMode>,
        supported: Option<Vec<PowerMizerMode>>,
    },
    SettingsChanged,
    Reset,
}

#[relm4::component(pub)]
impl relm4::Component for PowerFrame {
    type Init = ();
    type Input = PowerFrameMsg;
    type Output = OcPageMsg;
    type CommandOutput = ();

    view! {
        #[root]
        PageSection {
            set_name: fl!(I18N, "power-section"),
            set_hide_visible_container: true,
            #[watch]
            set_visible: !model.rows.is_empty(),

            append_header = &gtk::Button {
                set_label: &fl!(I18N, "default-button"),
                connect_clicked => PowerFrameMsg::Reset,
                set_halign: gtk::Align::End,
                set_hexpand: true,
                #[watch]
                set_visible: model.rows.get(&PowerSetting::PowerCap).is_some(),
            },
            append_child = model.rows.widget(),
        },
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            rows: FactoryHashMap::builder()
                .launch(AdjustmentCard::init(()).content.clone())
                .forward(sender.input_sender(), |()| PowerFrameMsg::SettingsChanged),
            performance_levels: PERFORMANCE_LEVELS.to_vec(),
            power_profile_modes_table: None,
            supported_power_mizer_modes: Vec::new(),
            heuristics_notebook: gtk::Notebook::new(),
            heuristics_components: Vec::new(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            PowerFrameMsg::PowerStats(power) => {
                self.rows.remove(&PowerSetting::PowerCap);
                if let Some(value) = power.cap_current {
                    self.rows.insert(
                        PowerSetting::PowerCap,
                        SettingRowInit::new(
                            fl!(I18N, "power-cap"),
                            AdjustmentInit {
                                unit: fl!(I18N, "watt"),
                                value,
                                default_value: Some(power.cap_default.unwrap_or_default()),
                                lower: power.cap_min.unwrap_or_default(),
                                upper: power.cap_max.unwrap_or_default(),
                                ..Default::default()
                            },
                        ),
                    );
                }
            }
            PowerFrameMsg::PerformanceLevel(level) => {
                self.rows.remove(&PowerSetting::PerformanceLevel);
                self.performance_levels = PERFORMANCE_LEVELS.to_vec();
                if let Some(level) = level {
                    if !self.performance_levels.contains(&level) {
                        self.performance_levels.push(level);
                    }
                    self.rows.insert(
                        PowerSetting::PerformanceLevel,
                        SettingRowInit::new(
                            fl!(I18N, "performance-level"),
                            SelectInit {
                                options: self
                                    .performance_levels
                                    .iter()
                                    .copied()
                                    .map(performance_option)
                                    .collect(),
                                selected: self
                                    .performance_levels
                                    .iter()
                                    .position(|candidate| *candidate == level)
                                    .map(|index| index as u32),
                                ..Default::default()
                            },
                        ),
                    );
                }
                let _ = sender.output(OcPageMsg::PerformanceLevelChanged);
            }
            PowerFrameMsg::SetPerformanceLevel(level) => {
                if self.rows.get(&PowerSetting::PerformanceLevel).is_some()
                    && let Some(index) = self
                        .performance_levels
                        .iter()
                        .position(|candidate| *candidate == level)
                {
                    self.rows.send(
                        &PowerSetting::PerformanceLevel,
                        SettingRowMsg::SetValue(SettingRowValue::Selected(Some(index as u32))),
                    );
                }
            }
            PowerFrameMsg::PowerProfileModes(table) => {
                self.rows.remove(&PowerSetting::PowerProfile);
                self.heuristics_components.clear();
                self.heuristics_notebook = gtk::Notebook::new();
                if let Some(table) = &table {
                    self.rows.insert(
                        PowerSetting::PowerProfile,
                        SettingRowInit {
                            info_text: fl!(I18N, "manual-level-needed"),
                            ..SettingRowInit::new(
                                fl!(I18N, "power-profile-mode"),
                                SelectInit {
                                    options: table
                                        .modes
                                        .values()
                                        .map(|mode| mode.name.clone().into())
                                        .collect(),
                                    selected: table
                                        .modes
                                        .keys()
                                        .position(|key| *key == table.active)
                                        .map(|index| index as u32),
                                    details: table
                                        .modes
                                        .values()
                                        .any(|mode| !mode.components.is_empty())
                                        .then(|| self.heuristics_notebook.clone().upcast()),
                                    ..Default::default()
                                },
                            )
                        },
                    );
                }
                self.power_profile_modes_table = table;
                self.update_heuristics();
            }
            PowerFrameMsg::PowerMizerInfo { active, supported } => {
                self.rows.remove(&PowerSetting::PowerMizer);
                self.supported_power_mizer_modes = supported.unwrap_or_default();
                if let Some(active) = active {
                    if !self.supported_power_mizer_modes.contains(&active) {
                        self.supported_power_mizer_modes.push(active);
                    }
                    self.rows.insert(
                        PowerSetting::PowerMizer,
                        SettingRowInit::new(
                            fl!(I18N, "power-mizer-mode"),
                            SelectInit {
                                options: self
                                    .supported_power_mizer_modes
                                    .iter()
                                    .copied()
                                    .map(power_mizer_option)
                                    .collect(),
                                selected: self
                                    .supported_power_mizer_modes
                                    .iter()
                                    .position(|mode| *mode == active)
                                    .map(|index| index as u32),
                                ..Default::default()
                            },
                        ),
                    );
                }
            }
            PowerFrameMsg::SettingsChanged => {
                if let Some(selected) = self.selected(PowerSetting::PowerProfile)
                    && let Some(table) = &mut self.power_profile_modes_table
                    && let Some(key) = table.modes.keys().nth(selected as usize).copied()
                    && table.active != key
                {
                    table.active = key;
                    self.update_heuristics();
                }
                APP_BROKER.send(AppMsg::SettingsChanged);
                let _ = sender.output(OcPageMsg::PerformanceLevelChanged);
            }
            PowerFrameMsg::Reset => {
                if self.rows.get(&PowerSetting::PowerCap).is_some() {
                    self.rows
                        .send(&PowerSetting::PowerCap, SettingRowMsg::Reset);
                }
            }
        }

        let manual = self.performance_level() == Some(PerformanceLevel::Manual);
        if self.rows.get(&PowerSetting::PowerProfile).is_some() {
            self.rows.send(
                &PowerSetting::PowerProfile,
                SettingRowMsg::SetSensitive(manual),
            );
        }
        let custom = self
            .power_profile_modes_table
            .as_ref()
            .and_then(|table| table.modes.get(&table.active))
            .is_some_and(|mode| mode.is_custom());
        self.heuristics_notebook.set_sensitive(manual && custom);

        // Hash-map factories append replacements; keep the card in setting order.
        let mut order = self
            .rows
            .iter()
            .map(|(key, row)| (*key, row.widget().clone()))
            .collect::<Vec<_>>();
        order.sort_by_key(|(key, _)| *key);
        self.rows.widget().set_sort_func(move |left, right| {
            let position = |row: &gtk::ListBoxRow| {
                order
                    .iter()
                    .position(|(_, widget)| row.child().as_ref() == Some(widget.upcast_ref()))
                    .unwrap_or(usize::MAX)
            };
            position(left).cmp(&position(right)).into()
        });
    }
}

impl PowerFrame {
    fn selected(&self, setting: PowerSetting) -> Option<u32> {
        self.rows.get(&setting)?.get_value().as_selected()
    }

    fn update_heuristics(&mut self) {
        while self.heuristics_notebook.n_pages() > 0 {
            self.heuristics_notebook.remove_page(Some(0));
        }
        self.heuristics_components.clear();
        if let Some(table) = &self.power_profile_modes_table
            && let Some(active) = table.modes.get(&table.active)
        {
            for component in &active.components {
                let list = PowerProfileHeuristicsList::builder()
                    .launch((component.values.clone(), table.value_names.clone()))
                    .detach();
                self.heuristics_notebook.append_page(
                    list.widget(),
                    Some(&gtk::Label::new(Some(
                        component.clock_type.as_deref().unwrap_or("All"),
                    ))),
                );
                self.heuristics_components.push(list);
            }
        }
        self.heuristics_notebook
            .set_show_tabs(self.heuristics_components.len() > 1);
    }

    pub fn get_user_cap(&self) -> Option<f64> {
        self.rows
            .get(&PowerSetting::PowerCap)?
            .get_changed_value()?
            .as_numeric()
            .filter(|value| *value != 0.0)
    }

    pub fn performance_level(&self) -> Option<PerformanceLevel> {
        self.performance_levels
            .get(self.selected(PowerSetting::PerformanceLevel)? as usize)
            .copied()
    }

    pub fn active_power_mizer_mode(&self) -> Option<PowerMizerMode> {
        self.supported_power_mizer_modes
            .get(self.selected(PowerSetting::PowerMizer)? as usize)
            .copied()
    }

    pub fn power_profile_mode(&self) -> Option<u16> {
        if self.performance_level() == Some(PerformanceLevel::Manual) {
            let table = self.power_profile_modes_table.as_ref()?;
            table
                .modes
                .keys()
                .nth(self.selected(PowerSetting::PowerProfile)? as usize)
                .copied()
        } else {
            None
        }
    }

    pub fn power_profile_mode_custom_heuristics(&self) -> Vec<Vec<Option<i32>>> {
        if let Some(table) = &self.power_profile_modes_table
            && let Some(mode) = table.modes.get(&table.active)
            && mode.is_custom()
        {
            self.heuristics_components
                .iter()
                .map(|list| list.model().get_values())
                .collect()
        } else {
            Vec::new()
        }
    }
}

fn performance_option(level: PerformanceLevel) -> SelectOption {
    let (label, description) = match level {
        PerformanceLevel::Auto => (
            fl!(I18N, "performance-level-auto"),
            fl!(I18N, "performance-level-auto-description"),
        ),
        PerformanceLevel::High => (
            fl!(I18N, "performance-level-high"),
            fl!(I18N, "performance-level-high-description"),
        ),
        PerformanceLevel::Low => (
            fl!(I18N, "performance-level-low"),
            fl!(I18N, "performance-level-low-description"),
        ),
        PerformanceLevel::Manual => (
            fl!(I18N, "performance-level-manual"),
            fl!(I18N, "performance-level-manual-description"),
        ),
        PerformanceLevel::ProfileStandard => (
            fl!(I18N, "performance-level-profile-standard"),
            fl!(I18N, "performance-level-profile-standard-description"),
        ),
        PerformanceLevel::ProfileMinSclk => (
            fl!(I18N, "performance-level-profile-min-sclk"),
            fl!(I18N, "performance-level-profile-min-sclk-description"),
        ),
        PerformanceLevel::ProfileMinMclk => (
            fl!(I18N, "performance-level-profile-min-mclk"),
            fl!(I18N, "performance-level-profile-min-mclk-description"),
        ),
        PerformanceLevel::ProfilePeak => (
            fl!(I18N, "performance-level-profile-peak"),
            fl!(I18N, "performance-level-profile-peak-description"),
        ),
        PerformanceLevel::PerfDeterminism | PerformanceLevel::ProfileExit => {
            (level.to_string(), String::new())
        }
    };
    SelectOption { label, description }
}

fn power_mizer_option(mode: PowerMizerMode) -> SelectOption {
    let (label, description) = match mode {
        PowerMizerMode::Auto => (
            fl!(I18N, "power-mizer-mode-auto"),
            fl!(I18N, "power-mizer-mode-auto-description"),
        ),
        PowerMizerMode::Adaptive => (
            fl!(I18N, "power-mizer-mode-adaptive"),
            fl!(I18N, "power-mizer-mode-adaptive-description"),
        ),
        PowerMizerMode::PreferMaximumPerformance => (
            fl!(I18N, "power-mizer-mode-prefer-maximum-performance"),
            fl!(
                I18N,
                "power-mizer-mode-prefer-maximum-performance-description"
            ),
        ),
        PowerMizerMode::PreferConsistentPerformance => (
            fl!(I18N, "power-mizer-mode-prefer-consistent-performance"),
            fl!(
                I18N,
                "power-mizer-mode-prefer-consistent-performance-description"
            ),
        ),
    };
    SelectOption { label, description }
}

#[cfg(all(test, feature = "gtk-tests"))]
mod tests;
