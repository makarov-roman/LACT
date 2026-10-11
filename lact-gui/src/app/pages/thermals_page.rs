mod fan_curve_frame;

use crate::app::components::gpu_stats_section::{
    GpuStat, GpuStatsSection, GpuStatsSectionConfig, GpuStatsSectionMsg,
};
use crate::app::pages::PageUpdate;
use crate::{
    APP_BROKER, I18N,
    app::{
        components::{
            adjustment_card::AdjustmentCard,
            page_section::PageSection,
            setting_row::{
                AdjustmentInit, BooleanInit, SettingRow, SettingRowInit, SettingRowMsg,
                SettingRowValue,
            },
        },
        msg::AppMsg,
        utils::ext::RelmLaunchable as _,
    },
};
use adw::prelude::*;
use amdgpu_sysfs::gpu_handle::fan_control::FanInfo;
use fan_curve_frame::{
    CurveSetupMsg, DEFAULT_SPEED_RANGE, DEFAULT_TEMP_RANGE, FanCurveFrame, FanCurveFrameMsg,
};
use gtk::glib;
use i18n_embed_fl::fl;
use lact_schema::{
    DeviceFlag, FanControlMode, NvidiaThermalOptions, PmfwOptions, config::FanControlSettings,
    default_fan_curve,
};
use relm4::{
    ComponentController, ComponentParts, ComponentSender, RelmWidgetExt, WidgetTemplate,
    factory::FactoryHashMap,
};
use std::collections::HashSet;

pub struct ThermalsPage {
    stats_section: relm4::Controller<GpuStatsSection>,
    pmfw_rows: FactoryHashMap<ThermalSetting, SettingRow<ThermalSetting>>,
    nvidia_target_temperature: FactoryHashMap<(), SettingRow<()>>,
    static_speed: FactoryHashMap<(), SettingRow<()>>,
    fan_curve_frame: relm4::Controller<FanCurveFrame>,
    selected_mode: Option<FanControlMode>,

    custom_control_supported: bool,
    has_fan_speed: bool,
    has_pmfw: bool,
    has_auto_threshold: bool,
    label_size_group: gtk::SizeGroup,
    input_size_group: gtk::SizeGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThermalSetting {
    TargetTemperature,
    AcousticLimit,
    AcousticTarget,
    MinimumPwm,
    ZeroRpmEnabled,
    ZeroRpmTemperature,
}

impl ThermalSetting {
    fn is_visible(self, mode: Option<FanControlMode>) -> bool {
        match self {
            Self::ZeroRpmEnabled | Self::ZeroRpmTemperature => mode != Some(FanControlMode::Static),
            _ => mode.is_none(),
        }
    }
}

#[derive(Debug)]
pub enum ThermalsPageMsg {
    FanControlInterval(u64),
    Update { update: PageUpdate, initial: bool },
    FanModeSelected(Option<FanControlMode>),
    RestNvidiaOptions,
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

