use crate::{
    APP_BROKER, I18N,
    app::{
        components::{
            adjustment_card::AdjustmentCard,
            setting_row::{
                AdjustmentInit, SelectInit, SettingRow, SettingRowInit, SettingRowMsg,
                SettingRowValue,
            },
        },
        graphs_window::plot::PlotColorScheme,
        msg::AppMsg,
    },
};
use adw::prelude::*;
use gtk::{gdk, glib};
use i18n_embed_fl::fl;
use indexmap::IndexMap;
use lact_schema::{FanCurveMap, TemperatureEntry, default_fan_curve};
use plotters::{
    chart::ChartBuilder,
    prelude::{Circle, EmptyElement, IntoDrawingArea, Text},
    series::{LineSeries, PointSeries},
    style::{
        Color, ShapeStyle, TextStyle,
        full_palette::{INDIGO, LIGHTBLUE},
        text_anchor::Pos,
    },
};
use plotters_cairo::CairoBackend;
use relm4::{
    ComponentParts, ComponentSender, RelmWidgetExt, WidgetTemplate, factory::FactoryHashMap,
};
use std::{
    cell::{Cell, RefCell},
    ops::RangeInclusive,
    rc::Rc,
};

pub const DEFAULT_TEMP_RANGE: RangeInclusive<f32> = 20.0..=115.0;
pub const DEFAULT_SPEED_RANGE: RangeInclusive<f32> = 0.0..=1.0;
const DEFAULT_CHANGE_THRESHOLD: u64 = 2;
const DEFAULT_AUTO_THRESHOLD: u64 = 0;
const DEFAULT_SPINDOWN_DELAY_MS: u64 = 5000;

const TEMPERATURE_DRAG_MARGIN: f32 = 4.0;
const PERCENTAGE_DRAG_MARGIN: f32 = 0.04;

pub(super) struct FanCurveFrame {
    adjustments: FactoryHashMap<CurveSetting, SettingRow<CurveSetting>>,
    label_size_group: gtk::SizeGroup,
    input_size_group: gtk::SizeGroup,
    /// PMFW fan control on AMD RDNA3+ with fixed length
    hw_based_fan_curve: bool,
    chart: Rc<ChartState>,
    temp_keys: Vec<String>,
}

struct ChartState {
    data: RefCell<Vec<(i32, f32)>>,
    speed_range: RefCell<RangeInclusive<f32>>,
    temperature_range: RefCell<RangeInclusive<f32>>,

    is_dragging: Cell<bool>,
    /// Index of the point currently being dragged
    drag_point: Cell<Option<usize>>,
    /// Where the point was last moved to
    drag_coord: Cell<Option<(f64, f64)>>,

    /// Index of the point currently being hovered on
    hover_point: Cell<Option<usize>>,
    /// Where the point was last hovered on
    hover_coord: Cell<Option<(f64, f64)>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum CurveSetting {
    TemperatureSensor,
    SpindownDelay,
    ChangeThreshold,
    AutoThreshold,
}

#[derive(Debug)]
pub(super) enum FanCurveFrameMsg {
    Curve(CurveSetupMsg),
    DragStart,
    DragUpdate(f64, f64),
    DragEnd,
    AddPoint,
    RemovePoint,
    DefaultCurve,
}

#[derive(Debug)]
pub(super) struct CurveSetupMsg {
    pub curve: FanCurveMap,
    pub hw_based: bool,
    pub current_temperatures: IndexMap<String, TemperatureEntry>,
    pub temperature_key: Option<String>,
    pub speed_range: RangeInclusive<f32>,
    pub temperature_range: RangeInclusive<f32>,
    // Non-PMFW only
    pub spindown_delay: Option<u64>,
    pub change_threshold: Option<u64>,
    /// Nvidia only
    pub auto_threshold_supported: bool,
    pub auto_threshold: Option<u64>,
}

#[relm4::component(pub)]
impl relm4::Component for FanCurveFrame {
    type Init = ();
    type Input = FanCurveFrameMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 5,

