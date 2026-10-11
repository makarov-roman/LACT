use super::{adjustment_value::AdjustmentValue, *};
use crate::app::test_utils::{assert_silent, find_widget, receive};
use relm4::{binding::Binding, factory::FactoryHashMap};
use std::{cell::Cell, rc::Rc};

#[test]
#[ignore = "requires a GTK display; run explicitly with --ignored"]
fn setting_rows_share_state_and_factory_behavior() {
    gtk::init().unwrap();
    let fatal_mask = gtk::glib::log_set_fatal_mask(
        Some("Gtk"),
        gtk::glib::LogLevels::LEVEL_ERROR | gtk::glib::LogLevels::LEVEL_CRITICAL,
    );
    let context = gtk::glib::MainContext::default();
    let _guard = context.acquire().unwrap();
    preserves_numeric_edit_and_unit_change_behavior(&context);
    mixed_factory_updates_widgets_and_routes_resets(&context);
    row_forwards_edits_and_shuts_down_replaced_controls(&context);
    row_reset_without_default_preserves_display_and_serialization(&context);
    select_rows_preserve_selection_and_edit_lifecycle(&context);
    gtk::glib::log_set_fatal_mask(Some("Gtk"), fatal_mask);
}

fn drain(context: &gtk::glib::MainContext) {
    while context.pending() {
        context.iteration(false);
    }
}

fn preserves_numeric_edit_and_unit_change_behavior(context: &gtk::glib::MainContext) {
    let (output, receiver) = relm4::channel();
    let mut rows = FactoryHashMap::<(), SettingRow<()>>::builder()
        .launch_default()
        .forward(&output, |()| ());
    rows.insert(
        (),
        SettingRowInit {
            control: AdjustmentInit {
                value: 250.0,
                default_value: Some(25.0),
                upper: 200.0,
                ..Default::default()
            }
            .into(),
            ..Default::default()
        },
    );
    drain(context);
    assert_eq!(rows[&()].get_value(), SettingRowValue::Numeric(200.0));
    assert_eq!(rows[&()].serialize(), Some(SettingRowValue::Numeric(200.0)));
    assert_silent(context, &receiver);
    let SettingControl::Adjustment(control) = &rows[&()].control else {
        panic!("expected a numeric row");
    };
    let adjustment = control.model().value.clone();
    let notified = Rc::new(Cell::new(false));
    adjustment.is_changed().connect_value_notify({
        let notified = notified.clone();
        move |binding| notified.set(binding.get())
    });
    let spin = find_widget::<gtk::SpinButton>(rows.widget().upcast_ref()).unwrap();

    // Changing display units must not enable Apply or change the raw value.
    control.emit(AdjustmentControlMsg::SetValueRatio(2.0));
    assert_silent(context, &receiver);
    assert_eq!(spin.value(), 400.0);
    assert_eq!(rows[&()].get_value(), SettingRowValue::Numeric(200.0));
    assert_eq!(rows[&()].serialize(), Some(SettingRowValue::Numeric(200.0)));
    assert!(!adjustment.is_changed().get());

    // Uncommitted text notifies Apply, but only committing it changes the raw value.
    spin.set_text("350");
    receive(context, &receiver);
    assert!(!adjustment.is_changed().get());
    spin.update();
    assert_eq!(rows[&()].serialize(), Some(SettingRowValue::Numeric(175.0)));
    assert!(notified.get());
    adjustment.set_value(200.0);
    assert_eq!(rows[&()].serialize(), Some(SettingRowValue::Numeric(200.0)));

    rows.send(&(), SettingRowMsg::Reset);
    drain(context);
    let scale = find_widget::<gtk::Scale>(rows.widget().upcast_ref()).unwrap();
    assert_eq!(spin.value(), 50.0);
    assert_eq!(scale.value(), 50.0);
    assert_eq!(rows[&()].serialize(), None);
    assert!(adjustment.is_changed().get());
    assert!(adjustment.is_default().get());

    adjustment.set_value(50.0);
    assert_eq!(rows[&()].serialize(), Some(SettingRowValue::Numeric(50.0)));
    assert!(!adjustment.is_default().get());
    adjustment.set_initial_value(25.0);
    assert_eq!(rows[&()].serialize(), None);
    assert!(adjustment.is_default().get());
    assert_eq!(spin.value(), 50.0);
    assert!(!notified.get());

    spin.set_value(1000.0);
    assert_eq!(adjustment.get_value(), 200.0);
    assert_eq!(adjustment.serialize(), Some(200.0));
    assert!(!adjustment.is_default().get());

    let zero_default = AdjustmentValue::new(50.0, Some(0.0), 0.0, 200.0, 1.0, 10.0);
    zero_default.reset();
    assert_eq!(zero_default.serialize(), None);
    assert!(zero_default.is_default().get());
    let unknown_default = AdjustmentValue::new(50.0, None, 0.0, 200.0, 1.0, 10.0);
    unknown_default.reset();
    assert_eq!(unknown_default.get_value(), 50.0);
    assert_eq!(unknown_default.serialize(), Some(50.0));
    assert!(!unknown_default.is_default().get());
    assert!(!unknown_default.is_changed().get());
    unknown_default.set_value_ratio(2.0);
    assert_eq!(unknown_default.serialize(), Some(50.0));
    assert!(!unknown_default.is_changed().get());
}