            PageSection {
                set_name: fl!(I18N, "thresholds-section"),
                set_hide_visible_container: true,
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

            PageSection {
                set_name: fl!(I18N, "fan-control-section"),
                add_css_class: "fan-control-section",
                // Disable fan configuration when overdrive is disabled on GPUs that have PMFW (RDNA3+)
                #[watch]
                set_sensitive: model.custom_control_supported,

                append_child = &gtk::Box {
                    add_css_class: "linked",
                    set_homogeneous: true,

                    #[name = "automatic_mode"]
                    gtk::ToggleButton {
                        set_label: &fl!(I18N, "auto-page"),
                        #[watch]
                        #[block_signal(automatic_mode_signal)]
                        set_active: model.selected_mode.is_none(),
                        connect_toggled[sender] => move |button| {
                            if button.is_active() {
                                sender.input(ThermalsPageMsg::FanModeSelected(None));
                            }
                        } @ automatic_mode_signal,
                    },

                    gtk::ToggleButton {
                        set_label: &fl!(I18N, "curve-page"),
                        set_group: Some(&automatic_mode),
                        #[watch]
                        #[block_signal(curve_mode_signal)]
                        set_active: model.selected_mode == Some(FanControlMode::Curve),
                        connect_toggled[sender] => move |button| {
                            if button.is_active() {
                                sender.input(ThermalsPageMsg::FanModeSelected(Some(FanControlMode::Curve)));
                            }
                        } @ curve_mode_signal,
                    },

                    gtk::ToggleButton {
                        set_label: &fl!(I18N, "static-page"),
                        set_group: Some(&automatic_mode),
                        #[watch]
                        #[block_signal(static_mode_signal)]
                        set_active: model.selected_mode == Some(FanControlMode::Static),
                        connect_toggled[sender] => move |button| {
                            if button.is_active() {
                                sender.input(ThermalsPageMsg::FanModeSelected(Some(FanControlMode::Static)));
                            }
                        } @ static_mode_signal,
                    },
                },

                append_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 5,
                    #[watch]
                    set_visible: model.fan_settings_available(),

                    model.fan_curve_frame.widget().clone() -> gtk::Box {
                        #[watch]
                        set_visible: model.selected_mode == Some(FanControlMode::Curve),
                    },

                    model.static_speed.widget().clone() -> gtk::ListBox {
                        #[watch]
                        set_visible: model.selected_mode == Some(FanControlMode::Static),
                    },

                    model.pmfw_rows.widget().clone() -> gtk::ListBox {
                        #[watch]
                        set_visible: model.pmfw_rows.keys().any(|setting| setting.is_visible(model.selected_mode)),
                    },

                    gtk::Button {
                        set_label: &fl!(I18N, "reset-now-button"),
                        set_size_group: &model.input_size_group,
                        set_halign: gtk::Align::End,
                        set_margin_vertical: 5,
                        set_tooltip_text: Some(&fl!(I18N, "pmfw-reset-warning")),
                        add_css_class: "destructive-action",
                        #[watch]
                        set_visible: model.selected_mode.is_none() && model.has_pmfw_options(),
                        connect_clicked => move |_| {
                            APP_BROKER.send(AppMsg::ResetPmfw);
                        }
                    },
                }
            }
        },
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let fan_curve_frame = FanCurveFrame::detach(());
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
                .launch(AdjustmentCard::init(()).content.clone())
                .forward(APP_BROKER.sender(), |()| AppMsg::SettingsChanged),
            nvidia_target_temperature: FactoryHashMap::builder()
                .launch(AdjustmentCard::init(()).content.clone())
                .forward(APP_BROKER.sender(), |()| AppMsg::SettingsChanged),
            static_speed: FactoryHashMap::builder()
                .launch(AdjustmentCard::init(()).content.clone())
                .forward(APP_BROKER.sender(), |()| AppMsg::SettingsChanged),
            stats_section,
            fan_curve_frame,
            label_size_group: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
            input_size_group: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
            custom_control_supported: false,
            has_fan_speed: false,
            has_pmfw: false,
            has_auto_threshold: false,
            selected_mode: None,
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
            ThermalsPageMsg::FanControlInterval(interval) => {
                self.fan_curve_frame
                    .emit(FanCurveFrameMsg::Interval(interval));
            }
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
                        self.selected_mode =
                            stats.fan.control_mode.filter(|_| stats.fan.control_enabled);

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
                        self.static_speed.clear();