            #[name = "drawing_area"]
            gtk::DrawingArea {
                set_expand: true,
                set_height_request: 350,
                set_draw_func[chart = model.chart.clone()] => move |_, ctx, width, height| {
                    chart.draw_chart(ctx, width, height, PlotColorScheme::current());
                },

                add_controller = gtk::GestureClick {
                    connect_pressed[sender] => move |_, _, x, y| {
                        sender.input(FanCurveFrameMsg::DragStart);
                        sender.input(FanCurveFrameMsg::DragUpdate(x, y));
                    },
                    connect_released[sender] => move |_, _, _x, _y| {
                        sender.input(FanCurveFrameMsg::DragEnd);
                    }
                },

                add_controller = gtk::EventControllerMotion {
                    connect_motion[sender] => move |_, x, y| {
                        sender.input(FanCurveFrameMsg::DragUpdate(x, y));
                    },
                },

                #[watch]
                set_cursor: if model.chart.is_dragging.get() {
                    gdk::Cursor::from_name("move", None)
                } else {
                    None
                }.as_ref(),
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 5,
                set_halign: gtk::Align::End,

                gtk::Button {
                    set_icon_name: "list-add-symbolic",
                    connect_clicked => FanCurveFrameMsg::AddPoint,
                    #[watch]
                    set_visible: !model.hw_based_fan_curve,
                },

                gtk::Button {
                    set_icon_name: "list-remove-symbolic",
                    connect_clicked => FanCurveFrameMsg::RemovePoint,
                    #[watch]
                    set_visible: !model.hw_based_fan_curve,
                },

                gtk::Button {
                    set_label: &fl!(I18N, "default-button"),
                    connect_clicked => FanCurveFrameMsg::DefaultCurve,
                },
            },

