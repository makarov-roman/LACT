use crate::{CONFIG, I18N, app::{overdrive_dialog::{OverdriveDialog, OverdriveDialogMsg}, process_monitor::{ProcessMonitorWindow, ProcessMonitorWindowMsg}}};
use anyhow::Context;
use gtk::{glib::clone, prelude::{Cast, DialogExtManual, FileChooserExt, FileExt, GtkWindowExt, OrientableExt, WidgetExt}, FileChooserAction, FileChooserDialog, ResponseType};
use i18n_embed_fl::fl;
use lact_schema::{DeviceStats, config::GpuConfig, DeviceListEntry, ClocksTable, PowerStates, ProfilesInfo, DeviceInfo, SystemInfo};
use amdgpu_sysfs::gpu_handle::power_profile_mode::PowerProfileModesTable;
use relm4::{AsyncComponentSender, Component, ComponentController, RelmObjectExt, binding::BoolBinding, prelude::{AsyncComponent, AsyncComponentParts}, tokio::time::sleep};
use relm4_components::open_dialog::{OpenDialog, OpenDialogMsg, OpenDialogResponse, OpenDialogSettings};
use relm4_components::save_dialog::{SaveDialog, SaveDialogMsg, SaveDialogResponse, SaveDialogSettings};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tracing::{trace, error};

use super::{apply_revealer::{ApplyRevealer, ApplyRevealerMsg}, confirmation_dialog::ConfirmationDialog, ext::RelmDefaultLauchable, graphs_window::{GraphsWindow, GraphsWindowMsg}, header::{Header, HeaderMsg}, pages::{PageUpdate, crash_page::CrashPage, info_page::InformationPage, oc_page::{OcPage, OcPageMsg}, software_page::{SoftwarePage, SoftwarePageMsg}, thermals_page::{ThermalsPage, ThermalsPageMsg}}, msg::{AppMsg, AppContentMsg, UiCommand}, show_error, show_embedded_info};

const PROCESS_POLL_INTERVAL_MS: u64 = 1500;

#[derive(Debug, Clone)]
pub struct InitialGpuData {
    pub info: DeviceInfo,
    pub stats: DeviceStats,
    pub clocks_table: Option<ClocksTable>,
    pub profile_modes: Option<Arc<PowerProfileModesTable>>,
    pub power_states: Option<PowerStates>,
    pub config: Option<GpuConfig>,
}

pub struct AppContentInit {
    pub system_info: SystemInfo,
    pub devices: Vec<DeviceListEntry>,
    pub conn_err: Option<Arc<anyhow::Error>>,
    pub root: gtk::ApplicationWindow,
    pub initial_gpu: Option<(String, InitialGpuData)>,
    pub profiles: Arc<ProfilesInfo>,
    pub embedded: bool,
}

pub struct AppContent {
    pub graphs_window: relm4::Controller<GraphsWindow>,
    pub process_monitor_window: relm4::Controller<ProcessMonitorWindow>,
    pub overdrive_dialog: relm4::Controller<OverdriveDialog>,
    pub ui_sensitive: BoolBinding,
    pub info_page: relm4::Controller<InformationPage>,
    pub oc_page: relm4::Controller<OcPage>,
    pub thermals_page: relm4::Controller<ThermalsPage>,
    pub software_page: relm4::Controller<SoftwarePage>,
    pub crash_page: relm4::Controller<CrashPage>,
    pub header: relm4::Controller<Header>,
    pub apply_revealer: relm4::Controller<ApplyRevealer>,
    pub root: gtk::ApplicationWindow,
}

#[derive(Debug)]
pub enum CommandOutput {
    ProfileImport(PathBuf),
}

#[relm4::component(pub, async)]
impl AsyncComponent for AppContent {
    type Init = AppContentInit;
    type Input = AppContentMsg;
    type Output = AppMsg;
    type CommandOutput = Option<CommandOutput>;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,

