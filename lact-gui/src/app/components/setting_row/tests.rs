use super::*;
use crate::app::components::{adjustment_value::AdjustmentValue, setting_value::SettingValue};
use futures::FutureExt;
use relm4::{binding::Binding, factory::FactoryHashMap};
use std::{cell::Cell, rc::Rc};

#[test]
#[ignore = "requires a GTK display; run explicitly with --ignored"]
fn numeric_and_boolean_rows_share_state_and_factory_behavior() {
    gtk::init().unwrap();
    let context = gtk::glib::MainContext::default();
    let _guard = context.acquire().unwrap();
    preserves_numeric_edit_and_unit_change_behavior(&context);
    mixed_factory_updates_widgets_and_routes_resets(&context);
    boolean_factory_supports_typed_values(&context);
    controls_notify_edits_but_not_initialization(&context);
    row_forwards_edits_and_shuts_down_replaced_controls(&context);
}

fn drain(context: &gtk::glib::MainContext) {
    while context.pending() {
        context.iteration(false);
    }
}

fn preserves_numeric_edit_and_unit_change_behavior(context: &gtk::glib::MainContext) {
    let mut rows = FactoryHashMap::<(), SettingRow<()>>::builder()
        .launch_default()
        .detach();
    rows.insert(
        (),
        SettingRowInit::new(
            String::new(),
            AdjustmentInit {
                value: 250.0,
                default_value: Some(25.0),
                upper: 200.0,
                ..Default::default()
            },
        ),
    );
    drain(context);
    assert_eq!(rows[&()].get_value(), SettingRowValue::Numeric(200.0));
    assert_eq!(rows[&()].get_changed_value(), None);
    let SettingControl::Adjustment(control) = &rows[&()].control else {
        panic!("expected a numeric row");
    };
    let adjustment = control.model().value.clone();
    let notified = Rc::new(Cell::new(false));
    adjustment.is_changed().connect_value_notify({
        let notified = notified.clone();
        move |binding| notified.set(binding.get())
    });

    adjustment.set_value(200.0);
    assert_eq!(rows[&()].get_changed_value(), None);
    adjustment.set_value(100.0);
    assert_eq!(
        rows[&()].get_changed_value(),
        Some(SettingRowValue::Numeric(100.0))
    );
    assert!(notified.get());
    adjustment.set_value(200.0);
    assert_eq!(
        rows[&()].get_changed_value(),
        Some(SettingRowValue::Numeric(200.0))
    );
    adjustment.set_value(150.0);

    // Uncommitted text must not overwrite the numeric value.
    let spin = find_widget::<gtk::SpinButton>(rows.widget().upcast_ref()).unwrap();
    spin.set_text("175");
    assert_eq!(
        rows[&()].get_changed_value(),
        Some(SettingRowValue::Numeric(150.0))
    );
    spin.update();
    assert_eq!(
        rows[&()].get_changed_value(),
        Some(SettingRowValue::Numeric(175.0))
    );

    control.emit(AdjustmentControlMsg::SetValueRatio(2.0));
    drain(context);
    assert_eq!(spin.value(), 350.0);
    assert_eq!(rows[&()].get_value(), SettingRowValue::Numeric(175.0));
    assert_eq!(rows[&()].get_changed_value(), None);
    assert!(!notified.get());

    rows.send(&(), SettingRowMsg::Reset);
    drain(context);
    let scale = find_widget::<gtk::Scale>(rows.widget().upcast_ref()).unwrap();
    assert_eq!(spin.value(), 50.0);
    assert_eq!(scale.value(), 50.0);
    assert_eq!(
        rows[&()].get_changed_value(),
        Some(SettingRowValue::Numeric(25.0))
    );
    assert!(adjustment.is_default().get());
    assert!(notified.get());

    adjustment.set_value(50.0);
    assert_eq!(
        rows[&()].get_changed_value(),
        Some(SettingRowValue::Numeric(50.0))
    );
    assert!(!adjustment.is_default().get());
    adjustment.set_initial_value(25.0);
    assert_eq!(rows[&()].get_changed_value(), None);
    assert!(adjustment.is_default().get());
    assert_eq!(spin.value(), 50.0);
    assert!(!notified.get());

    rows.send(&(), SettingRowMsg::Reset);
    drain(context);
    assert_eq!(rows[&()].get_changed_value(), None);

    spin.set_value(1000.0);
    assert_eq!(adjustment.get_value(), 200.0);
    assert_eq!(adjustment.get_changed_value(), Some(200.0));
    assert!(!adjustment.is_default().get());

    let zero_default = AdjustmentValue::new(50.0, Some(0.0), 0.0, 200.0, 1.0, 10.0);
    zero_default.reset();
    assert_eq!(zero_default.get_changed_value(), Some(0.0));
    assert!(zero_default.is_default().get());

    let unknown_default = AdjustmentValue::new(50.0, None, 0.0, 200.0, 1.0, 10.0);
    unknown_default.reset();
    assert_eq!(unknown_default.get_value(), 50.0);
    assert_eq!(unknown_default.get_changed_value(), None);
    assert!(!unknown_default.is_default().get());
}