fn mixed_factory_updates_widgets_and_routes_resets(context: &gtk::glib::MainContext) {
    // Both row kinds share title/input sizing; only numeric rows have bound labels.
    let groups: [gtk::SizeGroup; 4] =
        std::array::from_fn(|_| gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal));
    let (output, receiver) = relm4::channel();
    let mut rows = FactoryHashMap::<u8, SettingRow<u8>>::builder()
        .launch_default()
        .forward(&output, |()| ());
    rows.insert(
        0,
        SettingRowInit {
            label_group: Some(groups[0].clone()),
            input_group: Some(groups[1].clone()),
            title: "Enabled".into(),
            control: BooleanInit {
                value: true,
                default_value: Some(false),
            }
            .into(),
            ..Default::default()
        },
    );
    rows.insert(
        1,
        SettingRowInit {
            label_group: Some(groups[0].clone()),
            input_group: Some(groups[1].clone()),
            title: "Temperature".into(),
            control: AdjustmentInit {
                value: 50.0,
                default_value: Some(25.0),
                upper: 100.0,
                lower_label_group: Some(groups[2].clone()),
                upper_label_group: Some(groups[3].clone()),
                ..Default::default()
            }
            .into(),
            ..Default::default()
        },
    );
    drain(context);
    let bool_root = rows.widget().row_at_index(0).unwrap();
    let numeric_root = rows.widget().row_at_index(1).unwrap();
    assert!(bool_root.has_css_class("boolean-row"));
    assert!(numeric_root.has_css_class("adjustment-row"));
    let switch = find_widget::<gtk::Switch>(bool_root.upcast_ref()).unwrap();
    assert!(switch.is_active());
    assert_eq!(rows[&0].get_value().as_boolean(), Some(true));
    assert_eq!(rows[&1].get_value().as_numeric(), Some(50.0));
    assert_silent(context, &receiver);

    // Reset only the addressed row, and emit nothing on a repeated reset.
    rows.send(&0, SettingRowMsg::Reset);
    receive(context, &receiver);
    assert!(!switch.is_active());
    assert_eq!(rows[&0].serialize(), None);
    assert_eq!(rows[&1].serialize(), Some(SettingRowValue::Numeric(50.0)));
    rows.send(&0, SettingRowMsg::Reset);
    assert_silent(context, &receiver);
    rows.send(&0, SettingRowMsg::SetValue(SettingRowValue::Boolean(true)));
    receive(context, &receiver);
    assert!(switch.is_active());
    let SettingControl::Boolean(control) = &rows[&0].control else {
        unreachable!()
    };
    control.model().value.set_initial_value(false);
    assert!(!switch.is_active());
    assert_eq!(rows[&0].serialize(), None);

    assert_eq!(groups[0].widgets().len(), 2);
    assert_eq!(groups[1].widgets().len(), 2);
    assert_eq!(groups[2].widgets().len(), 1);
    assert_eq!(groups[3].widgets().len(), 1);
    rows.send(&0, SettingRowMsg::SetVisible(false));
    rows.send(&1, SettingRowMsg::SetVisible(false));
    drain(context);
    assert!(groups.iter().all(|group| group.widgets().is_empty()));
    rows.send(&0, SettingRowMsg::SetVisible(true));
    rows.send(&1, SettingRowMsg::SetVisible(true));
    drain(context);
    assert_eq!(groups[0].widgets().len(), 2);
    assert_eq!(groups[2].widgets().len(), 1);

    // Reloading a card may destroy rows after visibility has removed their widgets.
    rows.send(&0, SettingRowMsg::SetVisible(false));
    rows.send(&1, SettingRowMsg::SetVisible(false));
    drain(context);
    rows.clear();
    drain(context);
    assert!(groups.iter().all(|group| group.widgets().is_empty()));
}

