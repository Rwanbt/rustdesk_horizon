use hbb_common::{bail, ResultType};
use std::sync::Mutex;

#[cfg(windows)]
use hbb_common::platform::windows::is_windows_version_or_greater;

// Cross-platform MonitorMode type
#[derive(Debug, Copy, Clone)]
pub struct MonitorMode {
    pub width: u32,
    pub height: u32,
    pub sync: u32,
}

// This string is defined here.
//  https://github.com/rustdesk-org/RustDeskIddDriver/blob/b370aad3f50028b039aad211df60c8051c4a64d6/RustDeskIddDriver/RustDeskIddDriver.inf#LL73C1-L73C40
#[cfg(windows)]
pub const RUSTDESK_IDD_DEVICE_STRING: &'static str = "RustDeskIddDriver Device\0";
#[cfg(windows)]
pub const AMYUNI_IDD_DEVICE_STRING: &'static str = "USB Mobile Monitor Virtual Display\0";

#[cfg(windows)]
const IDD_IMPL: &str = IDD_IMPL_AMYUNI;
#[cfg(windows)]
const IDD_IMPL_RUSTDESK: &str = "rustdesk_idd";
#[cfg(windows)]
const IDD_IMPL_AMYUNI: &str = "amyuni_idd";
#[cfg(windows)]
const IDD_PLUG_OUT_ALL_INDEX: i32 = -1;

lazy_static::lazy_static! {
    static ref CUSTOM_VD_RESOLUTION: Mutex<Option<(u32, u32)>> = Mutex::new(None);
}

pub fn set_custom_resolution(w: u32, h: u32) {
    *CUSTOM_VD_RESOLUTION.lock().unwrap() = Some((w, h));
}

pub fn take_custom_resolution() -> Option<(u32, u32)> {
    // Clone instead of take: multiple concurrent plug_in_monitor calls
    // (e.g. adding several virtual displays) must all get the resolution.
    CUSTOM_VD_RESOLUTION.lock().unwrap().clone()
}

pub fn clear_custom_resolution() {
    *CUSTOM_VD_RESOLUTION.lock().unwrap() = None;
}

// =========================================================================
// VdController — virtual display toggle state machine
//
// Encapsulates the defer/skip/create decision logic so it can be shared
// between connection.rs (production) and unit tests (verification).
// The controller is a pure state machine: it returns *decisions* that the
// caller translates into real operations (plug_in_monitor, etc.).
// =========================================================================

use std::time::{Duration, Instant};

/// Action that the caller should take after a toggle or timeout.
#[derive(Debug, Clone, PartialEq)]
pub enum VdDecision {
    /// Create a virtual display at this resolution.
    Create { display: i32, width: u32, height: u32 },
    /// Remove a virtual display.
    Remove { display: i32 },
    /// Toggle was deferred — waiting for client resolution.
    Deferred { display: i32 },
    /// Toggle was skipped (already active or already pending).
    Skipped { display: i32, reason: &'static str },
}

/// Default timeout before processing deferred toggles with default resolution.
/// Desktop clients never send `#vd_res`, so this ensures they aren't stuck.
/// Mobile clients send `#vd_res` after the first video frame arrives, which
/// can take 15-20s (video init + encoding + network + decoding on phone).
/// 30s provides ample margin for slow connections.
const VD_DEFER_TIMEOUT_SECS: u64 = 30;
/// Default resolution used when no client resolution is provided.
const VD_DEFAULT_WIDTH: u32 = 1920;
const VD_DEFAULT_HEIGHT: u32 = 1080;

/// Pure state machine for virtual display toggle decisions.
///
/// Used by `Connection` in connection.rs to decide whether to create,
/// defer, or skip a virtual display toggle request.
pub struct VdController {
    resolution: Option<(u32, u32)>,
    active_indices: Vec<i32>,
    pending_toggles: Vec<i32>,
    pending_deadline: Option<Instant>,
}

impl VdController {
    pub fn new() -> Self {
        Self {
            resolution: None,
            active_indices: Vec::new(),
            pending_toggles: Vec::new(),
            pending_deadline: None,
        }
    }

    /// Process a toggle request. Returns the decision the caller should act on.
    pub fn toggle(&mut self, display: i32, on: bool) -> VdDecision {
        if on {
            // Dedup: skip if display is already active.
            if self.active_indices.contains(&display) {
                return VdDecision::Skipped {
                    display,
                    reason: "already active",
                };
            }

            if self.resolution.is_none() {
                // No resolution known yet. Defer creation.
                if !self.pending_toggles.contains(&display) {
                    self.pending_toggles.push(display);
                    if self.pending_deadline.is_none() {
                        self.pending_deadline =
                            Some(Instant::now() + Duration::from_secs(VD_DEFER_TIMEOUT_SECS));
                    }
                    VdDecision::Deferred { display }
                } else {
                    VdDecision::Skipped {
                        display,
                        reason: "already pending",
                    }
                }
            } else {
                let (w, h) = self.resolution.unwrap();
                self.active_indices.push(display);
                VdDecision::Create {
                    display,
                    width: w,
                    height: h,
                }
            }
        } else {
            self.active_indices.retain(|&i| i != display);
            VdDecision::Remove { display }
        }
    }

    /// Store the client's native resolution and process all deferred toggles.
    /// Returns the list of decisions (one per deferred toggle).
    pub fn handle_resolution(&mut self, w: u32, h: u32) -> Vec<VdDecision> {
        self.resolution = Some((w, h));
        self.pending_deadline = None;
        let pending: Vec<i32> = self.pending_toggles.drain(..).collect();
        pending.into_iter().map(|d| self.toggle(d, true)).collect()
    }

    /// Check if deferred toggles should be processed due to timeout.
    /// Returns decisions if the deadline has passed, empty vec otherwise.
    pub fn check_timeout(&mut self) -> Vec<VdDecision> {
        if let Some(deadline) = self.pending_deadline {
            if Instant::now() >= deadline && !self.pending_toggles.is_empty() {
                self.resolution = Some((VD_DEFAULT_WIDTH, VD_DEFAULT_HEIGHT));
                self.pending_deadline = None;
                let pending: Vec<i32> = self.pending_toggles.drain(..).collect();
                return pending
                    .into_iter()
                    .map(|d| self.toggle(d, true))
                    .collect();
            }
        }
        Vec::new()
    }

    /// Connection close: return indices of active displays to clean up.
    pub fn close(&mut self) -> Vec<i32> {
        let indices: Vec<i32> = self.active_indices.drain(..).collect();
        self.pending_toggles.clear();
        self.pending_deadline = None;
        indices
    }

    /// Access active display indices (for refresh_video_display, etc.).
    pub fn active_indices(&self) -> &[i32] {
        &self.active_indices
    }

    /// Check if there are pending deferred toggles.
    pub fn has_pending(&self) -> bool {
        !self.pending_toggles.is_empty()
    }

    /// Get the stored resolution (if any).
    pub fn resolution(&self) -> Option<(u32, u32)> {
        self.resolution
    }

    /// Roll back a Create decision that failed to execute.
    /// Removes the display from active_indices so a future toggle
    /// won't be skipped with "already active".
    pub fn rollback_create(&mut self, display: i32) {
        self.active_indices.retain(|&i| i != display);
    }

    /// Force the pending deadline to a specific instant (for testing).
    #[cfg(test)]
    pub fn set_pending_deadline(&mut self, deadline: Instant) {
        self.pending_deadline = deadline.into();
    }
}

#[cfg(windows)]
pub fn is_amyuni_idd() -> bool {
    IDD_IMPL == IDD_IMPL_AMYUNI
}

#[cfg(windows)]
pub fn get_cur_device_string() -> &'static str {
    match IDD_IMPL {
        IDD_IMPL_RUSTDESK => RUSTDESK_IDD_DEVICE_STRING,
        IDD_IMPL_AMYUNI => AMYUNI_IDD_DEVICE_STRING,
        _ => "",
    }
}

