use std::{
    ffi::{CStr, CString, c_char, c_void},
    ptr,
};

use anyhow::{Context, anyhow, bail};
use lact_schema::{NvidiaPowerMizerInfo, NvidiaPowerMizerMode};
use libloading::Library;

const LIBRARY_NAME: &str = "libnvidia-ml.so.1";
const NVML_SUCCESS: NvmlReturn = 0;

type NvmlReturn = i32;
type NvmlDevice = *mut c_void;

type NvmlInit = unsafe extern "C" fn() -> NvmlReturn;
type NvmlErrorString = unsafe extern "C" fn(NvmlReturn) -> *const c_char;
type NvmlDeviceGetHandleByPciBusId =
    unsafe extern "C" fn(*const c_char, *mut NvmlDevice) -> NvmlReturn;
type NvmlDeviceGetPowerMizerMode =
    unsafe extern "C" fn(NvmlDevice, *mut NvmlDevicePowerMizerModesV1) -> NvmlReturn;
type NvmlDeviceSetPowerMizerMode =
    unsafe extern "C" fn(NvmlDevice, *mut NvmlDevicePowerMizerControlV1) -> NvmlReturn;

pub struct PowerMizerNvml {
    _lib: Library,
    error_string: NvmlErrorString,
    device_get_handle_by_pci_bus_id: NvmlDeviceGetHandleByPciBusId,
    device_get_power_mizer_mode: NvmlDeviceGetPowerMizerMode,
    device_set_power_mizer_mode: NvmlDeviceSetPowerMizerMode,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
struct NvmlDevicePowerMizerModesV1 {
    current: u32,
    default: u32,
    supported_modes_mask: u32,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
struct NvmlDevicePowerMizerControlV1 {
    reserved: u32,
    mode: u32,
    supported_modes_mask: u32,
}

impl PowerMizerNvml {
    pub fn new() -> anyhow::Result<Self> {
        let lib = unsafe { Library::new(LIBRARY_NAME).context("Could not load NVML library") }?;

        let init: NvmlInit = unsafe { load(&lib, b"nvmlInit_v2\0")? };
        let error_string = unsafe { load(&lib, b"nvmlErrorString\0")? };
        let device_get_handle_by_pci_bus_id =
            unsafe { load(&lib, b"nvmlDeviceGetHandleByPciBusId_v2\0")? };
        let device_get_power_mizer_mode =
            unsafe { load(&lib, b"nvmlDeviceGetPowerMizerMode_v1\0")? };
        let device_set_power_mizer_mode =
            unsafe { load(&lib, b"nvmlDeviceSetPowerMizerMode_v1\0")? };

        let handle = Self {
            _lib: lib,
            error_string,
            device_get_handle_by_pci_bus_id,
            device_get_power_mizer_mode,
            device_set_power_mizer_mode,
        };

        unsafe {
            handle.handle_status(init())?;
        }

        Ok(handle)
    }

    pub fn get_info(&self, pci_bus_id: &str) -> anyhow::Result<NvidiaPowerMizerInfo> {
        let device = self.device_by_pci_bus_id(pci_bus_id)?;
        let mut modes = NvmlDevicePowerMizerModesV1::default();

        unsafe {
            self.handle_status((self.device_get_power_mizer_mode)(device, &mut modes))?;
        }

        Ok(NvidiaPowerMizerInfo {
            current: NvidiaPowerMizerMode::from_raw(modes.current),
            default: NvidiaPowerMizerMode::from_raw(modes.default),
            supported: supported_modes_from_mask(modes.supported_modes_mask),
        })
    }

    pub fn set_mode(&self, pci_bus_id: &str, mode: NvidiaPowerMizerMode) -> anyhow::Result<()> {
        let device = self.device_by_pci_bus_id(pci_bus_id)?;
        let mut modes = NvmlDevicePowerMizerModesV1::default();

        unsafe {
            self.handle_status((self.device_get_power_mizer_mode)(device, &mut modes))?;
        }

        let requested_mode = mode.as_raw();
        if modes.supported_modes_mask & (1 << requested_mode) == 0 {
            bail!("PowerMizer mode {mode:?} is not supported by this GPU");
        }

        if modes.current == requested_mode {
            return Ok(());
        }

        let mut control = NvmlDevicePowerMizerControlV1 {
            reserved: 0,
            mode: requested_mode,
            supported_modes_mask: modes.supported_modes_mask,
        };

        unsafe {
            self.handle_status((self.device_set_power_mizer_mode)(device, &mut control))?;
            self.handle_status((self.device_get_power_mizer_mode)(device, &mut modes))?;
        }

        if modes.current != requested_mode {
            bail!("PowerMizer mode was not applied by the NVIDIA driver");
        }

        Ok(())
    }

    fn device_by_pci_bus_id(&self, pci_bus_id: &str) -> anyhow::Result<NvmlDevice> {
        let pci_bus_id = CString::new(pci_bus_id).context("PCI bus id contains a nul byte")?;
        let mut device = ptr::null_mut();

        unsafe {
            self.handle_status((self.device_get_handle_by_pci_bus_id)(
                pci_bus_id.as_ptr(),
                &mut device,
            ))?;
        }

        if device.is_null() {
            bail!("NVML returned a null device handle");
        }

        Ok(device)
    }

    unsafe fn handle_status(&self, status: NvmlReturn) -> anyhow::Result<()> {
        if status == NVML_SUCCESS {
            Ok(())
        } else {
            let error = unsafe { (self.error_string)(status) };
            let error = if error.is_null() {
                "unknown NVML error".into()
            } else {
                unsafe { CStr::from_ptr(error) }.to_string_lossy()
            };

            Err(anyhow!("Got error {status} from NVML: {error}"))
        }
    }
}

fn supported_modes_from_mask(mask: u32) -> Vec<NvidiaPowerMizerMode> {
    (0..=3)
        .filter(|mode| mask & (1 << mode) != 0)
        .filter_map(NvidiaPowerMizerMode::from_raw)
        .collect()
}

unsafe fn load<T: Copy>(lib: &Library, symbol: &[u8]) -> anyhow::Result<T> {
    Ok(*unsafe { lib.get::<T>(symbol) }
        .with_context(|| format!("Could not load symbol {}", String::from_utf8_lossy(symbol)))?)
}