fn row_forwards_edits_and_shuts_down_replaced_controls(context: &gtk::glib::MainContext) {
    let (output, receiver) = relm4::channel();
    let mut rows = FactoryHashMap::<(), SettingRow<()>>::builder()
        .launch_default()
        .forward(&output, |()| ());
    rows.insert(
        (),
        SettingRowInit {
            title: "Enabled".into(),
            control: BooleanInit::default().into(),
            ..Default::default()
        },
    );
    drain(context);
    assert_silent(context, &receiver);
    let switch = find_widget::<gtk::Switch>(rows.widget().upcast_ref()).unwrap();
    switch.set_active(true);
    receive(context, &receiver);
    rows.insert(
        (),
        SettingRowInit {
            title: "Limit".into(),
            control: AdjustmentInit {
                value: 50.0,
                upper: 100.0,
                ..Default::default()
            }
            .into(),
            ..Default::default()
        },
    );
    drain(context);

    // A retained widget from the replaced child must no longer notify the factory.
    switch.set_active(false);
    assert_silent(context, &receiver);
    let spin = find_widget::<gtk::SpinButton>(rows.widget().upcast_ref()).unwrap();
    spin.set_text("75");
    receive(context, &receiver);
}

fn row_reset_without_default_preserves_display_and_serialization(context: &gtk::glib::MainContext) {
    let (output, receiver) = relm4::channel();
    let mut rows = FactoryHashMap::<(), SettingRow<()>>::builder()
        .launch_default()
        .forward(&output, |()| ());
    rows.insert(
        (),
        SettingRowInit {
            control: AdjustmentInit {
                value: 50.0,
                upper: 100.0,
                ..Default::default()
            }
            .into(),
            ..Default::default()
        },
    );
    assert_silent(context, &receiver);
    rows.send(&(), SettingRowMsg::Reset);
    assert_silent(context, &receiver);
    assert_eq!(rows[&()].serialize(), Some(SettingRowValue::Numeric(50.0)));
    rows.send(&(), SettingRowMsg::SetValue(SettingRowValue::Numeric(50.0)));
    assert_silent(context, &receiver);
    assert_eq!(rows[&()].serialize(), Some(SettingRowValue::Numeric(50.0)));
}

