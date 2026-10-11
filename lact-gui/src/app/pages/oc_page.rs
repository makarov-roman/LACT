mod card_layout;
mod clocks_frame;
mod igpu_frame;
mod performance_frame;
mod power_frame;
mod power_states;
mod vf_curve;

use crate::app::components::gpu_stats_section::{
    GpuStat, GpuStatsSection, GpuStatsSectionConfig, GpuStatsSectionMsg,
};
use crate::app::pages::PageUpdate;
use crate::app::pages::oc_page::igpu_frame::{IGpuFrame, IGpuFrameMsg};
use crate::app::utils::ext::RelmLaunchable as _;
use crate::app::{msg::AppMsg, utils::ext::RelmDefaultLauchable};
use adw::prelude::*;
use amdgpu_sysfs::gpu_handle::{PerformanceLevel, power_profile_mode::PowerProfileModesTable};
use card_layout::{CardLayout, ColumnBias};
use clocks_frame::{ClockDomain, ClocksFrame, ClocksFrameInit, ClocksFrameMsg};
use indexmap::IndexMap;
use lact_schema::config;
use lact_schema::{ClocksTable, DeviceInfo, PowerStates};
use power_frame::{PowerFrame, PowerFrameMsg};
use power_states::power_states_dialog::{PowerStatesDialog, PowerStatesDialogMsg};
use relm4::binding::BoolBinding;
use relm4::{ComponentController, ComponentParts, ComponentSender, RelmWidgetExt};
use std::collections::HashSet;
use std::sync::Arc;
use vf_curve::{VfCurveEditor, VfCurveEditorInit, VfCurveEditorMsg};

pub struct OcPage {
    stats_section: relm4::Controller<GpuStatsSection>,
    device_info: Option<Arc<DeviceInfo>>,

    power_frame: relm4::Controller<PowerFrame>,
    power_states_dialog: relm4::Controller<PowerStatesDialog>,
    gpu_clocks_frame: relm4::Controller<ClocksFrame>,
    vram_clocks_frame: relm4::Controller<ClocksFrame>,
    igpu_frame: relm4::Controller<IGpuFrame>,

    vf_curve_editor: relm4::Controller<VfCurveEditor>,
}

#[derive(Debug)]
pub enum OcPageMsg {
    Update {
        update: PageUpdate,
        initial: bool,
    },
    ClocksTable {
        table: Option<ClocksTable>,
        vf_curve_is_configured: bool,
    },
    ProfileModesTable(Option<PowerProfileModesTable>),
    PowerStates {
        pstates: PowerStates,
        configured: bool,
    },
    PerformanceLevelChanged,
    SetPerformanceLevel(PerformanceLevel),
    EnablePstateConfig,
    ShowPowerStates(ClockDomain),
    ShowVfCurveEditor,
    VfCurveEditingToggled(bool),
}

#[relm4::component(pub)]
impl relm4::Component for OcPage {
    type Init = BoolBinding;
    type Input = OcPageMsg;
    type Output = AppMsg;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 15,
            set_margin_all: 15,
            set_margin_top: 20, // align with gpu picker

            model.stats_section.widget(),

