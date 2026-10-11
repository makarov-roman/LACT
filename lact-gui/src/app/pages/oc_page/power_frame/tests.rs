use super::*;
use crate::app::test_utils::{assert_silent, find_widget, receive, wait_for};
use amdgpu_sysfs::gpu_handle::power_profile_mode::{PowerProfile, PowerProfileComponent};
use relm4::Component;
use std::collections::BTreeMap;

#[test]
#[ignore = "requires a GTK display; run explicitly with --ignored"]
fn power_factory_preserves_modes_heuristics_and_pending_cap() {
    adw::init().unwrap();
    let context = gtk::glib::MainContext::default();
    let _guard = context.acquire().unwrap();
    let drain = || {
        while context.pending() {
            context.iteration(false);
        }
    };
    let (output, receiver) = relm4::channel();
    let frame = PowerFrame::builder().launch(()).forward(&output, |msg| msg);
    let power = PowerStats {
        cap_current: Some(150.0),
        cap_default: Some(125.0),
        cap_min: Some(50.0),
        cap_max: Some(200.0),
        ..Default::default()
    };
    frame.emit(PowerFrameMsg::PowerStats(power.clone()));
    frame.emit(PowerFrameMsg::PerformanceLevel(Some(
        PerformanceLevel::Auto,
    )));
    frame.emit(PowerFrameMsg::PowerProfileModes(Some(
        PowerProfileModesTable {
            modes: BTreeMap::from([
                (
                    0,
                    PowerProfile {
                        name: "BOOTUP_DEFAULT".into(),
                        components: vec![PowerProfileComponent {
                            clock_type: None,
                            values: vec![Some(10)],
                        }],
                    },
                ),
                (
                    1,
                    PowerProfile {
                        name: "CUSTOM".into(),
                        components: vec![PowerProfileComponent {
                            clock_type: None,
                            values: vec![Some(20)],
                        }],
                    },
                ),
            ]),
            value_names: vec!["Threshold".into()],
            active: 1,
        },
    )));
    drain();
    // Consume the load notification before checking any edit notification.
    assert!(matches!(
        receive(&context, &receiver),
        OcPageMsg::PerformanceLevelChanged
    ));
    assert_silent(&context, &receiver);
    assert_eq!(frame.model().rows.len(), 2);
    assert_eq!(serialize(&frame).power_cap, Some(150.0));
    let profile = frame
        .model()
        .performance_frame
        .widgets()
        .modes_listbox
        .clone();
    let heuristics = frame
        .model()
        .performance_frame
        .widgets()
        .heuristics_notebook
        .clone();
    let performance = dropdown(&frame, PowerSetting::PerformanceLevel);
    assert_eq!(profile.selected_row().unwrap().index(), 1);
    assert!(!profile.is_sensitive());
    assert_eq!(serialize(&frame).power_profile_mode_index, None);
    assert!(heuristics.nth_page(Some(0)).unwrap().is_sensitive());

    let cap = frame.model().rows[&PowerSetting::PowerCap].widget().clone();
    let spin = find_widget::<gtk::SpinButton>(cap.upcast_ref()).unwrap();
    // The p-state dialog requests this edit without rebuilding the selector.
    frame.emit(PowerFrameMsg::SetPerformanceLevel(PerformanceLevel::Manual));
    assert!(matches!(
        receive(&context, &receiver),
        OcPageMsg::PerformanceLevelChanged
    ));
    wait_for(&context, || profile.is_sensitive());
    assert_eq!(
        dropdown(&frame, PowerSetting::PerformanceLevel),
        performance
    );
    assert_eq!(
        frame.model().rows[&PowerSetting::PerformanceLevel].serialize(),
        Some(SettingRowValue::Selected(Some(3)))
    );
    assert_eq!(
        frame.model().performance_level(),
        Some(PerformanceLevel::Manual)
    );
    assert_eq!(serialize(&frame).power_profile_mode_index, Some(1));
    assert!(profile.is_sensitive());
    assert!(heuristics.nth_page(Some(0)).unwrap().is_sensitive());
    assert_eq!(
        serialize(&frame).custom_power_profile_mode_hueristics,
        vec![vec![Some(20)]]
    );
    spin.set_value(175.0);
    drain();
    assert_eq!(serialize(&frame).power_cap, Some(175.0));

    profile.select_row(profile.row_at_index(0).as_ref());
    wait_for(&context, || {
        !heuristics.nth_page(Some(0)).unwrap().is_sensitive()
    });
    assert_eq!(serialize(&frame).power_profile_mode_index, Some(0));
    assert!(!heuristics.nth_page(Some(0)).unwrap().is_sensitive());
    assert!(
        serialize(&frame)
            .custom_power_profile_mode_hueristics
            .is_empty()
    );
    profile.select_row(profile.row_at_index(1).as_ref());
    wait_for(&context, || {
        heuristics.nth_page(Some(0)).unwrap().is_sensitive()
    });
    assert_eq!(serialize(&frame).power_profile_mode_index, Some(1));
    assert_eq!(
        serialize(&frame).custom_power_profile_mode_hueristics,
        vec![vec![Some(20)]]
    );

    frame.emit(PowerFrameMsg::PerformanceLevel(Some(
        PerformanceLevel::High,
    )));
    drain();
    assert_eq!(serialize(&frame).power_cap, Some(175.0));
    assert!(!profile.is_sensitive());
    assert_eq!(serialize(&frame).power_profile_mode_index, None);
    assert!(
        serialize(&frame)
            .custom_power_profile_mode_hueristics
            .is_empty()
    );
    assert_order(
        &frame,
        &[PowerSetting::PowerCap, PowerSetting::PerformanceLevel],
    );
    frame.emit(PowerFrameMsg::Reset);
    drain();
    assert_eq!(serialize(&frame).power_cap, None);
    assert_eq!(
        frame.model().performance_level(),
        Some(PerformanceLevel::High)
    );

    // A subsequent device load replaces the AMD selectors with the supported NVIDIA modes.
    frame.emit(PowerFrameMsg::PowerStats(power));
    frame.emit(PowerFrameMsg::PerformanceLevel(None));
    frame.emit(PowerFrameMsg::PowerProfileModes(None));
    frame.emit(PowerFrameMsg::PowerMizerInfo {
        active: Some(PowerMizerMode::Adaptive),
        supported: Some(vec![
            PowerMizerMode::PreferMaximumPerformance,
            PowerMizerMode::Adaptive,
        ]),
    });
    drain();
    assert_eq!(frame.model().rows.len(), 2);
    assert_eq!(serialize(&frame).power_cap, Some(150.0));
    assert_eq!(frame.model().performance_level(), None);
    assert_eq!(heuristics.n_pages(), 0);
    assert!(profile.selected_row().is_none());
    assert!(profile.row_at_index(0).is_none());
    assert_eq!(serialize(&frame).power_profile_mode_index, None);
    assert_eq!(
        serialize(&frame).power_mizer_mode,
        Some(PowerMizerMode::Adaptive)
    );
    let mizer = dropdown(&frame, PowerSetting::PowerMizer);
    assert_eq!(mizer.selected(), 1);
    mizer.set_selected(0);
    drain();
    assert_eq!(
        serialize(&frame).power_mizer_mode,
        Some(PowerMizerMode::PreferMaximumPerformance)
    );
    assert_order(&frame, &[PowerSetting::PowerCap, PowerSetting::PowerMizer]);

    frame.emit(PowerFrameMsg::PowerStats(PowerStats::default()));
    frame.emit(PowerFrameMsg::PowerMizerInfo {
        active: None,
        supported: None,
    });
    drain();
    assert!(frame.model().rows.is_empty());
    assert!(!frame.widget().get_visible());

    // Preserve reported active modes even if they are absent from the usual choices.
    frame.emit(PowerFrameMsg::PerformanceLevel(Some(
        PerformanceLevel::PerfDeterminism,
    )));
    frame.emit(PowerFrameMsg::PowerMizerInfo {
        active: Some(PowerMizerMode::Adaptive),
        supported: None,
    });
    drain();
    assert_eq!(
        frame.model().performance_level(),
        Some(PerformanceLevel::PerfDeterminism)
    );
    assert_eq!(
        serialize(&frame).power_mizer_mode,
        Some(PowerMizerMode::Adaptive)
    );
    assert_eq!(
        serialize(&frame).performance_level,
        Some(PerformanceLevel::PerfDeterminism)
    );
}

fn serialize(frame: &relm4::Controller<PowerFrame>) -> GpuConfig {
    frame.model().serialize()
}

fn dropdown(frame: &relm4::Controller<PowerFrame>, setting: PowerSetting) -> gtk::DropDown {
    find_widget(frame.model().rows[&setting].widget().upcast_ref()).unwrap()
}

fn assert_order(frame: &relm4::Controller<PowerFrame>, settings: &[PowerSetting]) {
    let visible_rows = (0..)
        .map_while(|index| frame.model().rows.widget().row_at_index(index))
        .filter(|row| row.get_visible())
        .collect::<Vec<_>>();
    for (index, setting) in settings.iter().enumerate() {
        assert_eq!(
            visible_rows[index].child().as_ref(),
            Some(frame.model().rows[setting].widget().upcast_ref()),
            "expected {setting:?} at position {index}",
        );
    }
}
