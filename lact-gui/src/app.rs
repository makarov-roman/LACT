mod app_content;
mod apply_revealer;
mod confirmation_dialog;
mod ext;
pub(crate) mod formatting;
pub mod graphs_window;
mod header;
mod info_row;
mod info_row_level;
pub(crate) mod msg;
mod overdrive_dialog;
mod page_section;
mod page_section_expander;
pub(crate) mod pages;
mod process_monitor;
pub(crate) mod styles;

use crate::{
    app::{
        app_content::{AppContent, AppContentInit, InitialGpuData},
    },
    APP_ID, GUI_VERSION, I18N,
};
use anyhow::{anyhow, Context};
use gtk::{
    glib::{self, clone, ControlFlow},
    prelude::{BoxExt, DialogExtManual, GtkWindowExt, OrientableExt, WidgetExt},
    ApplicationWindow, ButtonsType, MessageDialog, MessageType, ResponseType,
};
use i18n_embed_fl::fl;
use lact_client::{ConnectionStatusMsg, DaemonClient};
use lact_schema::{
    args::GuiArgs,
    config::GpuConfig,
    request::{ConfirmCommand, ProfileBase, SetClocksCommand},
    DeviceStats, GIT_COMMIT,
};
use msg::AppMsg;
use relm4::{
    component::AsyncComponentController,
    prelude::{AsyncComponent, AsyncComponentParts, AsyncController},
    AsyncComponentSender, MessageBroker, RelmWidgetExt,
};
use std::{cell::RefCell, os::unix::net::UnixStream, rc::Rc, sync::Arc, time::Duration};
use tracing::{debug, error, info, warn};

pub(crate) static APP_BROKER: MessageBroker<AppMsg> = MessageBroker::new();

pub struct AppModel {
    state: AppState,
    daemon_holder: Rc<RefCell<Option<(DaemonClient, Option<Arc<anyhow::Error>>)>>>,
    loading_status: String,
    selected_gpu_id: Option<String>,
}

enum AppState {
    Loading,
    Running {
        content: AsyncController<AppContent>,
        daemon_client: DaemonClient,
        stats_task_handle: Option<glib::JoinHandle<()>>,
    },
    Crashed,
}

#[relm4::component(pub, async)]
impl AsyncComponent for AppModel {
    type Init = GuiArgs;
    type Input = AppMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::ApplicationWindow::builder()
            .titlebar(&gtk::HeaderBar::new())
            .default_height(850)
            .default_width(1100)
            .icon_name(APP_ID)
            .title("LACT")
            .build() {
                #[name = "root_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,

                    #[name = "loading_page"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_vexpand: true,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_spacing: 12,

                        gtk::Spinner {
                            set_spinning: true,
                            set_size_request: (48, 48),
                        },

                        gtk::Label {
                            #[watch]
                            set_label: &model.loading_status,
                        },
                    },