            model.adjustments.widget().clone() -> gtk::ListBox {
                #[watch]
                set_visible: !model.adjustments.is_empty(),
            },
        },
    }

    fn post_view() {
        if self.chart.is_dragging.get() {
            drawing_area.queue_draw();
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            adjustments: FactoryHashMap::builder()
                .launch(AdjustmentCard::init(()).content.clone())
                .forward(APP_BROKER.sender(), |()| AppMsg::SettingsChanged),
            label_size_group: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
            input_size_group: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
            hw_based_fan_curve: false,
            chart: Rc::new(ChartState {
                data: RefCell::default(),
                speed_range: RefCell::new(DEFAULT_SPEED_RANGE),
                temperature_range: RefCell::new(DEFAULT_TEMP_RANGE),
                is_dragging: Cell::new(false),
                drag_coord: Cell::default(),
                drag_point: Cell::default(),
                hover_coord: Cell::default(),
                hover_point: Cell::default(),
            }),
            temp_keys: Vec::new(),
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
            FanCurveFrameMsg::Curve(msg) => {
                *self.chart.data.borrow_mut() =
                    normalize_fan_curve(msg.curve, &msg.temperature_range, &msg.speed_range)
                        .collect();
                *self.chart.speed_range.borrow_mut() = msg.speed_range;
                *self.chart.temperature_range.borrow_mut() = msg.temperature_range.clone();
                self.hw_based_fan_curve = msg.hw_based;

                let mut temp_keys = msg
                    .current_temperatures
                    .into_iter()
                    .filter(|(_, temp)| !temp.display_only)
                    .map(|(key, _)| key)
                    .collect::<Vec<_>>();
                temp_keys.sort();

                let selected_idx = msg
                    .temperature_key
                    .and_then(|current_key| temp_keys.iter().position(|key| *key == current_key))
                    .or_else(|| temp_keys.iter().position(|key| key == "edge"))
                    .or(if temp_keys.is_empty() { None } else { Some(0) });

                self.temp_keys = temp_keys;
                self.adjustments.clear();
                let lower_label_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
                let upper_label_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
                let auto_threshold_available =
                    msg.auto_threshold_supported && *msg.temperature_range.end() != 0.0;
                if !msg.hw_based && self.temp_keys.len() > 1 && !auto_threshold_available {
                    self.adjustments.insert(
                        CurveSetting::TemperatureSensor,
                        SettingRowInit {
                            label_group: Some(self.label_size_group.clone()),
                            input_group: Some(self.input_size_group.clone()),
                            ..SettingRowInit::new(
                                fl!(I18N, "temperature-sensor"),
                                SelectInit {
                                    options: self
                                        .temp_keys
                                        .iter()
                                        .cloned()
                                        .map(Into::into)
                                        .collect(),
                                    selected: selected_idx.map(|index| index as u32),
                                    ..Default::default()
                                },
                            )
                        },
                    );
                }
                if !msg.hw_based {
                    self.adjustments.insert(
                        CurveSetting::SpindownDelay,
                        SettingRowInit {
                            title_tooltip: fl!(I18N, "spindown-delay-tooltip"),
                            label_group: Some(self.label_size_group.clone()),
                            input_group: Some(self.input_size_group.clone()),
                            ..SettingRowInit::new(
                                glib::markup_escape_text(&fl!(I18N, "spindown-delay")).into(),
                                AdjustmentInit {
                                    value: msg.spindown_delay.unwrap_or(DEFAULT_SPINDOWN_DELAY_MS)
                                        as f64,
                                    default_value: Some(DEFAULT_SPINDOWN_DELAY_MS as f64),
                                    upper: 30_000.0,
                                    step_increment: 10.0,
                                    page_increment: 10.0,
                                    lower_label_group: Some(lower_label_group.clone()),
                                    upper_label_group: Some(upper_label_group.clone()),
                                    ..Default::default()
                                },
                            )
                        },
                    );
                    self.adjustments.insert(
                        CurveSetting::ChangeThreshold,
                        SettingRowInit {
                            label_group: Some(self.label_size_group.clone()),
                            input_group: Some(self.input_size_group.clone()),
                            ..SettingRowInit::new(
                                glib::markup_escape_text(&fl!(I18N, "speed-change-threshold"))
                                    .into(),
                                AdjustmentInit {
                                    value: msg.change_threshold.unwrap_or(DEFAULT_CHANGE_THRESHOLD)
                                        as f64,
                                    default_value: Some(DEFAULT_CHANGE_THRESHOLD as f64),
                                    upper: 10.0,
                                    page_increment: 1.0,
                                    lower_label_group: Some(lower_label_group.clone()),
                                    upper_label_group: Some(upper_label_group.clone()),
                                    ..Default::default()
                                },
                            )
                        },
                    );
                }
                if auto_threshold_available {
                    self.adjustments.insert(
                        CurveSetting::AutoThreshold,
                        SettingRowInit {
                            title_tooltip: fl!(I18N, "automatic-mode-threshold-tooltip"),
                            label_group: Some(self.label_size_group.clone()),
                            input_group: Some(self.input_size_group.clone()),
                            ..SettingRowInit::new(
                                glib::markup_escape_text(&fl!(I18N, "automatic-mode-threshold"))
                                    .into(),
                                AdjustmentInit {
                                    value: msg.auto_threshold.unwrap_or(DEFAULT_AUTO_THRESHOLD)
                                        as f64,
                                    upper: *msg.temperature_range.end() as f64,
                                    page_increment: 5.0,
                                    lower_label_group: Some(lower_label_group.clone()),
                                    upper_label_group: Some(upper_label_group.clone()),
                                    ..Default::default()
                                },
                            )
                        },
                    );
                }

                widgets.drawing_area.queue_draw();
            }
            FanCurveFrameMsg::DragStart => {
                self.chart.is_dragging.set(true);
            }
            FanCurveFrameMsg::DragUpdate(x, y) => {
                self.chart.hover_coord.set(Some((x, y)));

                if self.chart.is_dragging.get() {
                    self.chart.drag_coord.set(Some((x, y)));
                }

                widgets.drawing_area.queue_draw();
            }
            FanCurveFrameMsg::DragEnd => {
                self.chart.drag_coord.take();
                self.chart.drag_point.take();
                self.chart.is_dragging.set(false);
            }
            FanCurveFrameMsg::AddPoint => {
                let temp_range = self.chart.temperature_range.borrow();
                let speed_range = self.chart.speed_range.borrow();

                self.edit_curve(
                    |curve| {
                        curve.push((*temp_range.end() as i32, *speed_range.end()));
                    },
                    widgets,
                );
            }
            FanCurveFrameMsg::RemovePoint => {
                self.edit_curve(
                    |curve| {
                        curve.pop();
                    },
                    widgets,
                );
            }
            FanCurveFrameMsg::DefaultCurve => {
                self.edit_curve(
                    |curve| {
                        *curve = normalize_fan_curve(
                            default_fan_curve(),
                            &self.chart.temperature_range.borrow(),
                            &self.chart.speed_range.borrow(),
                        )
                        .collect();
                    },
                    widgets,
                );
                for setting in [CurveSetting::SpindownDelay, CurveSetting::ChangeThreshold] {
                    if self.adjustments.get(&setting).is_some() {
                        self.adjustments.send(&setting, SettingRowMsg::Reset);
                    }
                }
            }
        }
        self.update_view(widgets, sender);
    }
}