            gtk::Box {
                set_layout_manager: Some(CardLayout::new([
                    ColumnBias::Left,    // Core
                    ColumnBias::Right,   // VRAM
                    ColumnBias::Left,    // Power
                    ColumnBias::Right,   // Integrated Graphics

                ])),
                set_valign: gtk::Align::Start,

                model.gpu_clocks_frame.widget() {
                    add_css_class: "oc-page-section",
                },

                model.vram_clocks_frame.widget() {
                    add_css_class: "oc-page-section",
                },

                model.power_frame.widget() {
                    add_css_class: "oc-page-section",
                },

                model.igpu_frame.widget() {
                    add_css_class: "oc-page-section",
                },
            },
        },
    }

    fn init(
        settings_changed: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let stats_section = GpuStatsSection::detach(GpuStatsSectionConfig {
            stats: HashSet::from([
                GpuStat::Throttling,
                GpuStat::GpuClockTarget,
                GpuStat::GpuVoltage,
                GpuStat::Temperature,
                GpuStat::GpuClock,
                GpuStat::VramClock,
                GpuStat::GpuUsage,
                GpuStat::VramUsage,
                GpuStat::GttUsage,
                GpuStat::PowerUsage,
                GpuStat::FanSpeed,
                GpuStat::ExtraClocks,
            ]),
        });
        let vf_curve_editing = BoolBinding::new(false);
        let gpu_clocks_frame = ClocksFrame::launch(ClocksFrameInit {
            domain: ClockDomain::Gpu,
            vf_curve_editing: vf_curve_editing.clone(),
            show_all_pstates: BoolBinding::new(false),
        })
        .forward(sender.input_sender(), |msg| msg);
        let vram_clocks_frame = ClocksFrame::launch(ClocksFrameInit {
            domain: ClockDomain::Vram,
            vf_curve_editing: BoolBinding::new(false),
            show_all_pstates: BoolBinding::new(false),
        })
        .forward(sender.input_sender(), |msg| msg);
        let power_states_dialog =
            PowerStatesDialog::launch_default().forward(sender.input_sender(), |msg| msg);
        let power_frame = PowerFrame::launch_default().forward(sender.input_sender(), |msg| msg);
        let igpu_frame = IGpuFrame::launch_default().forward(sender.input_sender(), |msg| msg);

        let vf_curve_editor = VfCurveEditor::detach(VfCurveEditorInit {
            global_settings_changed: settings_changed,
            allow_editing: vf_curve_editing,
        });

        let model = Self {
            stats_section,
            device_info: None,
            power_frame,
            power_states_dialog,
            gpu_clocks_frame,
            vram_clocks_frame,
            igpu_frame,
            vf_curve_editor,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            OcPageMsg::Update { update, initial } => match &update {
                PageUpdate::Stats(stats) => {
                    self.power_states_dialog
                        .emit(PowerStatesDialogMsg::Stats(stats.clone()));
                    self.gpu_clocks_frame
                        .emit(ClocksFrameMsg::Stats(stats.clone()));
                    self.vram_clocks_frame
                        .emit(ClocksFrameMsg::Stats(stats.clone()));

                    self.stats_section
                        .emit(GpuStatsSectionMsg::Stats(stats.clone()));

                    self.vf_curve_editor
                        .emit(VfCurveEditorMsg::Stats(stats.clone()));

                    if initial {
                        self.power_frame
                            .emit(PowerFrameMsg::PowerStats(stats.power.clone()));
                        self.power_frame
                            .emit(PowerFrameMsg::PerformanceLevel(stats.performance_level));
                        self.power_frame.emit(PowerFrameMsg::PowerMizerInfo {
                            active: stats.active_power_mizer_mode,
                            supported: stats.supported_power_mizer_modes.clone(),
                        });
                    }
                }
                PageUpdate::Info(info) => {
                    let vram_clock_ratio = info.vram_clock_ratio();

                    self.device_info = Some(info.clone());
                    self.stats_section
                        .emit(GpuStatsSectionMsg::Info(info.clone()));
                    self.power_states_dialog
                        .emit(PowerStatesDialogMsg::VramClockRatio(vram_clock_ratio));
                    self.vram_clocks_frame
                        .emit(ClocksFrameMsg::VramRatio(vram_clock_ratio));

                    let uma_carveout = info
                        .drm_info
                        .as_ref()
                        .and_then(|info| info.amd_uma_carveout.clone());
                    self.igpu_frame
                        .emit(IGpuFrameMsg::UmaCarveout(uma_carveout));
                }
            },
            OcPageMsg::ClocksTable {
                table,
                vf_curve_is_configured,
            } => {
                let table = table.map(Arc::new);

                self.gpu_clocks_frame.emit(ClocksFrameMsg::Clocks {
                    table: table.clone(),
                    vf_curve_is_configured,
                });
                self.vram_clocks_frame.emit(ClocksFrameMsg::Clocks {
                    table: table.clone(),
                    vf_curve_is_configured,
                });
                self.vf_curve_editor
                    .emit(VfCurveEditorMsg::Clocks(table.clone()));
            }
            OcPageMsg::ProfileModesTable(modes_table) => {
                self.power_frame
                    .emit(PowerFrameMsg::PowerProfileModes(modes_table));
            }
            OcPageMsg::PowerStates {
                pstates,
                configured,
            } => {
                self.power_states_dialog
                    .emit(PowerStatesDialogMsg::PowerStates {
                        pstates: pstates.clone(),
                        configured,
                    });
                self.gpu_clocks_frame
                    .emit(ClocksFrameMsg::PowerStatesAvailable(
                        !pstates.core.is_empty(),
                    ));
                self.vram_clocks_frame
                    .emit(ClocksFrameMsg::PowerStatesAvailable(
                        !pstates.vram.is_empty(),
                    ));
                self.stats_section
                    .emit(GpuStatsSectionMsg::PowerStates(Arc::new(pstates)));
                sender.input(OcPageMsg::PerformanceLevelChanged);
            }
            OcPageMsg::PerformanceLevelChanged => {
                let custom_pstates_configurable =
                    self.get_performance_level() == Some(PerformanceLevel::Manual);
                self.power_states_dialog
                    .emit(PowerStatesDialogMsg::Configurable(
                        custom_pstates_configurable,
                    ));

                self.power_states_dialog
                    .emit(PowerStatesDialogMsg::PerformanceLevel(
                        self.get_performance_level(),
                    ));
            }
            OcPageMsg::SetPerformanceLevel(level) => {
                self.power_frame
                    .emit(PowerFrameMsg::SetPerformanceLevel(level));
            }
            OcPageMsg::EnablePstateConfig => {
                self.power_states_dialog
                    .emit(PowerStatesDialogMsg::EnableWithManualPerformanceLevel);
            }
            OcPageMsg::ShowPowerStates(domain) => {
                self.power_states_dialog.emit(PowerStatesDialogMsg::Show {
                    domain,
                    parent: root.clone().upcast(),
                });
            }
            OcPageMsg::ShowVfCurveEditor => {
                self.vf_curve_editor.emit(VfCurveEditorMsg::Show);
            }
            OcPageMsg::VfCurveEditingToggled(enabled) => {
                if enabled {
                    self.gpu_clocks_frame
                        .emit(ClocksFrameMsg::ResetGpuClockOffsets);
                } else {
                    self.vf_curve_editor.emit(VfCurveEditorMsg::ResetCurve);
                }
            }
        }

        self.update_view(widgets, sender);
    }
}