                        self.static_speed.insert(
                            (),
                            SettingRowInit {
                                title: glib::markup_escape_text(&fl!(I18N, "static-speed")).into(),
                                control: AdjustmentInit {
                                    value: (stats.fan.static_speed.unwrap_or(0.5) * 100.0).into(),
                                    lower: (*speed_range.start() as f64 * 100.0).round(),
                                    upper: (*speed_range.end() as f64 * 100.0).round(),
                                    page_increment: 5.0,
                                    ..Default::default()
                                }
                                .into(),
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
                        };
                        self.fan_curve_frame.emit(FanCurveFrameMsg::Curve(msg));

                        let info = stats.fan.pmfw_info;
                        let lower_label_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
                        let upper_label_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
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
                            if let Some(init) = fan_info_init(info) {
                                self.pmfw_rows.insert(
                                    setting,
                                    SettingRowInit {
                                        label_group: Some(self.label_size_group.clone()),
                                        input_group: Some(self.input_size_group.clone()),
                                        title: glib::markup_escape_text(&title).into(),
                                        control: AdjustmentInit {
                                            lower_label_group: Some(lower_label_group.clone()),
                                            upper_label_group: Some(upper_label_group.clone()),
                                            ..init
                                        }
                                        .into(),
                                        ..Default::default()
                                    },
                                );
                            }
                        }
                        if let Some(mut init) = fan_info_init(stats.nvidia_thermal_info.target_temp)
                        {
                            init.default_value =
                                stats.nvidia_thermal_info.target_temp_default.map(f64::from);
                            self.nvidia_target_temperature.insert(
                                (),
                                SettingRowInit {
                                    label_group: Some(self.label_size_group.clone()),
                                    input_group: Some(self.input_size_group.clone()),
                                    title: glib::markup_escape_text(&fl!(I18N, "target-temp"))
                                        .into(),
                                    control: AdjustmentInit {
                                        lower_label_group: Some(lower_label_group.clone()),
                                        upper_label_group: Some(upper_label_group.clone()),
                                        ..init
                                    }
                                    .into(),
                                    ..Default::default()
                                },
                            );
                        }
                        if let Some(value) = info.zero_rpm_enable {
                            self.pmfw_rows.insert(
                                ThermalSetting::ZeroRpmEnabled,
                                SettingRowInit {
                                    label_group: Some(self.label_size_group.clone()),
                                    title: glib::markup_escape_text(&fl!(I18N, "zero-rpm")).into(),
                                    control: BooleanInit {
                                        value,
                                        ..Default::default()
                                    }
                                    .into(),
                                    ..Default::default()
                                },
                            );
                        }
                        if let Some(init) = fan_info_init(info.zero_rpm_temperature) {
                            self.pmfw_rows.insert(
                                ThermalSetting::ZeroRpmTemperature,
                                SettingRowInit {
                                    label_group: Some(self.label_size_group.clone()),
                                    input_group: Some(self.input_size_group.clone()),
                                    title: glib::markup_escape_text(&fl!(
                                        I18N,
                                        "zero-rpm-stop-temp"
                                    ))
                                    .into(),
                                    control: AdjustmentInit {
                                        lower_label_group: Some(lower_label_group.clone()),
                                        upper_label_group: Some(upper_label_group.clone()),
                                        ..init
                                    }
                                    .into(),
                                    ..Default::default()
                                },
                            );
                        }
                        self.update_pmfw_rows_visibility();
                    }
                }
            },
            ThermalsPageMsg::FanModeSelected(mode) => {
                self.selected_mode = mode;
                self.update_pmfw_rows_visibility();
                APP_BROKER.send(AppMsg::SettingsChanged);
            }
            ThermalsPageMsg::RestNvidiaOptions => {
                if self.nvidia_target_temperature.get(&()).is_some() {
                    self.nvidia_target_temperature
                        .send(&(), SettingRowMsg::Reset);
                }
            }
        }

        self.update_view(widgets, sender);
    }
}

impl ThermalsPage {
    fn update_pmfw_rows_visibility(&self) {
        for setting in self.pmfw_rows.keys() {
            self.pmfw_rows.send(
                setting,
                SettingRowMsg::SetVisible(setting.is_visible(self.selected_mode)),
            );
        }
    }