fn mixed_factory_updates_widgets_and_routes_resets(context: &gtk::glib::MainContext) {
    let mut rows = FactoryHashMap::<u8, SettingRow<u8>>::builder()
        .launch_default()
        .detach();
    rows.insert(
        0,
        SettingRowInit::new(
            "Enabled".into(),
            BooleanInit {
                value: true,
                default_value: Some(false),
            },
        ),
    );
    rows.insert(
        1,
        SettingRowInit::new(
            "Temperature".into(),
            AdjustmentInit {
                value: 50.0,
                default_value: Some(25.0),
                upper: 100.0,
                ..Default::default()
            },
        ),
    );
    drain(context);
    let bool_root = rows.widget().row_at_index(0).unwrap();
    let numeric_root = rows.widget().row_at_index(1).unwrap();
    assert!(bool_root.has_css_class("boolean-row"));
    assert!(numeric_root.has_css_class("adjustment-row"));
    let switch = find_widget::<gtk::Switch>(bool_root.upcast_ref()).unwrap();
    let spin = find_widget::<gtk::SpinButton>(numeric_root.upcast_ref()).unwrap();
    assert!(switch.is_active());
    assert_eq!(rows[&0].get_value().as_boolean(), Some(true));
    assert_eq!(rows[&0].get_value().as_numeric(), None);
    assert_eq!(rows[&1].get_value().as_numeric(), Some(50.0));
    assert_eq!(rows[&1].get_value().as_boolean(), None);
    assert_eq!(rows[&0].get_changed_value(), None);
    assert_eq!(rows[&1].get_changed_value(), None);

    // The card requests resets; rows only display their controls.
    rows.send(&0, SettingRowMsg::Reset);
    drain(context);
    assert!(!switch.is_active());
    assert_eq!(
        rows[&0].get_changed_value(),
        Some(SettingRowValue::Boolean(false))
    );
    assert_eq!(rows[&1].get_changed_value(), None);

    switch.set_active(true);
    drain(context);
    assert_eq!(
        rows[&0].get_changed_value(),
        Some(SettingRowValue::Boolean(true))
    );

    // Display-unit changes must not enable Apply.
    let SettingControl::Adjustment(control) = &rows[&1].control else {
        panic!("expected an adjustment control");
    };
    control.emit(AdjustmentControlMsg::SetValueRatio(2.0));
    drain(context);
    assert_eq!(rows[&1].get_changed_value(), None);
    rows.send(&1, SettingRowMsg::Reset);
    drain(context);
    assert_eq!(
        rows[&1].get_changed_value(),
        Some(SettingRowValue::Numeric(25.0))
    );
    assert_eq!(spin.value(), 50.0);

    // Both row kinds share title/input sizing; only numeric rows have bound labels.
    let groups: [gtk::SizeGroup; 4] =
        std::array::from_fn(|_| gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal));
    for key in [0, 1] {
        match &rows[&key].control {
            SettingControl::Adjustment(control) => {
                control.emit(AdjustmentControlMsg::AddSizeGroup {
                    label_group: groups[0].clone(),
                    input_group: groups[1].clone(),
                    lower_label_group: groups[2].clone(),
                    upper_label_group: groups[3].clone(),
                })
            }
            SettingControl::Boolean(control) => control.emit(BooleanControlMsg::AddSizeGroup {
                label_group: groups[0].clone(),
                input_group: Some(groups[1].clone()),
            }),
        }
    }
    drain(context);
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

    rows.clear();
    drain(context);
    assert!(groups.iter().all(|group| group.widgets().is_empty()));
    rows.insert(
        0,
        SettingRowInit::new(
            String::new(),
            BooleanInit {
                value: false,
                ..Default::default()
            },
        ),
    );
    drain(context);
    assert_eq!(rows[&0].get_changed_value(), None);
}