                #[name = "root_stack"]
                gtk::Stack {
                    set_vexpand: false,
                    set_vhomogeneous: false,

                    add_binding: (&model.ui_sensitive, "sensitive"),

                    add_titled[Some("info_page"), &fl!(I18N, "info-page")] = model.info_page.widget(),
                    add_titled[Some("oc_page"), &fl!(I18N, "oc-page")] = model.oc_page.widget(),
                    add_titled[Some("thermals_page"), &fl!(I18N, "thermals-page")] = model.thermals_page.widget(),
                    add_titled[Some("software_page"), &fl!(I18N, "software-page")] = model.software_page.widget(),
                    add_named[Some("crash_page")] = model.crash_page.widget(),

                    set_visible_child_name: &CONFIG.read().selected_tab,
                    connect_visible_child_name_notify => move |stack| {
                        if let Some(name) = stack.visible_child_name() {
                            let name = name.to_string();
                            if name != "crash_page" {
                                CONFIG.write().edit(|config| {
                                    config.selected_tab = name;
                                });
                            }
                        }
                    },
                },
            },

            model.apply_revealer.widget(),
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let AppContentInit { 
            system_info, 
            devices, 
            conn_err, 
            root: window,
            initial_gpu,
            profiles,
            embedded,
        } = init;

        let info_page = InformationPage::detach_default();

        let oc_page = OcPage::builder()
            .launch(system_info.clone())
            .forward(sender.input_sender(), AppContentMsg::Action);
        let thermals_page = ThermalsPage::builder().launch(system_info.clone()).detach();

        let software_page = SoftwarePage::builder()
            .launch((system_info.clone(), embedded))
            .detach();

        let crash_page = CrashPage::builder()
            .launch(String::new())
            .forward(sender.input_sender(), AppContentMsg::Action);

        let overdrive_dialog = OverdriveDialog::builder()
            .transient_for(&window)
            .launch(OverdriveDialog {
                system_info: system_info.clone(),
            })
            .detach();

        let header = Header::builder()
            .update_root(|headerbar| {
                *headerbar = window
                    .titlebar()
                    .unwrap()
                    .downcast::<gtk::HeaderBar>()
                    .unwrap();
            })
            .launch((devices, system_info))
            .forward(sender.input_sender(), AppContentMsg::Action);

        let apply_revealer = ApplyRevealer::builder()
            .launch(())
            .forward(sender.input_sender(), AppContentMsg::Action);

        let graphs_window = GraphsWindow::detach_default();
        let process_monitor_window = ProcessMonitorWindow::detach_default();

        let ui_sensitive = BoolBinding::new(true);

        let model = AppContent {
            graphs_window,
            process_monitor_window,
            overdrive_dialog,
            info_page,
            oc_page,
            thermals_page,
            software_page,
            crash_page,
            apply_revealer,
            ui_sensitive: ui_sensitive.clone(),
            header,
            root: window,
        };

        let widgets = view_output!();

        if let Some(err) = conn_err {
            show_embedded_info(&model.root, anyhow::anyhow!(err.to_string()));
        }

        model
            .header
            .widgets()
            .stack_switcher
            .set_stack(Some(&widgets.root_stack));

        model.header.emit(HeaderMsg::Profiles(Box::new((*profiles).clone())));

        if let Some((_gpu_id, gpu_data)) = initial_gpu {
            sender.input(AppContentMsg::GpuDataUpdate {
                info: Some(Arc::new(gpu_data.info)),
                stats: Some(Arc::new(gpu_data.stats)),
                clocks_table: gpu_data.clocks_table,
                profile_modes: gpu_data.profile_modes,
                power_states: gpu_data.power_states,
                config: gpu_data.config,
            });
        } else {
            sender.output(AppMsg::ReloadData { full: true }).unwrap();
        }

        let task_sender = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    loop {
                        sleep(Duration::from_millis(PROCESS_POLL_INTERVAL_MS)).await;
                        task_sender.input(AppContentMsg::Action(AppMsg::FetchProcessList));
                    }
                })
                .drop_on_shutdown()
        });

        AsyncComponentParts { model, widgets }
    }

    async fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        trace!("processing state update");
        if let Err(err) = self.handle_msg(msg, sender.clone(), widgets).await {
            show_error(&self.root, &err);
        }
        self.update_view(widgets, sender);
    }

    async fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        if let Some(msg) = msg
            && let Err(err) = self.handle_cmd_output(msg, &sender).await
        {
            sender.input(AppContentMsg::Error(Arc::new(err.into())));
        }
    }
}

