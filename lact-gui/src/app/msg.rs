use super::{
    app_content::InitialGpuData,
    confirmation_dialog::ConfirmationOptions,
    header::profile_rule_window::{profile_row::ProfileRuleRowMsg, ProfileRuleWindowMsg},
};
use lact_client::ConnectionStatusMsg;
use lact_schema::{
    config::ProfileHooks, request::ProfileBase, DeviceListEntry, DeviceStats, ProfileRule,
    SystemInfo,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum AppMsg {
    ReloadData {
        full: bool,
    },
    ApplyChanges,
    RevertChanges,
    SettingsChanged,
    ResetClocks,
    ResetPmfw,
    ShowGraphsWindow,
    ShowProcessMonitor,
    DumpVBios,
    DebugSnapshot,
    ShowOverdriveDialog,
    EnableOverdrive,
    DisableOverdrive,
    ResetConfig,
    FetchProcessList,
    ReloadProfiles {
        state_sender: Option<relm4::Sender<ProfileRuleRowMsg>>,
    },
    SelectProfile {
        profile: Option<String>,
        auto_switch: bool,
    },
    CreateProfile(String, ProfileBase),
    DeleteProfile(String),
    MoveProfile(String, usize),
    RenameProfile(String, String),
    EvaluateProfile(ProfileRule, relm4::Sender<ProfileRuleWindowMsg>),
    SetProfileRule {
        name: String,
        rule: Option<ProfileRule>,
        hooks: ProfileHooks,
    },
    ImportProfile,
    ExportProfile(Option<String>),
    ConnectionStatus(ConnectionStatusMsg),
    DataLoaded {
        system_info: SystemInfo,
        devices: Vec<DeviceListEntry>,
        initial_gpu: Option<(String, InitialGpuData)>,
        profiles: Arc<lact_schema::ProfilesInfo>,
    },
    Error(Arc<anyhow::Error>),
    ImportProfilePath(std::path::PathBuf),
    DumpVBiosPath(std::path::PathBuf),
    ApplyConfig(String, Box<lact_schema::config::GpuConfig>),
    Crash(String),
    LoadingStatus(String),
    AskConfirmation(ConfirmationOptions, Box<AppMsg>),
}

#[derive(Debug, Clone)]
pub enum AppContentMsg {
    GpuDataUpdate {
        info: Option<Arc<lact_schema::DeviceInfo>>,
        stats: Option<Arc<lact_schema::DeviceStats>>,
        clocks_table: Option<lact_schema::ClocksTable>,
        profile_modes:
            Option<Arc<amdgpu_sysfs::gpu_handle::power_profile_mode::PowerProfileModesTable>>,
        power_states: Option<lact_schema::PowerStates>,
        config: Option<lact_schema::config::GpuConfig>,
    },
    Stats(Arc<DeviceStats>),
    Profiles(Arc<lact_schema::ProfilesInfo>),
    ProcessList(lact_schema::ProcessList),
    LoadingStatus(String),
    UiCommand(UiCommand),
    Action(AppMsg),
    ExportProfileData(Option<String>, Box<lact_schema::config::Profile>),
}

#[derive(Debug, Clone)]
pub enum UiCommand {
    ShowGraphs,
    ShowProcessMonitor,
    ShowOverdriveDialog,
    OverdriveLoading,
    OverdriveLoaded,
}

impl AppMsg {
    pub fn ask_confirmation(
        inner: AppMsg,
        title: String,
        message: impl Into<String>,
        buttons_type: gtk::ButtonsType,
    ) -> AppMsg {
        Self::AskConfirmation(
            ConfirmationOptions {
                title,
                message: message.into(),
                buttons_type,
            },
            Box::new(inner),
        )
    }
}
