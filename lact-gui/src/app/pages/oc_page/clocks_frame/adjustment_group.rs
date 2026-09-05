use super::{ClocksData, clock_title};
use crate::{
    APP_BROKER, I18N,
    app::{
        components::adjustment_row::{AdjustmentRow, AdjustmentRowInit, AdjustmentRowMsg},
        msg::AppMsg,
        utils::ext::RelmLaunchable,
    },
};
use gtk::prelude::{BoxExt, OrientableExt, WidgetExt};
use i18n_embed_fl::fl;
use indexmap::IndexMap;
use lact_schema::request::ClockspeedType;
use relm4::{ComponentController, css, prelude::FactoryComponent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClockCategory {
    CoreClock,
    CoreVoltage,
    VramClock,
    CoreCurveClock,
    VramCurveClock,
    CoreCurveVoltage,
    VramCurveVoltage,
}

impl ClockCategory {
    pub fn from_type(clock_type: ClockspeedType) -> Self {
        match clock_type {
            ClockspeedType::MaxCoreClock
            | ClockspeedType::MinCoreClock
            | ClockspeedType::GpuClockOffset(_) => ClockCategory::CoreClock,
            ClockspeedType::MinVoltage
            | ClockspeedType::MaxVoltage
            | ClockspeedType::VoltageOffset
            | ClockspeedType::VoltageBoost => ClockCategory::CoreVoltage,
            ClockspeedType::MaxMemoryClock
            | ClockspeedType::MinMemoryClock
            | ClockspeedType::MemClockOffset(_) => ClockCategory::VramClock,
            ClockspeedType::GpuVfCurveClock(_) => ClockCategory::CoreCurveClock,
            ClockspeedType::MemVfCurveClock(_) => ClockCategory::VramCurveClock,
            ClockspeedType::GpuVfCurveVoltage(_) => ClockCategory::CoreCurveVoltage,
            ClockspeedType::MemVfCurveVoltage(_) => ClockCategory::VramCurveVoltage,
            ClockspeedType::Reset => unreachable!(),
        }
    }

    pub fn is_core(&self) -> bool {
        Self::CORE.contains(self)
    }

    pub fn is_vram(&self) -> bool {
        Self::VRAM.contains(self)
    }

    pub const CORE: [ClockCategory; 4] = [
        ClockCategory::CoreClock,
        ClockCategory::CoreVoltage,
        ClockCategory::CoreCurveClock,
        ClockCategory::CoreCurveVoltage,
    ];

    pub const VRAM: [ClockCategory; 3] = [
        ClockCategory::VramClock,
        ClockCategory::VramCurveClock,
        ClockCategory::VramCurveVoltage,
    ];
}

pub struct AdjustmentGroup {
    adjustments: IndexMap<ClockspeedType, ClockEntry>,
    adjustments_widget: gtk::Box,
}

struct ClockEntry {
    row: relm4::Controller<AdjustmentRow>,
    is_secondary: bool,
}

impl AdjustmentGroup {
    pub fn is_empty(&self) -> bool {
        self.adjustments.is_empty()
    }

    pub fn has_secondary(&self) -> bool {
        self.adjustments.values().any(|row| row.is_secondary)
    }

    pub fn set_clock(&mut self, clock_type: ClockspeedType, data: ClocksData) {
        let row = AdjustmentRow::launch(AdjustmentRowInit {
            title: data.custom_title.unwrap_or_else(|| clock_title(clock_type)),
            info_text: if clock_type == ClockspeedType::VoltageBoost {
                fl!(I18N, "gpu-voltage-boost-tooltip")
            } else {
                String::new()
            },
            value: f64::from(data.current),
            lower: f64::from(data.min),
            upper: f64::from(data.max),
            step_increment: f64::from(data.step),
            ..Default::default()
        })
        .connect_receiver(|_, ()| APP_BROKER.send(AppMsg::SettingsChanged));

        if let Some(previous) = self.adjustments.get(&clock_type) {
            self.adjustments_widget.remove(previous.row.widget());
        }
        self.adjustments_widget.append(row.widget());
        self.adjustments.insert(
            clock_type,
            ClockEntry {
                row,
                is_secondary: data.is_secondary,
            },
        );
    }

    pub fn add_size_group(&self, label_group: gtk::SizeGroup, input_group: gtk::SizeGroup) {
        for entry in self.adjustments.values() {
            entry.row.emit(AdjustmentRowMsg::AddSizeGroup {
                label_group: label_group.clone(),
                input_group: input_group.clone(),
            });
        }
    }

    pub fn set_value_ratio(&self, ratio: f64) {
        for entry in self.adjustments.values() {
            entry.row.emit(AdjustmentRowMsg::ValueRatio(ratio));
        }
    }

    pub fn toggle_secondary_visibility(
        &self,
        show_secondary: bool,
        show_nvidia_options: bool,
        enable_gpu_locked: bool,
        enable_vram_locked: bool,
        vf_curve_editing: bool,
    ) {
        let mut any_visible = false;

        for (key, entry) in self.adjustments.iter() {
            let show_current = match key {
                ClockspeedType::MaxCoreClock | ClockspeedType::MinCoreClock
                    if show_nvidia_options =>
                {
                    enable_gpu_locked
                }
                ClockspeedType::MaxMemoryClock | ClockspeedType::MinMemoryClock
                    if show_nvidia_options =>
                {
                    enable_vram_locked
                }
                ClockspeedType::GpuClockOffset(_) if show_nvidia_options && vf_curve_editing => {
                    false
                }
                _ => !entry.is_secondary || show_secondary,
            };

            any_visible |= show_current;

            entry.row.emit(AdjustmentRowMsg::SetVisible(show_current));
        }

        // removes empty card
        self.adjustments_widget.set_visible(any_visible);
    }

    pub fn get_commands(&self) -> Vec<(ClockspeedType, Option<i32>)> {
        self.adjustments
            .iter()
            .map(|(clock_type, entry)| {
                (
                    *clock_type,
                    entry
                        .row
                        .model()
                        .get_changed_value()
                        .map(|value| value as i32),
                )
            })
            .collect()
    }

    pub fn reset_gpu_clock_offsets(&self) {
        for (clock_type, entry) in &self.adjustments {
            if matches!(clock_type, ClockspeedType::GpuClockOffset(_)) {
                entry.row.emit(AdjustmentRowMsg::SetValue(0.0));
            }
        }
    }

    pub fn get_raw_value(&self, clock_type: ClockspeedType) -> i32 {
        self.adjustments
            .get(&clock_type)
            .map(|entry| entry.row.model().get_value() as i32)
            .unwrap_or(0)
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for AdjustmentGroup {
    type Init = ();
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Index = ClockCategory;

    view! {
        self.adjustments_widget.clone() -> gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 5,
            set_valign: gtk::Align::Start,
            add_css_class: css::CARD,
        }
    }

    fn init_model(_: Self::Init, _: &Self::Index, _: relm4::FactorySender<Self>) -> Self {
        Self {
            adjustments: IndexMap::new(),
            adjustments_widget: gtk::Box::default(),
        }
    }
}