    fn fan_settings_available(&self) -> bool {
        self.has_fan_speed && (self.selected_mode.is_some() || self.has_pmfw_options())
    }

    pub fn serialize(
        &self,
    ) -> (
        Option<FanControlSettings>,
        PmfwOptions,
        NvidiaThermalOptions,
    ) {
        let fan_settings = self.selected_mode.map(|mode| FanControlSettings {
            mode,
            static_speed: self
                .static_speed
                .get(&())
                .and_then(SettingRow::serialize)
                .and_then(SettingRowValue::as_numeric)
                .map(|value| value as f32 / 100.0)
                .unwrap_or(0.5),
            ..self.fan_curve_frame.model().serialize()
        });
        let numeric = |setting| {
            self.pmfw_rows
                .get(&setting)
                .and_then(SettingRow::serialize)
                .and_then(SettingRowValue::as_numeric)
                .map(|value| value as u32)
        };
        (
            fan_settings,
            PmfwOptions {
                acoustic_limit: numeric(ThermalSetting::AcousticLimit),
                acoustic_target: numeric(ThermalSetting::AcousticTarget),
                minimum_pwm: numeric(ThermalSetting::MinimumPwm),
                target_temperature: numeric(ThermalSetting::TargetTemperature),
                zero_rpm_threshold: numeric(ThermalSetting::ZeroRpmTemperature),
                zero_rpm: self
                    .pmfw_rows
                    .get(&ThermalSetting::ZeroRpmEnabled)
                    .and_then(SettingRow::serialize)
                    .and_then(SettingRowValue::as_boolean),
            },
            NvidiaThermalOptions {
                target_temperature: self
                    .nvidia_target_temperature
                    .get(&())
                    .and_then(SettingRow::serialize)
                    .and_then(SettingRowValue::as_numeric)
                    .map(|value| value as u32),
            },
        )
    }

    fn has_pmfw_options(&self) -> bool {
        !self.pmfw_rows.is_empty()
    }
}

fn fan_info_init(info: Option<FanInfo>) -> Option<AdjustmentInit> {
    let info = info?;
    let (min, max) = info.allowed_range.unwrap_or((0, info.current));
    (min != 0 || max != 0).then_some(AdjustmentInit {
        value: info.current as f64,
        lower: min as f64,
        upper: max as f64,
        page_increment: 5.0,
        ..Default::default()
    })
}

#[cfg(all(test, feature = "gtk-tests"))]
mod tests {
    use super::*;
    use crate::app::test_utils::find_widget;
    use lact_schema::{DeviceStats, FanStats, NvidiaThermalInfo, PmfwInfo};
    use std::sync::Arc;