impl FanCurveFrame {
    pub fn get_curve(&self) -> FanCurveMap {
        self.chart.data.borrow().iter().copied().collect()
    }

    pub fn spindown_delay(&self) -> u64 {
        self.adjustments
            .get(&CurveSetting::SpindownDelay)
            .and_then(|row| row.get_value().as_numeric())
            .map(|value| value as u64)
            .unwrap_or(DEFAULT_SPINDOWN_DELAY_MS)
    }

    pub fn change_threshold(&self) -> u64 {
        self.adjustments
            .get(&CurveSetting::ChangeThreshold)
            .and_then(|row| row.get_value().as_numeric())
            .map(|value| value as u64)
            .unwrap_or(DEFAULT_CHANGE_THRESHOLD)
    }

    pub fn temperature_key(&self) -> Option<String> {
        let index = self
            .adjustments
            .get(&CurveSetting::TemperatureSensor)?
            .get_value()
            .as_selected()?;
        self.temp_keys.get(index as usize).cloned()
    }

    pub fn auto_threshold(&self) -> Option<u64> {
        self.adjustments
            .get(&CurveSetting::AutoThreshold)
            .and_then(|row| row.get_changed_value())
            .and_then(SettingRowValue::as_numeric)
            .map(|value| value as u64)
    }

    fn edit_curve(&self, f: impl FnOnce(&mut Vec<(i32, f32)>), widgets: &FanCurveFrameWidgets) {
        f(&mut self.chart.data.borrow_mut());

        widgets.drawing_area.queue_draw();
        APP_BROKER.send(AppMsg::SettingsChanged);
    }
}

impl ChartState {
    fn draw_chart(&self, ctx: &cairo::Context, width: i32, height: i32, colors: PlotColorScheme) {
        let cairo_backend = CairoBackend::new(ctx, (width as u32, height as u32)).unwrap();

        let translate_coords = [self.drag_coord.take(), self.hover_coord.take()];

        let translated_coords = draw_chart(
            cairo_backend,
            &self.data.borrow(),
            translate_coords,
            self.hover_point.get(),
            colors,
            &self.temperature_range.borrow(),
            &self.speed_range.borrow(),
        );

        if let Some(mut moved_value) = translated_coords[0] {
            let drag_point_idx = match self.drag_point.get() {
                Some(idx) => Some(idx),
                None => {
                    let point = self.data.borrow().iter().position(|(data_x, data_y)| {
                        (*data_x as f32 - moved_value.0).abs() <= TEMPERATURE_DRAG_MARGIN
                            && (*data_y - moved_value.1).abs() <= PERCENTAGE_DRAG_MARGIN
                    });
                    self.drag_point.set(point);
                    point
                }
            };
            if let Some(idx) = drag_point_idx {
                normalize_to_range(&mut moved_value.0, &self.temperature_range.borrow());
                normalize_to_range(&mut moved_value.1, &self.speed_range.borrow());
                self.data.borrow_mut()[idx] = (moved_value.0 as i32, moved_value.1);

                APP_BROKER.send(AppMsg::SettingsChanged);
            }
        }

        let hovered_point = translated_coords[1].and_then(|(x, y)| {
            self.data.borrow().iter().position(|(data_x, data_y)| {
                (*data_x as f32 - x).abs() <= TEMPERATURE_DRAG_MARGIN
                    && (*data_y - y).abs() <= PERCENTAGE_DRAG_MARGIN
            })
        });
        self.hover_point.set(hovered_point);
    }
}