fn boolean_factory_supports_typed_values(context: &gtk::glib::MainContext) {
    let mut rows = FactoryHashMap::<(), SettingRow<()>>::builder()
        .launch_default()
        .detach();
    rows.insert(
        (),
        SettingRowInit::new(
            String::new(),
            BooleanInit {
                value: false,
                default_value: Some(true),
            },
        ),
    );
    drain(context);
    assert_eq!(rows[&()].get_value(), SettingRowValue::Boolean(false));
    let switch = find_widget::<gtk::Switch>(rows.widget().upcast_ref()).unwrap();
    rows.send(&(), SettingRowMsg::Reset);
    drain(context);
    assert!(switch.is_active());
    assert_eq!(
        rows[&()].get_changed_value(),
        Some(SettingRowValue::Boolean(true))
    );
    let SettingControl::Boolean(control) = &rows[&()].control else {
        panic!("expected a boolean row");
    };
    control.model().value.set_initial_value(false);
    assert!(!switch.is_active());
    assert_eq!(rows[&()].get_changed_value(), None);
}

fn controls_notify_edits_but_not_initialization(context: &gtk::glib::MainContext) {
    let (output, receiver) = relm4::channel();
    let boolean = BooleanControl::builder()
        .launch((
            BooleanInit {
                value: true,
                default_value: Some(false),
            },
            gtk::Box::default(),
        ))
        .forward(&output, |()| ());
    let numeric = AdjustmentControl::builder()
        .launch((
            AdjustmentInit {
                value: 50.0,
                upper: 100.0,
                ..Default::default()
            },
            gtk::Box::default(),
        ))
        .forward(&output, |()| ());
    drain(context);
    assert_eq!(receiver.recv().now_or_never(), None);
    boolean.emit(BooleanControlMsg::Reset);
    drain(context);
    assert_eq!(receiver.recv().now_or_never(), Some(Some(())));
    boolean.emit(BooleanControlMsg::Reset);
    drain(context);
    assert_eq!(receiver.recv().now_or_never(), None);
    let switch = find_widget::<gtk::Switch>(boolean.widget().upcast_ref()).unwrap();
    switch.set_active(true);
    drain(context);
    assert_eq!(receiver.recv().now_or_never(), Some(Some(())));
    assert_eq!(receiver.recv().now_or_never(), None);
    numeric.emit(AdjustmentControlMsg::SetValueRatio(2.0));
    drain(context);
    assert_eq!(receiver.recv().now_or_never(), None);
    let spin = find_widget::<gtk::SpinButton>(numeric.widget().upcast_ref()).unwrap();
    spin.set_text("75");
    drain(context);
    assert_eq!(receiver.recv().now_or_never(), Some(Some(())));
}

fn row_forwards_edits_and_shuts_down_replaced_controls(context: &gtk::glib::MainContext) {
    let (output, receiver) = relm4::channel();
    let mut rows = FactoryHashMap::<(), SettingRow<()>>::builder()
        .launch_default()
        .forward(&output, |()| ());
    rows.insert(
        (),
        SettingRowInit::new("Enabled".into(), BooleanInit::default()),
    );
    drain(context);
    assert_eq!(receiver.recv().now_or_never(), None);

    let switch = find_widget::<gtk::Switch>(rows.widget().upcast_ref()).unwrap();
    switch.set_active(true);
    drain(context);
    assert_eq!(
        context
            .block_on(gtk::glib::future_with_timeout(
                std::time::Duration::from_secs(5),
                receiver.recv(),
            ))
            .unwrap(),
        Some(())
    );
    assert_eq!(receiver.recv().now_or_never(), None);

    rows.insert(
        (),
        SettingRowInit::new(
            "Limit".into(),
            AdjustmentInit {
                value: 50.0,
                upper: 100.0,
                ..Default::default()
            },
        ),
    );
    drain(context);
    assert_eq!(receiver.recv().now_or_never(), None);

    // A retained widget from the replaced child must no longer notify the factory.
    switch.set_active(false);
    assert!(
        context
            .block_on(gtk::glib::future_with_timeout(
                std::time::Duration::from_millis(50),
                receiver.recv(),
            ))
            .is_err()
    );
    let spin = find_widget::<gtk::SpinButton>(rows.widget().upcast_ref()).unwrap();
    spin.set_text("75");
    drain(context);
    assert_eq!(
        context
            .block_on(gtk::glib::future_with_timeout(
                std::time::Duration::from_secs(5),
                receiver.recv(),
            ))
            .unwrap(),
        Some(())
    );
}

fn find_widget<W: IsA<gtk::Widget>>(widget: &gtk::Widget) -> Option<W> {
    if let Ok(widget) = widget.clone().downcast::<W>() {
        return Some(widget);
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(widget) = find_widget(&widget) {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}
