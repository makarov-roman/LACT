mod fan_curve_frame;

use crate::app::components::gpu_stats_section::{
    GpuStat, GpuStatsSection, GpuStatsSectionConfig, GpuStatsSectionMsg,
};
use crate::app::pages::PageUpdate;
use crate::{
    APP_BROKER, I18N,
    app::{
        components::{
            adjustment_row::{AdjustmentRow, AdjustmentRowInit, AdjustmentRowMsg},
            page_section::PageSection,
        },
        msg::AppMsg,
        utils::ext::RelmLaunchable as _,
    },
};
use adw::prelude::*;
use amdgpu_sysfs::gpu_handle::fan_control::FanInfo;
use anyhow::anyhow;
use fan_curve_frame::{
    CurveSetupMsg, DEFAULT_SPEED_RANGE, DEFAULT_TEMP_RANGE, FanCurveFrame, FanCurveFrameInit,
    FanCurveFrameMsg,
};
use gtk::glib::{self, SignalHandlerId};
use i18n_embed_fl::fl;
use lact_schema::{
    DeviceFlag, FanControlMode,
    config::{FanControlSettings, FanCurve, GpuConfig},
    default_fan_curve,
};
use relm4::{
    ComponentController, ComponentParts, ComponentSender, RelmObjectExt, RelmWidgetExt,
    binding::{Binding, BoolBinding, ConnectBinding, StringBinding},
    factory::FactoryHashMap,
};
use std::collections::HashSet;
use std::sync::Arc;

const AUTO_PAGE: &str = "automatic";
const CURVE_PAGE: &str = "curve";
const STATIC_PAGE: &str = "static";

pub struct ThermalsPage {
    stats_section: relm4::Controller<GpuStatsSection>,
    pmfw_rows: FactoryHashMap<ThermalSetting, AdjustmentRow<ThermalSetting>>,
    nvidia_target_temperature: FactoryHashMap<(), AdjustmentRow<()>>,
    zero_rpm_temperature: FactoryHashMap<(), AdjustmentRow<()>>,
    static_speed: FactoryHashMap<(), AdjustmentRow<()>>,
    // Keep the latest edit here while the other tab processes its sync message.
    zero_rpm_temperature_edit: Option<u32>,
    fan_curve_frame: relm4::Controller<FanCurveFrame>,
    selected_mode: StringBinding,

    custom_control_supported: bool,
    has_fan_speed: bool,
    has_pmfw: bool,
    has_auto_threshold: bool,
    label_size_group: gtk::SizeGroup,
    input_size_group: gtk::SizeGroup,
    zero_rpm: BoolBinding,
    zero_rpm_available: bool,
    zero_rpm_change_signal: SignalHandlerId,
    target_temperature_default: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThermalSetting {
    TargetTemperature,
    AcousticLimit,
    AcousticTarget,
    MinimumPwm,
}

#[derive(Debug)]
pub enum ThermalsPageMsg {
    Update { update: PageUpdate, initial: bool },
    FanModeSelected,
    RestNvidiaOptions,
    ZeroRpmTemperatureEdited,
    ZeroRpmTemperatureChanged(f64),
}

#[relm4::component(pub)]
impl relm4::Component for ThermalsPage {
    type Init = ();
    type Input = ThermalsPageMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "thermals-page",
            set_spacing: 15,
            set_margin_all: 15,
            set_margin_top: 20, // align with gpu picker

            model.stats_section.widget(),

            PageSection::new(&fl!(I18N, "thresholds-section")) {
                #[watch]
                set_visible: !model.nvidia_target_temperature.is_empty(),

                append_header = &gtk::Button {
                    set_label: &fl!(I18N, "default-button"),
                    set_halign: gtk::Align::End,
                    set_hexpand: true,
                    connect_clicked => ThermalsPageMsg::RestNvidiaOptions,
                },

                append_child = model.nvidia_target_temperature.widget(),
            },