fn normalize_fan_curve<'a>(
    curve: impl IntoIterator<Item = (i32, f32)> + 'a,
    temperature_range: &'a RangeInclusive<f32>,
    speed_range: &'a RangeInclusive<f32>,
) -> impl Iterator<Item = (i32, f32)> + 'a {
    curve.into_iter().map(|(temp, mut speed)| {
        let mut temp = temp as f32;
        normalize_to_range(&mut temp, temperature_range);
        normalize_to_range(&mut speed, speed_range);
        (temp as i32, speed)
    })
}

fn normalize_to_range(value: &mut f32, range: &RangeInclusive<f32>) {
    *value = f32::max(*value, *range.start());
    *value = f32::min(*value, *range.end());
    *value = (*value * 100.0).round() / 100.0;
}

fn draw_chart<const I: usize>(
    backend: CairoBackend,
    data: &[(i32, f32)],
    translate_coords: [Option<(f64, f64)>; I],
    hovered_point: Option<usize>,
    colors: PlotColorScheme,
    temp_range: &RangeInclusive<f32>,
    speed_range: &RangeInclusive<f32>,
) -> [Option<(f32, f32)>; I] {
    let root = backend.into_drawing_area();
    root.fill(&colors.background).unwrap();

    let mut chart = ChartBuilder::on(&root)
        .x_label_area_size(45)
        .y_label_area_size(60)
        .margin(10)
        .build_cartesian_2d(
            *temp_range.start()..*temp_range.end(),
            *speed_range.start()..*speed_range.end(),
        )
        .unwrap();

    chart
        .configure_mesh()
        .axis_style(colors.border_secondary)
        .bold_line_style(colors.border)
        .max_light_lines(0)
        .x_label_formatter(&|temp| format!("{temp:.}°C"))
        .y_label_formatter(&|speed| format!("{:.0}%", speed * 100.0))
        .x_label_style(("sans-serif", 14, &colors.text))
        .y_label_style(("sans-serif", 14, &colors.text))
        .x_desc("Temperature")
        .y_desc("Speed")
        .draw()
        .unwrap();

    chart
        .draw_series(LineSeries::new(
            data.first()
                .map(|(_, y)| (*temp_range.start(), { *y }))
                .into_iter()
                .chain(data.iter().map(|(x, y)| (*x as f32, *y)))
                .chain(data.last().map(|(_, y)| (*temp_range.end(), *y))),
            &LIGHTBLUE,
        ))
        .unwrap();

    chart
        .draw_series(PointSeries::of_element(
            data.iter().map(|(x, y)| (*x as f32, *y)).enumerate(),
            8,
            ShapeStyle::from(&LIGHTBLUE).filled(),
            &|(i, coord), size, mut style| {
                if hovered_point == Some(i) {
                    style.color = INDIGO.to_rgba();
                }

                EmptyElement::at(coord)
                    + Circle::new((0, 0), size, style)
                    + Text::new(
                        format!("{:.0}% at {}°C", coord.1 * 100.0, coord.0),
                        (
                            if coord.0 - temp_range.start() < 5.0 {
                                0
                            } else if temp_range.end() - coord.0 < 8.0 {
                                -85
                            } else {
                                -35
                            },
                            if coord.1 - speed_range.start() < 0.06 {
                                -25
                            } else {
                                15
                            },
                        ),
                        TextStyle {
                            font: ("sans-serif", 15).into(),
                            color: colors.text.to_backend_color(),
                            pos: Pos::default(),
                        },
                    )
            },
        ))
        .unwrap();

    let mapped_coords = translate_coords.map(|coords| {
        coords.map(|(x, y)| {
            let coord_spec = chart.as_coord_spec();
            let x_range = coord_spec.get_x_axis_pixel_range();
            let y_range = coord_spec.get_y_axis_pixel_range();

            (
                (x as i32).clamp(x_range.start, x_range.end),
                (y as i32).clamp(y_range.start, y_range.end),
            )
        })
    });

    let coord_trans = chart.into_coord_trans();

    let mapped_coords = mapped_coords.map(|coord| coord.and_then(|(x, y)| coord_trans((x, y))));

    root.present().unwrap();

    mapped_coords
}

