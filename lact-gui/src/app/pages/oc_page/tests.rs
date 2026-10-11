use super::*;
use crate::app::test_utils::{find_widget, wait_for};
use lact_schema::{NvidiaClockOffset, NvidiaClocksTable, PowerStats};

#[test]
#[ignore = "requires a GTK display; run explicitly with --ignored"]
fn serializes_clock_domains_and_loaded_values_without_edits() {
    adw::init().unwrap();
    let context = gtk::glib::MainContext::default();
    let _guard = context.acquire().unwrap();
    relm4::main_application()
        .register(None::<&gtk::gio::Cancellable>)
        .unwrap();
    let page = OcPage::detach(BoolBinding::new(false));
    let offset = |current| NvidiaClockOffset {
        current,
        min: -1000,
        max: 1000,
    };
    page.model()
        .vram_clocks_frame
        .emit(ClocksFrameMsg::VramRatio(2.0));
    page.model()
        .power_frame
        .emit(PowerFrameMsg::PowerStats(PowerStats {
            cap_current: Some(200.0),
            cap_default: Some(150.0),
            cap_min: Some(100.0),
            cap_max: Some(300.0),
            ..Default::default()
        }));
    page.emit(OcPageMsg::ClocksTable {
        table: Some(ClocksTable::Nvidia(NvidiaClocksTable {
            gpu_offsets: IndexMap::from([(0, offset(100)), (2, offset(60))]),
            mem_offsets: IndexMap::from([(0, offset(500))]),
            gpu_clock_range: Some((300, 3000)),
            gpu_locked_clocks: Some((900, 2000)),
            vram_clock_range: Some((100, 2000)),
            ..Default::default()
        })),
        vf_curve_is_configured: false,
    });
    wait_for(&context, || {
        page.model()
            .serialize()
            .clocks_configuration
            .mem_clock_offsets
            .get(&0)
            == Some(&500)
    });
    let snapshot = page.model().serialize();
    assert_eq!(snapshot.power_cap, Some(200.0));
    let clocks = snapshot.clocks_configuration;
    assert_eq!(clocks.min_core_clock, Some(900));
    assert_eq!(clocks.max_core_clock, Some(2000));
    assert_eq!(clocks.min_memory_clock, None);
    assert_eq!(clocks.max_memory_clock, None);
    assert_eq!(clocks.gpu_clock_offsets.get(&0), Some(&100));
    assert_eq!(
        clocks.gpu_clock_offsets.contains_key(&2),
        crate::CONFIG.read().experimental_nvidia_pstate_offsets
    );

    // The offset follows the two locked-clock rows.
    let list =
        find_widget::<gtk::ListBox>(page.model().vram_clocks_frame.widget().upcast_ref()).unwrap();
    let offset_row = (0..)
        .map_while(|index| list.row_at_index(index))
        .filter(|row| row.has_css_class("setting-row"))
        .nth(2)
        .unwrap();
    let offset_spin = find_widget::<gtk::SpinButton>(offset_row.upcast_ref()).unwrap();
    wait_for(&context, || offset_spin.value() == 1000.0);
    offset_spin.set_value(1200.0);
    assert_eq!(
        page.model()
            .serialize()
            .clocks_configuration
            .mem_clock_offsets
            .get(&0),
        Some(&600)
    );

    page.model().power_frame.emit(PowerFrameMsg::Reset);
    page.model()
        .gpu_clocks_frame
        .emit(ClocksFrameMsg::ResetGpuClockOffsets);
    wait_for(&context, || {
        page.model()
            .serialize()
            .clocks_configuration
            .gpu_clock_offsets
            .is_empty()
    });
    assert_eq!(page.model().serialize().power_cap, None);
    assert_eq!(
        page.model()
            .serialize()
            .clocks_configuration
            .mem_clock_offsets
            .get(&0),
        Some(&600)
    );

    page.emit(OcPageMsg::ClocksTable {
        table: None,
        vf_curve_is_configured: false,
    });
    wait_for(&context, || {
        page.model().serialize().clocks_configuration == config::ClocksConfiguration::default()
    });
}
