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

use crate::{APP_ID, GUI_VERSION, I18N, app::app_content::{AppContent, AppContentInit, InitialGpuData}};
use anyhow::{anyhow, Context};
use gtk::{glib, prelude::{BoxExt, GtkWindowExt, OrientableExt, WidgetExt}, ApplicationWindow};
use i18n_embed_fl::fl;
use lact_client::{ConnectionStatusMsg, DaemonClient};
use lact_schema::{args::GuiArgs, GIT_COMMIT};
use msg::AppMsg;
use relm4::{AsyncComponentSender, MessageBroker, RelmWidgetExt, component::AsyncComponentController, prelude::{AsyncComponent, AsyncComponentParts, AsyncController}};
use std::{cell::RefCell, os::unix::net::UnixStream, rc::Rc, sync::Arc};
use tracing::{debug, error, info};

pub(crate) static APP_BROKER: MessageBroker<AppMsg> = MessageBroker::new();

pub struct AppModel {
    state: AppState,
    daemon_holder: Rc<RefCell<Option<(DaemonClient, Option<Arc<anyhow::Error>>)>>>,
    loading_status: String,
}

enum AppState {
    Loading,
    Running {
        content: AsyncController<AppContent>,
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
                                daemon_client,
                                system_info,
                                devices,
                                conn_err,
                                root: root.clone(),
                                initial_gpu,
                                profiles,
                            })
                            .detach();

                        let content_widgets = content.widget();
                        root.set_child(Some(content_widgets));

                        self.state = AppState::Running { content };
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
            AppState::Running { content } => {
                match msg {
                    AppMsg::ConnectionStatus(status) => match status {
                        ConnectionStatusMsg::Disconnected => widgets.reconnecting_dialog.present(),
                        ConnectionStatusMsg::Reconnected => widgets.reconnecting_dialog.hide(),
                    },
                    other => content.emit(other),
                }
            }
            AppState::Crashed => {}
        }
        self.update_view(widgets, sender);
    }
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