impl OcPage {
    pub fn serialize(&self) -> config::GpuConfig {
        let gpu = self.gpu_clocks_frame.model().serialize();
        let vram = self.vram_clocks_frame.model().serialize();
        let power = self.power_frame.model().serialize();
        config::GpuConfig {
            power_cap: power.power_cap,
            performance_level: power.performance_level,
            power_mizer_mode: power.power_mizer_mode,
            power_profile_mode_index: power.power_profile_mode_index,
            custom_power_profile_mode_hueristics: power.custom_power_profile_mode_hueristics,
            clocks_configuration: config::ClocksConfiguration {
                min_core_clock: gpu.min_core_clock,
                max_core_clock: gpu.max_core_clock,
                min_memory_clock: vram.min_memory_clock,
                max_memory_clock: vram.max_memory_clock,
                min_voltage: gpu.min_voltage,
                max_voltage: gpu.max_voltage,
                voltage_offset: gpu.voltage_offset,
                voltage_boost: gpu.voltage_boost,
                gpu_clock_offsets: gpu.gpu_clock_offsets,
                mem_clock_offsets: vram.mem_clock_offsets,
                gpu_vf_curve: gpu.gpu_vf_curve,
                mem_vf_curve: vram.mem_vf_curve,
                nvidia_gpu_vf_curve: self.vf_curve_editor.model().serialize(),
            },
            power_states: if self.get_performance_level() == Some(PerformanceLevel::Manual) {
                self.power_states_dialog.model().get_enabled_power_states()
            } else {
                IndexMap::new()
            },
            uma_carveout: self.igpu_frame.model().serialize(),
            ..Default::default()
        }
    }

    pub fn get_performance_level(&self) -> Option<PerformanceLevel> {
        self.power_frame.model().performance_level()
    }
}

#[cfg(all(test, feature = "gtk-tests"))]
mod tests;