impl AppContent {
    async fn handle_msg(
        &mut self,
        msg: AppContentMsg,
        sender: AsyncComponentSender<Self>,
        widgets: &<Self as AsyncComponent>::Widgets,
    ) -> Result<(), Arc<anyhow::Error>> {
        match msg {
            AppContentMsg::Error(err) => return Err(err),
            AppContentMsg::LoadingStatus(status) => {
                trace!("loading status: {status}");
            }
            AppContentMsg::Stats(stats) => {
                let update = PageUpdate::Stats(stats.clone());
                self.oc_page.emit(OcPageMsg::Update {
                    update: update.clone(),
                    initial: false,
                });
                self.thermals_page.emit(ThermalsPageMsg::Update {
                    update: update.clone(),
                    initial: false,
                });
                self.graphs_window.emit(GraphsWindowMsg::Stats {
                    stats,
                    selected_gpu_id: None,
                });
            }
            AppContentMsg::Profiles(profiles) => {
                self.header.emit(HeaderMsg::Profiles(Box::new((*profiles).clone())));
            }
            AppContentMsg::ProcessList(process_list) => {
                self.process_monitor_window
                    .emit(ProcessMonitorWindowMsg::Data(process_list));
            }
            AppContentMsg::GpuDataUpdate {
                info,
                stats,
                clocks_table,
                profile_modes,
                power_states,
                config,
            } => {
                if let Some(info) = info {
                    let update = PageUpdate::Info(info.clone());
                    self.info_page.emit(update.clone());
                    self.oc_page.emit(OcPageMsg::Update {
                        update: update.clone(),
                        initial: true,
                    });
                    self.software_page
                        .emit(SoftwarePageMsg::DeviceInfo(info.clone()));
                    self.header.emit(HeaderMsg::DeviceInfo(info.clone()));
                    self.thermals_page.emit(ThermalsPageMsg::Update {
                        update: update.clone(),
                        initial: true,
                    });

                    let vram_clock_ratio = info
                        .drm_info
                        .as_ref()
                        .map(|info| info.vram_clock_ratio)
                        .unwrap_or(1.0);
                    self.graphs_window
                        .emit(GraphsWindowMsg::VramClockRatio(vram_clock_ratio));
                }

                if let Some(stats) = stats {
                    let update = PageUpdate::Stats(stats.clone());
                    self.info_page.emit(update.clone());
                    self.thermals_page.emit(ThermalsPageMsg::Update {
                        update: update.clone(),
                        initial: true,
                    });
                    self.oc_page.emit(OcPageMsg::Update {
                        update: update.clone(),
                        initial: true,
                    });

                    self.graphs_window.emit(GraphsWindowMsg::Stats {
                        stats,
                        selected_gpu_id: None,
                    });
                }

                if let Some(clocks_table) = clocks_table {
                    self.oc_page.emit(OcPageMsg::ClocksTable(Some(clocks_table)));
                }

                if let Some(profile_modes) = profile_modes {
                    self.oc_page
                        .emit(OcPageMsg::ProfileModesTable(Some(profile_modes)));
                }

                if let Some(power_states) = power_states {
                    self.oc_page.emit(OcPageMsg::PowerStates {
                        pstates: power_states,
                        configured: config.as_ref().is_some_and(|config| !config.power_states.is_empty()),
                    });
                }

                self.ui_sensitive.set_value(true);
            }
            AppContentMsg::UiCommand(cmd) => match cmd {
                UiCommand::ShowGraphs => self.graphs_window.emit(GraphsWindowMsg::Show),
                UiCommand::ShowProcessMonitor => self.process_monitor_window.emit(ProcessMonitorWindowMsg::Show),
                UiCommand::ShowOverdriveDialog => self.overdrive_dialog.emit(OverdriveDialogMsg::Show),
                UiCommand::AskConfirmation(options, confirmed_msg) => {
                    let sender = sender.clone();
                    let mut controller = ConfirmationDialog::builder()
                        .launch((options, self.root.clone()))
                        .connect_receiver(move |_, response| {
                            if let gtk::ResponseType::Ok | gtk::ResponseType::Yes = response {
                                sender.input(AppContentMsg::Action(confirmed_msg.clone()));
                            }
                        });
                    controller.detach_runtime();
                }
            },
            AppContentMsg::Action(action) => match action {
                AppMsg::ApplyChanges => {
                    let gpu_id = self.current_gpu_id()?;
                    let config = self.collect_config();
                    sender.output(AppMsg::ApplyConfig(gpu_id, Box::new(config))).unwrap();
                }
                AppMsg::SettingsChanged => {
                    self.apply_revealer.emit(ApplyRevealerMsg::Show);
                }
                AppMsg::ShowGraphsWindow => sender.input(AppContentMsg::UiCommand(UiCommand::ShowGraphs)),
                AppMsg::ShowProcessMonitor => sender.input(AppContentMsg::UiCommand(UiCommand::ShowProcessMonitor)),
                AppMsg::ShowOverdriveDialog => sender.input(AppContentMsg::UiCommand(UiCommand::ShowOverdriveDialog)),
                AppMsg::ReloadData { .. } => {
                    self.apply_revealer.emit(ApplyRevealerMsg::Hide);
                    sender.output(action).unwrap();
                }
                AppMsg::ImportProfile => {
                    let json_filter = gtk::FileFilter::new();
                    json_filter.add_mime_type("application/json");

                    let settings = OpenDialogSettings {
                        filters: vec![json_filter],
                        ..Default::default()
                    };
                    let file_picker = OpenDialog::builder().launch(settings);
                    file_picker.emit(OpenDialogMsg::Open);
                    let stream = file_picker.into_stream();

                    sender.oneshot_command(async move {
                        if let Some(OpenDialogResponse::Accept(path)) = stream.recv_one().await {
                            Some(CommandOutput::ProfileImport(path))
                        } else {
                            None
                        }
                    });
                }
                AppMsg::AskConfirmation(options, confirmed_msg) => {
                    sender.input(AppContentMsg::UiCommand(UiCommand::AskConfirmation(options, *confirmed_msg)))
                }
                AppMsg::Crash(message) => {
                    sender.input(AppContentMsg::Crash(message));
                }
                AppMsg::DumpVBios => {
                    let file_chooser = FileChooserDialog::new(
                        Some("Save VBIOS file"),
                        Some(&self.root),
                        FileChooserAction::Save,
                        &[
                            ("Save", ResponseType::Accept),
                            ("Cancel", ResponseType::Cancel),
                        ],
                    );

                    let gpu_id = self.current_gpu_id().unwrap_or_default();
                    let file_name_suffix = gpu_id
                        .split_once('-')
                        .map(|(id, _)| id.replace(':', "_"))
                        .unwrap_or_default();
                    file_chooser.set_current_name(&format!("{file_name_suffix}_vbios_dump.rom"));
                    file_chooser.run_async(clone!(
                        #[strong]
                        sender,
                        move |diag, response| {
                            diag.close();

                            if response == gtk::ResponseType::Accept
                                && let Some(file) = diag.file()
                            {
                                if let Some(path) = file.path() {
                                    sender.output(AppMsg::DumpVBiosPath(path)).unwrap();
                                }
                            }
                        }
                    ));
                }
                other => sender.output(other).unwrap(),
            },
            AppContentMsg::Crash(message) => {
                self.header.widget().set_sensitive(false);
                self.apply_revealer.widget().set_sensitive(false);

                self.ui_sensitive.set_value(true);
                widgets.root_stack.set_visible_child_name("crash_page");
                self.crash_page.emit(message);
            }
            AppContentMsg::ExportProfileData(name, profile) => {
                let settings = SaveDialogSettings {
                    create_folders: true,
                    is_modal: true,
                    ..Default::default()
                };
                let diag = SaveDialog::builder().launch(settings);
                diag.emit(SaveDialogMsg::SaveAs(format!(
                    "LACT-profile-{}.json",
                    name.as_deref().unwrap_or("default")
                )));

                let stream = diag.into_stream();

                sender.oneshot_command(async move {
                    if let Some(SaveDialogResponse::Accept(path)) = stream.recv_one().await {
                        let contents = serde_json::to_string(&profile)
                            .expect("Could not serialize profile");

                        if let Err(err) = std::fs::write(path, contents) {
                            error!("Could not export profile: {err:#}");
                        }
                    }
                    None
                });
            }
        }
        Ok(())
    }