                    #[name = "content_container"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_vexpand: true,
                    },

                    #[name = "crash_page"]
                    gtk::Label {
                        set_vexpand: true,
                        set_valign: gtk::Align::Center,
                        set_halign: gtk::Align::Center,
                        set_margin_all: 20,
                        set_wrap: true,
                        set_selectable: true,
                    },
                }
            },

        #[name = "reconnecting_dialog"]
        gtk::Window {
            set_transient_for: Some(&root),
            set_modal: true,
            set_title: Some(&fl!(I18N, "daemon-connection-lost")),
            set_destroy_with_parent: true,
            connect_close_request[root] => move |_| {
                root.close();
                glib::Propagation::Stop
            },

            gtk::Label {
                set_margin_all: 10,
                set_label: &fl!(I18N, "reconnecting-to-daemon"),
            }
        },
    }

    async fn init(
        args: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        relm4::set_global_css(styles::COMBINED_CSS);

        let daemon_holder = Rc::new(RefCell::new(None));
        let daemon_holder_clone = daemon_holder.clone();

        let tcp_address = args.tcp_address.clone();
        relm4::spawn_local(glib::clone!(
            #[strong]
            sender,
            async move {
                sender.input(AppMsg::LoadingStatus(fl!(I18N, "connecting-to-daemon")));

                let (daemon_client, conn_err) = match tcp_address {
                    Some(remote_addr) => {
                        info!("establishing connection to {remote_addr}");
                        match DaemonClient::connect_tcp(&remote_addr).await {
                            Ok(conn) => (conn, None),
                            Err(err) => {
                                error!("TCP connection error: {err:#}");
                                match create_connection().await {
                                    Ok((conn, _)) => (conn, Some(Arc::new(err))),
                                    Err(e) => {
                                        sender.input(AppMsg::Error(Arc::new(e)));
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    None => match create_connection().await {
                        Ok((conn, err)) => (conn, err),
                        Err(e) => {
                            sender.input(AppMsg::Error(Arc::new(e)));
                            return;
                        }
                    },
                };

                let mut conn_status_rx = daemon_client.status_receiver();
                let sender_clone = sender.clone();
                relm4::spawn_local(async move {
                    loop {
                        if let Ok(msg) = conn_status_rx.recv().await {
                            sender_clone.input(AppMsg::ConnectionStatus(msg));
                        }
                    }
                });

                sender.input(AppMsg::LoadingStatus(fl!(I18N, "fetching-data")));

                match daemon_client.get_system_info().await {
                    Ok(system_info) => {
                        match daemon_client.list_devices().await {
                            Ok(devices) => {
                                let initial_gpu_id = devices.first().map(|d| d.id.clone());
                                let mut initial_gpu_data = None;
                                
                                if let Some(gpu_id) = &initial_gpu_id {
                                    match fetch_initial_gpu_data(&daemon_client, gpu_id).await {
                                        Ok(data) => initial_gpu_data = Some((gpu_id.clone(), data)),
                                        Err(err) => {
                                            sender.input(AppMsg::Error(Arc::new(err)));
                                            return;
                                        }
                                    }
                                }

                                match daemon_client.list_profiles(false).await {
                                    Ok(profiles) => {
                                        *daemon_holder_clone.borrow_mut() = Some((daemon_client, conn_err));
                                        sender.input(AppMsg::DataLoaded { 
                                            system_info, 
                                            devices,
                                            initial_gpu: initial_gpu_data,
                                            profiles: Arc::new(profiles),
                                        });
                                    }
                                    Err(err) => sender.input(AppMsg::Error(Arc::new(err))),
                                }
                            }
                            Err(err) => sender.input(AppMsg::Error(Arc::new(err))),
                        }
                    }
                    Err(err) => sender.input(AppMsg::Error(Arc::new(err))),
                }
            }
        ));

        let model = AppModel {
            state: AppState::Loading,
            daemon_holder,
            loading_status: fl!(I18N, "connecting-to-daemon"),
            selected_gpu_id: None,
        };

        let widgets = view_output!();

        widgets.loading_page.set_visible(true);
        widgets.content_container.set_visible(false);
        widgets.crash_page.set_visible(false);

        AsyncComponentParts { model, widgets }
    }

    async fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match &mut self.state {
            AppState::Loading => {
                match msg {
                    AppMsg::DataLoaded { system_info, devices, initial_gpu, profiles } => {
                        if system_info.version != GUI_VERSION
                            || system_info.commit.as_deref() != Some(GIT_COMMIT)
                        {
                            let err = anyhow!(
                                "Version mismatch between GUI and daemon ({GUI_VERSION}-{GIT_COMMIT} vs {}-{})! If you have updated LACT, you need to restart the service with sudo systemctl restart lactd.",
                                system_info.version,
                                system_info.commit.as_deref().unwrap_or_default()
                            );
                            show_error(root, &err);
                            return;
                        }

                        let (daemon_client, conn_err) = self.daemon_holder.borrow_mut().take().unwrap();

                        let content = AppContent::builder()
                            .launch(AppContentInit {
                                system_info,
                                devices,
                                conn_err,
                                root: root.clone(),
                                initial_gpu: initial_gpu.clone(),
                                profiles: profiles.clone(),
                                embedded: daemon_client.embedded,
                            })
                            .forward(sender.input_sender(), |msg| msg);

                        let content_widgets = content.widget();
                        root.set_child(Some(content_widgets));

                        let initial_gpu_id = initial_gpu.as_ref().map(|(id, _)| id.clone());
                        self.selected_gpu_id = initial_gpu_id.clone();
                        
                        let stats_task_handle = initial_gpu_id.map(|gpu_id| {
                            start_stats_update_loop(
                                gpu_id,
                                daemon_client.clone(),
                                content.sender().clone(),
                            )
                        });

                        self.state = AppState::Running { 
                            content,
                            daemon_client,
                            stats_task_handle,
                        };
                    }
                    AppMsg::Error(err) => {
                        self.state = AppState::Crashed;
                        widgets.crash_page.set_label(&format!("Failed to initialize: {err:#}"));
                        widgets.loading_page.set_visible(false);
                        widgets.crash_page.set_visible(true);
                    }
                    AppMsg::LoadingStatus(status) => {
                        self.loading_status = status;
                    }
                    _ => {}
                }
            }
            AppState::Running { content, daemon_client, stats_task_handle } => {
                match msg {
                    AppMsg::ConnectionStatus(status) => match status {
                        ConnectionStatusMsg::Disconnected => widgets.reconnecting_dialog.present(),
                        ConnectionStatusMsg::Reconnected => widgets.reconnecting_dialog.hide(),
                    },
                    AppMsg::ReloadProfiles { state_sender } => {
                        if let Err(err) = reload_profiles(daemon_client, content.sender(), state_sender).await {
                            show_error(root, &err);
                        }
                    }
                    AppMsg::ReloadData { full } => {
                        if let Some(gpu_id) = &self.selected_gpu_id {
                            if full {
                                if let Err(err) = update_gpu_data_full(gpu_id.clone(), daemon_client, content.sender(), stats_task_handle).await {
                                    show_error(root, &err);
                                }
                            } else {
                                if let Err(err) = update_gpu_data(gpu_id.clone(), daemon_client, content.sender(), stats_task_handle).await {
                                    show_error(root, &err);
                                }
                            }
                        }
                    }
                    AppMsg::SelectProfile { profile, auto_switch } => {
                        if let Err(err) = daemon_client.set_profile(profile, auto_switch).await {
                            show_error(root, &err);
                        }
                        sender.input(AppMsg::ReloadProfiles { state_sender: None });
                    }
                    AppMsg::CreateProfile(name, base) => {
                        match daemon_client.create_profile(name.clone(), base).await {
                            Ok(_) => {
                                let _ = daemon_client.set_profile(Some(name), true).await;
                                sender.input(AppMsg::ReloadProfiles { state_sender: None });
                            }
                            Err(err) => show_error(root, &err.into()),
                        }
                    }
                    AppMsg::RenameProfile(old_name, new_name) => {
                        if old_name != new_name {
                            let res = async {
                                let original_profile = daemon_client
                                    .get_profile(Some(old_name.clone()))
                                    .await
                                    .context("Could not get profile by old name")?
                                    .context("Original profile not found")?;
                                daemon_client
                                    .create_profile(new_name, ProfileBase::Provided(original_profile))
                                    .await
                                    .context("Could not create new profile")?;
                                daemon_client
                                    .delete_profile(old_name)
                                    .await
                                    .context("Could not delete old name")?;
                                anyhow::Ok(())
                            }.await;

                            if let Err(err) = res {
                                show_error(root, &err);
                            } else {
                                sender.input(AppMsg::ReloadProfiles { state_sender: None });
                            }
                        }
                    }
                    AppMsg::DeleteProfile(profile) => {
                        if let Err(err) = daemon_client.delete_profile(profile).await {
                            show_error(root, &err);
                        }
                        sender.input(AppMsg::ReloadProfiles { state_sender: None });
                    }
                    AppMsg::MoveProfile(name, new_position) => {
                        if let Err(err) = daemon_client.move_profile(name, new_position).await {
                            show_error(root, &err);
                        }
                        sender.input(AppMsg::ReloadProfiles { state_sender: None });
                    }
                    AppMsg::ApplyConfig(gpu_id, config) => {
                        if let Err(err) = apply_settings(gpu_id, *config, daemon_client, content.sender(), root).await {
                            show_error(root, &err);
                        }
                    }
                    AppMsg::RevertChanges => {
                        sender.input(AppMsg::ReloadData { full: false });
                    }
                    AppMsg::ResetClocks => {
                        if let Some(gpu_id) = &self.selected_gpu_id {
                            let res = async {
                                daemon_client
                                    .set_clocks_value(gpu_id, SetClocksCommand::reset())
                                    .await?;
                                daemon_client
                                    .confirm_pending_config(ConfirmCommand::Confirm)
                                    .await?;
                                anyhow::Ok(())
                            }.await;

                            if let Err(err) = res {
                                show_error(root, &err);
                            } else {
                                sender.input(AppMsg::ReloadData { full: false });
                            }
                        }
                    }
                    AppMsg::ResetPmfw => {
                        if let Some(gpu_id) = &self.selected_gpu_id {
                            let res = async {
                                daemon_client.reset_pmfw(gpu_id).await?;
                                daemon_client
                                    .confirm_pending_config(ConfirmCommand::Confirm)
                                    .await?;
                                anyhow::Ok(())
                            }.await;

                            if let Err(err) = res {
                                show_error(root, &err);
                            } else {
                                sender.input(AppMsg::ReloadData { full: false });
                            }
                        }
                    }
                    AppMsg::ImportProfilePath(path) => {
                        let res = async {
                            let file_name = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("Imported profile");

                            let contents = std::fs::read_to_string(&path).context("Could not read selected file")?;
                            let profile = serde_json::from_str::<lact_schema::config::Profile>(&contents)
                                .context("Could not parse profile")?;
                            let profile_name = file_name
                                .trim_start_matches("LACT-profile-")
                                .trim_end_matches(".json");

                            daemon_client
                                .create_profile(profile_name.to_owned(), ProfileBase::Provided(profile))
                                .await
                                .context("Could not import profile")?;
                            anyhow::Ok(())
                        }.await;

                        if let Err(err) = res {
                            show_error(root, &err);
                        } else {
                            sender.input(AppMsg::ReloadProfiles { state_sender: None });
                        }
                    }
                    AppMsg::DumpVBiosPath(path) => {
                        if let Some(gpu_id) = &self.selected_gpu_id {
                            let res = async {
                                let vbios_data = daemon_client.dump_vbios(gpu_id).await?;
                                std::fs::write(path, vbios_data).context("Could not save vbios file")?;
                                anyhow::Ok(())
                            }.await;

                            if let Err(err) = res {
                                show_error(root, &err);
                            }
                        }
                    }
                    AppMsg::DebugSnapshot => {
                        match daemon_client.generate_debug_snapshot().await {
                            Ok(path) => {
                                let diag = MessageDialog::builder()
                                    .title("Snapshot generated")
                                    .message_type(MessageType::Info)
                                    .use_markup(true)
                                    .text(format!("Debug snapshot saved at <b>{path}</b>"))
                                    .buttons(ButtonsType::Ok)
                                    .transient_for(root)
                                    .build();
                                diag.run_async(|diag, _| diag.hide());
                            }
                            Err(err) => show_error(root, &err.into()),
                        }
                    }
                    AppMsg::EnableOverdrive => {
                        if let Err(err) = daemon_client.enable_overdrive().await {
                            show_error(root, &err);
                        }
                        sender.input(AppMsg::ReloadData { full: true });
                    }
                    AppMsg::DisableOverdrive => {
                        if let Err(err) = daemon_client.disable_overdrive().await {
                            show_error(root, &err);
                        }
                        sender.input(AppMsg::ReloadData { full: true });
                    }
                    AppMsg::ResetConfig => {
                        if let Err(err) = daemon_client.reset_config().await {
                            show_error(root, &err);
                        }
                        sender.input(AppMsg::ReloadData { full: true });
                    }
                    AppMsg::FetchProcessList => {
                        if let Some(gpu_id) = &self.selected_gpu_id {
                            let res = async {
                                let process_list = daemon_client.get_process_list(gpu_id).await?;
                                anyhow::Ok(process_list)
                            }.await;

                            match res {
                                Ok(process_list) => {
                                    content.emit(AppMsg::ProcessList(process_list));
                                }
                                Err(err) => warn!("could not fetch process list: {err:#}"),
                            }
                        }
                    }
                    AppMsg::EvaluateProfile(rule, profile_sender) => {
                        match daemon_client.evaluate_profile_rule(rule).await {
                            Ok(matches) => {
                                let _ = profile_sender.send(crate::app::header::profile_rule_window::ProfileRuleWindowMsg::EvaluationResult(matches));
                            }
                            Err(err) => warn!("{err:#}"),
                        }
                    }
                    AppMsg::SetProfileRule { name, rule, hooks } => {
                        if let Err(err) = daemon_client.set_profile_rule(name, rule, hooks).await {
                            show_error(root, &err);
                        }
                        sender.input(AppMsg::ReloadProfiles { state_sender: None });
                    }
                    other => content.emit(other),
                }
            }
            AppState::Crashed => {}
        }
        self.update_view(widgets, sender);
    }
}

async fn reload_profiles(
    daemon_client: &DaemonClient,
    content_sender: &relm4::Sender<AppMsg>,
    state_sender: Option<relm4::Sender<crate::app::header::profile_rule_window::profile_row::ProfileRuleRowMsg>>,
) -> anyhow::Result<()> {
    let mut profiles = daemon_client
        .list_profiles(state_sender.is_some())
        .await?;

    if let Some(sender) = state_sender
        && let Some(state) = profiles.watcher_state.take()
    {
        let _ = sender.send(crate::app::header::profile_rule_window::profile_row::ProfileRuleRowMsg::WatcherState(state));
    }

    content_sender.send(AppMsg::Profiles(Arc::new(profiles))).unwrap();

    Ok(())
}

async fn update_gpu_data_full(
    gpu_id: String,
    daemon_client: &DaemonClient,
    content_sender: &relm4::Sender<AppMsg>,
    stats_task_handle: &mut Option<glib::JoinHandle<()>>,
) -> anyhow::Result<()> {
    let info = daemon_client.get_device_info(&gpu_id).await?;
    let stats = update_gpu_data(gpu_id.clone(), daemon_client, content_sender, stats_task_handle).await?;

    content_sender.send(AppMsg::GpuDataUpdate {
        info: Some(Arc::new(info)),
        stats: Some(stats),
        clocks_table: daemon_client.get_device_clocks_info(&gpu_id).await.ok().and_then(|i| i.table),
        profile_modes: daemon_client.get_device_power_profile_modes(&gpu_id).await.ok().map(Arc::new),
        power_states: daemon_client.get_power_states(&gpu_id).await.ok(),
        config: daemon_client.get_gpu_config(&gpu_id).await.ok().flatten(),
    }).unwrap();

    Ok(())
}

async fn update_gpu_data(
    gpu_id: String,
    daemon_client: &DaemonClient,
    content_sender: &relm4::Sender<AppMsg>,
    stats_task_handle: &mut Option<glib::JoinHandle<()>>,
) -> anyhow::Result<Arc<DeviceStats>> {
    if let Some(handle) = stats_task_handle.take() {
        handle.abort();
    }

    let stats = daemon_client.get_device_stats(&gpu_id).await?;
    let stats = Arc::new(stats);
    
    content_sender.send(AppMsg::Stats(stats.clone())).unwrap();

    *stats_task_handle = Some(start_stats_update_loop(gpu_id, daemon_client.clone(), content_sender.clone()));

    Ok(stats)
}

async fn apply_settings(
    gpu_id: String,
    ui_config: GpuConfig,
    daemon_client: &DaemonClient,
    content_sender: &relm4::Sender<AppMsg>,
    root: &ApplicationWindow,
) -> anyhow::Result<()> {
    let mut gpu_config = daemon_client
        .get_gpu_config(&gpu_id)
        .await?
        .unwrap_or_else(GpuConfig::default);

    if let Some(cap) = ui_config.power_cap {
        gpu_config.power_cap = Some(cap);
    }
    if let Some(level) = ui_config.performance_level {
        gpu_config.performance_level = Some(level);
    }
    if let Some(mode) = ui_config.power_profile_mode_index {
        gpu_config.power_profile_mode_index = Some(mode);
    }
    if !ui_config.custom_power_profile_mode_hueristics.is_empty() {
        gpu_config.custom_power_profile_mode_hueristics = ui_config.custom_power_profile_mode_hueristics;
    }
    gpu_config.fan_control_enabled = ui_config.fan_control_enabled;
    gpu_config.fan_control_settings = ui_config.fan_control_settings;
    gpu_config.pmfw_options = ui_config.pmfw_options;
    gpu_config.power_states = ui_config.power_states;
    
    gpu_config.clocks_configuration.min_core_clock = ui_config.clocks_configuration.min_core_clock;
    gpu_config.clocks_configuration.max_core_clock = ui_config.clocks_configuration.max_core_clock;
    gpu_config.clocks_configuration.min_memory_clock = ui_config.clocks_configuration.min_memory_clock;
    gpu_config.clocks_configuration.max_memory_clock = ui_config.clocks_configuration.max_memory_clock;
    gpu_config.clocks_configuration.min_voltage = ui_config.clocks_configuration.min_voltage;
    gpu_config.clocks_configuration.max_voltage = ui_config.clocks_configuration.max_voltage;
    gpu_config.clocks_configuration.voltage_offset = ui_config.clocks_configuration.voltage_offset;

    let delay = daemon_client.set_gpu_config(&gpu_id, gpu_config).await?;
    ask_settings_confirmation(delay, daemon_client, content_sender, root).await;
    
    content_sender.send(AppMsg::ReloadData { full: false }).unwrap();

    Ok(())
}

async fn ask_settings_confirmation(
    mut delay: u64,
    daemon_client: &DaemonClient,
    content_sender: &relm4::Sender<AppMsg>,
    root: &ApplicationWindow,
) {
    let text = confirmation_text(delay);
    let dialog = MessageDialog::builder()
        .title("Confirm settings")
        .text(text)
        .message_type(MessageType::Question)
        .buttons(ButtonsType::YesNo)
        .transient_for(root)
        .build();
    let confirmed = Rc::new(std::sync::atomic::AtomicBool::new(false));

    glib::source::timeout_add_local(
        Duration::from_secs(1),
        clone!(
            #[strong]
            dialog,
            #[strong]
            content_sender,
            #[strong]
            confirmed,
            move || {
                if confirmed.load(std::sync::atomic::Ordering::SeqCst) {
                    return ControlFlow::Break;
                }
                delay -= 1;

                let text = confirmation_text(delay);
                dialog.set_text(Some(&text));

                if delay == 0 {
                    dialog.hide();
                    let _ = content_sender.send(AppMsg::ReloadData { full: false });
                    ControlFlow::Break
                } else {
                    ControlFlow::Continue
                }
            }
        ),
    );

    let daemon_client = daemon_client.clone();
    let content_sender = content_sender.clone();
    dialog.run_async(move |diag, response| {
        confirmed.store(true, std::sync::atomic::Ordering::SeqCst);
        let command = match response {
            ResponseType::Yes => ConfirmCommand::Confirm,
            _ => ConfirmCommand::Revert,
        };
        diag.close();

        let daemon_client = daemon_client.clone();
        let content_sender = content_sender.clone();
        relm4::spawn_local(async move {
            let _ = daemon_client.confirm_pending_config(command).await;
            let _ = content_sender.send(AppMsg::ReloadData { full: false });
        });
    });
}

pub(super) fn show_error(parent: &ApplicationWindow, err: &anyhow::Error) {
    use gtk::{prelude::DialogExtManual, ButtonsType, MessageDialog, MessageType};
    use std::sync::atomic::{AtomicU32, Ordering};

    static ERROR_WINDOW_COUNT: AtomicU32 = AtomicU32::new(0);

    let text = format!("{err:?}")
        .lines()
        .map(str::trim)
        .collect::<Vec<&str>>()
        .join("\n");
    tracing::warn!("{text}");

    let errors_count = ERROR_WINDOW_COUNT.load(Ordering::SeqCst);
    if errors_count > 2 {
        tracing::warn!("Not showing error window, too many already open");
        return;
    }

    ERROR_WINDOW_COUNT.fetch_add(1, Ordering::SeqCst);

    let diag = MessageDialog::builder()
        .title("Error")
        .message_type(MessageType::Error)
        .text(text)
        .buttons(ButtonsType::Close)
        .transient_for(parent)
        .build();
    diag.run_async(|diag: &MessageDialog, _| {
        diag.close();
        ERROR_WINDOW_COUNT.fetch_sub(1, Ordering::SeqCst);
    })
}

fn show_embedded_info(parent: &ApplicationWindow, err: anyhow::Error) {
    use gtk::{prelude::{ButtonExt, DialogExtManual}, ButtonsType, DialogFlags, MessageDialog, MessageType};

    let error_text = format!("Error info: {err:#}\n\n");

    let text = format!(
        "Could not connect to daemon, running in embedded mode. \n\
         Please make sure that lactd service is running. \n\
         Using embedded mode, you will not be able to change any settings. \n\n\
         {error_text}\
         To enable the daemon, run the following command, then restart LACT:"
    );

    let text_label = gtk::Label::new(Some(&text));
    let enable_label = gtk::Entry::builder()
        .text("sudo systemctl enable --now lactd")
        .editable(false)
        .build();

    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(10)
        .margin_end(10)
        .build();

    let close_button = gtk::Button::builder().label("Close").build();

    vbox.append(&text_label);
    vbox.append(&enable_label);
    vbox.append(&close_button);

    let diag = gtk::MessageDialog::new(
        Some(parent),
        DialogFlags::MODAL,
        MessageType::Question,
        ButtonsType::Ok,
        "",
    );
    diag.set_title(Some("Daemon info"));
    diag.set_child(Some(&vbox));

    close_button.connect_clicked(glib::clone!(
        #[strong]
        diag,
        move |_| diag.hide()
    ));

    diag.run_async(|diag: &MessageDialog, _| {
        diag.hide();
    })
}

async fn create_connection() -> anyhow::Result<(DaemonClient, Option<Arc<anyhow::Error>>)> {
    match DaemonClient::connect().await {
        Ok(connection) => {
            debug!("Established daemon connection");
            Ok((connection, None))
        }
        Err(err) => {
            info!("could not connect to socket: {err:#}");
            info!("using a local daemon");

            let (server_stream, client_stream) = UnixStream::pair()?;
            client_stream.set_nonblocking(true)?;
            server_stream.set_nonblocking(true)?;

            std::thread::spawn(move || {
                if let Err(err) = lact_daemon::run_embedded(server_stream) {
                    error!("Builtin daemon error: {err}");
                }
            });

            let client = DaemonClient::from_stream(client_stream, true)?;
            Ok((client, Some(Arc::new(err))))
        }
    }
}

async fn fetch_initial_gpu_data(
    client: &DaemonClient,
    gpu_id: &str,
) -> anyhow::Result<InitialGpuData> {
    let info = client
        .get_device_info(gpu_id)
        .await
        .context("Could not fetch info")?;

    if info.driver == "nvidia" {
        return Err(anyhow!("Nvidia driver detected, but the management library could not be loaded. Check lact service status for more information."));
    } else if let Some(nvidia_version) = info.driver.strip_prefix("nvidia ")
        && let Some(major_version) = nvidia_version
            .split('.')
            .next()
            .and_then(|version| version.parse::<u32>().ok())
        && major_version < 560
    {
        return Err(anyhow!("Old Nvidia driver version detected ({major_version}), some features might be missing. Driver version 560 or newer is recommended."));
    }

    let stats = client
        .get_device_stats(gpu_id)
        .await
        .context("Could not fetch stats")?;

    let clocks_table = client
        .get_device_clocks_info(gpu_id)
        .await
        .ok()
        .and_then(|info| info.table);

    let profile_modes = client
        .get_device_power_profile_modes(gpu_id)
        .await
        .ok()
        .map(Arc::new);

    let power_states = client
        .get_power_states(gpu_id)
        .await
        .ok();

    let config = client
        .get_gpu_config(gpu_id)
        .await
        .ok()
        .flatten();

    Ok(InitialGpuData {
        info,
        stats,
        clocks_table,
        profile_modes,
        power_states,
        config,
    })
}

fn start_stats_update_loop(
    gpu_id: String,
    daemon_client: DaemonClient,
    content_sender: relm4::Sender<AppMsg>,
) -> glib::JoinHandle<()> {
    debug!("spawning new stats update task");
    relm4::spawn_local(async move {
        loop {
            let duration = Duration::from_millis(crate::CONFIG.read().stats_poll_interval_ms as u64);
            relm4::tokio::time::sleep(duration).await;

            match daemon_client.get_device_stats(&gpu_id).await {
                Ok(stats) => {
                    let _ = content_sender.send(AppMsg::Stats(Arc::new(stats)));
                }
                Err(err) => {
                    error!("could not fetch stats: {err:#}");
                }
            }

            match daemon_client.list_profiles(false).await {
                Ok(profiles) => {
                    let _ = content_sender.send(AppMsg::Profiles(Arc::new(profiles)));
                }
                Err(err) => {
                    error!("could not fetch profile info: {err:#}");
                }
            }
        }
    })
}

fn confirmation_text(seconds_left: u64) -> String {
    format!("Do you want to keep the new settings? (Reverting in {seconds_left} seconds)")
}