fn select_rows_preserve_selection_and_edit_lifecycle(context: &gtk::glib::MainContext) {
    let groups: [gtk::SizeGroup; 2] =
        std::array::from_fn(|_| gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal));
    let (output, receiver) = relm4::channel();
    let mut rows = FactoryHashMap::<(), SettingRow<()>>::builder()
        .launch_default()
        .forward(&output, |()| ());
    rows.insert(
        (),
        SettingRowInit {
            label_group: Some(groups[0].clone()),
            input_group: Some(groups[1].clone()),
            title: "Mode".into(),
            control: SelectInit {
                options: vec![
                    SelectOption {
                        label: "Auto".into(),
                        description: "Automatic clocks".into(),
                    },
                    SelectOption {
                        label: "Manual".into(),
                        description: "Manual clocks".into(),
                    },
                ],
                selected: Some(1),
                default_value: Some(0),
            }
            .into(),
            ..Default::default()
        },
    );
    drain(context);
    let root = rows.widget().row_at_index(0).unwrap();
    assert!(root.has_css_class("select-row"));
    let dropdown = find_widget::<gtk::DropDown>(root.upcast_ref()).unwrap();
    assert_eq!(dropdown.selected(), 1);
    assert_eq!(rows[&()].get_value().as_selected(), Some(1));
    assert_eq!(
        rows[&()].serialize(),
        Some(SettingRowValue::Selected(Some(1)))
    );
    assert_silent(context, &receiver);
    let SettingControl::Select(control) = &rows[&()].control else {
        unreachable!()
    };
    let description = control.widgets().description_label.clone();
    assert_eq!(description.text(), "Manual clocks");
    assert!(groups.iter().all(|group| group.widgets().len() == 1));
    rows.send(&(), SettingRowMsg::SetVisible(false));
    drain(context);
    assert!(!root.get_visible());
    assert!(groups.iter().all(|group| group.widgets().is_empty()));
    rows.send(&(), SettingRowMsg::SetVisible(true));
    rows.send(&(), SettingRowMsg::Reset);
    receive(context, &receiver);
    assert!(root.get_visible());
    assert!(groups.iter().all(|group| group.widgets().len() == 1));
    assert_eq!(dropdown.selected(), 0);
    assert_eq!(description.text(), "Automatic clocks");
    assert_eq!(rows[&()].serialize(), None);
    dropdown.set_selected(1);
    receive(context, &receiver);
    assert_eq!(rows[&()].get_value().as_selected(), Some(1));
    assert_eq!(description.text(), "Manual clocks");

    rows.send(&(), SettingRowMsg::SetVisible(false));
    drain(context);
    rows.insert(
        (),
        SettingRowInit {
            title: "Unavailable".into(),
            control: SelectInit::default().into(),
            ..Default::default()
        },
    );
    drain(context);
    assert!(groups.iter().all(|group| group.widgets().is_empty()));
    let empty = find_widget::<gtk::DropDown>(rows.widget().upcast_ref()).unwrap();
    assert_eq!(empty.selected(), gtk::INVALID_LIST_POSITION);
    assert_eq!(rows[&()].get_value().as_selected(), None);
    assert_eq!(rows[&()].serialize(), None);
    rows.send(&(), SettingRowMsg::Reset);
    dropdown.set_selected(0);
    assert_silent(context, &receiver);

    // Match GTK's fallback selection without marking initialization as an edit.
    rows.insert(
        (),
        SettingRowInit {
            title: "Unknown".into(),
            control: SelectInit {
                options: vec!["Auto".to_owned().into()],
                selected: Some(10),
                default_value: Some(10),
            }
            .into(),
            ..Default::default()
        },
    );
    drain(context);
    let dropdown = find_widget::<gtk::DropDown>(rows.widget().upcast_ref()).unwrap();
    assert_eq!(dropdown.selected(), 0);
    assert_eq!(rows[&()].get_value().as_selected(), Some(0));
    rows.send(&(), SettingRowMsg::Reset);
    receive(context, &receiver);
    assert_eq!(rows[&()].serialize(), None);
    rows.send(&(), SettingRowMsg::Reset);
    assert_silent(context, &receiver);
}