    async fn handle_cmd_output(
        &mut self,
        msg: CommandOutput,
        sender: &AsyncComponentSender<AppContent>,
    ) -> anyhow::Result<()> {
        match msg {
            CommandOutput::ProfileImport(path) => {
                sender.output(AppMsg::ImportProfilePath(path)).unwrap();
            }
        }

        Ok(())
    }

    fn current_gpu_id(&self) -> anyhow::Result<String> {
        self.header
            .model()
            .selected_gpu_id()
            .context("No GPU selected")
    }

    fn collect_config(&self) -> GpuConfig {
        let mut gpu_config = GpuConfig::default();

        let cap = self.oc_page.model().get_power_cap();
        gpu_config.power_cap = cap;

        let performance_level = self.oc_page.model().get_performance_level();
        gpu_config.performance_level = performance_level;
        gpu_config.power_profile_mode_index = self.oc_page.model().get_power_profile_mode();
        gpu_config.custom_power_profile_mode_hueristics = self
            .oc_page
            .model()
            .get_power_profile_mode_custom_heuristics();

        self.thermals_page.model().apply_config(&mut gpu_config);

        let clocks_commands = self.oc_page.model().get_clocks_commands();
        for command in clocks_commands {
            gpu_config.apply_clocks_command(&command);
        }

        let enabled_power_states = self.oc_page.model().get_enabled_power_states();
        gpu_config.power_states = enabled_power_states;

        gpu_config
    }
}