            PageSection::new(&fl!(I18N, "fan-control-section")) {
                // Disable fan configuration when overdrive is disabled on GPUs that have PMFW (RDNA3+)
                #[watch]
                set_sensitive: model.custom_control_supported,

                append_child = &gtk::StackSwitcher {
                    set_stack: Some(&stack),
                },

                #[name = "stack"]
                append_child = &gtk::Stack {
                    set_vexpand: false,
                    set_vhomogeneous: false,
                    #[watch]
                    set_visible: model.fan_settings_available(),

                    add_titled[Some(AUTO_PAGE), &fl!(I18N, "auto-page")] = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 5,

                        model.pmfw_rows.widget().clone() -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 5,
                            #[watch]
                            set_visible: !model.pmfw_rows.is_empty(),
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 5,
                            #[watch]
                            set_visible: model.zero_rpm_available,

                            gtk::Label {
                                set_label: &fl!(I18N, "zero-rpm"),
                                set_size_group: &model.label_size_group,
                                set_xalign: 0.0,
                            },

                            gtk::Switch {
                                bind: &model.zero_rpm,
                                set_hexpand: true,
                                set_halign: gtk::Align::End,
                            },
                        },

                        model.zero_rpm_temperature.widget().clone() -> gtk::Box {
                            #[watch]
                            set_visible: !model.zero_rpm_temperature.is_empty(),
                        },

                        gtk::Button {
                            set_label: &fl!(I18N, "reset-now-button"),
                            set_size_group: &model.input_size_group,
                            set_halign: gtk::Align::End,
                            set_margin_vertical: 5,
                            set_tooltip_text: Some(&fl!(I18N, "pmfw-reset-warning")),
                            add_css_class: "destructive-action",
                            #[watch]
                            set_visible: model.has_pmfw_options(),
                            connect_clicked => move |_| {
                                APP_BROKER.send(AppMsg::ResetPmfw);
                            }
                        },
                    },
                    add_titled[Some(CURVE_PAGE), &fl!(I18N, "curve-page")] = model.fan_curve_frame.widget(),
                    add_titled[Some(STATIC_PAGE), &fl!(I18N, "static-page")] = &gtk::Box {
                        set_valign: gtk::Align::Start,
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 5,

                        model.static_speed.widget(),
                    },