    #[test]
    #[ignore = "requires a GTK display; run explicitly with --ignored"]
    fn shared_pmfw_factory_preserves_visibility_sizing_and_pending_edits() {
        adw::init().unwrap();
        let context = glib::MainContext::default();
        let _guard = context.acquire().unwrap();
        let drain = || {
            while context.pending() {
                context.iteration(false);
            }
        };
        let page = ThermalsPage::detach(());
        let mut stats = DeviceStats {
            fan: FanStats {
                control_enabled: true,
                control_mode: Some(FanControlMode::Curve),
                pwm_current: Some(128),
                pmfw_info: PmfwInfo {
                    target_temp: Some(FanInfo {
                        current: 75,
                        allowed_range: Some((30, 100)),
                    }),
                    zero_rpm_enable: Some(true),
                    zero_rpm_temperature: Some(FanInfo {
                        current: 45,
                        allowed_range: Some((20, 80)),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        page.emit(ThermalsPageMsg::Update {
            update: PageUpdate::Stats(Arc::new(stats.clone())),
            initial: true,
        });
        drain();

        let list = page.model().pmfw_rows.widget().clone();
        let mut rows = (0..)
            .map_while(|index| list.row_at_index(index))
            .filter(|row| row.has_css_class("setting-row"));
        let target_row = rows.next().unwrap();
        let enabled_row = rows.next().unwrap();
        let threshold_row = rows.next().unwrap();
        assert_eq!(page.model().pmfw_rows.len(), 3);
        assert!(list.get_visible());
        assert!(!target_row.get_visible());
        assert!(enabled_row.get_visible());
        assert!(threshold_row.get_visible());
        let (_, pmfw, _) = page.model().serialize();
        assert_eq!(pmfw.target_temperature, Some(75));
        assert_eq!(pmfw.zero_rpm, Some(true));
        assert_eq!(pmfw.zero_rpm_threshold, Some(45));

        let switch = find_widget::<gtk::Switch>(enabled_row.upcast_ref()).unwrap();
        let spin = find_widget::<gtk::SpinButton>(threshold_row.upcast_ref()).unwrap();
        let title_box = switch.parent().unwrap().first_child().unwrap();
        assert!(page.model().label_size_group.widgets().contains(&title_box));
        assert!(
            !page
                .model()
                .input_size_group
                .widgets()
                .contains(switch.upcast_ref())
        );
        switch.set_active(false);
        spin.set_value(55.0);
        drain();

        for (mode, target_visible, zero_rpm_visible) in [
            (None, true, true),
            (Some(FanControlMode::Curve), false, true),
            (Some(FanControlMode::Static), false, false),
            (None, true, true),
        ] {
            page.emit(ThermalsPageMsg::FanModeSelected(mode));
            drain();
            assert_eq!(target_row.get_visible(), target_visible);
            assert_eq!(enabled_row.get_visible(), zero_rpm_visible);
            assert_eq!(threshold_row.get_visible(), zero_rpm_visible);
            assert_eq!(list.get_visible(), target_visible || zero_rpm_visible);
            assert_eq!(
                page.model().label_size_group.widgets().contains(&title_box),
                zero_rpm_visible
            );
            let (fan, pmfw, _) = page.model().serialize();
            assert_eq!(fan.as_ref().map(|settings| settings.mode), mode);
            assert_eq!(pmfw.zero_rpm, Some(false));
            assert_eq!(pmfw.zero_rpm_threshold, Some(55));
            assert_eq!(pmfw.target_temperature, Some(75));
        }

        // Reloading a device without zero-RPM support must leave no empty curve-mode card.
        drop((
            target_row,
            enabled_row,
            threshold_row,
            switch,
            spin,
            title_box,
        ));
        stats.fan.pmfw_info.zero_rpm_enable = None;
        stats.fan.pmfw_info.zero_rpm_temperature = None;
        page.emit(ThermalsPageMsg::Update {
            update: PageUpdate::Stats(Arc::new(stats)),
            initial: true,
        });
        drain();
        assert_eq!(page.model().pmfw_rows.len(), 1);
        assert!(!list.get_visible());
        assert!(page.model().label_size_group.widgets().is_empty());
        let (_, pmfw, _) = page.model().serialize();
        assert_eq!(pmfw.zero_rpm, None);
        assert_eq!(pmfw.zero_rpm_threshold, None);

        // The reset action uses the current row's default, including after a reload loses it.
        for (default, expected) in [(Some(80), None), (None, Some(85))] {
            page.emit(ThermalsPageMsg::Update {
                update: PageUpdate::Stats(Arc::new(DeviceStats {
                    nvidia_thermal_info: NvidiaThermalInfo {
                        target_temp: Some(FanInfo {
                            current: 85,
                            allowed_range: Some((60, 95)),
                        }),
                        target_temp_default: default,
                    },
                    ..Default::default()
                })),
                initial: true,
            });
            page.emit(ThermalsPageMsg::RestNvidiaOptions);
            drain();
            let (_, _, nvidia) = page.model().serialize();
            assert_eq!(nvidia.target_temperature, expected);
        }
    }
}