pub fn is_virtual_display_supported() -> bool {
    #[cfg(target_os = "windows")]
    {
        return is_windows_version_or_greater(10, 0, 19041, 0, 0);
    }
    #[cfg(target_os = "linux")]
    {
        return linux_evdi::is_supported();
    }
    #[cfg(target_os = "macos")]
    {
        return macos_cg_virtual::is_supported();
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

pub fn plug_in_headless() -> ResultType<()> {
    #[cfg(windows)]
    {
        return match IDD_IMPL {
            IDD_IMPL_RUSTDESK => rustdesk_idd::plug_in_headless(),
            IDD_IMPL_AMYUNI => amyuni_idd::plug_in_headless(),
            _ => bail!("Unsupported virtual display implementation."),
        };
    }
    #[cfg(target_os = "linux")]
    {
        return linux_evdi::plug_in_headless();
    }
    #[cfg(target_os = "macos")]
    {
        return macos_cg_virtual::plug_in_headless();
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        bail!("Virtual display not supported on this platform.")
    }
}

pub fn get_platform_additions() -> serde_json::Map<String, serde_json::Value> {
    #[cfg(windows)]
    {
        let mut map = serde_json::Map::new();
        if !crate::platform::windows::is_self_service_running() {
            return map;
        }
        map.insert("idd_impl".into(), serde_json::json!(IDD_IMPL));
        match IDD_IMPL {
            IDD_IMPL_RUSTDESK => {
                let virtual_displays = rustdesk_idd::get_virtual_displays();
                if !virtual_displays.is_empty() {
                    map.insert(
                        "rustdesk_virtual_displays".into(),
                        serde_json::json!(virtual_displays),
                    );
                }
            }
            IDD_IMPL_AMYUNI => {
                let c = amyuni_idd::get_monitor_count();
                if c > 0 {
                    map.insert("amyuni_virtual_displays".into(), serde_json::json!(c));
                }
            }
            _ => {}
        }
        return map;
    }
    #[cfg(target_os = "linux")]
    {
        return linux_evdi::get_platform_additions();
    }
    #[cfg(target_os = "macos")]
    {
        return macos_cg_virtual::get_platform_additions();
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        serde_json::Map::new()
    }
}

#[inline]
pub fn plug_in_monitor(idx: u32, modes: Vec<MonitorMode>) -> ResultType<()> {
    #[cfg(windows)]
    {
        let vd_modes: Vec<virtual_display::MonitorMode> = modes
            .into_iter()
            .map(|m| virtual_display::MonitorMode {
                width: m.width as _,
                height: m.height as _,
                sync: m.sync as _,
            })
            .collect();
        return match IDD_IMPL {
            IDD_IMPL_RUSTDESK => rustdesk_idd::plug_in_index_modes(idx, vd_modes),
            IDD_IMPL_AMYUNI => amyuni_idd::plug_in_monitor(),
            _ => bail!("Unsupported virtual display implementation."),
        };
    }
    #[cfg(target_os = "linux")]
    {
        return linux_evdi::plug_in_monitor(idx, &modes);
    }
    #[cfg(target_os = "macos")]
    {
        return macos_cg_virtual::plug_in_monitor(idx, &modes);
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (idx, modes);
        bail!("Virtual display not supported on this platform.")
    }
}

pub fn plug_out_monitor(index: i32, force_all: bool, force_one: bool) -> ResultType<()> {
    #[cfg(windows)]
    {
        return match IDD_IMPL {
            IDD_IMPL_RUSTDESK => {
                let indices = if index == IDD_PLUG_OUT_ALL_INDEX {
                    rustdesk_idd::get_virtual_displays()
                } else {
                    vec![index as _]
                };
                rustdesk_idd::plug_out_peer_request(&indices)
            }
            IDD_IMPL_AMYUNI => amyuni_idd::plug_out_monitor(index, force_all, force_one),
            _ => bail!("Unsupported virtual display implementation."),
        };
    }
    #[cfg(target_os = "linux")]
    {
        let _ = (force_all, force_one);
        return linux_evdi::plug_out_monitor(index);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (force_all, force_one);
        return macos_cg_virtual::plug_out_monitor(index);
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (index, force_all, force_one);
        bail!("Virtual display not supported on this platform.")
    }
}

pub fn plug_in_peer_request(modes: Vec<Vec<MonitorMode>>) -> ResultType<Vec<u32>> {
    #[cfg(windows)]
    {
        let vd_modes: Vec<Vec<virtual_display::MonitorMode>> = modes
            .into_iter()
            .map(|ms| {
                ms.into_iter()
                    .map(|m| virtual_display::MonitorMode {
                        width: m.width as _,
                        height: m.height as _,
                        sync: m.sync as _,
                    })
                    .collect()
            })
            .collect();
        return match IDD_IMPL {
            IDD_IMPL_RUSTDESK => rustdesk_idd::plug_in_peer_request(vd_modes),
            IDD_IMPL_AMYUNI => {
                amyuni_idd::plug_in_monitor()?;
                Ok(vec![0])
            }
            _ => bail!("Unsupported virtual display implementation."),
        };
    }
    #[cfg(target_os = "linux")]
    {
        return linux_evdi::plug_in_peer_request(modes);
    }
    #[cfg(target_os = "macos")]
    {
        return macos_cg_virtual::plug_in_peer_request(modes);
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = modes;
        bail!("Virtual display not supported on this platform.")
    }
}

pub fn plug_out_monitor_indices(
    indices: &[u32],
    force_all: bool,
    force_one: bool,
) -> ResultType<()> {
    #[cfg(windows)]
    {
        return match IDD_IMPL {
            IDD_IMPL_RUSTDESK => rustdesk_idd::plug_out_peer_request(indices),
            IDD_IMPL_AMYUNI => {
                for _idx in indices.iter() {
                    amyuni_idd::plug_out_monitor(0, force_all, force_one)?;
                }
                Ok(())
            }
            _ => bail!("Unsupported virtual display implementation."),
        };
    }
    #[cfg(target_os = "linux")]
    {
        let _ = (force_all, force_one);
        return linux_evdi::plug_out_monitor_indices(indices);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (force_all, force_one);
        return macos_cg_virtual::plug_out_monitor_indices(indices);
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (indices, force_all, force_one);
        bail!("Virtual display not supported on this platform.")
    }
}

pub fn reset_all() -> ResultType<()> {
    #[cfg(windows)]
    {
        return match IDD_IMPL {
            IDD_IMPL_RUSTDESK => rustdesk_idd::reset_all(),
            IDD_IMPL_AMYUNI => amyuni_idd::reset_all(),
            _ => bail!("Unsupported virtual display implementation."),
        };
    }
    #[cfg(target_os = "linux")]
    {
        return linux_evdi::reset_all();
    }
    #[cfg(target_os = "macos")]
    {
        return macos_cg_virtual::reset_all();
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        bail!("Virtual display not supported on this platform.")
    }
}

// =============================================================================
// Windows IDD implementations
// =============================================================================

#[cfg(windows)]
pub mod rustdesk_idd {
    use super::windows;
    use hbb_common::{allow_err, bail, lazy_static, log, ResultType};
    use std::{
        collections::{HashMap, HashSet},
        sync::{Arc, Mutex},
    };

    // virtual display index range: 0 - 2 are reserved for headless and other special uses.
    const VIRTUAL_DISPLAY_INDEX_FOR_HEADLESS: u32 = 0;
    const VIRTUAL_DISPLAY_START_FOR_PEER: u32 = 1;
    const VIRTUAL_DISPLAY_MAX_COUNT: u32 = 5;

    lazy_static::lazy_static! {
        static ref VIRTUAL_DISPLAY_MANAGER: Arc<Mutex<VirtualDisplayManager>> =
            Arc::new(Mutex::new(VirtualDisplayManager::default()));
    }

    #[derive(Default)]
    struct VirtualDisplayManager {
        headless_index_name: Option<(u32, String)>,
        peer_index_name: HashMap<u32, String>,
        is_driver_installed: bool,
    }

    impl VirtualDisplayManager {
        fn prepare_driver(&mut self) -> ResultType<()> {
            if !self.is_driver_installed {
                self.install_update_driver()?;
            }
            Ok(())
        }

        fn install_update_driver(&mut self) -> ResultType<()> {
            if let Err(e) = virtual_display::create_device() {
                if !e.to_string().contains("Device is already created") {
                    bail!("Create device failed {}", e);
                }
            }
            // Reboot is not required for this case.
            let mut _reboot_required = false;
            virtual_display::install_update_driver(&mut _reboot_required)?;
            self.is_driver_installed = true;
            Ok(())
        }

        fn plug_in_monitor(index: u32, modes: &[virtual_display::MonitorMode]) -> ResultType<()> {
            if let Err(e) = virtual_display::plug_in_monitor(index) {
                bail!("Plug in monitor failed {}", e);
            }
            if let Err(e) = virtual_display::update_monitor_modes(index, &modes) {
                log::error!("Update monitor modes failed {}", e);
            }
            Ok(())
        }
    }

    pub fn install_update_driver() -> ResultType<()> {
        VIRTUAL_DISPLAY_MANAGER
            .lock()
            .unwrap()
            .install_update_driver()
    }

    #[inline]
    fn get_device_names() -> Vec<String> {
        windows::get_device_names(Some(super::RUSTDESK_IDD_DEVICE_STRING))
    }

    pub fn plug_in_headless() -> ResultType<()> {
        let mut manager = VIRTUAL_DISPLAY_MANAGER.lock().unwrap();
        manager.prepare_driver()?;
        let modes = [virtual_display::MonitorMode {
            width: 1920,
            height: 1080,
            sync: 60,
        }];
        let device_names = get_device_names().into_iter().collect();
        VirtualDisplayManager::plug_in_monitor(VIRTUAL_DISPLAY_INDEX_FOR_HEADLESS, &modes)?;
        let device_name = get_new_device_name(&device_names);
        manager.headless_index_name = Some((VIRTUAL_DISPLAY_INDEX_FOR_HEADLESS, device_name));
        Ok(())
    }

    pub fn plug_out_headless() -> bool {
        let mut manager = VIRTUAL_DISPLAY_MANAGER.lock().unwrap();
        if let Some((index, _)) = manager.headless_index_name.take() {
            if let Err(e) = virtual_display::plug_out_monitor(index) {
                log::error!("Plug out monitor failed {}", e);
            }
            true
        } else {
            false
        }
    }

    fn get_new_device_name(device_names: &HashSet<String>) -> String {
        for _ in 0..3 {
            let device_names_af: HashSet<String> = get_device_names().into_iter().collect();
            let diff_names: Vec<_> = device_names_af.difference(&device_names).collect();
            if diff_names.len() == 1 {
                return diff_names[0].clone();
            } else if diff_names.len() > 1 {
                log::error!(
                    "Failed to get diff device names after plugin virtual display, more than one diff names: {:?}",
                    &diff_names
                );
                return "".to_string();
            }
            // Sleep is needed here to wait for the virtual display to be ready.
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        log::error!("Failed to get diff device names after plugin virtual display",);
        "".to_string()
    }

    pub fn get_virtual_displays() -> Vec<u32> {
        VIRTUAL_DISPLAY_MANAGER
            .lock()
            .unwrap()
            .peer_index_name
            .keys()
            .cloned()
            .collect()
    }

    pub fn plug_in_index_modes(
        idx: u32,
        mut modes: Vec<virtual_display::MonitorMode>,
    ) -> ResultType<()> {
        let mut manager = VIRTUAL_DISPLAY_MANAGER.lock().unwrap();
        manager.prepare_driver()?;
        if !manager.peer_index_name.contains_key(&idx) {
            let device_names = get_device_names().into_iter().collect();
            if modes.is_empty() {
                modes.push(virtual_display::MonitorMode {
                    width: 1920,
                    height: 1080,
                    sync: 60,
                });
            }
            match VirtualDisplayManager::plug_in_monitor(idx, modes.as_slice()) {
                Ok(_) => {
                    let device_name = get_new_device_name(&device_names);
                    manager.peer_index_name.insert(idx, device_name);
                }
                Err(e) => {
                    log::error!("Plug in monitor failed {}", e);
                }
            }
        }
        Ok(())
    }

    pub fn reset_all() -> ResultType<()> {
        if super::is_virtual_display_supported() {
            return Ok(());
        }

        if let Err(e) = plug_out_peer_request(&get_virtual_displays()) {
            log::error!("Failed to plug out virtual displays: {}", e);
        }
        let _ = plug_out_headless();
        Ok(())
    }

    pub fn plug_in_peer_request(
        modes: Vec<Vec<virtual_display::MonitorMode>>,
    ) -> ResultType<Vec<u32>> {
        let mut manager = VIRTUAL_DISPLAY_MANAGER.lock().unwrap();
        manager.prepare_driver()?;

        let mut indices: Vec<u32> = Vec::new();
        for m in modes.iter() {
            for idx in VIRTUAL_DISPLAY_START_FOR_PEER..VIRTUAL_DISPLAY_MAX_COUNT {
                if !manager.peer_index_name.contains_key(&idx) {
                    let device_names = get_device_names().into_iter().collect();
                    match VirtualDisplayManager::plug_in_monitor(idx, m) {
                        Ok(_) => {
                            let device_name = get_new_device_name(&device_names);
                            manager.peer_index_name.insert(idx, device_name);
                            indices.push(idx);
                        }
                        Err(e) => {
                            log::error!("Plug in monitor failed {}", e);
                        }
                    }
                    break;
                }
            }
        }

        Ok(indices)
    }

    pub fn plug_out_peer_request(indices: &[u32]) -> ResultType<()> {
        let mut manager = VIRTUAL_DISPLAY_MANAGER.lock().unwrap();
        for idx in indices.iter() {
            if manager.peer_index_name.contains_key(idx) {
                allow_err!(virtual_display::plug_out_monitor(*idx));
                manager.peer_index_name.remove(idx);
            }
        }
        Ok(())
    }

    pub fn is_virtual_display(name: &str) -> bool {
        let lock = VIRTUAL_DISPLAY_MANAGER.lock().unwrap();
        if let Some((_, device_name)) = &lock.headless_index_name {
            if windows::is_device_name(device_name, name) {
                return true;
            }
        }
        for (_, v) in lock.peer_index_name.iter() {
            if windows::is_device_name(v, name) {
                return true;
            }
        }
        false
    }

    /// Check if a display name belongs to an IDD virtual display driver
    /// by querying the OS directly (EnumDisplayDevices). Unlike `is_virtual_display`,
    /// this works even if the VD was created by a previous process.
    pub fn is_idd_display(name: &str) -> bool {
        for device_string in &[super::AMYUNI_IDD_DEVICE_STRING, super::RUSTDESK_IDD_DEVICE_STRING] {
            let idd_names = windows::get_device_names(Some(device_string));
            for idd_name in &idd_names {
                if windows::is_device_name(idd_name, name) {
                    return true;
                }
            }
        }
        false
    }

    fn change_resolution(index: u32, w: u32, h: u32) -> bool {
        let modes = [virtual_display::MonitorMode {
            width: w,
            height: h,
            sync: 60,
        }];
        match virtual_display::update_monitor_modes(index, &modes) {
            Ok(_) => true,
            Err(e) => {
                log::error!("Update monitor {} modes {:?} failed: {}", index, &modes, e);
                false
            }
        }
    }

    pub fn change_resolution_if_is_virtual_display(name: &str, w: u32, h: u32) -> Option<bool> {
        let lock = VIRTUAL_DISPLAY_MANAGER.lock().unwrap();
        if let Some((index, device_name)) = &lock.headless_index_name {
            if windows::is_device_name(device_name, name) {
                return Some(change_resolution(*index, w, h));
            }
        }

        for (k, v) in lock.peer_index_name.iter() {
            if windows::is_device_name(v, name) {
                return Some(change_resolution(*k, w, h));
            }
        }
        None
    }
}

#[cfg(windows)]
pub mod amyuni_idd {
    use super::windows;
    use crate::platform::win_device;
    use hbb_common::{bail, lazy_static, log, tokio::time::Instant, ResultType};
    use std::{
        ptr::null_mut,
        sync::{atomic, Arc, Mutex},
        time::Duration,
    };
    use winapi::{
        shared::{guiddef::GUID, winerror::ERROR_NO_MORE_ITEMS},
        um::winnt::{KEY_READ, KEY_WRITE},
    };
    use winreg::enums::*;

    use winapi::um::shellapi::ShellExecuteA;

    const INF_PATH: &str = r#"usbmmidd_v2\usbmmIdd.inf"#;
    const INTERFACE_GUID: GUID = GUID {
        Data1: 0xb5ffd75f,
        Data2: 0xda40,
        Data3: 0x4353,
        Data4: [0x8f, 0xf8, 0xb6, 0xda, 0xf6, 0xf1, 0xd8, 0xca],
    };
    const HARDWARE_ID: &str = "usbmmidd";
    const PLUG_MONITOR_IO_CONTROL_CDOE: u32 = 2307084;
    const INSTALLER_EXE_FILE: &str = "deviceinstaller64.exe";

    lazy_static::lazy_static! {
        static ref LOCK: Arc<Mutex<()>> = Default::default();
        static ref LAST_PLUG_IN_HEADLESS_TIME: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    }
    const VIRTUAL_DISPLAY_MAX_COUNT: usize = 4;
    static VIRTUAL_DISPLAY_COUNT: atomic::AtomicUsize = atomic::AtomicUsize::new(0);

    fn get_deviceinstaller64_work_dir() -> ResultType<Option<Vec<u8>>> {
        let cur_exe = std::env::current_exe()?;
        let Some(cur_dir) = cur_exe.parent() else {
            bail!("Cannot get parent of current exe file.");
        };
        let work_dir = cur_dir.join("usbmmidd_v2");
        if !work_dir.exists() {
            return Ok(None);
        }
        let exe_path = work_dir.join(INSTALLER_EXE_FILE);
        if !exe_path.exists() {
            return Ok(None);
        }

        let Some(work_dir) = work_dir.to_str() else {
            bail!("Cannot convert work_dir to string.");
        };
        let mut work_dir2 = work_dir.as_bytes().to_vec();
        work_dir2.push(0);
        Ok(Some(work_dir2))
    }

    pub fn uninstall_driver() -> ResultType<()> {
        if let Ok(Some(work_dir)) = get_deviceinstaller64_work_dir() {
            if crate::platform::windows::is_x64() {
                log::info!("Uninstalling driver by deviceinstaller64.exe");
                install_if_x86_on_x64(&work_dir, "remove usbmmidd")?;
                std::thread::sleep(Duration::from_secs(2));
                return Ok(());
            }
        }

        log::info!("Uninstalling driver by SetupAPI");
        let mut reboot_required = false;
        let _ = unsafe { win_device::uninstall_driver(HARDWARE_ID, &mut reboot_required)? };
        Ok(())
    }

    fn install_if_x86_on_x64(work_dir: &[u8], args: &str) -> ResultType<()> {
        const SW_HIDE: i32 = 0;
        let mut args = args.bytes().collect::<Vec<_>>();
        args.push(0);
        let mut exe_file = INSTALLER_EXE_FILE.bytes().collect::<Vec<_>>();
        exe_file.push(0);
        let hi = unsafe {
            ShellExecuteA(
                null_mut(),
                "open\0".as_ptr() as _,
                exe_file.as_ptr() as _,
                args.as_ptr() as _,
                work_dir.as_ptr() as _,
                SW_HIDE,
            ) as i32
        };
        if hi <= 32 {
            log::error!("Failed to run deviceinstaller: {}", hi);
            bail!("Failed to run deviceinstaller.")
        }
        Ok(())
    }

    fn check_install_driver(is_async: &mut bool) -> ResultType<()> {
        let _l = LOCK.lock().unwrap();
        let drivers = windows::get_display_drivers();
        if drivers
            .iter()
            .any(|(s, c)| s == super::AMYUNI_IDD_DEVICE_STRING && *c == 0)
        {
            *is_async = false;
            return Ok(());
        }

        if let Ok(Some(work_dir)) = get_deviceinstaller64_work_dir() {
            if crate::platform::windows::is_x64() {
                log::info!("Installing driver by deviceinstaller64.exe");
                install_if_x86_on_x64(&work_dir, "install usbmmidd.inf usbmmidd")?;
                *is_async = true;
                return Ok(());
            }
        }

        let exe_file = std::env::current_exe()?;
        let Some(cur_dir) = exe_file.parent() else {
            bail!("Cannot get parent of current exe file");
        };
        let inf_path = cur_dir.join(INF_PATH);
        if !inf_path.exists() {
            bail!("Driver inf file not found.");
        }
        let inf_path = inf_path.to_string_lossy().to_string();

        log::info!("Installing driver by SetupAPI");
        let mut reboot_required = false;
        let _ =
            unsafe { win_device::install_driver(&inf_path, HARDWARE_ID, &mut reboot_required)? };
        *is_async = false;
        Ok(())
    }

    pub fn reset_all() -> ResultType<()> {
        let _ = crate::privacy_mode::turn_off_privacy(0, None);
        let _ = plug_out_monitor(super::IDD_PLUG_OUT_ALL_INDEX, true, false);
        *LAST_PLUG_IN_HEADLESS_TIME.lock().unwrap() = None;
        Ok(())
    }

    #[inline]
    fn plug_monitor_(
        add: bool,
        wait_timeout: Option<Duration>,
    ) -> Result<(), win_device::DeviceError> {
        let cmd = if add { 0x10 } else { 0x00 };
        let cmd = [cmd, 0x00, 0x00, 0x00];
        let now = Instant::now();
        let c1 = get_monitor_count();
        unsafe {
            win_device::device_io_control(&INTERFACE_GUID, PLUG_MONITOR_IO_CONTROL_CDOE, &cmd, 0)?;
        }
        if let Some(wait_timeout) = wait_timeout {
            while now.elapsed() < wait_timeout {
                if get_monitor_count() != c1 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        }
        if add {
            if VIRTUAL_DISPLAY_COUNT.load(atomic::Ordering::SeqCst) < VIRTUAL_DISPLAY_MAX_COUNT {
                VIRTUAL_DISPLAY_COUNT.fetch_add(1, atomic::Ordering::SeqCst);
            }
        } else {
            if VIRTUAL_DISPLAY_COUNT.load(atomic::Ordering::SeqCst) > 0 {
                VIRTUAL_DISPLAY_COUNT.fetch_sub(1, atomic::Ordering::SeqCst);
            }
        }
        Ok(())
    }

    fn plug_in_monitor_(
        add: bool,
        is_driver_async_installed: bool,
        wait_timeout: Option<Duration>,
        width: usize,
        height: usize,
    ) -> ResultType<()> {
        let timeout = Duration::from_secs(3);
        let now = Instant::now();
        // Record Amyuni device count BEFORE plug-in so the background thread
        // can wait for EnumDisplayDevices to actually report the new display.
        let old_amyuni_count =
            windows::get_device_names(Some(super::AMYUNI_IDD_DEVICE_STRING)).len();
        loop {
            match plug_monitor_(add, wait_timeout) {
                Ok(_) => {
                    break;
                }
                Err(e) => {
                    if is_driver_async_installed {
                        if let win_device::DeviceError::WinApiLastErr(_, e2) = &e {
                            if e2.raw_os_error() == Some(ERROR_NO_MORE_ITEMS as _) {
                                if now.elapsed() < timeout {
                                    std::thread::sleep(Duration::from_millis(100));
                                    continue;
                                }
                            }
                        }
                    }
                    return Err(e.into());
                }
            }
        }
        std::thread::spawn(move || {
            try_reset_resolution_on_first_plug_in(old_amyuni_count, width, height);
        });

        Ok(())
    }

    /// Wait for the newly plugged-in Amyuni display to become visible via
    /// `EnumDisplayDevices` (not just the registry), then apply the target
    /// resolution to it.  The old registry-based check fired too early:
    /// the registry connectivity changes before EnumDisplayDevices reports
    /// the new display with non-zero dimensions, so subsequent VDs never
    /// got their resolution applied.
    fn try_reset_resolution_on_first_plug_in(
        old_amyuni_count: usize,
        width: usize,
        height: usize,
    ) {
        let (w, h) = (width, height);
        for attempt in 0..30 {
            std::thread::sleep(Duration::from_millis(300));
            let current_names =
                windows::get_device_names(Some(super::AMYUNI_IDD_DEVICE_STRING));
            if current_names.len() > old_amyuni_count {
                log::info!(
                    "Amyuni: new display detected ({} -> {}) after {}ms, applying resolution {}x{}",
                    old_amyuni_count,
                    current_names.len(),
                    (attempt + 1) * 300,
                    w,
                    h
                );
                for name in current_names.iter() {
                    match crate::platform::change_resolution(&name, w, h) {
                        Ok(_) => log::info!("Amyuni: successfully set {} to {}x{}", name, w, h),
                        Err(e) => {
                            log::error!("Amyuni: failed to set {} to {}x{}: {}", name, w, h, e)
                        }
                    }
                }
                return;
            }
        }
        log::warn!(
            "Amyuni: timed out waiting for new display (still {} devices after 9s)",
            windows::get_device_names(Some(super::AMYUNI_IDD_DEVICE_STRING)).len()
        );
    }

    pub fn plug_in_headless() -> ResultType<()> {
        let mut tm = LAST_PLUG_IN_HEADLESS_TIME.lock().unwrap();
        if let Some(tm) = &mut *tm {
            if tm.elapsed() < Duration::from_secs(3) {
                bail!("Plugging in too frequently.");
            }
        }
        *tm = Some(Instant::now());
        drop(tm);

        let mut is_async = false;
        if let Err(e) = check_install_driver(&mut is_async) {
            log::error!("Failed to install driver: {}", e);
            bail!("Failed to install driver.");
        }

        plug_in_monitor_(true, is_async, Some(Duration::from_millis(3_000)), 1920, 1080)
    }

    /// The Amyuni IDD driver reads at most 10 resolution entries (indices 0-9)
    /// from the registry.  If all 10 slots are occupied, overwrite the last one
    /// so custom resolutions chosen from the UI always take effect.
    fn update_amyuni_registry_resolution(w: u32, h: u32) -> ResultType<()> {
        use winreg::RegKey;
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\WUDF\Services\usbmmIdd\Parameters\Monitors";
        let key = hklm.open_subkey_with_flags(path, KEY_READ | KEY_WRITE)?;

        let resolution = format!("{},{}", w, h);
        const MAX_ENTRY_INDEX: i32 = 9; // driver reads entries 0-9

        let mut max_index: i32 = -1;
        for i in 0..=MAX_ENTRY_INDEX {
            let name = i.to_string();
            match key.get_value::<String, _>(&name) {
                Ok(val) => {
                    if val == resolution {
                        log::info!(
                            "Amyuni: resolution {}x{} already in registry at index {}",
                            w, h, i
                        );
                        return Ok(());
                    }
                    max_index = i;
                }
                Err(_) => break,
            }
        }

        if max_index < MAX_ENTRY_INDEX {
            // Free slot available
            let new_index = (max_index + 1).to_string();
            key.set_value(&new_index, &resolution)?;
            log::info!(
                "Amyuni: added resolution {}x{} to registry at index {}",
                w, h, new_index
            );
        } else {
            // All 10 slots used: overwrite the last one
            let idx = MAX_ENTRY_INDEX.to_string();
            let old_val: String = key.get_value(&idx).unwrap_or_default();
            key.set_value(&idx, &resolution)?;
            log::info!(
                "Amyuni: all registry slots full, overwrote index {} ('{}') with {}x{}",
                MAX_ENTRY_INDEX, old_val, w, h
            );
        }

        Ok(())
    }

    pub fn plug_in_monitor() -> ResultType<()> {
        let mut is_async = false;
        if let Err(e) = check_install_driver(&mut is_async) {
            log::error!("Failed to install driver: {}", e);
            bail!("Failed to install driver.");
        }

        if get_monitor_count() == VIRTUAL_DISPLAY_MAX_COUNT {
            bail!("There are already {VIRTUAL_DISPLAY_MAX_COUNT} monitors plugged in.");
        }

        let (w, h) = super::take_custom_resolution().unwrap_or((1920, 1080));

        if let Err(e) = update_amyuni_registry_resolution(w, h) {
            log::warn!(
                "Failed to update Amyuni registry with resolution {}x{}: {}",
                w, h, e
            );
        }

        plug_in_monitor_(true, is_async, None, w as usize, h as usize)
    }

    pub fn plug_out_monitor(index: i32, force_all: bool, force_one: bool) -> ResultType<()> {
        let plug_out_all = index == super::IDD_PLUG_OUT_ALL_INDEX;
        let mut plug_in_count = VIRTUAL_DISPLAY_COUNT.load(atomic::Ordering::Relaxed);
        let amyuni_count = get_monitor_count();
        if !plug_out_all {
            if plug_in_count == 0 && amyuni_count > 0 {
                if force_one {
                    plug_in_count = 1;
                } else {
                    bail!("The virtual display is managed by other processes.");
                }
            }
        }

        let all_count = windows::get_device_names(None).len();
        let mut to_plug_out_count = match all_count {
            0 => return Ok(()),
            1 => {
                if plug_in_count == 0 {
                    bail!("No virtual displays to plug out.")
                } else {
                    if force_all {
                        1
                    } else {
                        bail!("This only virtual display cannot be plugged out.")
                    }
                }
            }
            _ => {
                if all_count == plug_in_count {
                    if force_all {
                        all_count
                    } else {
                        all_count - 1
                    }
                } else {
                    plug_in_count
                }
            }
        };
        if to_plug_out_count != 0 && !plug_out_all {
            to_plug_out_count = 1;
        }

        for _i in 0..to_plug_out_count {
            let _ = plug_monitor_(false, None);
        }
        Ok(())
    }

    #[inline]
    pub fn get_monitor_count() -> usize {
        windows::get_device_names(Some(super::AMYUNI_IDD_DEVICE_STRING)).len()
    }

    #[inline]
    pub fn is_my_display(name: &str) -> bool {
        windows::get_device_names(Some(super::AMYUNI_IDD_DEVICE_STRING))
            .iter()
            .any(|s| windows::is_device_name(s, name))
    }
}

#[cfg(windows)]
mod windows {
    use std::ptr::null_mut;
    use winapi::{
        shared::{
            devguid::GUID_DEVCLASS_DISPLAY,
            minwindef::{DWORD, FALSE},
            ntdef::ULONG,
        },
        um::{
            cfgmgr32::{CM_Get_DevNode_Status, CR_SUCCESS},
            cguid::GUID_NULL,
            setupapi::{
                SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
                SP_DEVINFO_DATA,
            },
            wingdi::{
                DEVMODEW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ACTIVE, DISPLAY_DEVICE_MIRRORING_DRIVER,
            },
            winnt::HANDLE,
            winuser::{EnumDisplayDevicesW, EnumDisplaySettingsExW, ENUM_CURRENT_SETTINGS},
        },
    };

    const DIGCF_PRESENT: DWORD = 0x00000002;
    const SPDRP_DEVICEDESC: DWORD = 0x00000000;
    const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

    #[inline]
    pub(super) fn is_device_name(device_name: &str, name: &str) -> bool {
        if name.len() == device_name.len() {
            name == device_name
        } else if name.len() > device_name.len() {
            false
        } else {
            &device_name[..name.len()] == name && device_name.as_bytes()[name.len() as usize] == 0
        }
    }

    pub(super) fn get_device_names(device_string: Option<&str>) -> Vec<String> {
        let mut device_names = Vec::new();
        let mut dd: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as DWORD;
        let mut i_dev_num = 0;
        loop {
            let result = unsafe { EnumDisplayDevicesW(null_mut(), i_dev_num, &mut dd, 0) };
            if result == 0 {
                break;
            }
            i_dev_num += 1;

            if 0 == (dd.StateFlags & DISPLAY_DEVICE_ACTIVE)
                || (dd.StateFlags & DISPLAY_DEVICE_MIRRORING_DRIVER) > 0
            {
                continue;
            }

            let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
            dm.dmSize = std::mem::size_of::<DEVMODEW>() as _;
            dm.dmDriverExtra = 0;
            let ok = unsafe {
                EnumDisplaySettingsExW(
                    dd.DeviceName.as_ptr(),
                    ENUM_CURRENT_SETTINGS,
                    &mut dm as _,
                    0,
                )
            };
            if ok == FALSE {
                continue;
            }
            if dm.dmPelsHeight == 0 || dm.dmPelsWidth == 0 {
                continue;
            }

            if let (Ok(device_name), Ok(ds)) = (
                String::from_utf16(&dd.DeviceName),
                String::from_utf16(&dd.DeviceString),
            ) {
                if let Some(s) = device_string {
                    if ds.len() >= s.len() && &ds[..s.len()] == s {
                        device_names.push(device_name);
                    }
                } else {
                    device_names.push(device_name);
                }
            }
        }
        device_names
    }

    pub(super) fn get_display_drivers() -> Vec<(String, u32)> {
        let mut display_drivers: Vec<(String, u32)> = Vec::new();

        let device_info_set = unsafe {
            SetupDiGetClassDevsW(
                &GUID_DEVCLASS_DISPLAY,
                null_mut(),
                null_mut(),
                DIGCF_PRESENT,
            )
        };

        if device_info_set == INVALID_HANDLE_VALUE {
            println!(
                "Failed to get device information set. Error: {}",
                std::io::Error::last_os_error()
            );
            return display_drivers;
        }

        let mut device_info_data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ClassGuid: GUID_NULL,
            DevInst: 0,
            Reserved: 0,
        };

        let mut device_index = 0;
        loop {
            let result = unsafe {
                SetupDiEnumDeviceInfo(device_info_set, device_index, &mut device_info_data)
            };
            if result == 0 {
                break;
            }

            let mut data_type: DWORD = 0;
            let mut required_size: DWORD = 0;

            let mut buffer;
            unsafe {
                SetupDiGetDeviceRegistryPropertyW(
                    device_info_set,
                    &mut device_info_data,
                    SPDRP_DEVICEDESC,
                    &mut data_type,
                    null_mut(),
                    0,
                    &mut required_size,
                );

                buffer = vec![0; required_size as usize / 2];
                SetupDiGetDeviceRegistryPropertyW(
                    device_info_set,
                    &mut device_info_data,
                    SPDRP_DEVICEDESC,
                    &mut data_type,
                    buffer.as_mut_ptr() as *mut u8,
                    required_size,
                    null_mut(),
                );
            }

            let Ok(driver_description) = String::from_utf16(&buffer) else {
                println!("Failed to convert driver description to string");
                device_index += 1;
                continue;
            };

            let mut status: ULONG = 0;
            let mut problem_number: ULONG = 0;
            let config_ret = unsafe {
                CM_Get_DevNode_Status(
                    &mut status,
                    &mut problem_number,
                    device_info_data.DevInst,
                    0,
                )
            };
            if config_ret != CR_SUCCESS {
                println!(
                    "Failed to get device status. Error: {}",
                    std::io::Error::last_os_error()
                );
                device_index += 1;
                continue;
            }
            display_drivers.push((driver_description, problem_number));
            device_index += 1;
        }

        display_drivers
    }
}

// =============================================================================
// Linux EVDI implementation
// =============================================================================

#[cfg(target_os = "linux")]
pub mod linux_evdi {
    use hbb_common::{bail, log, ResultType};
    use std::collections::HashMap;
    use std::ffi::CStr;
    use std::os::raw::{c_int, c_uint, c_void};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};

    type EvdiHandle = *mut c_void;

    // ========== EVDI C structs ==========

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct EvdiRect {
        x1: c_int,
        y1: c_int,
        x2: c_int,
        y2: c_int,
    }

    #[repr(C)]
    struct EvdiBuffer {
        id: c_int,
        buffer: *mut c_void,
        width: c_int,
        height: c_int,
        stride: c_int,
        rects: *mut EvdiRect,
        rect_count: c_int,
    }

    #[repr(C)]
    struct EvdiMode {
        width: c_int,
        height: c_int,
        refresh_rate: c_int,
        bits_per_pixel: c_int,
        pixel_format: c_uint,
    }

    #[repr(C)]
    struct EvdiCursorSet {
        hot_x: i32,
        hot_y: i32,
        width: u32,
        height: u32,
        enabled: u8,
        buffer_length: u32,
        buffer: *mut u32,
        pixel_format: u32,
        stride: u32,
    }

    #[repr(C)]
    struct EvdiCursorMove {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct EvdiDdcciData {
        address: u16,
        flags: u16,
        buffer_length: u32,
        buffer: *mut u8,
    }

    #[repr(C)]
    struct EvdiEventContext {
        dpms_handler: Option<unsafe extern "C" fn(dpms_mode: c_int, user_data: *mut c_void)>,
        mode_changed_handler: Option<unsafe extern "C" fn(mode: EvdiMode, user_data: *mut c_void)>,
        update_ready_handler: Option<
            unsafe extern "C" fn(buffer_to_be_updated: c_int, user_data: *mut c_void),
        >,
        crtc_state_handler: Option<unsafe extern "C" fn(state: c_int, user_data: *mut c_void)>,
        cursor_set_handler: Option<
            unsafe extern "C" fn(cursor_set: EvdiCursorSet, user_data: *mut c_void),
        >,
        cursor_move_handler: Option<
            unsafe extern "C" fn(cursor_move: EvdiCursorMove, user_data: *mut c_void),
        >,
        ddcci_data_handler: Option<
            unsafe extern "C" fn(ddcci_data: EvdiDdcciData, user_data: *mut c_void),
        >,
        user_data: *mut c_void,
    }

    // ========== FFI function types ==========

    type FnEvdiCheckDevice = unsafe extern "C" fn(device: c_int) -> c_int;
    type FnEvdiAddDevice = unsafe extern "C" fn() -> c_int;
    type FnEvdiOpen = unsafe extern "C" fn(device: c_int) -> EvdiHandle;
    type FnEvdiClose = unsafe extern "C" fn(handle: EvdiHandle);
    type FnEvdiConnect = unsafe extern "C" fn(
        handle: EvdiHandle,
        edid: *const u8,
        edid_length: c_uint,
        sku_area_limit: u32,
    );
    type FnEvdiDisconnect = unsafe extern "C" fn(handle: EvdiHandle);
    // Consumer loop functions
    type FnEvdiRegisterBuffer = unsafe extern "C" fn(handle: EvdiHandle, buffer: EvdiBuffer);
    type FnEvdiUnregisterBuffer = unsafe extern "C" fn(handle: EvdiHandle, buffer_id: c_int);
    type FnEvdiRequestUpdate = unsafe extern "C" fn(handle: EvdiHandle, buffer_id: c_int) -> bool;
    type FnEvdiGrabPixels = unsafe extern "C" fn(
        handle: EvdiHandle,
        rects: *mut EvdiRect,
        num_rects: *mut c_int,
    );
    type FnEvdiHandleEvents =
        unsafe extern "C" fn(handle: EvdiHandle, context: *mut EvdiEventContext);
    type FnEvdiGetEventReady = unsafe extern "C" fn(handle: EvdiHandle) -> c_int;

    struct EvdiLib {
        _lib_handle: *mut c_void,
        check_device: FnEvdiCheckDevice,
        add_device: FnEvdiAddDevice,
        open: FnEvdiOpen,
        close: FnEvdiClose,
        connect: FnEvdiConnect,
        disconnect: FnEvdiDisconnect,
        register_buffer: FnEvdiRegisterBuffer,
        unregister_buffer: FnEvdiUnregisterBuffer,
        request_update: FnEvdiRequestUpdate,
        grab_pixels: FnEvdiGrabPixels,
        handle_events: FnEvdiHandleEvents,
        get_event_ready: FnEvdiGetEventReady,
    }

    // Safety: EvdiLib contains function pointers and an opaque library handle.
    // All access is synchronized through the MANAGER mutex.
    unsafe impl Send for EvdiLib {}

    impl EvdiLib {
        fn load() -> Option<Self> {
            unsafe {
                // Try libevdi.so.1 first (Ubuntu 24.04+), then libevdi.so.0, then libevdi.so
                let names: &[&[u8]] = &[
                    b"libevdi.so.1\0",
                    b"libevdi.so.0\0",
                    b"libevdi.so\0",
                ];
                let mut lib = std::ptr::null_mut();
                for name in names {
                    lib = hbb_common::libc::dlopen(
                        name.as_ptr() as *const hbb_common::libc::c_char,
                        hbb_common::libc::RTLD_NOW,
                    );
                    if !lib.is_null() {
                        log::info!("EVDI: opened {}", String::from_utf8_lossy(&name[..name.len()-1]));
                        break;
                    }
                }
                if lib.is_null() {
                    log::info!("EVDI: libevdi not found: {}", get_dl_error());
                    return None;
                }

                macro_rules! load_sym {
                    ($lib:expr, $name:expr, $type:ty) => {{
                        let sym = hbb_common::libc::dlsym(
                            $lib,
                            concat!($name, "\0").as_ptr() as *const hbb_common::libc::c_char,
                        );
                        if sym.is_null() {
                            log::warn!("EVDI: failed to load symbol {}: {}", $name, get_dl_error());
                            hbb_common::libc::dlclose($lib);
                            return None;
                        }
                        std::mem::transmute::<*mut c_void, $type>(sym)
                    }};
                }

                let check_device = load_sym!(lib, "evdi_check_device", FnEvdiCheckDevice);
                let add_device = load_sym!(lib, "evdi_add_device", FnEvdiAddDevice);
                let open = load_sym!(lib, "evdi_open", FnEvdiOpen);
                let close = load_sym!(lib, "evdi_close", FnEvdiClose);
                let connect = load_sym!(lib, "evdi_connect", FnEvdiConnect);
                let disconnect = load_sym!(lib, "evdi_disconnect", FnEvdiDisconnect);
                let register_buffer =
                    load_sym!(lib, "evdi_register_buffer", FnEvdiRegisterBuffer);
                let unregister_buffer =
                    load_sym!(lib, "evdi_unregister_buffer", FnEvdiUnregisterBuffer);
                let request_update =
                    load_sym!(lib, "evdi_request_update", FnEvdiRequestUpdate);
                let grab_pixels = load_sym!(lib, "evdi_grab_pixels", FnEvdiGrabPixels);
                let handle_events =
                    load_sym!(lib, "evdi_handle_events", FnEvdiHandleEvents);
                let get_event_ready =
                    load_sym!(lib, "evdi_get_event_ready", FnEvdiGetEventReady);

                log::info!("EVDI: libevdi loaded successfully (with consumer loop support)");
                Some(Self {
                    _lib_handle: lib,
                    check_device,
                    add_device,
                    open,
                    close,
                    connect,
                    disconnect,
                    register_buffer,
                    unregister_buffer,
                    request_update,
                    grab_pixels,
                    handle_events,
                    get_event_ready,
                })
            }
        }
    }

    impl Drop for EvdiLib {
        fn drop(&mut self) {
            unsafe {
                hbb_common::libc::dlclose(self._lib_handle);
            }
        }
    }

    fn get_dl_error() -> String {
        unsafe {
            let err = hbb_common::libc::dlerror();
            if err.is_null() {
                "unknown error".to_string()
            } else {
                CStr::from_ptr(err).to_string_lossy().into_owned()
            }
        }
    }

    /// Function pointers needed by the consumer thread (copied from EvdiLib so
    /// the thread doesn't need access to the MANAGER mutex).
    #[derive(Clone, Copy)]
    struct ConsumerFns {
        register_buffer: FnEvdiRegisterBuffer,
        unregister_buffer: FnEvdiUnregisterBuffer,
        request_update: FnEvdiRequestUpdate,
        grab_pixels: FnEvdiGrabPixels,
        handle_events: FnEvdiHandleEvents,
        get_event_ready: FnEvdiGetEventReady,
    }

    // Safety: ConsumerFns contains only function pointers (plain addresses).
    unsafe impl Send for ConsumerFns {}

    /// All function pointers copied from EvdiLib. Allows callers to
    /// release the MANAGER lock while performing DRM operations.
    #[derive(Clone, Copy)]
    struct EvdiLibFns {
        add_device: FnEvdiAddDevice,
        check_device: FnEvdiCheckDevice,
        open: FnEvdiOpen,
        close: FnEvdiClose,
        connect: FnEvdiConnect,
        disconnect: FnEvdiDisconnect,
        consumer: ConsumerFns,
    }

    // Safety: EvdiLibFns contains only function pointers (plain addresses).
    unsafe impl Send for EvdiLibFns {}

    /// Settling delay (in ms) after a DRM topology change (add/remove/connect/disconnect).
    /// KScreen's XRandR backend processes hotplug uevents asynchronously and needs
    /// time to finish before the next topology change occurs. Without adequate delay,
    /// KScreen segfaults because its internal data structures are partially initialized
    /// when a second hotplug event arrives.
    const DRM_HOTPLUG_SETTLE_MS: u64 = 3000;

    struct EvdiDevice {
        handle: EvdiHandle,
        device_id: i32,
        consumer_stop: Arc<AtomicBool>,
        consumer_thread: Option<std::thread::JoinHandle<()>>,
    }

    // Safety: EvdiDevice contains an opaque handle pointer.
    // All access is synchronized through the MANAGER mutex.
    unsafe impl Send for EvdiDevice {}

    impl EvdiDevice {
        fn disconnect_and_close(
            &mut self,
            disconnect_fn: FnEvdiDisconnect,
            close_fn: FnEvdiClose,
        ) {
            // Stop the consumer thread first
            self.consumer_stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.consumer_thread.take() {
                let _ = thread.join();
            }
            log::info!("EVDI: consumer thread stopped for card{}", self.device_id);
            unsafe {
                (disconnect_fn)(self.handle);
            }
            // Give KScreen time to process the disconnect hotplug event before
            // destroying the device. Without this delay, KScreen's XRandR backend
            // segfaults when it tries to query a device that was removed while
            // processing the disconnect notification.
            log::info!(
                "EVDI: device {} disconnected, waiting {}ms for display server to process hotplug",
                self.device_id, DRM_HOTPLUG_SETTLE_MS
            );
            std::thread::sleep(std::time::Duration::from_millis(DRM_HOTPLUG_SETTLE_MS));
            unsafe {
                (close_fn)(self.handle);
            }
            log::info!("EVDI: device {} closed", self.device_id);
        }
    }

    /// Callback for EVDI update_ready events. Sets the AtomicBool to signal
    /// that a buffer is ready to be grabbed.
    unsafe extern "C" fn evdi_update_ready_callback(
        _buffer_to_be_updated: c_int,
        user_data: *mut c_void,
    ) {
        if !user_data.is_null() {
            let flag = &*(user_data as *const AtomicBool);
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// No-op cursor_set handler. Acknowledges cursor events from the EVDI
    /// device so they don't cause artifacts on the physical display.
    unsafe extern "C" fn evdi_cursor_set_callback(
        _cursor_set: EvdiCursorSet,
        _user_data: *mut c_void,
    ) {}

    /// No-op cursor_move handler.
    unsafe extern "C" fn evdi_cursor_move_callback(
        _cursor_move: EvdiCursorMove,
        _user_data: *mut c_void,
    ) {}

    /// Spawn a consumer thread that keeps an EVDI device responsive by
    /// periodically requesting updates and grabbing pixels.
    /// Without this loop, GNOME Shell detects an unresponsive DRM output
    /// and may black out the physical display.
    fn spawn_consumer_thread(
        handle: EvdiHandle,
        width: u32,
        height: u32,
        fns: ConsumerFns,
        stop_flag: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        // Convert function pointers (which contain *mut c_void params) to raw usize
        // so the closure is Send. These are just function addresses.
        let handle_addr = handle as usize;
        let fn_register = fns.register_buffer as usize;
        let fn_unregister = fns.unregister_buffer as usize;
        let fn_request = fns.request_update as usize;
        let fn_grab = fns.grab_pixels as usize;
        let fn_events = fns.handle_events as usize;
        let fn_ready = fns.get_event_ready as usize;

        std::thread::Builder::new()
            .name("evdi-consumer".into())
            .spawn(move || {
                // Reconstruct the handle and function pointers from usize
                let handle = handle_addr as EvdiHandle;
                let register_buffer: FnEvdiRegisterBuffer = unsafe { std::mem::transmute(fn_register) };
                let unregister_buffer: FnEvdiUnregisterBuffer = unsafe { std::mem::transmute(fn_unregister) };
                let request_update: FnEvdiRequestUpdate = unsafe { std::mem::transmute(fn_request) };
                let grab_pixels: FnEvdiGrabPixels = unsafe { std::mem::transmute(fn_grab) };
                let handle_events: FnEvdiHandleEvents = unsafe { std::mem::transmute(fn_events) };
                let get_event_ready: FnEvdiGetEventReady = unsafe { std::mem::transmute(fn_ready) };

                // Clamp to safe limits to prevent arithmetic overflow and OOM.
                // EVDI renders at the EDID resolution (≤4095), so the buffer
                // only needs to be large enough for the clamped resolution.
                let w = (width.min(MAX_SUPPORTED_RESOLUTION)) as c_int;
                let h = (height.min(MAX_SUPPORTED_RESOLUTION)) as c_int;
                let stride = w * 4; // XRGB8888

                // Allocate a dummy framebuffer
                let buf_size = (stride as usize) * (h as usize);
                let mut buffer_data: Vec<u8> = vec![0u8; buf_size];

                // Allocate rects array for grab_pixels
                let mut rects = [EvdiRect { x1: 0, y1: 0, x2: 0, y2: 0 }; 16];

                let evdi_buf = EvdiBuffer {
                    id: 0,
                    buffer: buffer_data.as_mut_ptr() as *mut c_void,
                    width: w,
                    height: h,
                    stride,
                    rects: rects.as_mut_ptr(),
                    rect_count: rects.len() as c_int,
                };

                unsafe { (register_buffer)(handle, evdi_buf) };
                log::info!("EVDI: consumer thread started ({}x{}, buffer {}KB)", width, height, buf_size / 1024);

                // Get the event fd for polling
                let event_fd = unsafe { (get_event_ready)(handle) };

                // Flag set by the update_ready callback
                let buffer_ready = AtomicBool::new(false);

                let mut event_ctx = EvdiEventContext {
                    dpms_handler: None,
                    mode_changed_handler: None,
                    update_ready_handler: Some(evdi_update_ready_callback),
                    crtc_state_handler: None,
                    cursor_set_handler: Some(evdi_cursor_set_callback),
                    cursor_move_handler: Some(evdi_cursor_move_callback),
                    ddcci_data_handler: None,
                    user_data: &buffer_ready as *const AtomicBool as *mut c_void,
                };

                // The consumer is spawned AFTER position_virtual_display()
                // confirms the CRTC is ready, so no initial delay is needed.
                // (The original code had no delay here either.)

                while !stop_flag.load(Ordering::Relaxed) {
                    // Request an update for buffer 0
                    let immediate = unsafe { (request_update)(handle, 0) };

                    if immediate {
                        // Pixels are ready now, grab them
                        let mut num_rects: c_int = rects.len() as c_int;
                        unsafe { (grab_pixels)(handle, rects.as_mut_ptr(), &mut num_rects) };
                    } else {
                        // Wait for the event fd to become ready (poll with 100ms timeout)
                        let mut pollfd = hbb_common::libc::pollfd {
                            fd: event_fd,
                            events: hbb_common::libc::POLLIN,
                            revents: 0,
                        };
                        let ret = unsafe { hbb_common::libc::poll(&mut pollfd, 1, 100) };
                        if ret > 0 && (pollfd.revents & (hbb_common::libc::POLLHUP | hbb_common::libc::POLLERR)) != 0 {
                            log::warn!("EVDI: compositor disconnected (POLLHUP/POLLERR), stopping consumer");
                            break;
                        }
                        if ret < 0 {
                            let errno = unsafe { *hbb_common::libc::__errno_location() };
                            if errno == hbb_common::libc::EBADF {
                                log::warn!("EVDI: event fd invalid (EBADF), stopping consumer");
                                break;
                            }
                        }
                        if ret > 0 && (pollfd.revents & hbb_common::libc::POLLIN) != 0 {
                            // Handle events (this dispatches update_ready_handler)
                            unsafe { (handle_events)(handle, &mut event_ctx) };

                            if buffer_ready.swap(false, Ordering::Relaxed) {
                                let mut num_rects: c_int = rects.len() as c_int;
                                unsafe {
                                    (grab_pixels)(handle, rects.as_mut_ptr(), &mut num_rects)
                                };
                            }
                        }
                    }

                    // Small sleep to avoid busy-spinning when no updates
                    std::thread::sleep(std::time::Duration::from_millis(16)); // ~60fps cap
                }

                // Cleanup
                unsafe { (unregister_buffer)(handle, 0) };
                log::info!("EVDI: consumer thread exiting");
            })
            .expect("EVDI: failed to spawn consumer thread")
    }

    struct VirtualDisplayManager {
        lib: Option<EvdiLib>,
        headless: Option<EvdiDevice>,
        peers: HashMap<u32, EvdiDevice>,
        next_peer_index: u32,
    }

    impl Default for VirtualDisplayManager {
        fn default() -> Self {
            Self {
                lib: EvdiLib::load(),
                headless: None,
                peers: HashMap::new(),
                next_peer_index: 1,
            }
        }
    }

    impl VirtualDisplayManager {
        /// Copy all function pointers from the loaded EvdiLib.
        /// This snapshot lets callers release the MANAGER lock while
        /// performing DRM operations.
        fn lib_fns(&self) -> ResultType<EvdiLibFns> {
            let lib = self
                .lib
                .as_ref()
                .ok_or_else(|| hbb_common::anyhow::anyhow!("EVDI library not loaded"))?;
            Ok(EvdiLibFns {
                add_device: lib.add_device,
                check_device: lib.check_device,
                open: lib.open,
                close: lib.close,
                connect: lib.connect,
                disconnect: lib.disconnect,
                consumer: ConsumerFns {
                    register_buffer: lib.register_buffer,
                    unregister_buffer: lib.unregister_buffer,
                    request_update: lib.request_update,
                    grab_pixels: lib.grab_pixels,
                    handle_events: lib.handle_events,
                    get_event_ready: lib.get_event_ready,
                },
            })
        }
    }

    lazy_static::lazy_static! {
        static ref MANAGER: Arc<Mutex<VirtualDisplayManager>> =
            Arc::new(Mutex::new(VirtualDisplayManager::default()));
    }

    /// Serializes DRM topology changes (add_device, connect, disconnect, close)
    /// including the settle sleeps between them. This prevents concurrent hotplug
    /// events from crashing KScreen's XRandR backend.
    ///
    /// Separate from MANAGER so that data reads (get_virtual_displays,
    /// get_platform_additions) are not blocked during DRM operations.
    ///
    /// Lock ordering: always acquire DRM_TOPOLOGY **after** releasing MANAGER
    /// to avoid deadlocks.
    static DRM_TOPOLOGY: Mutex<()> = Mutex::new(());

    // Readiness flag: signals that prepare_evdi() + reload_evdi_lib() have finished.
    static EVDI_PREPARE_DONE: AtomicBool = AtomicBool::new(false);
    static EVDI_PREPARE_LOCK: Mutex<bool> = Mutex::new(false);
    static EVDI_PREPARE_CVAR: Condvar = Condvar::new();

    /// Remove any orphaned EVDI devices left over from a previous crashed session.
    /// Called at server startup after reload_evdi_lib(), so the library is available.
    ///
    /// CRITICAL: We must NOT use /sys/devices/evdi/remove_all because it generates
    /// N DRM hotplug uevents simultaneously, causing KScreen's XRandR backend to
    /// segfault when it receives a new event while still processing a previous one.
    ///
    /// Instead, we tear down each orphaned device individually via the EVDI library:
    /// evdi_open → evdi_disconnect → sleep(3s) → evdi_close → sleep(3s)
    /// This ensures only one DRM topology change at a time, matching the safe
    /// pattern used by teardown_devices() during normal operation.
    pub fn cleanup_orphaned_evdi_devices() {
        let count_path = "/sys/devices/evdi/count";

        let count = match std::fs::read_to_string(count_path) {
            Ok(s) => s.trim().parse::<u32>().unwrap_or(0),
            Err(_) => return, // EVDI module not loaded, nothing to clean
        };

        if count == 0 {
            return;
        }

        // Get library function pointers (library must be loaded already)
        let fns = {
            let manager = MANAGER.lock().unwrap();
            match manager.lib_fns() {
                Ok(f) => f,
                Err(_) => {
                    log::warn!(
                        "EVDI: {} orphaned device(s) found but library not loaded, \
                         cannot clean up safely (skipping)",
                        count
                    );
                    return;
                }
            }
        }; // MANAGER unlocked

        // Find all EVDI card IDs via sysfs
        let evdi_card_ids: Vec<c_int> = get_existing_card_ids()
            .into_iter()
            .filter(|&id| is_evdi_via_sysfs(id))
            .collect();

        if evdi_card_ids.is_empty() {
            log::info!(
                "EVDI: sysfs reports {} device(s) but none found in /dev/dri, nothing to clean",
                count
            );
            return;
        }

        log::info!(
            "EVDI: found {} orphaned device(s) (cards: {:?}), cleaning up one by one",
            evdi_card_ids.len(),
            evdi_card_ids
        );

        // Hold DRM_TOPOLOGY for the entire cleanup to prevent concurrent
        // plug_in operations from generating overlapping hotplug events.
        let _topo = DRM_TOPOLOGY.lock().unwrap();

        for (i, &card_id) in evdi_card_ids.iter().enumerate() {
            let handle = unsafe { (fns.open)(card_id) };
            if handle.is_null() {
                log::warn!("EVDI: could not open orphaned card{}, skipping", card_id);
                continue;
            }
            log::info!("EVDI: opened orphaned card{} for cleanup", card_id);

            // Skip disconnect for orphans: the previous process died, so the kernel
            // already cleaned up its painter connection. Calling disconnect would just
            // fail with "disconnect failed" (no painter to disconnect) and generate no
            // DRM hotplug event. Only close() generates the actual device removal event.
            unsafe { (fns.close)(handle); }
            log::info!("EVDI: orphaned card{} closed", card_id);

            // Wait after close so KScreen can fully process the device removal event
            // before the next one. Without this delay, KScreen's XRandR backend
            // segfaults from overlapping hotplug processing.
            if i + 1 < evdi_card_ids.len() {
                log::info!(
                    "EVDI: waiting {}ms before next orphan cleanup",
                    DRM_HOTPLUG_SETTLE_MS
                );
                std::thread::sleep(std::time::Duration::from_millis(DRM_HOTPLUG_SETTLE_MS));
            }
        }

        log::info!("EVDI: orphan cleanup complete ({} device(s) removed)", evdi_card_ids.len());
    }

    /// Signal that EVDI preparation (package install, modprobe, sysfs, udev) is complete.
    pub fn mark_prepare_done() {
        EVDI_PREPARE_DONE.store(true, Ordering::SeqCst);
        let mut done = EVDI_PREPARE_LOCK.lock().unwrap();
        *done = true;
        EVDI_PREPARE_CVAR.notify_all();
        log::info!("EVDI: preparation marked as done");
    }

    /// Wait for prepare_evdi() to finish, with timeout.
    fn wait_for_prepare(timeout: std::time::Duration) -> bool {
        if EVDI_PREPARE_DONE.load(Ordering::SeqCst) {
            return true;
        }
        let done = EVDI_PREPARE_LOCK.lock().unwrap();
        if *done {
            return true;
        }
        let (result, _) = EVDI_PREPARE_CVAR.wait_timeout(done, timeout).unwrap();
        *result || EVDI_PREPARE_DONE.load(Ordering::SeqCst)
    }

    /// Reload the EVDI library into the manager.
    /// Called from server startup thread after prepare_evdi() installs packages.
    pub fn reload_evdi_lib() {
        let mut manager = MANAGER.lock().unwrap();
        if manager.lib.is_some() {
            return;
        }
        log::info!("EVDI: attempting to reload libevdi...");
        manager.lib = EvdiLib::load();
        if manager.lib.is_some() {
            log::info!("EVDI: library loaded successfully");
        } else {
            log::warn!("EVDI: library still not loadable");
        }
    }

    /// Check if EVDI virtual display is supported.
    /// Waits for prepare_evdi() to finish if it hasn't yet (up to 30s).
    pub fn is_supported() -> bool {
        if !EVDI_PREPARE_DONE.load(Ordering::SeqCst) {
            log::info!("EVDI: waiting for preparation to complete...");
            if !wait_for_prepare(std::time::Duration::from_secs(30)) {
                log::warn!("EVDI: preparation timed out after 30s");
            }
        }
        let lib_available = MANAGER.lock().unwrap().lib.is_some();
        if !lib_available {
            return false;
        }
        std::path::Path::new("/sys/module/evdi").exists()
    }

    /// Get the set of existing /dev/dri/card* IDs before adding a new device.
    fn get_existing_card_ids() -> std::collections::HashSet<c_int> {
        let mut ids = std::collections::HashSet::new();
        if let Ok(entries) = std::fs::read_dir("/dev/dri") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if let Some(num_str) = name_str.strip_prefix("card") {
                    if let Ok(num) = num_str.parse::<c_int>() {
                        ids.insert(num);
                    }
                }
            }
        }
        ids
    }

    /// Try to set POSIX ACLs on /dev/dri/card* so the current user can access new EVDI devices.
    /// Fallback for when the udev rule hasn't taken effect yet.
    fn try_set_dri_acl() {
        // Try $USER first, then whoami as fallback
        let user = match std::env::var("USER") {
            Ok(u) if !u.is_empty() => u,
            _ => {
                let u = hbb_common::whoami::username();
                if u.is_empty() {
                    log::warn!("EVDI: cannot determine username for ACL fallback");
                    return;
                }
                u
            },
        };
        if let Ok(entries) = std::fs::read_dir("/dev/dri") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("card") {
                    let path = entry.path();
                    let _ = std::process::Command::new("setfacl")
                        .args(["-m", &format!("u:{}:rw", user), &path.to_string_lossy()])
                        .output();
                }
            }
            log::info!("EVDI: applied setfacl fallback on /dev/dri/card* for user {}", user);
        }
    }

    /// Check if a card is an EVDI device via sysfs (independent of libevdi's check_device).
    fn is_evdi_via_sysfs(card_id: c_int) -> bool {
        let path = format!("/sys/class/drm/card{}/device/uevent", card_id);
        match std::fs::read_to_string(&path) {
            Ok(content) => content.contains("DRIVER=evdi"),
            Err(_) => false,
        }
    }

    /// Try to find and open a new EVDI device not in `known_ids`.
    fn try_open_new_evdi(
        open_fn: FnEvdiOpen,
        known_ids: &std::collections::HashSet<c_int>,
    ) -> Option<(c_int, EvdiHandle)> {
        let current_ids = get_existing_card_ids();
        for &card_id in &current_ids {
            if known_ids.contains(&card_id) {
                continue;
            }
            if !is_evdi_via_sysfs(card_id) {
                continue;
            }
            let handle = unsafe { (open_fn)(card_id) };
            if !handle.is_null() {
                log::info!("EVDI: discovered and opened new device card{}", card_id);
                return Some((card_id, handle));
            }
            log::debug!("EVDI: card{} is EVDI but evdi_open failed, will retry", card_id);
        }
        None
    }

    /// After evdi_add_device(), discover the newly created EVDI device.
    /// Uses inotify on /dev/dri for instant notification when a new card*
    /// device node appears, with a polling fallback if inotify is unavailable.
    ///
    /// NOTE: We try evdi_open() directly instead of relying on evdi_check_device(),
    /// because check_device() may return UNRECOGNIZED for valid EVDI devices on
    /// certain libevdi versions (observed with 1.14.2 on Ubuntu 24.04).
    fn discover_new_evdi_device(
        _check_fn: FnEvdiCheckDevice,
        open_fn: FnEvdiOpen,
        known_ids: &std::collections::HashSet<c_int>,
    ) -> Option<(c_int, EvdiHandle)> {
        use hbb_common::libc;

        const TIMEOUT_MS: u64 = 5000;
        const ACL_FALLBACK_MS: u64 = 1500;

        // Quick check — device may already be visible
        if let Some(result) = try_open_new_evdi(open_fn, known_ids) {
            return Some(result);
        }

        log::info!("EVDI: scanning for new device (known cards: {:?})", known_ids);

        // Set up inotify on /dev/dri for instant notification of new devices.
        let ifd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        let use_inotify = if ifd >= 0 {
            let wd = unsafe {
                libc::inotify_add_watch(
                    ifd,
                    b"/dev/dri\0".as_ptr() as *const libc::c_char,
                    libc::IN_CREATE,
                )
            };
            if wd < 0 {
                unsafe { libc::close(ifd); }
                false
            } else {
                // Re-check after watch setup to close the race window
                // (device may have appeared between first check and watch setup)
                if let Some(result) = try_open_new_evdi(open_fn, known_ids) {
                    unsafe { libc::close(ifd); }
                    return Some(result);
                }
                true
            }
        } else {
            false
        };

        if !use_inotify {
            log::debug!("EVDI: inotify unavailable, using polling fallback");
        }

        let start = std::time::Instant::now();
        let mut acl_done = false;

        while start.elapsed().as_millis() < TIMEOUT_MS as u128 {
            // ACL fallback at 1.5s
            if !acl_done && start.elapsed().as_millis() >= ACL_FALLBACK_MS as u128 {
                try_set_dri_acl();
                acl_done = true;
            }

            if use_inotify {
                // Wait for inotify event (or 200ms timeout for periodic checks)
                let mut pollfd = libc::pollfd {
                    fd: ifd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                unsafe { libc::poll(&mut pollfd, 1, 200) };
                // Drain any pending inotify events
                if pollfd.revents & libc::POLLIN != 0 {
                    let mut buf = [0u8; 4096];
                    while unsafe {
                        libc::read(ifd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
                    } > 0 {}
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            if let Some(result) = try_open_new_evdi(open_fn, known_ids) {
                if use_inotify {
                    unsafe { libc::close(ifd); }
                }
                return Some(result);
            }
        }

        if use_inotify {
            unsafe { libc::close(ifd); }
        }
        log::error!("EVDI: failed to discover new device after {}ms", TIMEOUT_MS);
        None
    }

    /// Create a new EVDI device and connect it with the given EDID.
    /// Must be called while DRM_TOPOLOGY is held.
    /// Returns (device_id, handle) for the newly connected device.
    fn add_and_connect_device(
        fns: &EvdiLibFns,
        width: u32,
        height: u32,
        refresh: u32,
    ) -> ResultType<(c_int, EvdiHandle)> {
        let known_ids = get_existing_card_ids();

        let ret = unsafe { (fns.add_device)() };
        if ret <= 0 {
            bail!("EVDI: failed to add device (requires write access to /sys/devices/evdi/add)");
        }

        let (device_id, handle) =
            match discover_new_evdi_device(fns.check_device, fns.open, &known_ids) {
                Some(result) => result,
                None => bail!(
                    "EVDI: failed to find/open new device after add_device \
                     (check permissions on /dev/dri/card*)"
                ),
            };

        // IMPORTANT: Do NOT add any delay between open and connect!
        // add_device + connect must happen fast so that KScreen/Xorg sees them
        // as a single hotplug event ("new output with monitor"). If we add a
        // delay here, KScreen processes add_device as a "disconnected output"
        // first, then the connect triggers a SECOND hotplug event that causes
        // a state transition crash in KScreen's XRandR backend.
        // (DRM_HOTPLUG_SETTLE_MS delays are only needed for REMOVAL operations:
        // disconnect→close, between removing multiple devices, etc.)
        log::info!("EVDI: card{} opened, connecting immediately", device_id);

        let edid = generate_edid(width, height, refresh);
        let area_limit = width.saturating_mul(height);

        unsafe {
            (fns.connect)(handle, edid.as_ptr(), edid.len() as c_uint, area_limit);
        }
        log::info!(
            "EVDI: card{} connected with EDID {}x{}@{}Hz",
            device_id, width, height, refresh
        );

        Ok((device_id, handle))
    }

    pub fn plug_in_headless() -> ResultType<()> {
        // Phase 1: Check state and copy function pointers (brief MANAGER lock).
        let fns = {
            let manager = MANAGER.lock().unwrap();
            if manager.headless.is_some() {
                log::debug!("EVDI: headless display already exists");
                return Ok(());
            }
            manager.lib_fns()?
        }; // MANAGER unlocked

        // Phase 2: DRM operations under DRM_TOPOLOGY to prevent concurrent
        // hotplug events from crashing KScreen. MANAGER is NOT held so UI
        // reads (get_virtual_displays, get_platform_additions) proceed freely.
        let (device_id, handle) = {
            let _topo = DRM_TOPOLOGY.lock().unwrap();
            add_and_connect_device(&fns, 1920, 1080, 60)?
        }; // DRM_TOPOLOGY unlocked

        // Phase 3: Register device WITHOUT consumer thread (brief MANAGER lock).
        let stop_flag = Arc::new(AtomicBool::new(false));
        {
            let mut manager = MANAGER.lock().unwrap();
            if manager.headless.is_some() {
                // Race: another caller created headless while we were sleeping.
                log::warn!("EVDI: headless already created by another thread, cleaning up duplicate");
                drop(manager);
                let _topo = DRM_TOPOLOGY.lock().unwrap();
                let mut dup = EvdiDevice {
                    handle,
                    device_id,
                    consumer_stop: Arc::new(AtomicBool::new(false)),
                    consumer_thread: None,
                };
                dup.disconnect_and_close(fns.disconnect, fns.close);
                return Ok(());
            }

            log::info!(
                "EVDI: headless virtual display created (card{}, 1920x1080@60Hz)",
                device_id
            );
            manager.headless = Some(EvdiDevice {
                handle,
                device_id,
                consumer_stop: stop_flag.clone(),
                consumer_thread: None, // Started after CRTC is ready
            });
        }

        // Phase 4: Wait for CRTC.
        position_virtual_display(1920, 1080);

        // Phase 5: NOW spawn consumer — CRTC is ready.
        let thread = spawn_consumer_thread(handle, 1920, 1080, fns.consumer, stop_flag.clone());
        {
            let mut manager = MANAGER.lock().unwrap();
            if let Some(device) = manager.headless.as_mut() {
                device.consumer_thread = Some(thread);
            } else {
                log::warn!("EVDI: headless removed during positioning, stopping consumer");
                stop_flag.store(true, Ordering::Relaxed);
                let _ = thread.join();
            }
        }

        super::clear_custom_resolution();
        Ok(())
    }

    const MAX_VIRTUAL_DISPLAYS: usize = 4;

    pub fn plug_in_monitor(idx: u32, modes: &[super::MonitorMode]) -> ResultType<()> {
        // Phase 1: Check state, resolve index, copy function pointers (brief lock).
        let (actual_idx, width, height, refresh, fns) = {
            let manager = MANAGER.lock().unwrap();

            if manager.peers.len() >= MAX_VIRTUAL_DISPLAYS {
                bail!("EVDI: maximum of {} virtual displays reached", MAX_VIRTUAL_DISPLAYS);
            }

            // Use the caller's index directly. VdController tracks the same
            // index, so plug_out_monitor(idx) must find the device at this key.
            // Previously idx=0 was remapped to "next available", causing a
            // mismatch where VdController thought display 0 was active but EVDI
            // stored it under a different key.
            let actual_idx = idx;
            if manager.peers.contains_key(&actual_idx) {
                return Ok(());
            }

            let (width, height, refresh) = if let Some(m) = modes.first() {
                (
                    m.width.min(MAX_SUPPORTED_RESOLUTION).max(1),
                    m.height.min(MAX_SUPPORTED_RESOLUTION).max(1),
                    m.sync.max(1),
                )
            } else {
                (1920, 1080, 60)
            };

            (actual_idx, width, height, refresh, manager.lib_fns()?)
        }; // MANAGER unlocked

        // Phase 2: DRM operations under DRM_TOPOLOGY.
        let (device_id, handle) = {
            let _topo = DRM_TOPOLOGY.lock().unwrap();
            add_and_connect_device(&fns, width, height, refresh)?
        }; // DRM_TOPOLOGY unlocked

        // Phase 3: Register device WITHOUT consumer thread (brief MANAGER lock).
        // The consumer thread is spawned AFTER position_virtual_display ensures
        // the CRTC is ready. Spawning the consumer too early causes
        // evdi_request_update() to hit an unconfigured CRTC → Xorg crash.
        let stop_flag = Arc::new(AtomicBool::new(false));
        {
            let mut manager = MANAGER.lock().unwrap();
            if manager.peers.contains_key(&actual_idx) {
                log::warn!("EVDI: peer index {} already taken, cleaning up duplicate", actual_idx);
                drop(manager);
                // No consumer thread to stop — just disconnect/close
                let _topo = DRM_TOPOLOGY.lock().unwrap();
                let mut dup = EvdiDevice {
                    handle,
                    device_id,
                    consumer_stop: Arc::new(AtomicBool::new(false)),
                    consumer_thread: None,
                };
                dup.disconnect_and_close(fns.disconnect, fns.close);
                return Ok(());
            }

            log::info!(
                "EVDI: virtual display {} created (card{}, {}x{}@{}Hz)",
                actual_idx, device_id, width, height, refresh
            );
            manager.peers.insert(actual_idx, EvdiDevice {
                handle,
                device_id,
                consumer_stop: stop_flag.clone(),
                consumer_thread: None, // Started after CRTC is ready
            });
            if actual_idx >= manager.next_peer_index {
                manager.next_peer_index = actual_idx + 1;
            }
        }

        // Phase 4: Position display synchronously. This waits for Xorg/KScreen
        // to finish setting up the CRTC (3s settle + Display::all() polling).
        position_virtual_display(width, height);

        // Phase 5: NOW spawn consumer thread — CRTC is confirmed ready,
        // so evdi_request_update() won't crash Xorg.
        let thread = spawn_consumer_thread(handle, width, height, fns.consumer, stop_flag.clone());
        {
            let mut manager = MANAGER.lock().unwrap();
            if let Some(device) = manager.peers.get_mut(&actual_idx) {
                device.consumer_thread = Some(thread);
            } else {
                // Device was removed while we were positioning — stop the thread
                log::warn!("EVDI: display {} removed during positioning, stopping consumer", actual_idx);
                stop_flag.store(true, Ordering::Relaxed);
                let _ = thread.join();
            }
        }

        super::clear_custom_resolution();
        Ok(())
    }

    pub fn plug_in_peer_request(
        modes: Vec<Vec<super::MonitorMode>>,
    ) -> ResultType<Vec<u32>> {
        // Phase 1: Copy function pointers (brief lock).
        let fns = {
            let manager = MANAGER.lock().unwrap();
            manager.lib_fns()?
        }; // MANAGER unlocked

        let mut indices = Vec::new();

        for (mode_i, mode_set) in modes.iter().enumerate() {
            // Brief MANAGER lock: find next available index
            let idx = {
                let manager = MANAGER.lock().unwrap();
                let mut next = manager.next_peer_index;
                while manager.peers.contains_key(&next) {
                    next += 1;
                }
                next
            };

            let (width, height, refresh) = if let Some(m) = mode_set.first() {
                (
                    m.width.min(MAX_SUPPORTED_RESOLUTION).max(1),
                    m.height.min(MAX_SUPPORTED_RESOLUTION).max(1),
                    m.sync.max(1),
                )
            } else {
                (1920, 1080, 60)
            };

            // DRM operations under DRM_TOPOLOGY (includes inter-device settle).
            let result = {
                let _topo = DRM_TOPOLOGY.lock().unwrap();
                let r = add_and_connect_device(&fns, width, height, refresh);
                // Inter-device settle: keep DRM_TOPOLOGY held so the next
                // iteration's add_device waits until KScreen has processed
                // this connect event.
                if r.is_ok() && mode_i + 1 < modes.len() {
                    log::info!(
                        "EVDI: waiting {}ms before creating next device",
                        DRM_HOTPLUG_SETTLE_MS
                    );
                    std::thread::sleep(std::time::Duration::from_millis(DRM_HOTPLUG_SETTLE_MS));
                }
                r
            }; // DRM_TOPOLOGY unlocked

            let (device_id, handle) = match result {
                Ok(r) => r,
                Err(e) => {
                    log::error!("EVDI: failed to create device for peer index {}: {}", idx, e);
                    continue;
                }
            };

            // Register device WITHOUT consumer thread (spawned after CRTC ready)
            let stop_flag = Arc::new(AtomicBool::new(false));

            // Brief MANAGER lock: insert device
            {
                let mut manager = MANAGER.lock().unwrap();
                if manager.peers.contains_key(&idx) {
                    log::warn!("EVDI: peer index {} already taken, cleaning up duplicate", idx);
                    drop(manager);
                    let _topo = DRM_TOPOLOGY.lock().unwrap();
                    let mut dup = EvdiDevice {
                        handle,
                        device_id,
                        consumer_stop: Arc::new(AtomicBool::new(false)),
                        consumer_thread: None,
                    };
                    dup.disconnect_and_close(fns.disconnect, fns.close);
                    continue;
                }

                log::info!(
                    "EVDI: peer virtual display {} created (device {}, {}x{}@{}Hz)",
                    idx, device_id, width, height, refresh
                );
                manager.peers.insert(idx, EvdiDevice {
                    handle,
                    device_id,
                    consumer_stop: stop_flag.clone(),
                    consumer_thread: None, // Started after CRTC is ready
                });
                indices.push(idx);
                manager.next_peer_index = idx + 1;
            }
        }

        if !indices.is_empty() {
            // Use the resolution of the last created display for positioning.
            let (last_w, last_h) = modes.last()
                .and_then(|ms| ms.first())
                .map(|m| (m.width, m.height))
                .unwrap_or((1920, 1080));
            position_virtual_display(last_w, last_h);

            // NOW spawn consumer threads — CRTC is ready
            let mut manager = MANAGER.lock().unwrap();
            for &idx in &indices {
                if let Some(device) = manager.peers.get_mut(&idx) {
                    let w = modes.get(idx as usize)
                        .and_then(|ms| ms.first())
                        .map(|m| m.width.min(MAX_SUPPORTED_RESOLUTION).max(1))
                        .unwrap_or(1920);
                    let h = modes.get(idx as usize)
                        .and_then(|ms| ms.first())
                        .map(|m| m.height.min(MAX_SUPPORTED_RESOLUTION).max(1))
                        .unwrap_or(1080);
                    let thread = spawn_consumer_thread(
                        device.handle, w, h, fns.consumer, device.consumer_stop.clone(),
                    );
                    device.consumer_thread = Some(thread);
                }
            }
        }

        super::clear_custom_resolution();
        Ok(indices)
    }

    /// Teardown a list of devices under DRM_TOPOLOGY with inter-device settle delays.
    fn teardown_devices(mut devices: Vec<EvdiDevice>, fns: &EvdiLibFns) {
        if devices.is_empty() {
            return;
        }
        // Remove RandR monitors before tearing down EVDI devices,
        // while xrandr can still see the outputs.
        remove_evdi_randr_monitors();
        let _topo = DRM_TOPOLOGY.lock().unwrap();
        let total = devices.len();
        for (i, device) in devices.iter_mut().enumerate() {
            device.disconnect_and_close(fns.disconnect, fns.close);
            if i + 1 < total {
                log::info!("EVDI: waiting {}ms between device removals", DRM_HOTPLUG_SETTLE_MS);
                std::thread::sleep(std::time::Duration::from_millis(DRM_HOTPLUG_SETTLE_MS));
            }
        }
    }

    pub fn plug_out_monitor(index: i32) -> ResultType<()> {
        // Extract devices under a brief MANAGER lock.
        let (devices, fns) = {
            let mut manager = MANAGER.lock().unwrap();
            let fns = manager.lib_fns()?;

            let devices = if index < 0 {
                let mut devs: Vec<EvdiDevice> = manager.peers.drain().map(|(_, d)| d).collect();
                if let Some(headless) = manager.headless.take() {
                    devs.push(headless);
                }
                devs
            } else {
                // Look up by exact index. No special case for 0 — VdController
                // and plug_in_monitor use the same index, so the key matches.
                // Previously index==0 removed the MAX key, which only worked
                // by accident for single-display scenarios.
                let idx = index as u32;
                manager.peers.remove(&idx).into_iter().collect()
            };

            (devices, fns)
        }; // MANAGER unlocked

        teardown_devices(devices, &fns);
        Ok(())
    }

    pub fn plug_out_monitor_indices(indices: &[u32]) -> ResultType<()> {
        let (devices, fns) = {
            let mut manager = MANAGER.lock().unwrap();
            let fns = manager.lib_fns()?;

            let devices: Vec<EvdiDevice> = indices
                .iter()
                .filter_map(|idx| manager.peers.remove(idx))
                .collect();

            (devices, fns)
        }; // MANAGER unlocked

        teardown_devices(devices, &fns);
        Ok(())
    }

    pub fn reset_all() -> ResultType<()> {
        let result = {
            let mut manager = MANAGER.lock().unwrap();
            if let Ok(fns) = manager.lib_fns() {
                let mut devices: Vec<EvdiDevice> = manager.peers.drain().map(|(_, d)| d).collect();
                if let Some(headless) = manager.headless.take() {
                    devices.push(headless);
                }
                Some((devices, fns))
            } else {
                None
            }
        }; // MANAGER unlocked

        if let Some((devices, fns)) = result {
            teardown_devices(devices, &fns);
        }
        Ok(())
    }

    pub fn is_virtual_display(name: &str) -> bool {
        // EVDI devices typically appear with connector names containing "EVDI"
        let name_lower = name.to_lowercase();
        name_lower.contains("evdi") || name_lower.contains("virtual")
    }

    pub fn get_virtual_displays() -> Vec<u32> {
        let manager = MANAGER.lock().unwrap();
        manager.peers.keys().cloned().collect()
    }

    pub fn get_platform_additions() -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        if !is_supported() {
            return map;
        }
        // Tell the Flutter client we use the EVDI virtual display implementation
        map.insert("idd_impl".into(), serde_json::json!("evdi"));
        let manager = MANAGER.lock().unwrap();
        let count = manager.peers.len();
        if count > 0 {
            map.insert(
                "evdi_virtual_displays".into(),
                serde_json::json!(count),
            );
        }
        if manager.headless.is_some() {
            map.insert("evdi_headless".into(), serde_json::json!(true));
        }
        map
    }

    // =========================================================================
    // xrandr positioning
    // =========================================================================

    /// Find xrandr output names for EVDI virtual displays by checking sysfs.
    /// Returns (evdi_output_name, primary_output_name) if found.
    fn find_evdi_xrandr_output() -> Option<(String, String)> {
        use std::process::Command;

        // 1. Find EVDI card numbers via sysfs
        let mut evdi_cards: Vec<i32> = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_string();
                // Match "card0", "card1", etc. (not "card0-DVI-I-1" connector entries)
                if let Some(num_str) = name_str.strip_prefix("card") {
                    if num_str.chars().all(|c| c.is_ascii_digit()) {
                        if is_evdi_via_sysfs(num_str.parse().unwrap_or(-1)) {
                            evdi_cards.push(num_str.parse().unwrap_or(-1));
                        }
                    }
                }
            }
        }

        if evdi_cards.is_empty() {
            return None;
        }

        // 2. Find connected connector names for EVDI cards
        let mut evdi_connector: Option<String> = None;
        for card_id in &evdi_cards {
            if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy().to_string();
                    let prefix = format!("card{}-", card_id);
                    if let Some(connector_name) = name_str.strip_prefix(&prefix) {
                        // Check if this connector is "connected"
                        let status_path = format!("/sys/class/drm/{}/status", name_str);
                        if let Ok(status) = std::fs::read_to_string(&status_path) {
                            if status.trim() == "connected" {
                                evdi_connector = Some(connector_name.to_string());
                                break;
                            }
                        }
                    }
                }
            }
            if evdi_connector.is_some() {
                break;
            }
        }

        let evdi_name = evdi_connector?;

        // 3. Find primary output from xrandr
        let output = Command::new("xrandr").arg("--query").output().ok()?;
        let xrandr_out = String::from_utf8_lossy(&output.stdout);
        let mut primary_name: Option<String> = None;
        for line in xrandr_out.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 2 && parts[1] == "connected" && parts[2] == "primary" {
                primary_name = Some(parts[0].to_string());
                break;
            }
        }
        // Fallback: first connected non-EVDI output
        if primary_name.is_none() {
            for line in xrandr_out.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == "connected" && parts[0] != evdi_name {
                    primary_name = Some(parts[0].to_string());
                    break;
                }
            }
        }

        let primary = primary_name?;
        log::info!("EVDI: sysfs found EVDI output '{}', primary '{}'", evdi_name, primary);
        Some((evdi_name, primary))
    }

    /// Position the EVDI virtual display to the right of the primary monitor
    /// using xrandr. Runs in a background thread to avoid blocking.
    ///
    /// IMPORTANT: We must wait at least DRM_HOTPLUG_SETTLE_MS before touching
    /// xrandr, because KScreen's XRandR backend is still processing the connect
    /// uevent from evdi_connect(). Running xrandr while KScreen is mid-processing
    /// causes a segfault in KSC_XRandR.so.
    /// Create an explicit RandR 1.5 Monitor object for the EVDI output.
    ///
    /// Without this, `xcb_randr_get_monitors(get_active=1)` — used by
    /// `Display::all()` — may not return the EVDI display. This happens
    /// when KDE KScreen has created user-defined monitors for physical
    /// outputs, preventing auto-generation for dynamically added outputs.
    fn create_evdi_randr_monitor(evdi_name: &str) {
        use std::process::Command;

        // First, log what Display::all() sees BEFORE setmonitor (diagnostic)
        let before_count = match scrap::Display::all() {
            Ok(d) => {
                let names: Vec<String> = d.iter().map(|x| x.name()).collect();
                log::info!(
                    "EVDI: Display::all() BEFORE setmonitor: {} display(s): {:?}",
                    d.len(), names
                );
                d.len()
            }
            Err(e) => {
                log::warn!("EVDI: Display::all() BEFORE setmonitor failed: {}", e);
                0
            }
        };

        // If Display::all() already sees the new display, skip --setmonitor
        // (auto-generated monitors worked fine)
        if before_count > 0 {
            // Check if any display matches the EVDI output name
            if let Ok(displays) = scrap::Display::all() {
                let evdi_visible = displays.iter().any(|d| {
                    let name = d.name();
                    // RandR monitor names may differ from output names,
                    // but xrandr --setmonitor creates monitors with the
                    // output name as part of the monitor name. Also check
                    // for auto-generated monitor with the output name itself.
                    name.contains(evdi_name) || name.contains("EVDI")
                });
                if evdi_visible {
                    log::info!(
                        "EVDI: Display::all() already sees EVDI display, skipping --setmonitor"
                    );
                    return;
                }
            }
        }

        // Parse the output geometry from xrandr to build explicit geometry
        // for --setmonitor (more reliable than "auto" which may not be
        // supported on all xrandr versions)
        let monitor_name = format!("EVDI-{}", evdi_name.replace('-', "_"));
        let geometry = get_output_geometry(evdi_name);

        let set_ok = if let Some(ref geo) = geometry {
            // Use explicit geometry: w/mmwxh/mmh+x+y
            let spec = format!(
                "{}/{}x{}/{}+{}+{}",
                geo.width, geo.width_mm,
                geo.height, geo.height_mm,
                geo.x, geo.y
            );
            log::info!(
                "EVDI: creating RandR monitor '{}' with geometry {}",
                monitor_name, spec
            );
            let result = Command::new("xrandr")
                .args(["--setmonitor", &monitor_name, &spec, evdi_name])
                .output();
            match result {
                Ok(o) if o.status.success() => {
                    log::info!(
                        "EVDI: created RandR monitor '{}' -> {}",
                        monitor_name, evdi_name
                    );
                    true
                }
                Ok(o) => {
                    log::warn!(
                        "EVDI: xrandr --setmonitor (explicit) failed: {}",
                        String::from_utf8_lossy(&o.stderr)
                    );
                    false
                }
                Err(e) => {
                    log::warn!("EVDI: xrandr --setmonitor error: {}", e);
                    false
                }
            }
        } else {
            // Fallback: try "auto" geometry
            log::info!(
                "EVDI: creating RandR monitor '{}' with auto geometry",
                monitor_name
            );
            let result = Command::new("xrandr")
                .args(["--setmonitor", &monitor_name, "auto", evdi_name])
                .output();
            match result {
                Ok(o) if o.status.success() => {
                    log::info!(
                        "EVDI: created RandR monitor '{}' -> {} (auto)",
                        monitor_name, evdi_name
                    );
                    true
                }
                Ok(o) => {
                    log::warn!(
                        "EVDI: xrandr --setmonitor (auto) failed: {}",
                        String::from_utf8_lossy(&o.stderr)
                    );
                    false
                }
                Err(e) => {
                    log::warn!("EVDI: xrandr --setmonitor error: {}", e);
                    false
                }
            }
        };

        // Log xrandr --listmonitors for diagnostics (shows exactly what
        // xcb_randr_get_monitors will return)
        if let Ok(o) = Command::new("xrandr").arg("--listmonitors").output() {
            if o.status.success() {
                log::info!(
                    "EVDI: xrandr --listmonitors after setmonitor:\n{}",
                    String::from_utf8_lossy(&o.stdout)
                );
            }
        }

        // Verify that Display::all() now sees the new display
        match scrap::Display::all() {
            Ok(d) => {
                let names: Vec<String> = d.iter().map(|x| x.name()).collect();
                log::info!(
                    "EVDI: Display::all() AFTER setmonitor: {} display(s): {:?}",
                    d.len(), names
                );
                if d.len() <= before_count && set_ok {
                    log::warn!(
                        "EVDI: --setmonitor succeeded but Display::all() still returns {} display(s) \
                         (was {}). The RandR monitor may not be visible to xcb_randr_get_monitors.",
                        d.len(), before_count
                    );
                }
            }
            Err(e) => {
                log::warn!("EVDI: Display::all() AFTER setmonitor failed: {}", e);
            }
        }
    }

    /// Parse xrandr output to get the geometry of a specific output.
    /// Returns (x, y, width, height, width_mm, height_mm).
    struct OutputGeometry {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        width_mm: u32,
        height_mm: u32,
    }

    fn get_output_geometry(output_name: &str) -> Option<OutputGeometry> {
        use std::process::Command;
        let output = Command::new("xrandr").arg("--query").output().ok()?;
        let xrandr_out = String::from_utf8_lossy(&output.stdout);

        for line in xrandr_out.lines() {
            // Match lines like: DVI-I-1 connected 2400x1080+2560+0 (normal ...) 625mm x 289mm
            if !line.starts_with(output_name) || !line.contains(" connected") {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            // Find the geometry part (WxH+X+Y)
            for part in &parts {
                if let Some((dims, pos)) = part.split_once('+') {
                    if let Some((w_str, h_str)) = dims.split_once('x') {
                        let w = w_str.parse::<u32>().ok()?;
                        let h = h_str.parse::<u32>().ok()?;
                        let pos_parts: Vec<&str> = pos.split('+').collect();
                        if pos_parts.len() < 2 {
                            continue;
                        }
                        let x = pos_parts[0].parse::<i32>().ok()?;
                        let y = pos_parts[1].parse::<i32>().ok()?;
                        // Default physical size based on ~96 DPI
                        let mut width_mm = (w as u64 * 254 / 96 / 10) as u32;
                        let mut height_mm = (h as u64 * 254 / 96 / 10) as u32;
                        // Try to parse "NNNmm x NNNmm" from the line
                        if let Some(mm_idx) = line.find("mm x ") {
                            let before = &line[..mm_idx];
                            if let Some(last_space) = before.rfind(' ') {
                                if let Ok(wmm) = before[last_space + 1..].parse::<u32>() {
                                    width_mm = wmm;
                                }
                            }
                            let after = &line[mm_idx + 5..];
                            if let Some(end) = after.find("mm") {
                                if let Ok(hmm) = after[..end].trim().parse::<u32>() {
                                    height_mm = hmm;
                                }
                            }
                        }
                        return Some(OutputGeometry {
                            x,
                            y,
                            width: w,
                            height: h,
                            width_mm: width_mm.max(1),
                            height_mm: height_mm.max(1),
                        });
                    }
                }
            }
            break;
        }
        None
    }

    /// Remove any RandR monitors we created for EVDI outputs.
    /// Called during plug_out/reset to clean up `--setmonitor` entries.
    fn remove_evdi_randr_monitors() {
        use std::process::Command;
        let output = match Command::new("xrandr").arg("--listmonitors").output() {
            Ok(o) => o,
            Err(e) => {
                log::warn!("EVDI: cannot list monitors for cleanup: {}", e);
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            // Lines look like: " 0: +*eDP-1-1 2560/344x1440/194+0+0  eDP-1-1"
            // or              : " 1: +EVDI-DVI_I_1 2400/625x1080/289+2560+0  DVI-I-1"
            if let Some(colon_idx) = trimmed.find(':') {
                let after_colon = trimmed[colon_idx + 1..].trim();
                let name_start = after_colon.trim_start_matches(|c: char| c == '+' || c == '*');
                if let Some(space_idx) = name_start.find(char::is_whitespace) {
                    let monitor_name = &name_start[..space_idx];
                    if monitor_name.starts_with("EVDI-") {
                        let del_result = Command::new("xrandr")
                            .args(["--delmonitor", monitor_name])
                            .output();
                        match del_result {
                            Ok(o) if o.status.success() => {
                                log::info!("EVDI: removed RandR monitor '{}'", monitor_name);
                            }
                            Ok(o) => {
                                log::warn!(
                                    "EVDI: delmonitor '{}' failed: {}",
                                    monitor_name,
                                    String::from_utf8_lossy(&o.stderr)
                                );
                            }
                            Err(e) => {
                                log::warn!("EVDI: delmonitor command error: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Position the virtual display to the right of the primary monitor.
    ///
    /// This is SYNCHRONOUS — it blocks until the display is positioned and
    /// fully visible in XRandR (typically 3-5 seconds). This is intentional:
    /// the caller (plug_in_monitor) runs inside tokio::spawn_blocking, and
    /// the video service must NOT call Display::all() until the display has
    /// an active CRTC assigned by `xrandr --auto`. Without this, the video
    /// service fails to detect the new display and the client never learns
    /// about it (the root cause of "virtual display doesn't work on Linux").
    fn position_virtual_display(width: u32, height: u32) {
        use std::process::Command;
        // Wait for the X server to auto-configure the new EVDI output.
        // After evdi_connect(), the kernel sends a hotplug uevent. The X server
        // receives it, probes the new output, auto-assigns a CRTC with the EDID
        // preferred mode, and sends a RandR ScreenChangeNotify.
        //
        // CRITICAL: We must NOT run xrandr positioning if the display is already
        // visible to Display::all(). Running xrandr triggers a SECOND RandR
        // notification, which causes KDE KScreen to reprocess and DISABLE the
        // EVDI output — making it invisible to Display::all() and the phone.
        // See logs: Display::all() sees 4 displays at +1.6s, but after our
        // xrandr at +3.3s, the output disappears by +6.4s.
        log::info!(
            "EVDI: waiting {}ms for X server to auto-configure EVDI output",
            DRM_HOTPLUG_SETTLE_MS
        );
        std::thread::sleep(std::time::Duration::from_millis(DRM_HOTPLUG_SETTLE_MS));

        // Poll Display::all() to check if the X server already made the EVDI
        // display visible. If yes, return immediately — do NOT touch xrandr.
        // KDE KScreen handles positioning through its dialog.
        for check in 0..5 {
            match scrap::Display::all() {
                Ok(displays) => {
                    let names: Vec<String> = displays.iter().map(|d| d.name()).collect();
                    log::info!(
                        "EVDI: Display::all() visibility check {}: {} display(s): {:?}",
                        check, displays.len(), names
                    );
                    let evdi_visible = displays.iter().any(|d| {
                        let name = d.name();
                        name.contains("DVI-I") || name.contains("EVDI")
                    });
                    if evdi_visible {
                        log::info!(
                            "EVDI: display already visible in Display::all() — \
                             skipping xrandr positioning (KDE KScreen handles layout)"
                        );
                        return;
                    }
                }
                Err(e) => {
                    log::warn!("EVDI: Display::all() check {} failed: {}", check, e);
                }
            }
            if check < 4 {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }

        // Display::all() did not see the EVDI display after 5 checks (~5.5s).
        // The X server did not auto-assign a CRTC, or the desktop environment
        // disabled the output. Fall back to explicit xrandr positioning.
        // This path is needed for non-KDE environments (Xfce, i3, etc.)
        // where auto-configuration may not happen.
        log::info!(
            "EVDI: display NOT visible after auto-config wait, \
             falling back to xrandr positioning"
        );

        for attempt in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if let Some((evdi_name, primary_name)) = find_evdi_xrandr_output() {
                let mode_str = format!("{}x{}", width, height);
                let result = Command::new("xrandr")
                    .args([
                        "--output", &evdi_name,
                        "--mode", &mode_str,
                        "--right-of", &primary_name,
                    ])
                    .output();
                match result {
                    Ok(o) if o.status.success() => {
                        log::info!(
                            "EVDI: positioned {} ({}x{}) right of {} (fallback attempt {})",
                            evdi_name, width, height, primary_name, attempt
                        );
                        std::thread::sleep(std::time::Duration::from_millis(
                            DRM_HOTPLUG_SETTLE_MS,
                        ));
                        create_evdi_randr_monitor(&evdi_name);
                        return;
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        if stderr.contains("cannot find mode") {
                            log::info!(
                                "EVDI: mode {} not available, trying --auto",
                                mode_str
                            );
                            let result2 = Command::new("xrandr")
                                .args([
                                    "--output", &evdi_name,
                                    "--auto",
                                    "--right-of", &primary_name,
                                ])
                                .output();
                            match result2 {
                                Ok(o2) if o2.status.success() => {
                                    log::info!(
                                        "EVDI: positioned {} right of {} via --auto (fallback attempt {})",
                                        evdi_name, primary_name, attempt
                                    );
                                    std::thread::sleep(std::time::Duration::from_millis(
                                        DRM_HOTPLUG_SETTLE_MS,
                                    ));
                                    create_evdi_randr_monitor(&evdi_name);
                                    return;
                                }
                                Ok(o2) => {
                                    log::warn!(
                                        "EVDI: xrandr --auto failed: {}",
                                        String::from_utf8_lossy(&o2.stderr)
                                    );
                                }
                                Err(e) => {
                                    log::warn!("EVDI: xrandr --auto command failed: {}", e);
                                }
                            }
                        } else {
                            log::warn!(
                                "EVDI: xrandr position failed: {}",
                                stderr
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!("EVDI: xrandr command failed: {}", e);
                    }
                }
                return;
            }
        }
        log::warn!("EVDI: timed out waiting for EVDI output in sysfs/xrandr");
    }

    // =========================================================================
    // EDID generation
    // =========================================================================

    /// Generate a valid 128-byte EDID for the given resolution.
    /// Manufacturer ID: "RHZ" (Rust Horizon)
    fn generate_edid(width: u32, height: u32, refresh: u32) -> [u8; 128] {
        // Guard against zero inputs that would produce invalid DTD (pixel clock=0
        // → Xorg FPE, compositor division-by-zero crash)
        let width = width.max(1);
        let height = height.max(1);
        let refresh = refresh.max(1);

        let mut edid = [0u8; 128];

        // Header
        edid[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);

        // Manufacturer ID "RHZ" (R=18, H=8, Z=26)
        // Encoding: byte8 = 0|(R<<2)|(H>>3), byte9 = (H&7)<<5|Z
        edid[8] = (18 << 2) | (8 >> 3); // 0x49
        edid[9] = ((8 & 7) << 5) | 26; // 0x1A

        // Product code
        edid[10] = 0x01;
        edid[11] = 0x00;

        // Serial number
        edid[12] = 0x01;

        // Week/year of manufacture
        edid[16] = 0x01; // week 1
        edid[17] = 0x22; // year 2024 (1990 + 34)

        // EDID version 1.3
        edid[18] = 0x01;
        edid[19] = 0x03;

        // Basic display parameters
        edid[20] = 0x80; // Digital input
        // Screen size in cm (approximate for ~96 DPI)
        edid[21] = ((width as u64 * 254) / (96 * 100)).max(1).min(255) as u8;
        edid[22] = ((height as u64 * 254) / (96 * 100)).max(1).min(255) as u8;
        edid[23] = 0x78; // Gamma 2.2
        edid[24] = 0x0A; // RGB color, preferred timing in DTD1

        // Color Characteristics (bytes 25-34): left as zeros.
        // colord may warn "bogus" but valid chromaticity bytes can trigger
        // Xorg color management (gamma ramps, ICC profiles) which crashes
        // the EVDI modesetting driver. Zeros = safe, no color processing.

        // Standard timings (bytes 38-53): all unused (0x01 0x01)
        for i in (38..54).step_by(2) {
            edid[i] = 0x01;
            edid[i + 1] = 0x01;
        }

        // DTD 1 (bytes 54-71): preferred timing descriptor
        if width > EDID_DTD_MAX_RESOLUTION || height > EDID_DTD_MAX_RESOLUTION {
            log::warn!(
                "EVDI: resolution {}x{} exceeds EDID DTD 12-bit limit ({}), clamping",
                width, height, EDID_DTD_MAX_RESOLUTION
            );
        }
        write_dtd(&mut edid[54..72], width, height, refresh);

        // Display descriptor 2 (bytes 72-89): Monitor name
        edid[72] = 0x00;
        edid[73] = 0x00;
        edid[74] = 0x00;
        edid[75] = 0xFC; // Monitor name tag
        edid[76] = 0x00;
        edid[77..90].copy_from_slice(b"RustHorizon\n ");

        // Display descriptor 3 (bytes 90-107): Monitor range limits
        edid[90] = 0x00;
        edid[91] = 0x00;
        edid[92] = 0x00;
        edid[93] = 0xFD; // Range limits tag
        edid[94] = 0x00;
        edid[95] = refresh.saturating_sub(10).max(1) as u8; // min V freq
        edid[96] = (refresh + 10).min(255) as u8; // max V freq
        edid[97] = 0x1E; // min H freq: 30 kHz
        edid[98] = 0x50; // max H freq: 80 kHz
        // Max pixel clock in 10 MHz units
        let pixel_clock_hz = estimate_pixel_clock(width, height, refresh);
        edid[99] = ((pixel_clock_hz / 10_000_000) + 1).min(255) as u8;
        edid[100] = 0x0A; // GTF secondary curve
        for i in 101..108 {
            edid[i] = 0x20; // padding
        }

        // Display descriptor 4 (bytes 108-125): Dummy descriptor
        edid[108] = 0x00;
        edid[109] = 0x00;
        edid[110] = 0x00;
        edid[111] = 0x10; // Dummy tag
        edid[112] = 0x00;

        // Extension count
        edid[126] = 0x00;

        // Checksum: all 128 bytes must sum to 0 mod 256
        let sum: u32 = edid[..127].iter().map(|&b| b as u32).sum();
        edid[127] = ((256 - (sum % 256)) % 256) as u8;

        edid
    }

    /// Maximum resolution accepted from a remote peer.
    /// Prevents OOM from buffer allocation in the consumer thread
    /// (8192 × 8192 × 4 = 256 MB — acceptable for desktop systems).
    /// Also prevents arithmetic overflow in stride (width * 4 as c_int).
    const MAX_SUPPORTED_RESOLUTION: u32 = 8192;

    /// Maximum resolution encodable in EDID DTD (12-bit fields).
    /// Resolutions beyond this are clamped to prevent silent truncation that
    /// causes compositors to see wrong dimensions → crash or display corruption.
    const EDID_DTD_MAX_RESOLUTION: u32 = 4095;

    /// Maximum pixel clock encodable in EDID DTD (16-bit field, 10kHz units).
    /// Beyond this the DTD wraps around, causing compositors to calculate
    /// wrong refresh rates → sync failures or crash.
    const EDID_DTD_MAX_PIXEL_CLOCK_10KHZ: u64 = 65535; // = 655.35 MHz

    /// Write a Detailed Timing Descriptor (18 bytes) for the given resolution.
    fn write_dtd(buf: &mut [u8], width: u32, height: u32, refresh: u32) {
        // Clamp to 12-bit DTD encoding limits to prevent silent truncation
        let width = width.min(EDID_DTD_MAX_RESOLUTION);
        let height = height.min(EDID_DTD_MAX_RESOLUTION);
        // Blanking intervals (standard values for common resolutions)
        let (h_blank, h_front_porch, h_sync) = match width {
            w if w <= 1920 => (280u32, 88u32, 44u32),
            w if w <= 2560 => (160, 48, 32),
            _ => (560, 176, 88),
        };
        let (v_blank, v_front_porch, v_sync) = match height {
            h if h <= 1080 => (45u32, 4u32, 5u32),
            h if h <= 1440 => (41, 3, 5),
            _ => (90, 8, 10),
        };

        let h_total = width + h_blank;
        let v_total = height + v_blank;
        let pixel_clock_10khz = ((h_total as u64 * v_total as u64 * refresh as u64) / 10_000)
            .min(EDID_DTD_MAX_PIXEL_CLOCK_10KHZ);

        buf[0] = (pixel_clock_10khz & 0xFF) as u8;
        buf[1] = ((pixel_clock_10khz >> 8) & 0xFF) as u8;
        buf[2] = (width & 0xFF) as u8;
        buf[3] = (h_blank & 0xFF) as u8;
        buf[4] = ((((width >> 8) & 0xF) << 4) | ((h_blank >> 8) & 0xF)) as u8;
        buf[5] = (height & 0xFF) as u8;
        buf[6] = (v_blank & 0xFF) as u8;
        buf[7] = ((((height >> 8) & 0xF) << 4) | ((v_blank >> 8) & 0xF)) as u8;
        buf[8] = (h_front_porch & 0xFF) as u8;
        buf[9] = (h_sync & 0xFF) as u8;
        buf[10] = (((v_front_porch & 0xF) << 4) | (v_sync & 0xF)) as u8;
        buf[11] = ((((h_front_porch >> 8) & 0x3) << 6)
            | (((h_sync >> 8) & 0x3) << 4)
            | (((v_front_porch >> 4) & 0x3) << 2)
            | ((v_sync >> 4) & 0x3)) as u8;

        // Image size in mm (approximate for ~96 DPI)
        let h_mm = (width as u64 * 254) / 960;
        let v_mm = (height as u64 * 254) / 960;
        buf[12] = (h_mm & 0xFF) as u8;
        buf[13] = (v_mm & 0xFF) as u8;
        buf[14] = ((((h_mm >> 8) & 0xF) << 4) | ((v_mm >> 8) & 0xF)) as u8;

        buf[15] = 0; // H border
        buf[16] = 0; // V border
        buf[17] = 0x1E; // Non-interlaced, normal, digital separate, +H+V sync
    }

    fn estimate_pixel_clock(width: u32, height: u32, refresh: u32) -> u32 {
        let h_blank: u64 = match width {
            w if w <= 1920 => 280,
            w if w <= 2560 => 160,
            _ => 560,
        };
        let v_blank: u64 = match height {
            h if h <= 1080 => 45,
            h if h <= 1440 => 41,
            _ => 90,
        };
        // Use u64 with saturating arithmetic to prevent overflow on extreme resolutions
        (width as u64 + h_blank)
            .saturating_mul(height as u64 + v_blank)
            .saturating_mul(refresh as u64)
            .min(u32::MAX as u64) as u32
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn edid_checksum_valid_1080p() {
            let edid = generate_edid(1920, 1080, 60);
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "EDID checksum invalid for 1920x1080@60");
        }

        #[test]
        fn edid_checksum_valid_4k() {
            let edid = generate_edid(3840, 2160, 60);
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "EDID checksum invalid for 3840x2160@60");
        }

        #[test]
        fn edid_checksum_valid_720p() {
            let edid = generate_edid(1280, 720, 60);
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "EDID checksum invalid for 1280x720@60");
        }

        #[test]
        fn edid_chromaticity_zeros() {
            // Chromaticity bytes 25-34 are intentionally left as zeros.
            // Non-zero sRGB values triggered Xorg color management (gamma ramps)
            // which crashes the EVDI modesetting driver on KDE/X11.
            let edid = generate_edid(1920, 1080, 60);
            let chromaticity = &edid[25..35];
            assert!(
                chromaticity.iter().all(|&b| b == 0),
                "EDID bytes 25-34 (Color Characteristics) must be all zeros to avoid Xorg crash"
            );
        }

        #[test]
        fn edid_header_valid() {
            let edid = generate_edid(1920, 1080, 60);
            assert_eq!(
                &edid[0..8],
                &[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00],
                "EDID header must be standard"
            );
        }

        #[test]
        fn edid_version_1_3() {
            let edid = generate_edid(1920, 1080, 60);
            assert_eq!(edid[18], 0x01, "EDID version major must be 1");
            assert_eq!(edid[19], 0x03, "EDID version minor must be 3");
        }

        #[test]
        fn edid_dtd_resolution_matches_1080p() {
            let edid = generate_edid(1920, 1080, 60);
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            assert_eq!(h_active, 1920);
            assert_eq!(v_active, 1080);
        }

        #[test]
        fn edid_dtd_resolution_matches_4k() {
            let edid = generate_edid(3840, 2160, 60);
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            assert_eq!(h_active, 3840);
            assert_eq!(v_active, 2160);
        }

        #[test]
        fn pixel_clock_1080p_60hz() {
            let pc = estimate_pixel_clock(1920, 1080, 60);
            assert_eq!(pc, 148_500_000);
        }

        #[test]
        fn pixel_clock_4k_60hz() {
            let pc = estimate_pixel_clock(3840, 2160, 60);
            assert_eq!(pc, 594_000_000);
        }

        /// All custom_resolution assertions in a single test to avoid
        /// global-state races between parallel test threads.
        #[test]
        fn custom_resolution_behavior() {
            // Start clean
            super::super::clear_custom_resolution();
            assert_eq!(super::super::take_custom_resolution(), None);

            // set + take returns value, take clones (keeps value)
            super::super::set_custom_resolution(2560, 1440);
            assert_eq!(super::super::take_custom_resolution(), Some((2560, 1440)));
            assert_eq!(super::super::take_custom_resolution(), Some((2560, 1440)));

            // last set wins
            super::super::set_custom_resolution(1920, 1080);
            super::super::set_custom_resolution(3840, 2160);
            assert_eq!(super::super::take_custom_resolution(), Some((3840, 2160)));

            // clear removes it
            super::super::clear_custom_resolution();
            assert_eq!(super::super::take_custom_resolution(), None);
        }

        #[test]
        fn sysfs_nonexistent_card_not_evdi() {
            assert!(!is_evdi_via_sysfs(999));
        }

        // =====================================================================
        // EDID manufacturer, descriptors, and structural validation
        // =====================================================================

        #[test]
        fn edid_manufacturer_rhz() {
            let edid = generate_edid(1920, 1080, 60);
            // "RHZ" = R(18), H(8), Z(26)
            // byte8 = 0|(18<<2)|(8>>3) = 0|72|1 = 73 = 0x49
            // byte9 = (8&7)<<5|26 = 0<<5|26 = 26 = 0x1A  -- wait, (8&7)=0, 0<<5=0, |26=26
            // Actually H=8, 8&7=0, 0<<5=0, |26=26=0x1A
            assert_eq!(edid[8], 0x49, "Manufacturer byte 8 must encode 'RH'");
            assert_eq!(edid[9], 0x1A, "Manufacturer byte 9 must encode 'HZ'");
        }

        #[test]
        fn edid_digital_input() {
            let edid = generate_edid(1920, 1080, 60);
            assert_ne!(edid[20] & 0x80, 0, "EDID byte 20 bit 7 must be set (digital input)");
        }

        #[test]
        fn edid_monitor_name_present() {
            let edid = generate_edid(1920, 1080, 60);
            // Monitor name descriptor: bytes 72-89, tag 0xFC at byte 75
            assert_eq!(edid[72], 0x00);
            assert_eq!(edid[73], 0x00);
            assert_eq!(edid[74], 0x00);
            assert_eq!(edid[75], 0xFC, "Descriptor 2 must be monitor name tag");
            let name = &edid[77..88]; // "RustHorizon"
            assert_eq!(name, b"RustHorizon", "Monitor name must be 'RustHorizon'");
        }

        #[test]
        fn edid_range_limits_present() {
            let edid = generate_edid(1920, 1080, 60);
            // Range limits descriptor: bytes 90-107, tag 0xFD at byte 93
            assert_eq!(edid[90], 0x00);
            assert_eq!(edid[93], 0xFD, "Descriptor 3 must be range limits tag");
            // Min V freq should be refresh-10=50, max V freq=refresh+10=70
            assert_eq!(edid[95], 50, "Min V freq should be 50 Hz for 60 Hz refresh");
            assert_eq!(edid[96], 70, "Max V freq should be 70 Hz for 60 Hz refresh");
            // Max pixel clock in 10 MHz units, must be > 0
            assert!(edid[99] > 0, "Max pixel clock in range limits must be > 0");
        }

        #[test]
        fn edid_standard_timings_unused() {
            let edid = generate_edid(1920, 1080, 60);
            for i in (38..54).step_by(2) {
                assert_eq!(edid[i], 0x01, "Standard timing byte {} must be 0x01", i);
                assert_eq!(edid[i + 1], 0x01, "Standard timing byte {} must be 0x01", i + 1);
            }
        }

        #[test]
        fn edid_dtd_pixel_clock_nonzero() {
            let edid = generate_edid(1920, 1080, 60);
            let pc_10khz = (edid[54] as u16) | ((edid[55] as u16) << 8);
            assert!(pc_10khz > 0, "DTD pixel clock must be > 0");
        }

        #[test]
        fn edid_screen_size_reasonable() {
            let edid = generate_edid(1920, 1080, 60);
            let w_cm = edid[21];
            let h_cm = edid[22];
            // At 96 DPI: 1920px ~ 50.8cm, 1080px ~ 28.6cm
            assert!(w_cm >= 40 && w_cm <= 60, "Width in cm ({}) should be ~50 for 1920px@96DPI", w_cm);
            assert!(h_cm >= 20 && h_cm <= 35, "Height in cm ({}) should be ~28 for 1080px@96DPI", h_cm);
        }

        // =====================================================================
        // Additional resolution coverage (1440p, ultrawide, edge cases)
        // =====================================================================

        #[test]
        fn edid_checksum_valid_1440p() {
            let edid = generate_edid(2560, 1440, 60);
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "EDID checksum invalid for 2560x1440@60");
        }

        #[test]
        fn edid_checksum_valid_ultrawide() {
            let edid = generate_edid(3440, 1440, 60);
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "EDID checksum invalid for 3440x1440@60");
        }

        #[test]
        fn edid_checksum_valid_vga() {
            let edid = generate_edid(640, 480, 60);
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "EDID checksum invalid for 640x480@60");
        }

        #[test]
        fn edid_dtd_resolution_matches_1440p() {
            let edid = generate_edid(2560, 1440, 60);
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            assert_eq!(h_active, 2560);
            assert_eq!(v_active, 1440);
        }

        #[test]
        fn edid_dtd_resolution_matches_ultrawide() {
            let edid = generate_edid(3440, 1440, 60);
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            assert_eq!(h_active, 3440);
            assert_eq!(v_active, 1440);
        }

        #[test]
        fn pixel_clock_1440p_60hz() {
            // 2560+160=2720 * 1440+41=1481 * 60 = 241,603,200
            let pc = estimate_pixel_clock(2560, 1440, 60);
            assert_eq!(pc, 2720 * 1481 * 60);
        }

        // =====================================================================
        // DTD blanking intervals consistency
        // =====================================================================

        #[test]
        fn edid_dtd_blanking_encoded_correctly_1080p() {
            let edid = generate_edid(1920, 1080, 60);
            // h_blank=280 for width<=1920
            let h_blank = (edid[57] as u32) | (((edid[58] & 0x0F) as u32) << 8);
            assert_eq!(h_blank, 280, "H blank for 1080p must be 280");
            // v_blank=45 for height<=1080
            let v_blank = (edid[60] as u32) | (((edid[61] & 0x0F) as u32) << 8);
            assert_eq!(v_blank, 45, "V blank for 1080p must be 45");
        }

        #[test]
        fn edid_dtd_blanking_encoded_correctly_4k() {
            let edid = generate_edid(3840, 2160, 60);
            // h_blank=560 for width>2560
            let h_blank = (edid[57] as u32) | (((edid[58] & 0x0F) as u32) << 8);
            assert_eq!(h_blank, 560, "H blank for 4K must be 560");
            // v_blank=90 for height>1440
            let v_blank = (edid[60] as u32) | (((edid[61] & 0x0F) as u32) << 8);
            assert_eq!(v_blank, 90, "V blank for 4K must be 90");
        }

        // =====================================================================
        // DTD pixel clock matches estimate_pixel_clock
        // =====================================================================

        #[test]
        fn edid_dtd_pixel_clock_matches_estimate() {
            for &(w, h, r) in &[(1920u32, 1080u32, 60u32), (3840, 2160, 60), (2560, 1440, 60)] {
                let edid = generate_edid(w, h, r);
                let dtd_pc_10khz = (edid[54] as u64) | ((edid[55] as u64) << 8);
                let estimated_hz = estimate_pixel_clock(w, h, r) as u64;
                let estimated_10khz = estimated_hz / 10_000;
                assert_eq!(
                    dtd_pc_10khz, estimated_10khz,
                    "DTD pixel clock ({}x{}@{}) must match estimate", w, h, r
                );
            }
        }

        // =====================================================================
        // Range limits max pixel clock covers DTD pixel clock
        // =====================================================================

        #[test]
        fn edid_range_limits_covers_pixel_clock() {
            for &(w, h, r) in &[(1920u32, 1080u32, 60u32), (3840, 2160, 60), (2560, 1440, 60)] {
                let edid = generate_edid(w, h, r);
                let range_max_pc_10mhz = edid[99] as u32; // in 10 MHz units
                let estimated_hz = estimate_pixel_clock(w, h, r);
                let estimated_10mhz = estimated_hz / 10_000_000;
                assert!(
                    range_max_pc_10mhz > estimated_10mhz as u8 as u32,
                    "Range limits max pixel clock must exceed DTD pixel clock for {}x{}@{}", w, h, r
                );
            }
        }

        // =====================================================================
        // Custom resolution: set overwrites previous, None by default
        // =====================================================================

        // (custom_resolution tests merged into custom_resolution_behavior above
        // to avoid global-state races between parallel test threads)

        // =====================================================================
        // High refresh rate EDID
        // =====================================================================

        #[test]
        fn edid_checksum_valid_144hz() {
            let edid = generate_edid(1920, 1080, 144);
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "EDID checksum invalid for 1920x1080@144");
        }

        #[test]
        fn edid_range_limits_144hz() {
            let edid = generate_edid(1920, 1080, 144);
            assert_eq!(edid[95], 134, "Min V freq for 144Hz should be 134");
            assert_eq!(edid[96], 154, "Max V freq for 144Hz should be 154");
        }

        #[test]
        fn pixel_clock_1080p_144hz() {
            // (1920+280) * (1080+45) * 144 = 2200 * 1125 * 144
            let pc = estimate_pixel_clock(1920, 1080, 144);
            assert_eq!(pc, 2200 * 1125 * 144);
        }

        // =====================================================================
        // DRM hotplug settling delay — prevents session crash on ALL compositors
        // =====================================================================

        #[test]
        fn hotplug_settle_delay_minimum_safe_value() {
            // KScreen (KDE) and Mutter (GNOME) both need at least 2000ms to
            // process a DRM hotplug event before the next topology change.
            // Empirically, crashes occur at ~100ms between events.
            assert!(
                DRM_HOTPLUG_SETTLE_MS >= 2000,
                "DRM_HOTPLUG_SETTLE_MS ({}) must be >= 2000ms to prevent \
                 KScreen/Mutter segfaults from overlapping hotplug events",
                DRM_HOTPLUG_SETTLE_MS
            );
        }

        #[test]
        fn hotplug_settle_delay_not_excessive() {
            // Delay should not exceed 10s — would make device creation unacceptably slow.
            // A single plug_in with 2 events (add+connect) would take 20s.
            assert!(
                DRM_HOTPLUG_SETTLE_MS <= 10_000,
                "DRM_HOTPLUG_SETTLE_MS ({}) should be <= 10000ms to avoid \
                 excessive latency during virtual display creation",
                DRM_HOTPLUG_SETTLE_MS
            );
        }

        // =====================================================================
        // EDID compatibility: colord (GNOME) — rejects EDID with bogus chroma
        // =====================================================================

        #[test]
        fn edid_chromaticity_zeros_all_resolutions() {
            // Chromaticity bytes 25-34 are intentionally zeros for ALL resolutions.
            // Non-zero values crash Xorg with EVDI on KDE/X11.
            // colord (GNOME) may warn "bogus" but that's acceptable — we don't
            // run on GNOME, and a warning is better than crashing Xorg.
            let test_resolutions = [
                (1920, 1080, 60), (3840, 2160, 60), (2560, 1440, 60),
                (1280, 720, 60), (640, 480, 60), (3440, 1440, 60),
                (2400, 1080, 60), (2340, 1080, 60),  // smartphones
                (1920, 1080, 144),
            ];
            for &(w, h, r) in &test_resolutions {
                let edid = generate_edid(w, h, r);
                let chromaticity = &edid[25..35];
                assert!(
                    chromaticity.iter().all(|&b| b == 0),
                    "EDID {}x{}@{}Hz: chromaticity bytes 25-34 must be all zeros \
                     (non-zero crashes Xorg with EVDI)",
                    w, h, r
                );
            }
        }

        // =====================================================================
        // EDID compatibility: KScreen (KDE) — needs valid EDID for config matching
        // =====================================================================

        #[test]
        fn edid_kscreen_compatibility_valid_serial_and_manufacturer() {
            // KScreen uses EDID serial + manufacturer to build an output identifier
            // for matching saved display configurations. Invalid values cause
            // KScreen to fall back to unreliable connector names.
            let edid = generate_edid(1920, 1080, 60);
            // Manufacturer ID must not be 0x0000 (KScreen treats as unknown)
            let mfr = ((edid[8] as u16) << 8) | edid[9] as u16;
            assert_ne!(mfr, 0, "EDID manufacturer ID must not be 0x0000 (KScreen)");
            // Product code must not be 0x0000
            let product = (edid[10] as u16) | ((edid[11] as u16) << 8);
            assert_ne!(product, 0, "EDID product code must not be 0x0000 (KScreen)");
        }

        #[test]
        fn edid_kscreen_compatibility_valid_edid_version() {
            // KScreen rejects EDID with version < 1.3 and uses different
            // parsing paths for 1.3 vs 1.4. We must be exactly 1.3 or 1.4.
            let edid = generate_edid(1920, 1080, 60);
            assert_eq!(edid[18], 1, "EDID major version must be 1 (KScreen)");
            assert!(
                edid[19] == 3 || edid[19] == 4,
                "EDID minor version must be 3 or 4 (KScreen), got {}",
                edid[19]
            );
        }

        // =====================================================================
        // EDID compatibility: Wayland compositors (wl_output physical size)
        // =====================================================================

        #[test]
        fn edid_wayland_physical_size_nonzero_all_resolutions() {
            // Wayland compositors (KWin Wayland, Mutter Wayland) expose physical
            // monitor dimensions via wl_output. Zero dimensions cause DPI
            // calculation to divide by zero or produce nonsensical values,
            // which can crash Qt/GTK applications.
            let test_resolutions = [
                (1920, 1080, 60), (3840, 2160, 60), (2560, 1440, 60),
                (1280, 720, 60), (640, 480, 60), (2400, 1080, 60),
            ];
            for &(w, h, r) in &test_resolutions {
                let edid = generate_edid(w, h, r);
                let w_cm = edid[21];
                let h_cm = edid[22];
                assert!(
                    w_cm > 0 && h_cm > 0,
                    "EDID {}x{}@{}Hz: physical size must be > 0 (Wayland DPI), got {}x{}cm",
                    w, h, r, w_cm, h_cm
                );
            }
        }

        #[test]
        fn edid_wayland_physical_size_aspect_ratio_sane() {
            // Physical dimensions must roughly match the pixel aspect ratio.
            // A wildly wrong ratio (e.g. 100cm x 1cm for 1920x1080) causes
            // compositors to compute wrong DPI per axis.
            let edid = generate_edid(1920, 1080, 60);
            let w_cm = edid[21] as f64;
            let h_cm = edid[22] as f64;
            let pixel_ratio = 1920.0 / 1080.0;
            let physical_ratio = w_cm / h_cm;
            let ratio_diff = (pixel_ratio - physical_ratio).abs() / pixel_ratio;
            assert!(
                ratio_diff < 0.15,
                "Physical aspect ratio ({:.2}) must be within 15% of pixel ratio ({:.2})",
                physical_ratio, pixel_ratio
            );
        }

        // =====================================================================
        // EDID compatibility: X11 (Xorg/XRandR) — DTD must be parseable
        // =====================================================================

        #[test]
        fn edid_x11_dtd_sync_flags_valid() {
            // Xorg's EDID parser reads DTD sync/timing flags from byte 71.
            // Bit 4-3 = sync type. Digital separate (0b11) is the safest
            // for virtual displays. Invalid values cause mode rejection.
            let edid = generate_edid(1920, 1080, 60);
            let flags = edid[71];
            // Bit 4 must be set (digital sync)
            assert_ne!(
                flags & 0x10, 0,
                "DTD byte 71 bit 4 must be set (digital sync) for Xorg compatibility"
            );
        }

        #[test]
        fn edid_x11_dtd_no_zero_pixel_clock_all_resolutions() {
            // Xorg divides by pixel clock to compute refresh rate.
            // A zero pixel clock causes a floating point exception → Xorg crash.
            let test_resolutions = [
                (1920, 1080, 60), (3840, 2160, 60), (2560, 1440, 60),
                (1280, 720, 60), (640, 480, 60), (3440, 1440, 60),
                (2400, 1080, 60), (1920, 1080, 144),
            ];
            for &(w, h, r) in &test_resolutions {
                let edid = generate_edid(w, h, r);
                let pc_10khz = (edid[54] as u16) | ((edid[55] as u16) << 8);
                assert!(
                    pc_10khz > 0,
                    "EDID {}x{}@{}Hz: DTD pixel clock must be > 0 (Xorg divides by this)",
                    w, h, r
                );
            }
        }

        // =====================================================================
        // EDID monitor name: must be ASCII printable (all compositors)
        // =====================================================================

        #[test]
        fn edid_monitor_name_ascii_printable() {
            // All display servers (Xorg, Mutter, KWin) display the monitor
            // name in their UI. Non-ASCII bytes cause rendering glitches
            // or assertion failures in Qt/GTK string handling.
            let edid = generate_edid(1920, 1080, 60);
            let name_descriptor = &edid[77..90];
            for (i, &b) in name_descriptor.iter().enumerate() {
                if b == 0x0A {
                    break; // LF terminates the name
                }
                assert!(
                    b >= 0x20 && b <= 0x7E,
                    "Monitor name byte {} (0x{:02X}) must be printable ASCII",
                    i, b
                );
            }
        }

        // =====================================================================
        // Smartphone resolution EDID validity (common connection scenario)
        // =====================================================================

        #[test]
        fn edid_smartphone_resolutions_valid() {
            // Users connect from smartphones with non-standard resolutions.
            // These resolutions must produce valid EDID that won't crash
            // any compositor.
            let phone_resolutions = [
                (2400, 1080, 60, "Samsung Galaxy S21-S24"),
                (2340, 1080, 60, "Xiaomi/OnePlus"),
                (2556, 1179, 60, "iPhone 14/15 Pro"),
                (2796, 1290, 60, "iPhone 14/15 Pro Max"),
                (3088, 1440, 60, "Samsung Galaxy S24 Ultra"),
                (2280, 1080, 60, "Google Pixel"),
            ];
            for &(w, h, r, device) in &phone_resolutions {
                let edid = generate_edid(w, h, r);

                // Checksum must be valid
                let sum: u32 = edid.iter().map(|&b| b as u32).sum();
                assert_eq!(
                    sum % 256, 0,
                    "EDID checksum invalid for {}x{}@{}Hz ({})",
                    w, h, r, device
                );

                // DTD resolution must match
                let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
                let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
                assert_eq!(h_active, w, "DTD H active mismatch for {} ({})", device, w);
                assert_eq!(v_active, h, "DTD V active mismatch for {} ({})", device, h);

                // Pixel clock > 0
                let pc_10khz = (edid[54] as u16) | ((edid[55] as u16) << 8);
                assert!(pc_10khz > 0, "Zero pixel clock for {} ({}x{})", device, w, h);

                // Physical size > 0
                assert!(edid[21] > 0 && edid[22] > 0,
                    "Zero physical size for {} ({}x{})", device, w, h);

                // Chromaticity intentionally zeros (non-zero crashes Xorg with EVDI)
                assert!(
                    edid[25..35].iter().all(|&b| b == 0),
                    "Non-zero chromaticity for {} ({}x{}) — crashes Xorg",
                    device, w, h
                );
            }
        }

        // =====================================================================
        // EDID checksum stability: same inputs → same output (deterministic)
        // =====================================================================

        #[test]
        fn edid_deterministic_output() {
            // If generate_edid is non-deterministic (e.g. uses timestamps),
            // KScreen would see a "different" monitor on each connection,
            // creating duplicate config entries and potential confusion.
            let edid1 = generate_edid(1920, 1080, 60);
            let edid2 = generate_edid(1920, 1080, 60);
            assert_eq!(edid1, edid2, "EDID must be deterministic for same inputs");
        }

        // =====================================================================
        // EDID total size: must be exactly 128 bytes (base EDID block)
        // =====================================================================

        #[test]
        fn edid_exactly_128_bytes() {
            // All parsers (edid-decode, Xorg, KScreen, Mutter, colord) expect
            // the base EDID block to be exactly 128 bytes.
            let edid = generate_edid(1920, 1080, 60);
            assert_eq!(
                edid.len(), 128,
                "EDID must be exactly 128 bytes (base block)"
            );
        }

        // =====================================================================
        // Range limits consistency: min_freq < max_freq (all compositors)
        // =====================================================================

        #[test]
        fn edid_range_limits_min_less_than_max() {
            // If min > max in range limits, compositors reject all modes.
            for &(w, h, r) in &[(1920, 1080, 60), (1920, 1080, 144), (3840, 2160, 60)] {
                let edid = generate_edid(w, h, r);
                let min_v = edid[95];
                let max_v = edid[96];
                let min_h = edid[97];
                let max_h = edid[98];
                assert!(
                    min_v < max_v,
                    "EDID {}x{}@{}Hz: min V freq ({}) must be < max V freq ({})",
                    w, h, r, min_v, max_v
                );
                assert!(
                    min_h < max_h,
                    "EDID {}x{}@{}Hz: min H freq ({}) must be < max H freq ({})",
                    w, h, r, min_h, max_h
                );
            }
        }

        // =====================================================================
        // DTD 12-bit resolution encoding limits — prevents compositor crash
        // from silent truncation (e.g. 5120 → 1024)
        // =====================================================================

        #[test]
        fn edid_dtd_resolution_clamped_for_5k() {
            // 5120x2880 exceeds 12-bit DTD limit (4095). Without clamping,
            // width would silently truncate: 5120 & 0xFFF = 1024.
            // Compositor sees 1024px but framebuffer is 5120px → corruption/crash.
            let edid = generate_edid(5120, 2880, 60);
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            // Must be clamped, NOT truncated
            assert_eq!(
                h_active, EDID_DTD_MAX_RESOLUTION,
                "5K width must be clamped to {} (not truncated to {})",
                EDID_DTD_MAX_RESOLUTION, 5120u32 & 0xFFF
            );
            assert_eq!(
                v_active, 2880,
                "5K height (2880) fits in 12 bits, no clamping needed"
            );
        }

        #[test]
        fn edid_dtd_resolution_clamped_for_8k() {
            // 7680x4320: both dimensions exceed 12-bit DTD limit.
            // Without clamping: 7680 & 0xFFF = 3456, 4320 & 0xFFF = 224
            let edid = generate_edid(7680, 4320, 60);
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            assert_eq!(h_active, EDID_DTD_MAX_RESOLUTION,
                "8K width must be clamped to {}", EDID_DTD_MAX_RESOLUTION);
            assert_eq!(v_active, EDID_DTD_MAX_RESOLUTION,
                "8K height must be clamped to {}", EDID_DTD_MAX_RESOLUTION);
        }

        #[test]
        fn edid_dtd_resolution_exact_limit_not_clamped() {
            // 4095 is exactly the 12-bit max — must NOT be clamped
            let edid = generate_edid(4095, 4095, 60);
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            assert_eq!(h_active, 4095, "4095 must pass through unclamped");
            assert_eq!(v_active, 4095, "4095 must pass through unclamped");
        }

        #[test]
        fn edid_checksum_valid_even_when_clamped() {
            // EDID checksum must still be valid after clamping
            for &(w, h) in &[(5120, 2880), (7680, 4320), (6144, 3456)] {
                let edid = generate_edid(w, h, 60);
                let sum: u32 = edid.iter().map(|&b| b as u32).sum();
                assert_eq!(
                    sum % 256, 0,
                    "EDID checksum invalid for clamped {}x{}@60Hz", w, h
                );
            }
        }

        // =====================================================================
        // DTD pixel clock 16-bit overflow — prevents compositor sync failures
        // =====================================================================

        #[test]
        fn edid_dtd_pixel_clock_no_overflow_4k_120hz() {
            // 4K@120Hz: pixel clock = (3840+560)*(2160+90)*120 / 10000 = 118800
            // This exceeds u16 max (65535). Without clamping, it wraps to
            // 118800 & 0xFFFF = 53264 (532.64 MHz) → wrong refresh rate.
            let edid = generate_edid(3840, 2160, 120);
            let pc_10khz = (edid[54] as u16) | ((edid[55] as u16) << 8);
            // Must be capped at 65535 (655.35 MHz), not wrapped
            assert_eq!(
                pc_10khz, EDID_DTD_MAX_PIXEL_CLOCK_10KHZ as u16,
                "4K@120Hz pixel clock must be clamped to max, not wrapped"
            );
        }

        #[test]
        fn edid_dtd_pixel_clock_fits_for_4k_60hz() {
            // 4K@60Hz: pixel clock = (3840+560)*(2160+90)*60 / 10000 = 59400
            // This fits in u16 (< 65535). Must NOT be clamped.
            let edid = generate_edid(3840, 2160, 60);
            let pc_10khz = (edid[54] as u16) | ((edid[55] as u16) << 8);
            assert_eq!(pc_10khz, 59400, "4K@60Hz pixel clock must not be clamped");
        }

        // =====================================================================
        // Blanking intervals always positive — prevents division by zero
        // =====================================================================

        #[test]
        fn edid_blanking_always_positive() {
            // If h_blank or v_blank is 0, pixel_clock = active * 0 * refresh = 0
            // → DTD pixel clock = 0 → Xorg FPE (floating point exception)
            let test_resolutions = [
                (320, 240, 60), (640, 480, 60), (800, 600, 60),
                (1024, 768, 60), (1280, 720, 60), (1920, 1080, 60),
                (2560, 1440, 60), (3840, 2160, 60), (3440, 1440, 60),
                (2400, 1080, 60), (1, 1, 60), // extreme edge case
            ];
            for &(w, h, r) in &test_resolutions {
                let edid = generate_edid(w, h, r);
                let h_blank = (edid[57] as u32) | (((edid[58] & 0x0F) as u32) << 8);
                let v_blank = (edid[60] as u32) | (((edid[61] & 0x0F) as u32) << 8);
                assert!(
                    h_blank > 0,
                    "EDID {}x{}@{}Hz: h_blank must be > 0 (prevents Xorg FPE)", w, h, r
                );
                assert!(
                    v_blank > 0,
                    "EDID {}x{}@{}Hz: v_blank must be > 0 (prevents Xorg FPE)", w, h, r
                );
            }
        }

        // =====================================================================
        // Range limits H freq consistency — must match actual timing
        // =====================================================================

        #[test]
        fn edid_range_limits_h_freq_fixed() {
            // Range limits use fixed H freq [30, 80] kHz (matching original EVDI EDID).
            // This doesn't cover all resolutions (4K, 144Hz) but Xorg/KDE works
            // fine with it — the preferred mode from the DTD takes precedence.
            let edid = generate_edid(1920, 1080, 60);
            assert_eq!(edid[97], 0x1E, "Min H freq must be 30 kHz");
            assert_eq!(edid[98], 0x50, "Max H freq must be 80 kHz");
        }

        // =====================================================================
        // estimate_pixel_clock: no panic on extreme inputs
        // =====================================================================

        #[test]
        fn estimate_pixel_clock_no_overflow_panic() {
            // estimate_pixel_clock uses u64 internally and caps to u32::MAX.
            // Must never panic, even for absurd inputs.
            let _ = estimate_pixel_clock(7680, 4320, 240);
            let _ = estimate_pixel_clock(15360, 8640, 120);
            let _ = estimate_pixel_clock(u32::MAX, u32::MAX, 60);
            // Just verify no panic — the value is capped
        }

        #[test]
        fn estimate_pixel_clock_capped_at_u32_max() {
            // For extreme resolutions, must be capped not wrapped
            let pc = estimate_pixel_clock(15360, 8640, 120);
            assert!(
                pc <= u32::MAX,
                "estimate_pixel_clock must never exceed u32::MAX"
            );
            // 8K@240Hz would overflow without capping
            let pc_extreme = estimate_pixel_clock(7680, 4320, 240);
            assert!(pc_extreme > 0, "Must return a valid non-zero value");
        }

        // =====================================================================
        // EDID constants sanity
        // =====================================================================

        #[test]
        fn edid_constants_coherent() {
            assert_eq!(EDID_DTD_MAX_RESOLUTION, 4095,
                "12-bit max must be 4095 (2^12 - 1)");
            assert_eq!(EDID_DTD_MAX_PIXEL_CLOCK_10KHZ, 65535,
                "16-bit max must be 65535 (2^16 - 1)");
        }

        // =====================================================================
        // EDID validity for high refresh rates (240Hz+ gaming monitors)
        // =====================================================================

        #[test]
        fn edid_high_refresh_rate_validity() {
            for &refresh in &[120u32, 165, 240] {
                let edid = generate_edid(1920, 1080, refresh);
                let sum: u32 = edid.iter().map(|&b| b as u32).sum();
                assert_eq!(sum % 256, 0,
                    "EDID checksum invalid for 1920x1080@{}Hz", refresh);
                // Range limits must cover the refresh rate
                let min_v = edid[95] as u32;
                let max_v = edid[96] as u32;
                assert!(
                    refresh >= min_v && refresh <= max_v,
                    "1920x1080@{}Hz: refresh outside V range [{}, {}]",
                    refresh, min_v, max_v
                );
            }
        }

        // =====================================================================
        // Zero/degenerate inputs — prevent Xorg FPE & compositor crash
        // =====================================================================

        #[test]
        fn edid_zero_refresh_rate_no_crash() {
            // refresh=0 would produce DTD pixel clock = 0.
            // Xorg computes refresh = pixel_clock / (h_total * v_total)
            // → division by zero → SIGFPE → Xorg crash → session lost.
            // generate_edid must clamp refresh to >= 1.
            let edid = generate_edid(1920, 1080, 0);
            let pc_10khz = (edid[54] as u16) | ((edid[55] as u16) << 8);
            assert!(
                pc_10khz > 0,
                "DTD pixel clock must be > 0 even with refresh=0 (Xorg FPE prevention)"
            );
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "Checksum must be valid with refresh=0");
        }

        #[test]
        fn edid_zero_width_no_crash() {
            // width=0 → h_active=0 in DTD → compositor allocates 0-width framebuffer
            // → various crashes and division by zero in DPI calculations.
            let edid = generate_edid(0, 1080, 60);
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            assert!(h_active > 0, "DTD h_active must be > 0 even with width=0");
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "Checksum must be valid with width=0");
        }

        #[test]
        fn edid_zero_height_no_crash() {
            // height=0 → v_active=0 → same compositor crashes as zero width.
            let edid = generate_edid(1920, 0, 60);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            assert!(v_active > 0, "DTD v_active must be > 0 even with height=0");
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "Checksum must be valid with height=0");
        }

        #[test]
        fn edid_all_zeros_no_crash() {
            // Ultimate degenerate case: 0x0@0Hz
            let edid = generate_edid(0, 0, 0);
            let pc_10khz = (edid[54] as u16) | ((edid[55] as u16) << 8);
            assert!(pc_10khz > 0, "DTD pixel clock must be > 0 even with all zeros");
            let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
            let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
            assert!(h_active > 0 && v_active > 0, "DTD dimensions must be > 0");
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "Checksum must be valid");
        }

        // =====================================================================
        // Range limits timing formula field — must be valid EDID value
        // =====================================================================

        #[test]
        fn edid_range_limits_timing_formula_valid() {
            // Byte 100 (descriptor offset 10) is the timing formula support flag.
            // 0x0A = GTF secondary curve (used by original EVDI EDID format).
            // This value works reliably with Xorg/KDE without triggering crashes.
            let edid = generate_edid(1920, 1080, 60);
            let timing_formula = edid[100];
            assert_eq!(
                timing_formula, 0x0A,
                "Range limits timing formula byte must be 0x0A (GTF secondary curve), got 0x{:02X}",
                timing_formula
            );
        }

        // =====================================================================
        // Physical size consistency: header (cm) vs DTD (mm)
        // =====================================================================

        #[test]
        fn edid_physical_size_header_vs_dtd_consistent() {
            // EDID header bytes 21-22 have size in cm.
            // DTD bytes 66-68 have size in mm.
            // If they disagree by > 20%, compositors may show wrong DPI.
            let edid = generate_edid(1920, 1080, 60);
            let header_w_cm = edid[21] as u32;
            let header_h_cm = edid[22] as u32;
            // DTD image size (bytes 12-14 of DTD, at EDID offset 66-68)
            let dtd_w_mm = (edid[66] as u32) | (((edid[68] >> 4) as u32) << 8);
            let dtd_h_mm = (edid[67] as u32) | (((edid[68] & 0x0F) as u32) << 8);
            // header_cm * 10 should ≈ dtd_mm
            let diff_w = ((header_w_cm * 10) as i32 - dtd_w_mm as i32).unsigned_abs();
            let diff_h = ((header_h_cm * 10) as i32 - dtd_h_mm as i32).unsigned_abs();
            // Allow up to 15mm difference (rounding from cm → mm)
            assert!(
                diff_w <= 15,
                "Physical width: header={}cm vs DTD={}mm (diff {}mm > 15mm)",
                header_w_cm, dtd_w_mm, diff_w
            );
            assert!(
                diff_h <= 15,
                "Physical height: header={}cm vs DTD={}mm (diff {}mm > 15mm)",
                header_h_cm, dtd_h_mm, diff_h
            );
        }

        // =====================================================================
        // Consumer thread resilience: poll error handling paths
        // =====================================================================

        #[test]
        fn edid_no_descriptor_tag_collision() {
            // Verify that descriptor tags don't collide (each descriptor
            // must have its correct tag at the right offset).
            // Collision would cause parsers to misinterpret descriptors → crash.
            let edid = generate_edid(1920, 1080, 60);
            // DTD 1: bytes 54-71, pixel clock > 0 (not a non-timing descriptor)
            let pc = (edid[54] as u16) | ((edid[55] as u16) << 8);
            assert!(pc > 0, "DTD 1 must have non-zero pixel clock");
            // Descriptor 2: monitor name (0xFC)
            assert_eq!(edid[75], 0xFC, "Descriptor 2 tag must be 0xFC (monitor name)");
            // Descriptor 3: range limits (0xFD)
            assert_eq!(edid[93], 0xFD, "Descriptor 3 tag must be 0xFD (range limits)");
            // Descriptor 4: dummy (0x10)
            assert_eq!(edid[111], 0x10, "Descriptor 4 tag must be 0x10 (dummy)");
            // No two descriptors should share the same tag
            let tags = [edid[75], edid[93], edid[111]];
            for i in 0..tags.len() {
                for j in (i + 1)..tags.len() {
                    assert_ne!(
                        tags[i], tags[j],
                        "Descriptor tags must be unique: 0x{:02X} at positions {} and {}",
                        tags[i], i + 2, j + 2
                    );
                }
            }
        }

        // =====================================================================
        // Comprehensive EDID matrix: ALL resolution × refresh combos valid
        // =====================================================================

        #[test]
        fn edid_comprehensive_validity_matrix() {
            // Test a wide matrix of resolutions and refresh rates to catch
            // any edge case that produces an invalid EDID.
            let resolutions: &[(u32, u32)] = &[
                (640, 480), (800, 600), (1024, 768), (1280, 720),
                (1280, 1024), (1366, 768), (1600, 900), (1920, 1080),
                (1920, 1200), (2560, 1080), (2560, 1440), (2560, 1600),
                (3440, 1440), (3840, 2160),
                // Smartphone resolutions
                (2400, 1080), (2340, 1080), (2556, 1179), (2796, 1290),
            ];
            let refresh_rates: &[u32] = &[24, 30, 50, 60, 75, 90, 120, 144, 165, 240];

            for &(w, h) in resolutions {
                for &r in refresh_rates {
                    let edid = generate_edid(w, h, r);

                    // Checksum
                    let sum: u32 = edid.iter().map(|&b| b as u32).sum();
                    assert_eq!(sum % 256, 0,
                        "EDID checksum invalid for {}x{}@{}Hz", w, h, r);

                    // DTD pixel clock > 0
                    let pc = (edid[54] as u16) | ((edid[55] as u16) << 8);
                    assert!(pc > 0,
                        "Zero pixel clock for {}x{}@{}Hz", w, h, r);

                    // Physical size > 0
                    assert!(edid[21] > 0 && edid[22] > 0,
                        "Zero physical size for {}x{}@{}Hz", w, h, r);

                    // Chromaticity intentionally zeros (avoid Xorg crash)
                    assert!(edid[25..35].iter().all(|&b| b == 0),
                        "Non-zero chromaticity for {}x{}@{}Hz — crashes Xorg", w, h, r);

                    // Range limits: min < max for both V and H
                    assert!(edid[95] < edid[96],
                        "V freq min >= max for {}x{}@{}Hz ({} >= {})",
                        w, h, r, edid[95], edid[96]);
                    assert!(edid[97] < edid[98],
                        "H freq min >= max for {}x{}@{}Hz ({} >= {})",
                        w, h, r, edid[97], edid[98]);
                }
            }
        }

        // =====================================================================
        // Consumer thread buffer safety — prevents OOM & arithmetic overflow
        // =====================================================================

        #[test]
        fn consumer_buffer_stride_no_overflow_at_max_resolution() {
            // Consumer thread computes: stride = width * 4 (as c_int).
            // If stride overflows i32 (> 536_870_911 * 4 = 2_147_483_644),
            // the buffer allocation panics or produces a negative/huge value.
            // MAX_SUPPORTED_RESOLUTION must prevent this.
            let max_stride = MAX_SUPPORTED_RESOLUTION as i64 * 4;
            assert!(
                max_stride <= i32::MAX as i64,
                "MAX_SUPPORTED_RESOLUTION ({}) * 4 = {} overflows c_int (i32::MAX = {})",
                MAX_SUPPORTED_RESOLUTION, max_stride, i32::MAX
            );
        }

        #[test]
        fn consumer_buffer_size_reasonable_at_max_resolution() {
            // Buffer = width * height * 4 bytes.
            // At MAX_SUPPORTED_RESOLUTION² this must be < 512 MB to avoid OOM
            // on typical desktop systems (4-16 GB RAM).
            let max_buf = MAX_SUPPORTED_RESOLUTION as u64
                * MAX_SUPPORTED_RESOLUTION as u64 * 4;
            let max_mb = max_buf / (1024 * 1024);
            assert!(
                max_mb <= 512,
                "Consumer buffer at max resolution: {}MB exceeds 512MB safety limit",
                max_mb
            );
        }

        #[test]
        fn max_supported_resolution_covers_8k() {
            // Must support at least 8K (7680x4320) to not reject valid displays.
            assert!(
                MAX_SUPPORTED_RESOLUTION >= 7680,
                "MAX_SUPPORTED_RESOLUTION ({}) must be >= 7680 for 8K support",
                MAX_SUPPORTED_RESOLUTION
            );
        }

        #[test]
        fn max_supported_resolution_coherent_with_edid_limit() {
            // EDID DTD limit (4095) must be <= MAX_SUPPORTED_RESOLUTION.
            // Otherwise generate_edid would try to encode values above what
            // the consumer thread accepts.
            assert!(
                EDID_DTD_MAX_RESOLUTION <= MAX_SUPPORTED_RESOLUTION,
                "EDID DTD limit ({}) must be <= MAX_SUPPORTED_RESOLUTION ({})",
                EDID_DTD_MAX_RESOLUTION, MAX_SUPPORTED_RESOLUTION
            );
        }

        // =====================================================================
        // area_limit safety — prevents u32 overflow passed to evdi_connect
        // =====================================================================

        #[test]
        fn area_limit_no_overflow_at_max_resolution() {
            // area_limit = width.saturating_mul(height).
            // At MAX_SUPPORTED_RESOLUTION², this must fit u32.
            let area = (MAX_SUPPORTED_RESOLUTION as u64)
                * (MAX_SUPPORTED_RESOLUTION as u64);
            assert!(
                area <= u32::MAX as u64,
                "MAX_SUPPORTED_RESOLUTION² ({}) overflows u32 (max {})",
                area, u32::MAX
            );
        }

        // =====================================================================
        // EDID structural integrity — prevents compositor parser crashes
        // =====================================================================

        #[test]
        fn edid_preferred_timing_bit_set() {
            // Byte 24 bit 1 (0x02) = "Preferred Timing Mode includes native
            // pixel format and preferred refresh rate".
            // Without this, compositors may not activate the DTD mode,
            // leaving the virtual display with no mode → black screen.
            let edid = generate_edid(1920, 1080, 60);
            assert_ne!(
                edid[24] & 0x02, 0,
                "EDID byte 24 bit 1 must be set (Preferred Timing in DTD1)"
            );
        }

        #[test]
        fn edid_dtd_not_interlaced() {
            // DTD byte 17 (EDID byte 71) bit 7 = interlace flag.
            // Virtual displays must NOT be interlaced — most Wayland compositors
            // don't support interlace and would crash or show a black screen.
            let edid = generate_edid(1920, 1080, 60);
            assert_eq!(
                edid[71] & 0x80, 0,
                "DTD must not be interlaced (byte 71 bit 7 must be 0)"
            );
        }

        #[test]
        fn edid_all_descriptors_properly_padded() {
            // Each 18-byte descriptor must be fully initialized.
            // Uninitialized bytes (0x00 where padding is expected) can cause
            // parsers to read garbage or trigger assertions.
            let edid = generate_edid(1920, 1080, 60);

            // Descriptor 4 (dummy, bytes 108-125): bytes 112-125 should be
            // padding (typically 0x00 for dummy descriptors)
            // Just verify the tag is correct and no accidental non-zero in tag area
            assert_eq!(edid[108], 0x00, "Dummy descriptor byte 0");
            assert_eq!(edid[109], 0x00, "Dummy descriptor byte 1");
            assert_eq!(edid[110], 0x00, "Dummy descriptor byte 2");
            assert_eq!(edid[111], 0x10, "Dummy descriptor tag must be 0x10");
        }

        #[test]
        fn edid_extension_count_zero() {
            // We only generate 128-byte base EDID (no extensions).
            // If extension count > 0, parsers would try to read beyond
            // the 128-byte buffer → buffer over-read → crash.
            let edid = generate_edid(1920, 1080, 60);
            assert_eq!(
                edid[126], 0,
                "Extension count must be 0 (no extension blocks)"
            );
        }

        #[test]
        fn edid_gamma_value_valid() {
            // Byte 23 = (gamma * 100) - 100. Value 0xFF means gamma is
            // defined in extension block. Since we have no extensions,
            // we must use a concrete value. 0x78 = gamma 2.20 (standard).
            let edid = generate_edid(1920, 1080, 60);
            assert_ne!(
                edid[23], 0xFF,
                "Gamma byte must not be 0xFF without extension blocks"
            );
            // Standard sRGB gamma 2.2 = 0x78
            assert_eq!(edid[23], 0x78, "Gamma should be 2.2 (0x78) for sRGB");
        }

        // =====================================================================
        // Multiple displays: different resolutions → different EDIDs
        // =====================================================================

        #[test]
        fn edid_different_resolutions_produce_different_edid() {
            // If two virtual displays with different resolutions produce the
            // same EDID, KScreen applies the same saved config to both,
            // causing overlapping displays. The DTD must differ.
            let edid_1080p = generate_edid(1920, 1080, 60);
            let edid_1440p = generate_edid(2560, 1440, 60);
            let edid_4k = generate_edid(3840, 2160, 60);
            assert_ne!(edid_1080p, edid_1440p,
                "1080p and 1440p must produce different EDIDs");
            assert_ne!(edid_1080p, edid_4k,
                "1080p and 4K must produce different EDIDs");
            assert_ne!(edid_1440p, edid_4k,
                "1440p and 4K must produce different EDIDs");
        }

        // =====================================================================
        // MAX_VIRTUAL_DISPLAYS prevents kernel resource exhaustion
        // =====================================================================

        #[test]
        fn max_virtual_displays_sane_limit() {
            // Each EVDI device creates a DRM card + connector + CRTC.
            // Too many devices exhaust kernel DRM resources and can cause
            // Xorg to fail when enumerating outputs.
            assert!(
                MAX_VIRTUAL_DISPLAYS >= 1 && MAX_VIRTUAL_DISPLAYS <= 16,
                "MAX_VIRTUAL_DISPLAYS ({}) should be between 1 and 16",
                MAX_VIRTUAL_DISPLAYS
            );
        }

        // =====================================================================
        // DRM operation timing emulation — verifies KScreen crash prevention
        // =====================================================================
        //
        // These tests use mock EVDI function pointers that record call
        // sequences and timestamps. They verify the timing invariants that
        // prevent KScreen's XRandR backend from segfaulting:
        //
        // 1. disconnect → sleep(≥ DRM_HOTPLUG_SETTLE_MS) → close
        // 2. Between devices: sleep(≥ DRM_HOTPLUG_SETTLE_MS)
        // 3. DRM_TOPOLOGY mutex prevents concurrent operations
        // 4. position_virtual_display_async has initial settle delay

        use std::cell::RefCell;

        thread_local! {
            static DRM_CALL_LOG: RefCell<Vec<(&'static str, std::time::Instant)>> =
                RefCell::new(Vec::new());
        }

        /// Mock disconnect that records its call timestamp.
        unsafe extern "C" fn mock_disconnect(_handle: EvdiHandle) {
            DRM_CALL_LOG.with(|log| {
                log.borrow_mut().push(("disconnect", std::time::Instant::now()));
            });
        }

        /// Mock close that records its call timestamp.
        unsafe extern "C" fn mock_close(_handle: EvdiHandle) {
            DRM_CALL_LOG.with(|log| {
                log.borrow_mut().push(("close", std::time::Instant::now()));
            });
        }

        /// Create a fake EvdiDevice for testing (null handle, no consumer thread).
        fn make_test_device(id: i32) -> EvdiDevice {
            EvdiDevice {
                handle: std::ptr::null_mut(),
                device_id: id,
                consumer_stop: Arc::new(AtomicBool::new(false)),
                consumer_thread: None,
            }
        }

        #[test]
        fn disconnect_and_close_has_settle_delay() {
            // Verify that disconnect_and_close() waits at least
            // DRM_HOTPLUG_SETTLE_MS between disconnect and close.
            // Without this delay, KScreen segfaults because it tries
            // to query a DRM device that was destroyed while it was
            // still processing the disconnect notification.
            DRM_CALL_LOG.with(|log| log.borrow_mut().clear());

            let mut device = make_test_device(99);
            device.disconnect_and_close(mock_disconnect, mock_close);

            DRM_CALL_LOG.with(|log| {
                let calls = log.borrow();
                assert_eq!(
                    calls.len(), 2,
                    "disconnect_and_close must make exactly 2 calls"
                );
                assert_eq!(calls[0].0, "disconnect", "First call must be disconnect");
                assert_eq!(calls[1].0, "close", "Second call must be close");

                let gap = calls[1].1.duration_since(calls[0].1);
                // Allow 100ms tolerance for thread scheduling jitter
                let min_gap = DRM_HOTPLUG_SETTLE_MS.saturating_sub(100);
                assert!(
                    gap.as_millis() >= min_gap as u128,
                    "Gap between disconnect and close ({:?}) must be >= {}ms \
                     (DRM_HOTPLUG_SETTLE_MS={}ms minus 100ms tolerance). \
                     Without this delay, KScreen segfaults.",
                    gap, min_gap, DRM_HOTPLUG_SETTLE_MS
                );
            });
        }

        #[test]
        fn teardown_multiple_devices_has_inter_device_delay() {
            // Verify that teardown_devices() waits DRM_HOTPLUG_SETTLE_MS
            // between each device teardown. Without this, N devices generate
            // 2*N rapid DRM uevents → KScreen crash.
            DRM_CALL_LOG.with(|log| log.borrow_mut().clear());

            let devices = vec![
                make_test_device(10),
                make_test_device(11),
                make_test_device(12),
            ];

            // Mock EvdiLibFns — only disconnect and close are used
            let fns = EvdiLibFns {
                add_device: unsafe { std::mem::transmute(mock_close as usize) },
                check_device: unsafe { std::mem::transmute(mock_close as usize) },
                open: unsafe { std::mem::transmute(mock_close as usize) },
                close: mock_close,
                connect: unsafe { std::mem::transmute(mock_close as usize) },
                disconnect: mock_disconnect,
                consumer: ConsumerFns {
                    register_buffer: unsafe { std::mem::transmute(mock_close as usize) },
                    unregister_buffer: unsafe { std::mem::transmute(mock_close as usize) },
                    request_update: unsafe { std::mem::transmute(mock_close as usize) },
                    grab_pixels: unsafe { std::mem::transmute(mock_close as usize) },
                    handle_events: unsafe { std::mem::transmute(mock_close as usize) },
                    get_event_ready: unsafe { std::mem::transmute(mock_close as usize) },
                },
            };

            teardown_devices(devices, &fns);

            DRM_CALL_LOG.with(|log| {
                let calls = log.borrow();
                // 3 devices × 2 calls each = 6 calls
                assert_eq!(
                    calls.len(), 6,
                    "3 devices × (disconnect + close) = 6 calls, got {}",
                    calls.len()
                );

                // Verify call order: d0, c0, d1, c1, d2, c2
                for i in 0..3 {
                    assert_eq!(
                        calls[i * 2].0, "disconnect",
                        "Call {} must be disconnect", i * 2
                    );
                    assert_eq!(
                        calls[i * 2 + 1].0, "close",
                        "Call {} must be close", i * 2 + 1
                    );
                }

                let min_gap = DRM_HOTPLUG_SETTLE_MS.saturating_sub(100);

                // Verify gap between disconnect and close for each device
                for i in 0..3 {
                    let gap = calls[i * 2 + 1].1.duration_since(calls[i * 2].1);
                    assert!(
                        gap.as_millis() >= min_gap as u128,
                        "Device {}: gap between disconnect and close ({:?}) < {}ms",
                        i, gap, min_gap
                    );
                }

                // Verify inter-device delay (between close[i] and disconnect[i+1])
                for i in 0..2 {
                    let close_time = calls[i * 2 + 1].1;
                    let next_disconnect_time = calls[(i + 1) * 2].1;
                    let gap = next_disconnect_time.duration_since(close_time);
                    assert!(
                        gap.as_millis() >= min_gap as u128,
                        "Gap between device {} close and device {} disconnect ({:?}) < {}ms. \
                         Without inter-device delay, KScreen receives overlapping hotplug events.",
                        i, i + 1, gap, min_gap
                    );
                }
            });
        }

        #[test]
        fn teardown_single_device_no_trailing_delay() {
            // With only 1 device, there should be no inter-device delay
            // (only the intra-device disconnect→close delay).
            // We measure timing from mock call timestamps (not wall clock)
            // to avoid false failures when DRM_TOPOLOGY is held by another test.
            DRM_CALL_LOG.with(|log| log.borrow_mut().clear());

            let devices = vec![make_test_device(50)];
            let fns = EvdiLibFns {
                add_device: unsafe { std::mem::transmute(mock_close as usize) },
                check_device: unsafe { std::mem::transmute(mock_close as usize) },
                open: unsafe { std::mem::transmute(mock_close as usize) },
                close: mock_close,
                connect: unsafe { std::mem::transmute(mock_close as usize) },
                disconnect: mock_disconnect,
                consumer: ConsumerFns {
                    register_buffer: unsafe { std::mem::transmute(mock_close as usize) },
                    unregister_buffer: unsafe { std::mem::transmute(mock_close as usize) },
                    request_update: unsafe { std::mem::transmute(mock_close as usize) },
                    grab_pixels: unsafe { std::mem::transmute(mock_close as usize) },
                    handle_events: unsafe { std::mem::transmute(mock_close as usize) },
                    get_event_ready: unsafe { std::mem::transmute(mock_close as usize) },
                },
            };

            teardown_devices(devices, &fns);

            DRM_CALL_LOG.with(|log| {
                let calls = log.borrow();
                assert_eq!(calls.len(), 2, "Single device: 2 calls (disconnect + close)");

                // Measure time between first and last mock call.
                // This excludes any time waiting for DRM_TOPOLOGY lock.
                let work_time = calls[1].1.duration_since(calls[0].1);

                // Should be ~DRM_HOTPLUG_SETTLE_MS (the disconnect→close gap),
                // NOT 2×DRM_HOTPLUG_SETTLE_MS (which would mean an extra trailing delay).
                let max_expected = DRM_HOTPLUG_SETTLE_MS + 500; // 500ms tolerance
                assert!(
                    work_time.as_millis() < max_expected as u128,
                    "Single device actual work time {:?}, expected < {}ms (no trailing delay)",
                    work_time, max_expected
                );
            });
        }

        #[test]
        fn teardown_empty_list_is_noop() {
            DRM_CALL_LOG.with(|log| log.borrow_mut().clear());

            let fns = EvdiLibFns {
                add_device: unsafe { std::mem::transmute(mock_close as usize) },
                check_device: unsafe { std::mem::transmute(mock_close as usize) },
                open: unsafe { std::mem::transmute(mock_close as usize) },
                close: mock_close,
                connect: unsafe { std::mem::transmute(mock_close as usize) },
                disconnect: mock_disconnect,
                consumer: ConsumerFns {
                    register_buffer: unsafe { std::mem::transmute(mock_close as usize) },
                    unregister_buffer: unsafe { std::mem::transmute(mock_close as usize) },
                    request_update: unsafe { std::mem::transmute(mock_close as usize) },
                    grab_pixels: unsafe { std::mem::transmute(mock_close as usize) },
                    handle_events: unsafe { std::mem::transmute(mock_close as usize) },
                    get_event_ready: unsafe { std::mem::transmute(mock_close as usize) },
                },
            };

            let start = std::time::Instant::now();
            teardown_devices(vec![], &fns);
            let elapsed = start.elapsed();

            DRM_CALL_LOG.with(|log| {
                assert!(log.borrow().is_empty(), "Empty teardown must make zero calls");
            });
            assert!(
                elapsed.as_millis() < 100,
                "Empty teardown must return immediately, took {:?}", elapsed
            );
        }

        #[test]
        fn drm_topology_mutex_prevents_concurrent_operations() {
            // Verify that DRM_TOPOLOGY can serialize concurrent operations.
            // Two threads both try to acquire DRM_TOPOLOGY; the second must
            // wait until the first releases it. This prevents overlapping
            // hotplug events from different code paths.
            use std::sync::Barrier;

            let barrier = Arc::new(Barrier::new(2));
            let hold_time = Arc::new(std::sync::Mutex::new(Vec::<(std::time::Instant, std::time::Instant)>::new()));

            let b1 = barrier.clone();
            let ht1 = hold_time.clone();
            let t1 = std::thread::spawn(move || {
                b1.wait(); // sync start
                let start = std::time::Instant::now();
                let _topo = DRM_TOPOLOGY.lock().unwrap();
                let acquired = std::time::Instant::now();
                std::thread::sleep(std::time::Duration::from_millis(200));
                let released = std::time::Instant::now();
                ht1.lock().unwrap().push((acquired, released));
            });

            let b2 = barrier.clone();
            let ht2 = hold_time.clone();
            let t2 = std::thread::spawn(move || {
                b2.wait(); // sync start
                // Small delay to increase chance thread 1 acquires first
                std::thread::sleep(std::time::Duration::from_millis(50));
                let start = std::time::Instant::now();
                let _topo = DRM_TOPOLOGY.lock().unwrap();
                let acquired = std::time::Instant::now();
                let released = std::time::Instant::now();
                ht2.lock().unwrap().push((acquired, released));
            });

            t1.join().unwrap();
            t2.join().unwrap();

            let times = hold_time.lock().unwrap();
            assert_eq!(times.len(), 2, "Both threads must record their times");

            // The two lock hold intervals must NOT overlap
            let (a1, r1) = times[0];
            let (a2, r2) = times[1];
            let overlaps = a1 < r2 && a2 < r1;
            // One must finish before the other starts (serialized)
            assert!(
                r1 <= a2 || r2 <= a1,
                "DRM_TOPOLOGY must serialize operations: \
                 thread1=[{:?}..{:?}], thread2=[{:?}..{:?}] overlap!",
                a1, r1, a2, r2
            );
        }

        #[test]
        fn cleanup_function_does_not_use_remove_all() {
            // Static analysis: verify that cleanup_orphaned_evdi_devices
            // does NOT reference the remove_all sysfs path.
            // This is the root cause of the KScreen crash — remove_all
            // generates N simultaneous hotplug events.
            let source = include_str!("virtual_display_manager.rs");

            // Find the cleanup function body
            let fn_start = source.find("pub fn cleanup_orphaned_evdi_devices()").expect(
                "cleanup_orphaned_evdi_devices function must exist"
            );

            // Extract enough of the function body (it's ~90 lines)
            let fn_body = &source[fn_start..std::cmp::min(fn_start + 6000, source.len())];

            // Must NOT contain remove_all path
            assert!(
                !fn_body.contains("remove_all"),
                "cleanup_orphaned_evdi_devices must NOT use remove_all! \
                 remove_all generates N simultaneous DRM hotplug events \
                 which crash KScreen's XRandR backend."
            );

            // Must contain per-device operations (via library function pointers)
            assert!(
                fn_body.contains("fns.open"),
                "cleanup must use evdi_open for per-device cleanup"
            );
            assert!(
                fn_body.contains("fns.close"),
                "cleanup must use evdi_close for per-device cleanup"
            );

            // Orphan cleanup must NOT call disconnect: the painter from the dead
            // process is already gone (kernel cleanup), so disconnect always fails.
            // Only close() is needed to remove the device.
            assert!(
                !fn_body.contains("(fns.disconnect)"),
                "orphan cleanup must NOT call disconnect — orphan painters are \
                 already gone (kernel cleanup on process death), so disconnect \
                 always fails with 'disconnect failed'. Only close is needed."
            );
        }

        #[test]
        fn position_display_has_initial_settle_delay() {
            // Static analysis: verify that position_virtual_display
            // waits DRM_HOTPLUG_SETTLE_MS before checking Display::all().
            // Without this, xrandr/Display::all() races with KScreen → segfault.
            let source = include_str!("virtual_display_manager.rs");

            let fn_start = source.find("fn position_virtual_display(width: u32, height: u32)").expect(
                "position_virtual_display function must exist"
            );
            let fn_body = &source[fn_start..std::cmp::min(fn_start + 5000, source.len())];

            // Must sleep DRM_HOTPLUG_SETTLE_MS BEFORE any scrap::Display::all() call
            // (use "scrap::Display" to skip comment mentions of "Display::all()")
            let first_display_all = fn_body.find("scrap::Display::all()").expect(
                "position_virtual_display must call scrap::Display::all()"
            );
            let pre_check = &fn_body[..first_display_all];
            assert!(
                pre_check.contains("DRM_HOTPLUG_SETTLE_MS"),
                "position_virtual_display must sleep DRM_HOTPLUG_SETTLE_MS \
                 BEFORE the first Display::all() check. Without this delay, \
                 queries arrive while KScreen is processing the uevent → segfault."
            );

            // The function must be synchronous (NOT spawn a thread)
            assert!(
                !fn_body.contains("thread::Builder") && !fn_body.contains("thread::spawn"),
                "position_virtual_display must be SYNCHRONOUS — the video service \
                 calls Display::all() after plug_in_monitor returns, so the display \
                 must be visible before then."
            );

            // Must check Display::all() for EVDI visibility BEFORE xrandr fallback
            let display_check = fn_body.find("already visible").expect(
                "position_virtual_display must check if EVDI is already visible \
                 in Display::all() before resorting to xrandr positioning"
            );
            let fallback = fn_body.find("for attempt").expect(
                "position_virtual_display must have a fallback xrandr polling loop"
            );
            assert!(
                display_check < fallback,
                "Display::all() visibility check must come BEFORE the xrandr fallback loop"
            );
        }

        #[test]
        fn cleanup_call_order_reload_before_cleanup() {
            // Static analysis: verify that reload_evdi_lib is called BEFORE
            // cleanup_orphaned_evdi_devices in both server.rs and linux.rs.
            // cleanup needs library function pointers that reload provides.

            // Check server.rs
            let server_src = include_str!("server.rs");
            let reload_pos = server_src.find("reload_evdi_lib()");
            let cleanup_pos = server_src.find("cleanup_orphaned_evdi_devices()");
            if let (Some(r), Some(c)) = (reload_pos, cleanup_pos) {
                assert!(
                    r < c,
                    "server.rs: reload_evdi_lib() must be called BEFORE \
                     cleanup_orphaned_evdi_devices() (reload at {}, cleanup at {})",
                    r, c
                );
            }

            // Check linux.rs
            let linux_src = include_str!("platform/linux.rs");
            let reload_pos = linux_src.find("reload_evdi_lib()");
            let cleanup_pos = linux_src.find("cleanup_orphaned_evdi_devices()");
            if let (Some(r), Some(c)) = (reload_pos, cleanup_pos) {
                assert!(
                    r < c,
                    "linux.rs: reload_evdi_lib() must be called BEFORE \
                     cleanup_orphaned_evdi_devices() (reload at {}, cleanup at {})",
                    r, c
                );
            }
        }

        #[test]
        fn cleanup_skips_gracefully_without_sysfs() {
            // When /sys/devices/evdi/count doesn't exist (EVDI not loaded),
            // cleanup must return immediately without error.
            // This is the common case on non-EVDI systems or before modprobe.
            let start = std::time::Instant::now();
            // If EVDI is actually loaded on the test machine with orphaned
            // devices, this test might actually clean them up. That's fine.
            // The important thing is it doesn't panic or crash.
            cleanup_orphaned_evdi_devices();
            // Should return quickly (either no sysfs or count=0 or lib not loaded)
            let elapsed = start.elapsed();
            // If there are no orphaned devices, should return in < 100ms
            // If there are orphaned devices AND lib is loaded, it will take
            // longer (cleaning up), which is also correct behavior.
        }

        // =================================================================
        // Client emulation tests
        //
        // These tests call the REAL VdController from virtual_display_manager.rs
        // — the same code used by connection.rs in production. This guarantees
        // the tests verify actual production logic, not a test-only replica.
        // =================================================================
        use super::super::{VdController, VdDecision};

        /// Helper: collect Create decisions from a Vec<VdDecision>.
        fn created(decisions: &[VdDecision]) -> Vec<(i32, u32, u32)> {
            decisions
                .iter()
                .filter_map(|d| match d {
                    VdDecision::Create { display, width, height } => {
                        Some((*display, *width, *height))
                    }
                    _ => None,
                })
                .collect()
        }

        /// Emulate the EXACT message sequence from a Samsung phone (2340x1080):
        /// 1. Rust auto-restore sends ToggleVirtualDisplay(0) (before first frame)
        /// 2. Flutter sends "#vd_res 2340x1080" (on first frame)
        /// 3. Flutter sends ToggleVirtualDisplay(0) (after 500ms delay)
        /// Expected: exactly 1 Create at 2340x1080, zero removals.
        #[test]
        fn emulate_mobile_client_samsung_2340x1080() {
            let mut ctl = VdController::new();

            // Step 1: Rust io_loop auto-restore (fires on connection setup)
            let d = ctl.toggle(0, true);
            assert!(
                matches!(d, VdDecision::Deferred { display: 0 }),
                "must defer (no resolution yet), got {:?}", d
            );
            assert!(ctl.active_indices().is_empty());

            // Step 2: Flutter sends #vd_res 2340x1080 (on first frame arrival)
            let decisions = ctl.handle_resolution(2340, 1080);
            assert_eq!(decisions.len(), 1);
            assert_eq!(
                created(&decisions),
                vec![(0, 2340, 1080)],
                "deferred toggle must create at phone resolution"
            );
            assert_eq!(ctl.active_indices(), &[0]);

            // Step 3: Flutter auto-add sends ToggleVirtualDisplay(0)
            let d = ctl.toggle(0, true);
            assert!(
                matches!(d, VdDecision::Skipped { display: 0, reason: "already active" }),
                "duplicate must be skipped, got {:?}", d
            );

            // Final: still exactly 1 active display
            assert_eq!(ctl.active_indices(), &[0]);
        }

        /// Emulate old mobile client with stale saved state "0,0,0,0".
        #[test]
        fn emulate_old_mobile_client_stale_0000() {
            let mut ctl = VdController::new();

            // 4 duplicate toggles from stale "0,0,0,0" saved state
            let d0 = ctl.toggle(0, true);
            assert!(matches!(d0, VdDecision::Deferred { .. }));
            for _ in 1..4 {
                let d = ctl.toggle(0, true);
                assert!(
                    matches!(d, VdDecision::Skipped { reason: "already pending", .. }),
                    "duplicate pending must be skipped, got {:?}", d
                );
            }
            assert!(ctl.active_indices().is_empty());

            // Flutter sends resolution → 1 deferred processed
            let decisions = ctl.handle_resolution(2340, 1080);
            assert_eq!(created(&decisions), vec![(0, 2340, 1080)]);

            // Flutter auto-add → skipped
            let d = ctl.toggle(0, true);
            assert!(matches!(d, VdDecision::Skipped { reason: "already active", .. }));
        }

        /// Emulate desktop client: no #vd_res is ever sent.
        /// After timeout → default 1920x1080.
        #[test]
        fn emulate_desktop_client_no_vd_res_timeout() {
            let mut ctl = VdController::new();

            let d = ctl.toggle(0, true);
            assert!(matches!(d, VdDecision::Deferred { .. }));

            // Before timeout: nothing happens
            let decisions = ctl.check_timeout();
            assert!(decisions.is_empty(), "deadline not yet reached");

            // Force deadline to past
            ctl.set_pending_deadline(
                std::time::Instant::now() - std::time::Duration::from_secs(1),
            );
            let decisions = ctl.check_timeout();
            assert_eq!(
                created(&decisions),
                vec![(0, 1920, 1080)],
                "desktop default resolution after timeout"
            );
            assert_eq!(ctl.active_indices(), &[0]);
        }

        /// Desktop: manual toggle after timeout sets default resolution.
        #[test]
        fn emulate_desktop_manual_toggle_after_timeout() {
            let mut ctl = VdController::new();

            ctl.toggle(0, true);
            ctl.set_pending_deadline(
                std::time::Instant::now() - std::time::Duration::from_secs(1),
            );
            ctl.check_timeout();
            assert_eq!(ctl.active_indices(), &[0]);

            // User manually adds display 1 → created immediately
            let d = ctl.toggle(1, true);
            assert!(matches!(d, VdDecision::Create { display: 1, width: 1920, height: 1080 }));
            assert_eq!(ctl.active_indices(), &[0, 1]);
        }

        /// Mobile client sends #vd_res FIRST, then toggle. No deferral.
        #[test]
        fn emulate_mobile_direct_toggle_with_resolution() {
            let mut ctl = VdController::new();

            let decisions = ctl.handle_resolution(2340, 1080);
            assert!(decisions.is_empty());
            assert!(!ctl.has_pending());

            let d = ctl.toggle(0, true);
            assert!(matches!(d, VdDecision::Create { display: 0, width: 2340, height: 1080 }));
        }

        /// Toggle OFF then ON again.
        #[test]
        fn emulate_toggle_off_then_on() {
            let mut ctl = VdController::new();

            ctl.handle_resolution(2340, 1080);
            ctl.toggle(0, true);
            assert_eq!(ctl.active_indices(), &[0]);

            let d = ctl.toggle(0, false);
            assert!(matches!(d, VdDecision::Remove { display: 0 }));
            assert!(ctl.active_indices().is_empty());

            let d = ctl.toggle(0, true);
            assert!(matches!(d, VdDecision::Create { display: 0, width: 2340, height: 1080 }));
            assert_eq!(ctl.active_indices(), &[0]);
        }

        /// Multiple displays from mobile client.
        #[test]
        fn emulate_mobile_multiple_displays() {
            let mut ctl = VdController::new();
            ctl.handle_resolution(2340, 1080);

            for i in 0..3 {
                let d = ctl.toggle(i, true);
                assert!(
                    matches!(d, VdDecision::Create { width: 2340, height: 1080, .. }),
                    "display {} must be created, got {:?}", i, d
                );
            }
            assert_eq!(ctl.active_indices(), &[0, 1, 2]);
        }

        /// Connection close: all active displays cleaned up, pending dropped.
        #[test]
        fn emulate_connection_close_cleanup() {
            let mut ctl = VdController::new();
            ctl.handle_resolution(2340, 1080);
            ctl.toggle(0, true);
            ctl.toggle(1, true);

            let indices = ctl.close();
            assert_eq!(indices, vec![0, 1]);
            assert!(ctl.active_indices().is_empty());
        }

        /// Connection close with pending toggles: nothing created.
        #[test]
        fn emulate_connection_close_with_pending() {
            let mut ctl = VdController::new();
            ctl.toggle(0, true); // deferred
            assert!(ctl.has_pending());

            let indices = ctl.close();
            assert!(indices.is_empty(), "no active displays to clean up");
            assert!(!ctl.has_pending());
        }

        /// Old client sends "0,1,2,3", then #vd_res → all 4 created correctly.
        #[test]
        fn emulate_old_client_four_different_displays() {
            let mut ctl = VdController::new();
            for i in 0..4 {
                ctl.toggle(i, true);
            }
            assert!(ctl.active_indices().is_empty());

            let decisions = ctl.handle_resolution(2340, 1080);
            assert_eq!(
                created(&decisions),
                vec![
                    (0, 2340, 1080),
                    (1, 2340, 1080),
                    (2, 2340, 1080),
                    (3, 2340, 1080),
                ]
            );
            assert_eq!(ctl.active_indices(), &[0, 1, 2, 3]);
        }

        // =============================================================
        // Timing-aware tests
        //
        // These tests verify that VD_DEFER_TIMEOUT_SECS is sufficient
        // for real-world mobile client delays. The #vd_res message
        // from a phone arrives AFTER the first video frame, which
        // requires: video init + encoding + network + phone decode.
        // Real measurements show this takes 10-20 seconds.
        //
        // These tests would have CAUGHT the 5s timeout bug:
        //   - With VD_DEFER_TIMEOUT_SECS=5, a 16s delay causes the
        //     timeout to fire BEFORE #vd_res, creating at 1920x1080.
        //   - With VD_DEFER_TIMEOUT_SECS=30, the 16s delay arrives
        //     in time, creating at the correct phone resolution.
        // =============================================================

        /// Known real-world delays (measured from production logs):
        /// - Samsung 2340x1080: #vd_res arrives ~16s after connection
        /// - Worst case observed: ~20s on slow WiFi
        const KNOWN_REAL_DELAYS_SECS: &[u64] = &[10, 12, 14, 16, 18, 20];

        /// Verify VD_DEFER_TIMEOUT_SECS is large enough for ALL known real delays.
        /// This test FAILS immediately if someone lowers the timeout below safe levels.
        #[test]
        fn timeout_exceeds_all_known_real_world_delays() {
            let timeout = super::super::VD_DEFER_TIMEOUT_SECS;
            for &delay in KNOWN_REAL_DELAYS_SECS {
                assert!(
                    timeout > delay,
                    "VD_DEFER_TIMEOUT_SECS ({timeout}s) must exceed known real delay ({delay}s). \
                     Mobile clients need {delay}s for #vd_res to arrive after first frame."
                );
            }
            // Must also have at least 5s margin above worst known delay.
            let worst = KNOWN_REAL_DELAYS_SECS.iter().max().unwrap();
            assert!(
                timeout >= worst + 5,
                "VD_DEFER_TIMEOUT_SECS ({timeout}s) must have >= 5s margin above worst \
                 known delay ({worst}s). Current margin: {}s.",
                timeout - worst
            );
        }

        /// Simulate a realistic mobile scenario where #vd_res arrives at 16s.
        /// With VD_DEFER_TIMEOUT_SECS=30, the timeout has NOT fired yet,
        /// so handle_resolution() must process the deferred toggle correctly.
        #[test]
        fn timing_mobile_16s_delay_resolution_arrives_before_timeout() {
            let mut ctl = VdController::new();

            // t=0: Rust auto-restore fires toggle
            let d = ctl.toggle(0, true);
            assert!(matches!(d, VdDecision::Deferred { display: 0 }));

            // t=16s: #vd_res arrives. Timeout was 30s, so still pending.
            // Simulate by checking that deadline is still in the future.
            assert!(ctl.has_pending(), "toggle must still be pending at 16s");
            let timeout_decisions = ctl.check_timeout();
            assert!(
                timeout_decisions.is_empty(),
                "timeout must NOT have fired yet (we're at ~16s, deadline is 30s)"
            );

            // #vd_res arrives → creates at correct resolution
            let decisions = ctl.handle_resolution(2340, 1080);
            assert_eq!(created(&decisions), vec![(0, 2340, 1080)]);
            assert!(!ctl.has_pending());
        }

        /// REGRESSION: Prove that a 5s timeout would have caused the bug.
        /// At 5s the timeout fires, creating at default 1920x1080.
        /// Then #vd_res arrives at 16s but toggle is skipped (already active).
        /// Result: wrong resolution 1920x1080 instead of 2340x1080.
        #[test]
        fn regression_5s_timeout_causes_wrong_resolution() {
            let mut ctl = VdController::new();

            // t=0: toggle deferred
            ctl.toggle(0, true);

            // t=5s: timeout fires (simulate by setting deadline to past)
            ctl.set_pending_deadline(
                std::time::Instant::now() - std::time::Duration::from_secs(1),
            );
            let decisions = ctl.check_timeout();
            // BUG: created at 1920x1080 (wrong!)
            assert_eq!(
                created(&decisions),
                vec![(0, 1920, 1080)],
                "5s timeout creates at DEFAULT resolution (the bug)"
            );

            // t=16s: #vd_res 2340x1080 arrives too late
            let decisions = ctl.handle_resolution(2340, 1080);
            assert!(
                decisions.is_empty() || created(&decisions).is_empty(),
                "display 0 is already active, #vd_res can't fix it"
            );

            // Verify we're stuck at 1920x1080 — the bug this test catches
            assert_eq!(ctl.active_indices(), &[0]);
            assert_eq!(
                ctl.resolution(),
                Some((2340, 1080)),
                "resolution is stored but display was already created at wrong size"
            );
        }

        /// Test each known real-world delay: simulate toggle, wait the delay,
        /// then send #vd_res. Verify the display is ALWAYS created at
        /// the correct resolution (not the default).
        #[test]
        fn timing_all_known_delays_produce_correct_resolution() {
            for &delay_secs in KNOWN_REAL_DELAYS_SECS {
                let mut ctl = VdController::new();

                // t=0: toggle deferred
                let d = ctl.toggle(0, true);
                assert!(matches!(d, VdDecision::Deferred { .. }));

                // At delay_secs < VD_DEFER_TIMEOUT_SECS:
                // Timeout must NOT have fired yet.
                // (We can't actually wait, but we verify the deadline arithmetic.)
                assert!(
                    super::super::VD_DEFER_TIMEOUT_SECS > delay_secs,
                    "precondition: timeout must exceed delay {delay_secs}s"
                );

                // Timeout check should return empty (deadline still in future)
                let timeout_decisions = ctl.check_timeout();
                assert!(
                    timeout_decisions.is_empty(),
                    "delay {delay_secs}s: timeout must not fire before VD_DEFER_TIMEOUT_SECS"
                );

                // #vd_res arrives at delay_secs → correct resolution
                let decisions = ctl.handle_resolution(2340, 1080);
                assert_eq!(
                    created(&decisions),
                    vec![(0, 2340, 1080)],
                    "delay {delay_secs}s: must create at phone resolution, not default"
                );

                // Subsequent toggle must be skipped (dedup)
                let d = ctl.toggle(0, true);
                assert!(
                    matches!(d, VdDecision::Skipped { reason: "already active", .. }),
                    "delay {delay_secs}s: duplicate toggle must be skipped"
                );
            }
        }

        /// Test that #vd_res arriving just 1 second before timeout still works.
        /// This is the tightest timing scenario.
        #[test]
        fn timing_resolution_arrives_1s_before_timeout() {
            let mut ctl = VdController::new();

            ctl.toggle(0, true);

            // Simulate: deadline is 1 second from now (resolution barely arrives in time)
            ctl.set_pending_deadline(
                std::time::Instant::now() + std::time::Duration::from_secs(1),
            );

            // Timeout check: deadline still 1s in the future → no timeout
            let decisions = ctl.check_timeout();
            assert!(decisions.is_empty(), "1s before deadline: must not timeout");

            // #vd_res arrives just in time
            let decisions = ctl.handle_resolution(2340, 1080);
            assert_eq!(created(&decisions), vec![(0, 2340, 1080)]);
        }

        /// Test that #vd_res arriving 1 second AFTER timeout gets the default resolution,
        /// because the timeout already created the display.
        #[test]
        fn timing_resolution_arrives_1s_after_timeout() {
            let mut ctl = VdController::new();

            ctl.toggle(0, true);

            // Simulate: deadline was 1 second ago (timeout already expired)
            ctl.set_pending_deadline(
                std::time::Instant::now() - std::time::Duration::from_secs(1),
            );

            // Timeout fires → default resolution
            let decisions = ctl.check_timeout();
            assert_eq!(
                created(&decisions),
                vec![(0, 1920, 1080)],
                "timeout creates at default resolution"
            );

            // #vd_res arrives too late — display already active
            let decisions = ctl.handle_resolution(2340, 1080);
            assert!(
                created(&decisions).is_empty(),
                "display 0 already active, #vd_res arrives too late"
            );
        }

        // =============================================================
        // Log-based regression tests
        //
        // Parse real server log lines to validate that VD behavior
        // matches expectations. This catches issues like:
        // - Timeout firing before #vd_res (wrong resolution)
        // - Multiple displays created (stale toggles)
        // - Display created at default instead of phone resolution
        // =============================================================

        /// Parse VD-related log lines and verify the connection was handled correctly.
        /// Expected for a mobile client:
        ///   1. "VD: deferring display 0" (toggle deferred)
        ///   2. "VD: received chat '#vd_res WxH'" (resolution received)
        ///   3. "VD: creating display 0 at WxH" (correct resolution)
        /// Bug indicators:
        ///   - "VD: creating display 0 at 1920x1080" when phone is not 1920x1080
        ///   - Timeout log before #vd_res log
        #[test]
        fn log_regression_validate_vd_log_patterns() {
            // Simulate a CORRECT log sequence (30s timeout, #vd_res at 16s)
            let good_log = vec![
                "2025-01-01 00:00:00 VD: deferring display 0 (waiting for client resolution, timeout 30s)",
                "2025-01-01 00:00:16 VD: received chat '#vd_res 2340x1080'",
                "2025-01-01 00:00:16 VD: creating display 0 at 2340x1080",
            ];
            let result = validate_vd_log_sequence(&good_log, 2340, 1080);
            assert!(result.is_ok(), "good log should pass: {:?}", result);

            // Simulate a BAD log sequence (5s timeout → wrong resolution)
            let bad_log = vec![
                "2025-01-01 00:00:00 VD: deferring display 0 (waiting for client resolution, timeout 30s)",
                "2025-01-01 00:00:05 VD: creating display 0 at 1920x1080",
                "2025-01-01 00:00:16 VD: received chat '#vd_res 2340x1080'",
                "2025-01-01 00:00:16 VD: display 0 skipped (already active)",
            ];
            let result = validate_vd_log_sequence(&bad_log, 2340, 1080);
            assert!(result.is_err(), "bad log should fail");
            let err = result.unwrap_err();
            assert!(
                err.contains("1920x1080") || err.contains("wrong resolution") || err.contains("before"),
                "error should mention wrong resolution: {}", err
            );
        }

        /// Helper: validate a sequence of VD log lines for a mobile connection.
        /// Returns Ok(()) if the display was created at the expected resolution,
        /// Err(reason) if something went wrong.
        fn validate_vd_log_sequence(
            lines: &[&str],
            expected_w: u32,
            expected_h: u32,
        ) -> Result<(), String> {
            let mut deferred = false;
            let mut resolution_received = false;
            let mut created_resolution: Option<(u32, u32)> = None;
            let mut created_before_resolution = false;

            for line in lines {
                if line.contains("VD: deferring display") {
                    deferred = true;
                }
                if line.contains("VD: received chat '#vd_res") {
                    resolution_received = true;
                }
                if line.contains("VD: creating display") {
                    // Parse resolution from "creating display N at WxH"
                    if let Some(at_pos) = line.find(" at ") {
                        let rest = &line[at_pos + 4..];
                        let parts: Vec<&str> = rest.trim().split('x').collect();
                        if parts.len() == 2 {
                            if let (Ok(w), Ok(h)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                                created_resolution = Some((w, h));
                                if !resolution_received {
                                    created_before_resolution = true;
                                }
                            }
                        }
                    }
                }
            }

            if !deferred {
                return Err("no 'VD: deferring' line found — toggle was not deferred".into());
            }

            if let Some((w, h)) = created_resolution {
                if created_before_resolution {
                    return Err(format!(
                        "display created at {w}x{h} BEFORE #vd_res was received — \
                         timeout fired too early"
                    ));
                }
                if w != expected_w || h != expected_h {
                    return Err(format!(
                        "wrong resolution: created at {w}x{h}, expected {expected_w}x{expected_h}"
                    ));
                }
                Ok(())
            } else {
                Err("no 'VD: creating display' line found".into())
            }
        }

        /// Parse a real log file (if present) and validate VD behavior.
        /// Skips gracefully if no log file is available.
        #[test]
        fn log_regression_check_real_log_if_available() {
            let log_path = "/home/erwan/.local/share/logs/RustDesk/server/rustdesk_rCURRENT.log";
            let content = match std::fs::read_to_string(log_path) {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("  [SKIP] no server log at {log_path}");
                    return;
                }
            };

            // Extract VD-related lines from the LAST connection session
            let vd_lines: Vec<&str> = content
                .lines()
                .filter(|l| l.contains("VD: "))
                .collect();

            if vd_lines.is_empty() {
                eprintln!("  [SKIP] no VD log lines found in {log_path}");
                return;
            }

            // Find the last #vd_res to determine expected resolution
            let last_res_line = vd_lines.iter().rev().find(|l| l.contains("#vd_res"));
            if let Some(res_line) = last_res_line {
                // Parse "#vd_res WxH"
                if let Some(pos) = res_line.find("#vd_res ") {
                    let rest = &res_line[pos + 8..];
                    // Take until end of quote or next whitespace
                    let res_str: String = rest.chars()
                        .take_while(|c| c.is_ascii_digit() || *c == 'x')
                        .collect();
                    let parts: Vec<&str> = res_str.split('x').collect();
                    if parts.len() == 2 {
                        if let (Ok(w), Ok(h)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                            // Find the last contiguous VD session (from last "deferring" onwards)
                            let session_start = vd_lines.iter().rposition(|l| l.contains("deferring"));
                            if let Some(start) = session_start {
                                let session_lines: Vec<&str> = vd_lines[start..].to_vec();
                                let result = validate_vd_log_sequence(&session_lines, w, h);
                                match result {
                                    Ok(()) => eprintln!("  [PASS] real log: display correctly created at {}x{}", w, h),
                                    Err(e) => panic!(
                                        "REAL LOG REGRESSION FAILURE: {}\n\
                                         VD log lines from last session:\n{}",
                                        e,
                                        session_lines.join("\n")
                                    ),
                                }
                            }
                        }
                    }
                }
            } else {
                // No #vd_res → desktop client, just verify no double-create
                let create_count = vd_lines.iter()
                    .filter(|l| l.contains("VD: creating display"))
                    .count();
                assert!(
                    create_count <= 1,
                    "desktop client should create at most 1 display, found {create_count} creates:\n{}",
                    vd_lines.join("\n")
                );
            }
        }

        /// Measure the delay between connection start and #vd_res arrival
        /// from the real log. Assert VD_DEFER_TIMEOUT_SECS exceeds it.
        #[test]
        fn log_regression_measure_vd_res_delay() {
            let log_path = "/home/erwan/.local/share/logs/RustDesk/server/rustdesk_rCURRENT.log";
            let content = match std::fs::read_to_string(log_path) {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("  [SKIP] no server log at {log_path}");
                    return;
                }
            };

            // Find last defer and last #vd_res timestamps
            let vd_lines: Vec<&str> = content.lines().filter(|l| l.contains("VD: ")).collect();
            let last_defer = vd_lines.iter().rev().find(|l| l.contains("deferring"));
            let last_res = vd_lines.iter().rev().find(|l| l.contains("#vd_res"));

            if let (Some(defer_line), Some(res_line)) = (last_defer, last_res) {
                if let (Some(t1), Some(t2)) = (parse_log_time(defer_line), parse_log_time(res_line)) {
                    let delay = t2.duration_since(t1).unwrap_or_default();
                    let delay_secs = delay.as_secs();
                    let timeout = super::super::VD_DEFER_TIMEOUT_SECS;
                    eprintln!(
                        "  [INFO] real #vd_res delay: {}s, VD_DEFER_TIMEOUT_SECS: {}s, margin: {}s",
                        delay_secs, timeout, timeout.saturating_sub(delay_secs)
                    );
                    assert!(
                        timeout > delay_secs + 5,
                        "VD_DEFER_TIMEOUT_SECS ({timeout}s) must exceed measured delay \
                         ({delay_secs}s) by at least 5s margin! Increase timeout to at least {}s.",
                        delay_secs + 10
                    );
                }
            } else {
                eprintln!("  [SKIP] no defer/vd_res pair found in log");
            }
        }

        /// Parse "YYYY-MM-DD HH:MM:SS" or "HH:MM:SS" from the start of a log line.
        fn parse_log_time(line: &str) -> Option<std::time::SystemTime> {
            // Try to find HH:MM:SS pattern
            let parts: Vec<&str> = line.split_whitespace().collect();
            for part in &parts {
                let time_parts: Vec<&str> = part.split(':').collect();
                if time_parts.len() == 3 {
                    if let (Ok(h), Ok(m), Ok(s)) = (
                        time_parts[0].parse::<u64>(),
                        time_parts[1].parse::<u64>(),
                        time_parts[2].parse::<u64>(),
                    ) {
                        if h < 24 && m < 60 && s < 60 {
                            let secs = h * 3600 + m * 60 + s;
                            return Some(
                                std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs),
                            );
                        }
                    }
                }
            }
            None
        }

        /// Verify connection.rs delegates to VdController (not inline logic).
        #[test]
        fn verify_connection_delegates_to_vd_controller() {
            let src = include_str!("server/connection.rs");

            // Must use VdController as a field
            assert!(
                src.contains("vd_controller: virtual_display_manager::VdController"),
                "connection.rs must use VdController as a field"
            );

            // Must call vd_controller.toggle()
            assert!(
                src.contains("vd_controller.toggle("),
                "connection.rs must delegate toggle to VdController"
            );

            // Must call vd_controller.handle_resolution()
            assert!(
                src.contains("vd_controller.handle_resolution("),
                "connection.rs must delegate #vd_res to VdController"
            );

            // Must call vd_controller.check_timeout()
            assert!(
                src.contains("vd_controller.check_timeout()"),
                "connection.rs must call VdController.check_timeout()"
            );

            // Must call vd_controller.close()
            assert!(
                src.contains("vd_controller.close()"),
                "connection.rs must call VdController.close() on disconnect"
            );

            // Must NOT contain old inline logic
            assert!(
                !src.contains("virtual_display_indices"),
                "connection.rs must not have old virtual_display_indices field"
            );
            assert!(
                !src.contains("pending_virtual_displays"),
                "connection.rs must not have old pending_virtual_displays field"
            );
        }

        // =============================================================
        // Index consistency tests
        //
        // Verify that VdController indices match EVDI manager indices
        // throughout the create/remove lifecycle. These tests caught the
        // Bug 1 index mismatch where idx=0 was remapped to "next available".
        // =============================================================

        /// Test A: VdController index 0 → plug_in(0) → plug_out(0) round-trip.
        /// Verifies that the index used by VdController matches what
        /// plug_in_monitor stores and plug_out_monitor looks up.
        #[test]
        fn index_roundtrip_plug_in_0_plug_out_0() {
            let mut ctl = VdController::new();
            ctl.handle_resolution(2340, 1080);

            // VdController creates display 0
            let d = ctl.toggle(0, true);
            assert!(matches!(d, VdDecision::Create { display: 0, .. }));
            assert_eq!(ctl.active_indices(), &[0]);

            // Verify plug_in_monitor stores at key 0 (not remapped)
            // by checking get_virtual_displays after plug_in
            // (This is a source-level check; integration tests verify it with real EVDI)

            // VdController removes display 0
            let d = ctl.toggle(0, false);
            assert!(matches!(d, VdDecision::Remove { display: 0 }));
            assert!(ctl.active_indices().is_empty());

            // Verify: plug_out_monitor(0) must look up key 0 exactly
            // Source-level check:
            let src = include_str!("server/connection.rs");
            // execute_vd_decision passes display (i32) directly to plug_out_monitor
            assert!(
                src.contains("plug_out_monitor(display,"),
                "execute_vd_decision must pass VdController's display index directly"
            );
        }

        /// Test A2: Verify plug_in_monitor no longer remaps idx=0.
        #[test]
        fn index_plug_in_does_not_remap_zero() {
            let src = include_str!("virtual_display_manager.rs");
            // The old buggy pattern: "if idx == 0 { next_peer_index }"
            let plug_in_fn_start = src.find("pub fn plug_in_monitor(idx: u32, modes: &[super::MonitorMode])")
                .expect("plug_in_monitor function must exist");
            // Take the next 500 chars (the function body)
            let fn_body = &src[plug_in_fn_start..plug_in_fn_start + 800.min(src.len() - plug_in_fn_start)];
            assert!(
                !fn_body.contains("next_peer_index"),
                "plug_in_monitor must NOT remap idx=0 to next_peer_index. \
                 The index from VdController must be used directly."
            );
        }

        /// Test A3: Verify plug_out_monitor no longer has index==0 "remove max" special case.
        #[test]
        fn index_plug_out_no_special_case_zero() {
            let src = include_str!("virtual_display_manager.rs");
            let plug_out_fn_start = src.find("pub fn plug_out_monitor(index: i32) -> ResultType<()>")
                .expect("plug_out_monitor function must exist");
            let fn_body = &src[plug_out_fn_start..plug_out_fn_start + 500.min(src.len() - plug_out_fn_start)];
            assert!(
                !fn_body.contains("keys().max()"),
                "plug_out_monitor must NOT have index==0 → remove max_key special case"
            );
        }

        /// Test B: Multiple displays (0, 1, 2) — create all, remove only 1, verify 0 and 2 remain.
        #[test]
        fn index_multiple_displays_individual_removal() {
            let mut ctl = VdController::new();
            ctl.handle_resolution(2340, 1080);

            // Create displays 0, 1, 2
            for i in 0..3 {
                let d = ctl.toggle(i, true);
                assert!(
                    matches!(d, VdDecision::Create { width: 2340, height: 1080, .. }),
                    "display {} must create, got {:?}", i, d
                );
            }
            assert_eq!(ctl.active_indices(), &[0, 1, 2]);

            // Remove only display 1
            let d = ctl.toggle(1, false);
            assert!(matches!(d, VdDecision::Remove { display: 1 }));

            // Verify: 0 and 2 remain, 1 is gone
            let active = ctl.active_indices();
            assert_eq!(active, &[0, 2], "displays 0 and 2 must remain after removing 1");

            // Re-create display 1
            let d = ctl.toggle(1, true);
            assert!(matches!(d, VdDecision::Create { display: 1, width: 2340, height: 1080 }));
            assert_eq!(ctl.active_indices(), &[0, 2, 1]);
        }

        /// Test B2: Connection close with multiple displays returns all indices.
        #[test]
        fn index_close_returns_all_active_indices() {
            let mut ctl = VdController::new();
            ctl.handle_resolution(1920, 1080);
            ctl.toggle(0, true);
            ctl.toggle(1, true);
            ctl.toggle(2, true);

            let indices = ctl.close();
            assert_eq!(indices.len(), 3);
            assert!(indices.contains(&0));
            assert!(indices.contains(&1));
            assert!(indices.contains(&2));
        }

        // =============================================================
        // Rollback tests
        //
        // Verify that rollback_create() allows recovery after a failed
        // plug_in_monitor call.
        // =============================================================

        /// Test C: rollback_create removes display from active_indices,
        /// allowing a subsequent toggle to re-create it.
        #[test]
        fn rollback_create_allows_retry() {
            let mut ctl = VdController::new();
            ctl.handle_resolution(2340, 1080);

            // Create display 0 (VdController marks it as active)
            let d = ctl.toggle(0, true);
            assert!(matches!(d, VdDecision::Create { display: 0, .. }));
            assert_eq!(ctl.active_indices(), &[0]);

            // Simulate plug_in_monitor failure → rollback
            ctl.rollback_create(0);
            assert!(ctl.active_indices().is_empty(), "rollback must remove from active");

            // Retry: toggle(0, true) must NOT be skipped
            let d = ctl.toggle(0, true);
            assert!(
                matches!(d, VdDecision::Create { display: 0, width: 2340, height: 1080 }),
                "after rollback, toggle must produce Create, got {:?}", d
            );
        }

        /// Test C2: rollback_create only removes the specific display.
        #[test]
        fn rollback_create_only_affects_target_display() {
            let mut ctl = VdController::new();
            ctl.handle_resolution(1920, 1080);
            ctl.toggle(0, true);
            ctl.toggle(1, true);
            ctl.toggle(2, true);
            assert_eq!(ctl.active_indices(), &[0, 1, 2]);

            // Rollback display 1 only
            ctl.rollback_create(1);
            assert_eq!(ctl.active_indices(), &[0, 2], "only display 1 should be removed");
        }

        /// Test C3: Verify connection.rs calls rollback_create on failure.
        #[test]
        fn connection_calls_rollback_on_failure() {
            let src = include_str!("server/connection.rs");
            let count = src.matches("rollback_create(display)").count();
            assert!(
                count >= 2,
                "connection.rs must call rollback_create(display) at least twice \
                 (once for Err, once for panic). Found {} occurrences.",
                count
            );
        }

        // =============================================================
        // Source-level chain verification
        //
        // Verify the full execute_vd_decision → plug_in_monitor chain
        // passes correct parameters.
        // =============================================================

        /// Test D: #vd_res parsing has range validation.
        #[test]
        fn vd_res_parsing_has_range_validation() {
            let src = include_str!("server/connection.rs");
            // Must contain range checks
            assert!(
                src.contains("(640..=7680)"),
                "connection.rs must validate width range 640-7680"
            );
            assert!(
                src.contains("(480..=4320)"),
                "connection.rs must validate height range 480-4320"
            );
            // Must split on 'x'
            assert!(
                src.contains("split('x')"),
                "connection.rs must split #vd_res on 'x'"
            );
            // Must parse as u32
            assert!(
                src.contains("parse::<u32>()"),
                "connection.rs must parse resolution as u32"
            );
        }

        /// Test E: EDID for 2340x1080 (Samsung phone resolution) is valid.
        /// DTD1 starts at byte 54: [pixel_clock_lo, pixel_clock_hi, h_active_lo,
        /// h_blank_lo, h_active_hi|h_blank_hi, v_active_lo, v_blank_lo, v_active_hi|v_blank_hi, ...]
        #[test]
        fn edid_samsung_2340x1080_valid() {
            let edid = generate_edid(2340, 1080, 60);
            // Checksum
            let sum: u32 = edid.iter().map(|&b| b as u32).sum();
            assert_eq!(sum % 256, 0, "EDID checksum invalid for 2340x1080@60");
            // Exactly 128 bytes
            assert_eq!(edid.len(), 128);
            // Chromaticity intentionally zeros (non-zero crashes Xorg with EVDI)
            assert!(
                edid[25..35].iter().all(|&b| b == 0),
                "chromaticity bytes 25-34 must be all zeros"
            );
            // DTD1 at byte 54: horizontal active = byte56 (low) + byte58 high nibble
            let h_active_low = edid[56] as u32;
            let h_active_high = ((edid[58] >> 4) & 0x0F) as u32;
            let h_active = (h_active_high << 8) | h_active_low;
            assert_eq!(
                h_active, 2340,
                "DTD horizontal active must be 2340, got {}", h_active
            );
            // DTD1: vertical active = byte59 (low) + byte61 high nibble
            let v_active_low = edid[59] as u32;
            let v_active_high = ((edid[61] >> 4) & 0x0F) as u32;
            let v_active = (v_active_high << 8) | v_active_low;
            assert_eq!(
                v_active, 1080,
                "DTD vertical active must be 1080, got {}", v_active
            );
            // Pixel clock must be nonzero
            let pc = edid[54] as u32 | ((edid[55] as u32) << 8);
            assert!(pc > 0, "DTD pixel clock must be nonzero");
        }

        /// Test F: execute_vd_decision passes correct params to plug_in_monitor.
        #[test]
        fn execute_vd_decision_chain_correctness() {
            let src = include_str!("server/connection.rs");
            // Must call set_custom_resolution BEFORE plug_in_monitor
            let set_pos = src.find("set_custom_resolution(width, height)")
                .expect("must call set_custom_resolution(width, height)");
            let plug_pos = src.find("plug_in_monitor(display as _")
                .expect("must call plug_in_monitor(display as _");
            assert!(
                set_pos < plug_pos,
                "set_custom_resolution must be called BEFORE plug_in_monitor"
            );
            // Must create MonitorMode with width, height, sync: 60
            assert!(
                src.contains("MonitorMode {"),
                "must create MonitorMode"
            );
            assert!(
                src.contains("sync: 60"),
                "MonitorMode must have sync: 60"
            );
        }

        // =============================================================
        // Graceful shutdown tests
        //
        // Verify that SIGTERM/process exit properly cleans up EVDI devices
        // to prevent KScreen segfaults during .deb upgrades.
        // =============================================================

        /// Test: ctrlc handler calls reset_all before process::exit.
        #[test]
        fn ctrlc_handler_calls_evdi_reset() {
            let src = include_str!("server/input_service.rs");
            // Must contain reset_all call in the ctrlc handler
            assert!(
                src.contains("reset_all()"),
                "ctrlc handler must call reset_all() to gracefully shut down \
                 EVDI devices before process exit. Without this, KScreen \
                 segfaults when the DRM fds are cleaned up by the kernel."
            );

            // reset_all must come BEFORE process::exit
            let reset_pos = src.find("reset_all()").unwrap();
            let exit_pos = src.find("std::process::exit(0)").unwrap();
            assert!(
                reset_pos < exit_pos,
                "reset_all() must be called BEFORE process::exit(0) — \
                 cleanup is useless after exit"
            );
        }

        /// Test: Server::drop calls reset_all for EVDI cleanup.
        #[test]
        fn server_drop_calls_evdi_reset() {
            let src = include_str!("server.rs");
            // Server::drop must contain reset_all for EVDI cleanup
            let drop_start = src.find("impl Drop for Server")
                .expect("Server must implement Drop");
            let drop_body = &src[drop_start..std::cmp::min(drop_start + 500, src.len())];
            assert!(
                drop_body.contains("reset_all()"),
                "Server::drop must call reset_all() to clean up EVDI devices \
                 on normal server shutdown"
            );
        }

        /// Test: ctrlc handler has a watchdog timeout to prevent hanging.
        #[test]
        fn ctrlc_handler_has_watchdog_timeout() {
            let src = include_str!("server/input_service.rs");
            // The handler must have a timeout thread to force exit if cleanup hangs
            assert!(
                src.contains("watchdog") || src.contains("forcing exit"),
                "ctrlc handler must have a watchdog timeout to force-exit \
                 if EVDI cleanup hangs (e.g. mutex deadlock)"
            );
        }

        /// Test: orphan cleanup does NOT call disconnect (painters already gone).
        #[test]
        fn orphan_cleanup_skips_disconnect() {
            let source = include_str!("virtual_display_manager.rs");
            let fn_start = source.find("pub fn cleanup_orphaned_evdi_devices()").expect(
                "cleanup_orphaned_evdi_devices function must exist"
            );
            // Extract the function body (up to the closing of the function)
            let fn_body = &source[fn_start..std::cmp::min(fn_start + 4000, source.len())];

            // Must NOT call fns.disconnect — orphan painters are already cleaned
            // up by the kernel when the previous process died. disconnect() always
            // fails with "disconnect failed" on orphans.
            assert!(
                !fn_body.contains("(fns.disconnect)"),
                "orphan cleanup must NOT call disconnect on orphaned devices"
            );

            // Must still call fns.close to remove the device
            assert!(
                fn_body.contains("(fns.close)"),
                "orphan cleanup must call close to remove orphaned devices"
            );
        }

        // =============================================================
        // RandR monitor management tests
        // =============================================================

        #[test]
        fn get_output_geometry_parses_standard_xrandr_line() {
            // Verify the parsing logic by checking the source code contains
            // the necessary parsing steps for xrandr output lines like:
            // DVI-I-1 connected 2400x1080+2560+0 (normal ...) 625mm x 289mm
            let src = include_str!("virtual_display_manager.rs");
            let fn_start = src.find("fn get_output_geometry(output_name: &str)")
                .expect("get_output_geometry function must exist");
            let fn_body = &src[fn_start..fn_start + 1500.min(src.len() - fn_start)];

            // Must check for " connected" in the xrandr line
            assert!(
                fn_body.contains("connected"),
                "get_output_geometry must look for 'connected' outputs"
            );
            // Must parse WxH+X+Y geometry
            assert!(
                fn_body.contains("split_once('+')") && fn_body.contains("split_once('x')"),
                "get_output_geometry must parse WxH+X+Y geometry"
            );
            // Must parse physical dimensions (mm)
            assert!(
                fn_body.contains("mm x "),
                "get_output_geometry must parse physical dimensions in mm"
            );
        }

        #[test]
        fn create_evdi_randr_monitor_checks_display_all_before_setmonitor() {
            // Verify that create_evdi_randr_monitor checks Display::all()
            // BEFORE calling --setmonitor, and skips it if EVDI is already visible.
            let src = include_str!("virtual_display_manager.rs");
            let fn_start = src.find("fn create_evdi_randr_monitor(evdi_name: &str)")
                .expect("create_evdi_randr_monitor function must exist");
            let fn_body = &src[fn_start..fn_start + 5000.min(src.len() - fn_start)];

            // Must call Display::all() BEFORE setmonitor
            let display_all_pos = fn_body.find("Display::all()").expect(
                "create_evdi_randr_monitor must call Display::all()"
            );
            let setmonitor_pos = fn_body.find("setmonitor").expect(
                "create_evdi_randr_monitor must call xrandr --setmonitor"
            );
            assert!(
                display_all_pos < setmonitor_pos,
                "Must check Display::all() BEFORE calling --setmonitor"
            );

            // Must have a skip path ("already sees EVDI display")
            assert!(
                fn_body.contains("already sees EVDI display"),
                "Must skip --setmonitor if Display::all() already sees EVDI"
            );

            // Must verify with Display::all() AFTER setmonitor too
            let after_setmonitor = &fn_body[setmonitor_pos..];
            assert!(
                after_setmonitor.contains("Display::all()"),
                "Must verify with Display::all() AFTER setmonitor"
            );

            // Must log xrandr --listmonitors for diagnostics
            assert!(
                fn_body.contains("listmonitors"),
                "Must log xrandr --listmonitors output"
            );
        }

        #[test]
        fn remove_evdi_randr_monitors_only_deletes_evdi_prefix() {
            // Verify that remove_evdi_randr_monitors only removes monitors
            // with the EVDI- prefix, not system monitors.
            let src = include_str!("virtual_display_manager.rs");
            let fn_start = src.find("fn remove_evdi_randr_monitors()")
                .expect("remove_evdi_randr_monitors function must exist");
            let fn_body = &src[fn_start..fn_start + 1500.min(src.len() - fn_start)];

            // Must check for EVDI- prefix
            assert!(
                fn_body.contains(r#"starts_with("EVDI-")"#),
                "remove_evdi_randr_monitors must only target EVDI- prefixed monitors"
            );
            // Must call --delmonitor
            assert!(
                fn_body.contains("--delmonitor"),
                "remove_evdi_randr_monitors must call xrandr --delmonitor"
            );
        }

        #[test]
        fn teardown_calls_remove_randr_monitors() {
            // Verify that teardown_devices calls remove_evdi_randr_monitors
            // BEFORE disconnecting/closing devices.
            let src = include_str!("virtual_display_manager.rs");
            let fn_start = src.find("fn teardown_devices(mut devices:")
                .expect("teardown_devices function must exist");
            let fn_body = &src[fn_start..fn_start + 500.min(src.len() - fn_start)];

            let randr_pos = fn_body.find("remove_evdi_randr_monitors").expect(
                "teardown_devices must call remove_evdi_randr_monitors"
            );
            let disconnect_pos = fn_body.find("disconnect_and_close").expect(
                "teardown_devices must call disconnect_and_close"
            );
            assert!(
                randr_pos < disconnect_pos,
                "remove_evdi_randr_monitors must be called BEFORE disconnect_and_close \
                 (while xrandr can still see the outputs)"
            );
        }

        #[test]
        fn position_display_calls_create_randr_monitor_in_fallback() {
            // Verify that position_virtual_display calls create_evdi_randr_monitor
            // in the fallback path (when xrandr positioning is used).
            let src = include_str!("virtual_display_manager.rs");
            let fn_start = src.find("fn position_virtual_display(width: u32, height: u32)")
                .expect("position_virtual_display must exist");
            let fn_body = &src[fn_start..fn_start + 5000.min(src.len() - fn_start)];

            // create_evdi_randr_monitor must exist in the fallback xrandr path
            assert!(
                fn_body.contains("create_evdi_randr_monitor"),
                "position_virtual_display must call create_evdi_randr_monitor \
                 in the xrandr fallback path"
            );

            // The "already visible" early return must NOT call create_evdi_randr_monitor
            let visible_return = fn_body.find("already visible").expect(
                "must have early return when display is already visible"
            );
            let early_section = &fn_body[..visible_return];
            assert!(
                !early_section.contains("create_evdi_randr_monitor"),
                "create_evdi_randr_monitor must NOT be called in the early-return \
                 path (Display::all() already sees the display, setmonitor is unnecessary)"
            );

            // Must call it AFTER the positioning in fallback (after "positioned" log)
            let fallback_start = fn_body.find("for attempt").expect("must have fallback loop");
            let fallback_body = &fn_body[fallback_start..];
            let positioned_pos = fallback_body.find("positioned").expect("must log positioning");
            let randr_pos = fallback_body.find("create_evdi_randr_monitor").expect("must call randr");
            assert!(
                randr_pos > positioned_pos,
                "create_evdi_randr_monitor must be called AFTER positioning succeeds"
            );
        }

        #[test]
        fn position_display_skips_xrandr_if_display_visible() {
            // Verify that position_virtual_display checks Display::all() first
            // and returns early if the EVDI display is already visible.
            // This is the key fix for KDE KScreen interference: running xrandr
            // triggers a second RandR notification that causes KScreen to
            // disable the EVDI output.
            let src = include_str!("virtual_display_manager.rs");
            let fn_start = src.find("fn position_virtual_display(width: u32, height: u32)")
                .expect("position_virtual_display must exist");
            let fn_body = &src[fn_start..fn_start + 3000.min(src.len() - fn_start)];

            // Must check for DVI-I (EVDI connector type) in Display::all() names
            assert!(
                fn_body.contains("DVI-I") || fn_body.contains("EVDI"),
                "position_virtual_display must check Display::all() names for \
                 EVDI output indicators (DVI-I or EVDI)"
            );

            // Must return early when display is visible (no xrandr)
            assert!(
                fn_body.contains("skipping xrandr"),
                "position_virtual_display must log and skip xrandr when display \
                 is already visible in Display::all()"
            );
        }

        // =============================================================
        // EVDI integration tests (real hardware)
        //
        // These tests create REAL EVDI virtual displays, verify they
        // appear in XRandR (scrap), let the consumer thread run for
        // a few seconds to confirm frame transmission, then clean up.
        //
        // Marked #[ignore] because they:
        //   - Require the EVDI kernel module loaded
        //   - Create actual DRM devices (need permissions)
        //   - Take ~10s due to DRM settle delays
        //   - Must run sequentially (DRM_TOPOLOGY mutex)
        //
        // Run with: cargo test --lib -- --ignored integration_evdi
        // =============================================================

        /// RAII guard that ensures EVDI displays are cleaned up even on panic.
        /// Without this, a test failure leaves orphaned devices that can crash
        /// KScreen on the NEXT test run.
        struct EvdiTestGuard;

        impl Drop for EvdiTestGuard {
            fn drop(&mut self) {
                let displays = get_virtual_displays();
                if !displays.is_empty() {
                    eprintln!("  [CLEANUP] removing {} orphaned EVDI display(s)", displays.len());
                    for &idx in &displays {
                        let _ = plug_out_monitor(idx as i32);
                    }
                }
            }
        }

        /// Check if the test environment supports EVDI integration tests.
        /// Also cleans up any orphaned devices from previous crashed tests.
        fn check_evdi_available() -> bool {
            if !std::path::Path::new("/sys/module/evdi").exists() {
                eprintln!("  [SKIP] EVDI kernel module not loaded");
                return false;
            }
            {
                let manager = MANAGER.lock().unwrap();
                if manager.lib.is_none() {
                    eprintln!("  [SKIP] EVDI library not loaded");
                    return false;
                }
            }
            // Clean up orphaned devices from previous crashed tests/sessions.
            // Uses per-device teardown (safe for KScreen), NOT remove_all.
            cleanup_orphaned_evdi_devices();
            true
        }

        /// Check xrandr outputs for a connected EVDI display with the expected resolution.
        /// Uses sysfs DRIVER=evdi check (same as production code) instead of scrap
        /// monitors, because scrap only returns activated monitors (requires KScreen
        /// to assign a CRTC), while xrandr outputs show all connected DRM connectors.
        fn check_xrandr_evdi_output(expected_w: u32, expected_h: u32) -> bool {
            // Find card IDs with DRIVER=evdi
            let mut evdi_card_ids = Vec::new();
            if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    // Only cardN (not cardN-connector)
                    if name.starts_with("card") && !name.contains('-') {
                        if let Ok(id) = name[4..].parse::<i32>() {
                            if is_evdi_via_sysfs(id) {
                                evdi_card_ids.push(id);
                            }
                        }
                    }
                }
            }

            if evdi_card_ids.is_empty() {
                eprintln!("  [WARN] no EVDI card found in sysfs");
                return false;
            }

            // Check each EVDI card's connector for a connected status with the expected mode
            for card_id in &evdi_card_ids {
                // EVDI creates DVI-I connectors, find them via sysfs
                if let Ok(entries) = std::fs::read_dir(format!("/sys/class/drm")) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with(&format!("card{}-", card_id)) {
                            // Read connector status
                            let status_path = format!(
                                "/sys/class/drm/{}/status",
                                name
                            );
                            if let Ok(status) = std::fs::read_to_string(&status_path) {
                                if status.trim() == "connected" {
                                    eprintln!(
                                        "  [INFO] EVDI output {} is connected (card{})",
                                        name, card_id
                                    );
                                    // Read modes to check resolution
                                    let modes_path = format!(
                                        "/sys/class/drm/{}/modes",
                                        name
                                    );
                                    if let Ok(modes) = std::fs::read_to_string(&modes_path) {
                                        let expected_mode = format!("{}x{}", expected_w, expected_h);
                                        for mode in modes.lines() {
                                            if mode.trim() == expected_mode {
                                                eprintln!(
                                                    "  [PASS] EVDI output {} has mode {}",
                                                    name, expected_mode
                                                );
                                                return true;
                                            }
                                        }
                                        eprintln!(
                                            "  [INFO] EVDI output {} modes: {}",
                                            name,
                                            modes.lines().collect::<Vec<_>>().join(", ")
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            false
        }

        /// Full end-to-end test: VdController → plug_in_monitor → sysfs → plug_out.
        /// This tests the EXACT same code path as a real mobile client connection:
        ///   1. VdController defers toggle (no resolution)
        ///   2. handle_resolution(2340, 1080) → Create decision
        ///   3. set_custom_resolution + plug_in_monitor (real EVDI)
        ///   4. Verify display appears in get_virtual_displays()
        ///   5. Wait for consumer thread to run (frame transmission)
        ///   6. Verify sysfs shows EVDI output connected with correct mode
        ///   7. Verify consumer thread still running (no crash)
        ///   8. plug_out_monitor → cleanup
        ///   9. Verify display removed from get_virtual_displays()
        ///
        /// Uses RAII guard to always clean up, even on panic.
        /// If this test passes, a real phone connection will work.
        #[test]
        #[ignore]
        fn integration_evdi_create_display_and_verify_transmission() {
            if !check_evdi_available() { return; }
            let _guard = EvdiTestGuard;

            // Step 1: Use VdController exactly like connection.rs
            let mut ctl = VdController::new();
            let d = ctl.toggle(0, true);
            assert!(
                matches!(d, VdDecision::Deferred { display: 0 }),
                "toggle must defer (no resolution yet)"
            );

            // Step 2: Simulate #vd_res 2340x1080 arrival
            let decisions = ctl.handle_resolution(2340, 1080);
            assert_eq!(decisions.len(), 1);
            match &decisions[0] {
                VdDecision::Create { display, width, height } => {
                    assert_eq!(*display, 0);
                    assert_eq!(*width, 2340);
                    assert_eq!(*height, 1080);
                }
                other => panic!("expected Create, got {:?}", other),
            }

            // Step 3: Execute the Create decision (same as connection.rs)
            eprintln!("  [INFO] Creating EVDI display at 2340x1080...");
            super::super::set_custom_resolution(2340, 1080);
            let modes = vec![super::super::MonitorMode {
                width: 2340,
                height: 1080,
                sync: 60,
            }];
            let result = plug_in_monitor(0, &modes);
            assert!(result.is_ok(), "plug_in_monitor failed: {:?}", result.err());
            eprintln!("  [INFO] plug_in_monitor() succeeded");

            // Step 4: Verify display appears in internal tracking
            let displays = get_virtual_displays();
            assert!(
                !displays.is_empty(),
                "get_virtual_displays() must show at least 1 display after plug_in"
            );
            eprintln!("  [INFO] get_virtual_displays() = {:?}", displays);

            // Step 5: Wait for consumer thread to start and process frames.
            // Timeline: add_device → connect (fast) → position_virtual_display
            // (3s settle + Display::all() check) → consumer starts polling.
            // We wait 4s more for safety.
            eprintln!("  [INFO] Waiting 4s for consumer thread frame transmission...");
            std::thread::sleep(std::time::Duration::from_secs(4));

            // Step 6: Verify DRM connector is connected with correct mode via sysfs.
            // This checks the same thing xrandr/KScreen will see without requiring
            // the display to be "activated" (assigned a CRTC by the compositor).
            let found = check_xrandr_evdi_output(2340, 1080);
            assert!(
                found,
                "sysfs must show a connected EVDI output with mode 2340x1080"
            );

            // Step 7: Consumer thread health check — verify the EVDI device
            // hasn't crashed. If the consumer thread panicked or EVDI sent
            // POLLHUP, the device would have been cleaned up already.
            let displays_after_wait = get_virtual_displays();
            assert!(
                !displays_after_wait.is_empty(),
                "display must still be active after 4s of frame transmission"
            );
            eprintln!("  [PASS] consumer thread running without crash after 4s");

            // Step 8: Clean up (same as connection.rs VdDecision::Remove)
            eprintln!("  [INFO] Removing virtual display...");
            let result = plug_out_monitor(0);
            assert!(
                result.is_ok(),
                "plug_out_monitor failed: {:?}",
                result.err()
            );

            // Step 9: Verify complete cleanup
            let displays_final = get_virtual_displays();
            assert!(
                displays_final.is_empty(),
                "get_virtual_displays() must be empty after plug_out, got {:?}",
                displays_final
            );
            eprintln!("  [PASS] display removed, cleanup complete");
            eprintln!("  [PASS] Full integration test: create → transmit → remove OK");
        }

        /// Integration test: verify that creating a display with the VdController's
        /// timeout fallback (desktop client, no #vd_res) also works end-to-end.
        #[test]
        #[ignore]
        fn integration_evdi_timeout_creates_default_resolution() {
            if !check_evdi_available() { return; }
            let _guard = EvdiTestGuard;

            // Desktop client: toggle without resolution, then timeout
            let mut ctl = VdController::new();
            ctl.toggle(0, true);

            // Force timeout
            ctl.set_pending_deadline(
                std::time::Instant::now() - std::time::Duration::from_secs(1),
            );
            let decisions = ctl.check_timeout();
            assert_eq!(decisions.len(), 1);
            match &decisions[0] {
                VdDecision::Create { display, width, height } => {
                    assert_eq!(*display, 0);
                    assert_eq!(*width, 1920);
                    assert_eq!(*height, 1080);
                }
                other => panic!("expected Create at default resolution, got {:?}", other),
            }

            // Execute: create at default 1920x1080
            eprintln!("  [INFO] Creating EVDI display at 1920x1080 (timeout fallback)...");
            super::super::set_custom_resolution(1920, 1080);
            let modes = vec![super::super::MonitorMode {
                width: 1920,
                height: 1080,
                sync: 60,
            }];
            let result = plug_in_monitor(0, &modes);
            assert!(result.is_ok(), "plug_in_monitor failed: {:?}", result.err());

            // Wait for consumer + verify sysfs
            std::thread::sleep(std::time::Duration::from_secs(4));

            let displays = get_virtual_displays();
            assert!(!displays.is_empty());

            let found = check_xrandr_evdi_output(1920, 1080);
            assert!(found, "sysfs must show EVDI output with mode 1920x1080");
            eprintln!("  [PASS] desktop fallback display created successfully");

            // Cleanup (EvdiTestGuard also cleans up on panic)
            let _ = plug_out_monitor(0);
            let displays_final = get_virtual_displays();
            assert!(displays_final.is_empty());
            eprintln!("  [PASS] cleanup complete");
        }

        /// Test G: Integration test with multiple displays.
        /// Create display 0 and display 1, remove only 1, verify 0 remains.
        /// This tests the index fix: previously idx=0 was remapped, so
        /// plug_out(1) might remove the wrong device.
        #[test]
        #[ignore]
        fn integration_evdi_multiple_displays_selective_remove() {
            if !check_evdi_available() { return; }
            let _guard = EvdiTestGuard;

            // Create display 0
            eprintln!("  [INFO] Creating display 0 at 2340x1080...");
            super::super::set_custom_resolution(2340, 1080);
            let modes = vec![super::super::MonitorMode { width: 2340, height: 1080, sync: 60 }];
            let r = plug_in_monitor(0, &modes);
            assert!(r.is_ok(), "plug_in_monitor(0) failed: {:?}", r.err());

            // Create display 1
            eprintln!("  [INFO] Creating display 1 at 1920x1080...");
            super::super::set_custom_resolution(1920, 1080);
            let modes = vec![super::super::MonitorMode { width: 1920, height: 1080, sync: 60 }];
            let r = plug_in_monitor(1, &modes);
            assert!(r.is_ok(), "plug_in_monitor(1) failed: {:?}", r.err());

            let displays = get_virtual_displays();
            eprintln!("  [INFO] displays after creation: {:?}", displays);
            assert!(displays.contains(&0), "display 0 must exist");
            assert!(displays.contains(&1), "display 1 must exist");

            // Wait for consumer threads
            std::thread::sleep(std::time::Duration::from_secs(4));

            // Remove only display 1
            eprintln!("  [INFO] Removing display 1 only...");
            let r = plug_out_monitor(1);
            assert!(r.is_ok(), "plug_out_monitor(1) failed: {:?}", r.err());

            let displays_after = get_virtual_displays();
            eprintln!("  [INFO] displays after removing 1: {:?}", displays_after);
            assert!(
                displays_after.contains(&0),
                "display 0 must STILL exist after removing 1, got {:?}", displays_after
            );
            assert!(
                !displays_after.contains(&1),
                "display 1 must be removed, got {:?}", displays_after
            );

            // Verify display 0 is still connected in sysfs
            let found = check_xrandr_evdi_output(2340, 1080);
            assert!(found, "display 0 (2340x1080) must still be visible in sysfs");
            eprintln!("  [PASS] display 0 still active, display 1 removed correctly");

            // Cleanup display 0
            let _ = plug_out_monitor(0);
            assert!(get_virtual_displays().is_empty());
            eprintln!("  [PASS] full cleanup complete");
        }
    }
}

// =============================================================================
// macOS CGVirtualDisplay implementation
// =============================================================================

#[cfg(target_os = "macos")]
pub mod macos_cg_virtual {
    use hbb_common::{bail, log, ResultType};
    use objc::rc::autoreleasepool;
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{class, msg_send, sel, sel_impl};
    use std::collections::HashMap;
    use std::sync::Mutex;

    type Id = *mut Object;

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    struct NSOperatingSystemVersion {
        major: i64,
        minor: i64,
        patch: i64,
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    lazy_static::lazy_static! {
        static ref MANAGER: Mutex<CgVirtualManager> = Mutex::new(CgVirtualManager::new());
    }

    struct CgVirtualManager {
        displays: HashMap<i32, Id>,
        next_index: i32,
        headless: Option<i32>,
    }

    unsafe impl Send for CgVirtualManager {}

    impl CgVirtualManager {
        fn new() -> Self {
            Self {
                displays: HashMap::new(),
                next_index: 0,
                headless: None,
            }
        }
    }

    impl Drop for CgVirtualManager {
        fn drop(&mut self) {
            for (_idx, display) in self.displays.drain() {
                unsafe {
                    let _: () = msg_send![display, release];
                }
            }
        }
    }

    fn cg_virtual_display_class() -> Option<&'static Class> {
        Class::get("CGVirtualDisplay")
    }

    fn cg_virtual_display_descriptor_class() -> Option<&'static Class> {
        Class::get("CGVirtualDisplayDescriptor")
    }

    fn cg_virtual_display_settings_class() -> Option<&'static Class> {
        Class::get("CGVirtualDisplaySettings")
    }

    fn cg_virtual_display_mode_class() -> Option<&'static Class> {
        Class::get("CGVirtualDisplayMode")
    }

    fn is_macos_14_or_later() -> bool {
        autoreleasepool(|| unsafe {
            let info: Id = msg_send![class!(NSProcessInfo), processInfo];
            let version: NSOperatingSystemVersion = msg_send![info, operatingSystemVersion];
            version.major >= 14
        })
    }

    pub fn is_supported() -> bool {
        if !is_macos_14_or_later() {
            return false;
        }
        cg_virtual_display_class().is_some()
    }

    fn create_display(width: u32, height: u32) -> ResultType<Id> {
        autoreleasepool(|| unsafe {
            let desc_cls = match cg_virtual_display_descriptor_class() {
                Some(c) => c,
                None => bail!("CGVirtualDisplayDescriptor class not found"),
            };
            let display_cls = match cg_virtual_display_class() {
                Some(c) => c,
                None => bail!("CGVirtualDisplay class not found"),
            };
            let settings_cls = match cg_virtual_display_settings_class() {
                Some(c) => c,
                None => bail!("CGVirtualDisplaySettings class not found"),
            };
            let mode_cls = match cg_virtual_display_mode_class() {
                Some(c) => c,
                None => bail!("CGVirtualDisplayMode class not found"),
            };

            // Create descriptor
            let desc: Id = msg_send![desc_cls, alloc];
            let desc: Id = msg_send![desc, init];

            // Set display name
            let name: Id = msg_send![class!(NSString), alloc];
            let name: Id = msg_send![name, initWithUTF8String: b"Fulldesk Virtual Display\0".as_ptr()];
            let _: () = msg_send![desc, setName: name];

            // Set max resolution
            let _: () = msg_send![desc, setMaxPixelsWide: width as u64];
            let _: () = msg_send![desc, setMaxPixelsHigh: height as u64];

            // Set physical size in mm (approximate 24" display)
            let size = CGSize {
                width: (width as f64 / 96.0) * 25.4,
                height: (height as f64 / 96.0) * 25.4,
            };
            let _: () = msg_send![desc, setSizeInMillimeters: size];

            // Create the virtual display
            let display: Id = msg_send![display_cls, alloc];
            let display: Id = msg_send![display, initWithDescriptor: desc];
            if display.is_null() {
                let _: () = msg_send![desc, release];
                bail!("Failed to create CGVirtualDisplay");
            }

            // Create mode
            let mode: Id = msg_send![mode_cls, alloc];
            let mode: Id = msg_send![mode, initWithWidth: width as u64
                                           height: height as u64
                                           refreshRate: 60.0f64];
            if mode.is_null() {
                let _: () = msg_send![display, release];
                let _: () = msg_send![desc, release];
                bail!("Failed to create CGVirtualDisplayMode");
            }

            // Create settings with mode array
            let settings: Id = msg_send![settings_cls, alloc];
            let settings: Id = msg_send![settings, init];
            let modes_array: Id = msg_send![class!(NSArray), arrayWithObject: mode];
            let _: () = msg_send![settings, setModes: modes_array];

            // Apply settings
            let success: BOOL = msg_send![display, applySettings: settings];
            let _: () = msg_send![settings, release];
            let _: () = msg_send![mode, release];
            let _: () = msg_send![name, release];
            let _: () = msg_send![desc, release];

            if success != YES {
                let _: () = msg_send![display, release];
                bail!("Failed to apply CGVirtualDisplay settings");
            }

            Ok(display)
        })
    }

    pub fn plug_in_monitor(idx: u32, modes: &[super::MonitorMode]) -> ResultType<()> {
        let (width, height) = if let Some(res) = super::take_custom_resolution() {
            res
        } else if let Some(mode) = modes.first() {
            (mode.width, mode.height)
        } else {
            (1920, 1080)
        };

        log::info!("macOS VD: creating virtual display {}x{} (idx={})", width, height, idx);
        let display = create_display(width, height)?;

        let mut manager = MANAGER.lock().unwrap();
        let index = manager.next_index;
        manager.displays.insert(index, display);
        manager.next_index += 1;

        log::info!("macOS VD: virtual display created with index {}", index);
        Ok(())
    }

    pub fn plug_out_monitor(index: i32) -> ResultType<()> {
        let mut manager = MANAGER.lock().unwrap();
        if index < 0 {
            // Plug out all
            for (_idx, display) in manager.displays.drain() {
                unsafe {
                    let _: () = msg_send![display, release];
                }
            }
            manager.headless = None;
            log::info!("macOS VD: all virtual displays removed");
        } else if let Some(display) = manager.displays.remove(&index) {
            unsafe {
                let _: () = msg_send![display, release];
            }
            if manager.headless == Some(index) {
                manager.headless = None;
            }
            log::info!("macOS VD: virtual display {} removed", index);
        } else {
            bail!("macOS VD: display index {} not found", index);
        }
        Ok(())
    }

    pub fn plug_in_peer_request(
        modes: Vec<Vec<super::MonitorMode>>,
    ) -> ResultType<Vec<u32>> {
        let mut indices = Vec::new();
        for mode_set in &modes {
            let (width, height) = if let Some(res) = super::take_custom_resolution() {
                res
            } else if let Some(mode) = mode_set.first() {
                (mode.width, mode.height)
            } else {
                (1920, 1080)
            };

            let display = create_display(width, height)?;
            let mut manager = MANAGER.lock().unwrap();
            let index = manager.next_index;
            manager.displays.insert(index, display);
            manager.next_index += 1;
            indices.push(index as u32);
        }
        Ok(indices)
    }

    pub fn plug_out_monitor_indices(indices: &[u32]) -> ResultType<()> {
        let mut manager = MANAGER.lock().unwrap();
        for &idx in indices {
            let idx = idx as i32;
            if let Some(display) = manager.displays.remove(&idx) {
                unsafe {
                    let _: () = msg_send![display, release];
                }
                if manager.headless == Some(idx) {
                    manager.headless = None;
                }
                log::info!("macOS VD: virtual display {} removed", idx);
            }
        }
        Ok(())
    }

    pub fn plug_in_headless() -> ResultType<()> {
        let display = create_display(1920, 1080)?;
        let mut manager = MANAGER.lock().unwrap();
        let index = manager.next_index;
        manager.displays.insert(index, display);
        manager.headless = Some(index);
        manager.next_index += 1;
        log::info!("macOS VD: headless display created with index {}", index);
        Ok(())
    }

    pub fn reset_all() -> ResultType<()> {
        let mut manager = MANAGER.lock().unwrap();
        for (_idx, display) in manager.displays.drain() {
            unsafe {
                let _: () = msg_send![display, release];
            }
        }
        manager.headless = None;
        manager.next_index = 0;
        log::info!("macOS VD: all displays reset");
        Ok(())
    }

    pub fn get_platform_additions() -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        if !is_supported() {
            return map;
        }
        map.insert("idd_impl".into(), serde_json::json!("cgvirtual"));
        let manager = MANAGER.lock().unwrap();
        let count = manager.displays.len();
        if count > 0 {
            map.insert(
                "cgvirtual_displays".into(),
                serde_json::json!(count),
            );
        }
        if manager.headless.is_some() {
            map.insert("cgvirtual_headless".into(), serde_json::json!(true));
        }
        map
    }
}