                    add_binding: (&model.selected_mode, "visible-child-name"),
                    connect_visible_child_name_notify => ThermalsPageMsg::FanModeSelected @ mode_selected_signal,
                }
            }
        },
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let zero_rpm = BoolBinding::new(false);
        let zero_rpm_change_signal = zero_rpm.connect_value_notify(|_| {
            APP_BROKER.send(AppMsg::SettingsChanged);
        });
        let fan_curve_frame = FanCurveFrame::builder()
            .launch(FanCurveFrameInit {
                zero_rpm: zero_rpm.clone(),
            })
            .forward(
                sender.input_sender(),
                ThermalsPageMsg::ZeroRpmTemperatureChanged,
            );
        let stats_section = GpuStatsSection::detach(GpuStatsSectionConfig {
            stats: HashSet::from([
                GpuStat::Throttling,
                GpuStat::Temperature,
                GpuStat::PowerUsage,
                GpuStat::FanSpeed,
            ]),
        });

        let model = Self {
            pmfw_rows: FactoryHashMap::builder()
                .launch_default()
                .forward(APP_BROKER.sender(), |()| AppMsg::SettingsChanged),
            nvidia_target_temperature: FactoryHashMap::builder()
                .launch_default()
                .forward(APP_BROKER.sender(), |()| AppMsg::SettingsChanged),
            zero_rpm_temperature: FactoryHashMap::builder()
                .launch_default()
                .forward(sender.input_sender(), |()| {
                    ThermalsPageMsg::ZeroRpmTemperatureEdited
                }),
            static_speed: FactoryHashMap::builder()
                .launch_default()
                .forward(APP_BROKER.sender(), |()| AppMsg::SettingsChanged),
            zero_rpm_temperature_edit: None,
            stats_section,
            fan_curve_frame,
            label_size_group: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
            input_size_group: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
            zero_rpm,
            zero_rpm_available: false,
            zero_rpm_change_signal,
            target_temperature_default: None,
            custom_control_supported: false,
            has_fan_speed: false,
            has_pmfw: false,
            has_auto_threshold: false,
            selected_mode: StringBinding::new(AUTO_PAGE),
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            ThermalsPageMsg::Update { update, initial } => match update {
                PageUpdate::Info(info) => {
                    self.stats_section
                        .emit(GpuStatsSectionMsg::Info(info.clone()));

                    self.custom_control_supported =
                        info.flags.contains(&DeviceFlag::ConfigurableFanControl);
                    self.has_pmfw = info.flags.contains(&DeviceFlag::HasPmfw);
                    self.has_auto_threshold = info.flags.contains(&DeviceFlag::AutoFanThreshold);
                }
                PageUpdate::Stats(stats) => {
                    self.stats_section
                        .emit(GpuStatsSectionMsg::Stats(stats.clone()));

                    self.has_fan_speed =
                        stats.fan.pwm_current.is_some() || stats.fan.speed_current.is_some();
                    if initial {
                        let page_name = match stats.fan.control_mode {
                            Some(mode) if stats.fan.control_enabled => match mode {
                                FanControlMode::Static => STATIC_PAGE,
                                FanControlMode::Curve => CURVE_PAGE,
                            },
                            _ => AUTO_PAGE,
                        };

                        widgets.stack.block_signal(&widgets.mode_selected_signal);
                        self.selected_mode.set(page_name.to_owned());
                        widgets.stack.unblock_signal(&widgets.mode_selected_signal);

                        let speed_range = stats
                            .fan
                            .pwm_min
                            .zip(stats.fan.pwm_max)
                            .map(|(min, max)| {
                                let min = min as f32 / f32::from(u8::MAX);
                                let max = max as f32 / f32::from(u8::MAX);
                                min..=max
                            })
                            .unwrap_or(DEFAULT_SPEED_RANGE);

                        self.pmfw_rows.clear();
                        self.nvidia_target_temperature.clear();
                        self.zero_rpm_temperature.clear();
                        self.static_speed.clear();
                        self.zero_rpm_temperature_edit = None;

                        self.static_speed.insert(
                            (),
                            AdjustmentRowInit {
                                title: glib::markup_escape_text(&fl!(I18N, "static-speed")).into(),
                                value: (stats.fan.static_speed.unwrap_or(0.5) * 100.0).into(),
                                lower: (*speed_range.start() as f64 * 100.0).round(),
                                upper: (*speed_range.end() as f64 * 100.0).round(),
                                page_increment: 5.0,
                                ..Default::default()
                            },
                        );

                        let temperature_range = stats
                            .fan
                            .temperature_range
                            .map(|(start, end)| start as f32..=end as f32)
                            .unwrap_or(DEFAULT_TEMP_RANGE);

                        let msg = CurveSetupMsg {
                            curve: stats.fan.curve.clone().unwrap_or_else(default_fan_curve),
                            hw_based: self.has_pmfw,
                            current_temperatures: stats.temps.clone(),
                            temperature_key: stats.fan.temperature_key.clone(),
                            spindown_delay: stats.fan.spindown_delay_ms,
                            change_threshold: stats.fan.change_threshold,
                            speed_range,
                            temperature_range,
                            auto_threshold_supported: self.has_auto_threshold,
                            auto_threshold: stats.fan.auto_threshold,
                            zero_rpm_available: stats.fan.pmfw_info.zero_rpm_enable.is_some(),
                            zero_rpm_temperature: stats.fan.pmfw_info.zero_rpm_temperature,
                        };
                        self.fan_curve_frame.emit(FanCurveFrameMsg::Curve(msg));

                        let info = stats.fan.pmfw_info;
                        for (setting, title, info) in [
                            (
                                ThermalSetting::TargetTemperature,
                                fl!(I18N, "target-temp"),
                                info.target_temp,
                            ),
                            (
                                ThermalSetting::AcousticLimit,
                                fl!(I18N, "acoustic-limit"),
                                info.acoustic_limit,
                            ),
                            (
                                ThermalSetting::AcousticTarget,
                                fl!(I18N, "acoustic-target"),
                                info.acoustic_target,
                            ),
                            (
                                ThermalSetting::MinimumPwm,
                                fl!(I18N, "min-fan-speed"),
                                info.minimum_pwm,
                            ),
                        ] {
                            if let Some(init) = fan_info_init(title, info) {
                                self.pmfw_rows.insert(setting, init);
                                self.pmfw_rows.send(
                                    &setting,
                                    AdjustmentRowMsg::AddSizeGroup {
                                        label_group: self.label_size_group.clone(),
                                        input_group: self.input_size_group.clone(),
                                    },
                                );
                            }
                        }
                        for (rows, title, info) in [
                            (
                                &mut self.nvidia_target_temperature,
                                fl!(I18N, "target-temp"),
                                stats.nvidia_thermal_info.target_temp,
                            ),
                            (
                                &mut self.zero_rpm_temperature,
                                fl!(I18N, "zero-rpm-stop-temp"),
                                info.zero_rpm_temperature,
                            ),
                        ] {
                            if let Some(init) = fan_info_init(title, info) {
                                rows.insert((), init);
                                rows.send(
                                    &(),
                                    AdjustmentRowMsg::AddSizeGroup {
                                        label_group: self.label_size_group.clone(),
                                        input_group: self.input_size_group.clone(),
                                    },
                                );
                            }
                        }
                        self.target_temperature_default =
                            stats.nvidia_thermal_info.target_temp_default;
                        self.zero_rpm_available = info.zero_rpm_enable.is_some();
                        self.zero_rpm.block_signal(&self.zero_rpm_change_signal);
                        self.zero_rpm.set(info.zero_rpm_enable.unwrap_or(false));
                        self.zero_rpm.unblock_signal(&self.zero_rpm_change_signal);
                    }
                }
            },
            ThermalsPageMsg::FanModeSelected => {
                APP_BROKER.send(AppMsg::SettingsChanged);
            }
            ThermalsPageMsg::ZeroRpmTemperatureEdited => {
                if let Some(value) = self
                    .zero_rpm_temperature
                    .get(&())
                    .and_then(|row| row.get_changed_value())
                {
                    self.zero_rpm_temperature_edit = Some(value as u32);
                    self.fan_curve_frame
                        .emit(FanCurveFrameMsg::SyncZeroRpmTemperature(value));
                }
                APP_BROKER.send(AppMsg::SettingsChanged);
            }
            ThermalsPageMsg::ZeroRpmTemperatureChanged(value) => {
                if !self.zero_rpm_temperature.is_empty() {
                    self.zero_rpm_temperature_edit = Some(value as u32);
                    self.zero_rpm_temperature
                        .send(&(), AdjustmentRowMsg::SyncValue(value));
                }
                APP_BROKER.send(AppMsg::SettingsChanged);
            }
            ThermalsPageMsg::RestNvidiaOptions => {
                if let Some(default) = self.target_temperature_default {
                    if !self.nvidia_target_temperature.is_empty() {
                        self.nvidia_target_temperature
                            .send(&(), AdjustmentRowMsg::SetValue(default as f64));
                    }
                } else {
                    APP_BROKER.send(AppMsg::Error(Arc::new(anyhow!(
                        "No default target temperature present"
                    ))));
                }
            }
        }

        self.update_view(widgets, sender);
    }
}