#[cfg(all(test, feature = "gtk-tests"))]
mod tests {
    use super::*;
    use crate::app::test_utils::find_widget;
    use relm4::{Component, ComponentController};

    #[test]
    #[ignore = "requires a GTK display; run explicitly with --ignored"]
    fn sensor_row_preserves_keys_and_clears_on_device_reload() {
        adw::init().unwrap();
        let context = glib::MainContext::default();
        let _guard = context.acquire().unwrap();
        let drain = || {
            while context.pending() {
                context.iteration(false);
            }
        };
        let frame = FanCurveFrame::builder().launch(()).detach();
        let setup = |hw_based, auto_threshold_supported, temperature_key: &str| CurveSetupMsg {
            curve: default_fan_curve(),
            hw_based,
            current_temperatures: ["junction", "edge", "display-only"]
                .into_iter()
                .map(|key| {
                    (
                        key.to_owned(),
                        TemperatureEntry {
                            value: amdgpu_sysfs::hw_mon::Temperature {
                                current: Some(50.0),
                                crit: None,
                                crit_hyst: None,
                            },
                            primary: true,
                            display_only: key == "display-only",
                        },
                    )
                })
                .collect(),
            temperature_key: Some(temperature_key.to_owned()),
            speed_range: DEFAULT_SPEED_RANGE,
            temperature_range: DEFAULT_TEMP_RANGE,
            spindown_delay: None,
            change_threshold: None,
            auto_threshold_supported,
            auto_threshold: None,
        };
        frame.emit(FanCurveFrameMsg::Curve(setup(false, false, "junction")));
        drain();
        assert_eq!(frame.model().temperature_key().as_deref(), Some("junction"));
        let dropdown = {
            let rows = &frame.model().adjustments;
            assert_eq!(rows.len(), 3);
            assert!(rows.values().all(|row| row.get_changed_value().is_none()));
            find_widget::<gtk::DropDown>(
                rows[&CurveSetting::TemperatureSensor].widget().upcast_ref(),
            )
            .unwrap()
        };
        assert_eq!(dropdown.model().unwrap().n_items(), 2);
        dropdown.set_selected(0);
        drain();
        assert_eq!(frame.model().temperature_key().as_deref(), Some("edge"));
        frame.emit(FanCurveFrameMsg::DefaultCurve);
        drain();
        assert_eq!(frame.model().temperature_key().as_deref(), Some("edge"));

        for (hw_based, auto_threshold_supported) in [(true, false), (false, true)] {
            frame.emit(FanCurveFrameMsg::Curve(setup(
                hw_based,
                auto_threshold_supported,
                "edge",
            )));
            drain();
            assert_eq!(frame.model().temperature_key(), None);
            assert!(
                frame
                    .model()
                    .adjustments
                    .get(&CurveSetting::TemperatureSensor)
                    .is_none()
            );
        }
        frame.emit(FanCurveFrameMsg::Curve(setup(false, false, "missing")));
        drain();
        assert_eq!(frame.model().temperature_key().as_deref(), Some("edge"));
        assert!(
            frame
                .model()
                .adjustments
                .values()
                .all(|row| row.get_changed_value().is_none())
        );
    }
}