impl ThermalsPage {
    fn fan_settings_available(&self) -> bool {
        self.has_fan_speed && (self.selected_mode.value() != AUTO_PAGE || self.has_pmfw_options())
    }

    pub fn apply_config(&self, config: &mut GpuConfig) {
        let selected_page = self.selected_mode.value();

        if selected_page == AUTO_PAGE {
            config.fan_control_enabled = false;
        } else {
            config.fan_control_enabled = true;
            let fan_settings = config
                .fan_control_settings
                .get_or_insert_with(FanControlSettings::default);

            match selected_page.as_str() {
                CURVE_PAGE => {
                    fan_settings.mode = FanControlMode::Curve;

                    let fan_curve_model = self.fan_curve_frame.model();
                    fan_settings.curve = FanCurve(fan_curve_model.get_curve());
                    fan_settings.change_threshold = Some(fan_curve_model.change_threshold());
                    fan_settings.spindown_delay_ms = Some(fan_curve_model.spindown_delay());

                    if let Some(threshold) = fan_curve_model.auto_threshold() {
                        fan_settings.auto_threshold = Some(threshold);
                    }

                    if let Some(temp_key) = fan_curve_model.temperature_key() {
                        fan_settings.temperature_key = temp_key;
                    }
                }
                STATIC_PAGE => {
                    fan_settings.mode = FanControlMode::Static;
                    fan_settings.static_speed = self
                        .static_speed
                        .get(&())
                        .map(|row| row.get_value() as f32 / 100.0)
                        .unwrap_or(0.5);
                }
                _ => unreachable!("Invalid fan control page selected"),
            }
        }

        let pmfw_config = &mut config.pmfw_options;
        for (setting, config_value) in [
            (
                ThermalSetting::AcousticLimit,
                &mut pmfw_config.acoustic_limit,
            ),
            (
                ThermalSetting::AcousticTarget,
                &mut pmfw_config.acoustic_target,
            ),
            (
                ThermalSetting::TargetTemperature,
                &mut pmfw_config.target_temperature,
            ),
            (ThermalSetting::MinimumPwm, &mut pmfw_config.minimum_pwm),
        ] {
            if let Some(value) = self
                .pmfw_rows
                .get(&setting)
                .and_then(|row| row.get_changed_value())
            {
                *config_value = Some(value as u32);
            }
        }
        if let Some(value) = self.zero_rpm_temperature_edit {
            pmfw_config.zero_rpm_threshold = Some(value);
        }
        if let Some(value) = self
            .nvidia_target_temperature
            .get(&())
            .and_then(|row| row.get_changed_value())
        {
            config.nvidia_thermal_options.target_temperature = Some(value as u32);
        }
        if self.zero_rpm_available {
            pmfw_config.zero_rpm = Some(self.zero_rpm.value());
        }
    }

    fn has_pmfw_options(&self) -> bool {
        self.zero_rpm_available
            || !self.pmfw_rows.is_empty()
            || !self.zero_rpm_temperature.is_empty()
    }
}

fn fan_info_init(title: String, info: Option<FanInfo>) -> Option<AdjustmentRowInit> {
    let info = info?;
    let (min, max) = info.allowed_range.unwrap_or((0, info.current));
    (min != 0 || max != 0).then(|| AdjustmentRowInit {
        title: glib::markup_escape_text(&title).into(),
        value: info.current as f64,
        lower: min as f64,
        upper: max as f64,
        page_increment: 5.0,
        ..Default::default()
    })
}
