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
    CUSTOM_VD_RESOLUTION.lock().unwrap().take()
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
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
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
    #[cfg(not(any(windows, target_os = "linux")))]
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
    #[cfg(not(any(windows, target_os = "linux")))]
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
    #[cfg(not(any(windows, target_os = "linux")))]
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
    #[cfg(not(any(windows, target_os = "linux")))]
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
    #[cfg(not(any(windows, target_os = "linux")))]
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
    #[cfg(not(any(windows, target_os = "linux")))]
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
    #[cfg(not(any(windows, target_os = "linux")))]
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
    use crate::platform::{reg_display_settings, win_device};
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
        let reg_connectivity_old = reg_display_settings::read_reg_connectivity();
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
        if let Ok(old_connectivity_old) = reg_connectivity_old {
            std::thread::spawn(move || {
                try_reset_resolution_on_first_plug_in(old_connectivity_old.len(), width, height);
            });
        }

        Ok(())
    }

    fn try_reset_resolution_on_first_plug_in(
        old_connectivity_len: usize,
        width: usize,
        height: usize,
    ) {
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(300));
            if let Ok(reg_connectivity_new) = reg_display_settings::read_reg_connectivity() {
                if reg_connectivity_new.len() != old_connectivity_len {
                    let (w, h) = (width, height);
                    log::info!(
                        "Amyuni: applying resolution {}x{} to virtual display(s)",
                        w,
                        h
                    );
                    for name in
                        windows::get_device_names(Some(super::AMYUNI_IDD_DEVICE_STRING)).iter()
                    {
                        match crate::platform::change_resolution(&name, w, h) {
                            Ok(_) => log::info!("Amyuni: successfully set {} to {}x{}", name, w, h),
                            Err(e) => {
                                log::error!("Amyuni: failed to set {} to {}x{}: {}", name, w, h, e)
                            }
                        }
                    }
                    break;
                }
            }
        }
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

    fn update_amyuni_registry_resolution(w: u32, h: u32) -> ResultType<()> {
        use winreg::RegKey;
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\WUDF\Services\usbmmIdd\Parameters\Monitors";
        let key = hklm.open_subkey_with_flags(path, KEY_READ | KEY_WRITE)?;

        let resolution = format!("{},{}", w, h);

        let mut max_index: i32 = -1;
        for i in 0..20 {
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

        let new_index = (max_index + 1).to_string();
        key.set_value(&new_index, &resolution)?;
        log::info!(
            "Amyuni: added resolution {}x{} to registry at index {}",
            w, h, new_index
        );

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
    use std::sync::{Arc, Mutex};

    type EvdiHandle = *mut c_void;

    // FFI function types for libevdi
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

    struct EvdiLib {
        _lib_handle: *mut c_void,
        check_device: FnEvdiCheckDevice,
        add_device: FnEvdiAddDevice,
        open: FnEvdiOpen,
        close: FnEvdiClose,
        connect: FnEvdiConnect,
        disconnect: FnEvdiDisconnect,
    }

    // Safety: EvdiLib contains function pointers and an opaque library handle.
    // All access is synchronized through the MANAGER mutex.
    unsafe impl Send for EvdiLib {}

    impl EvdiLib {
        fn load() -> Option<Self> {
            unsafe {
                // Try libevdi.so.0 first, then libevdi.so
                let lib = hbb_common::libc::dlopen(
                    b"libevdi.so.0\0".as_ptr() as *const hbb_common::libc::c_char,
                    hbb_common::libc::RTLD_NOW,
                );
                let lib = if lib.is_null() {
                    let lib = hbb_common::libc::dlopen(
                        b"libevdi.so\0".as_ptr() as *const hbb_common::libc::c_char,
                        hbb_common::libc::RTLD_NOW,
                    );
                    if lib.is_null() {
                        log::info!("EVDI: libevdi.so not found: {}", get_dl_error());
                        return None;
                    }
                    lib
                } else {
                    lib
                };

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

                log::info!("EVDI: libevdi loaded successfully");
                Some(Self {
                    _lib_handle: lib,
                    check_device,
                    add_device,
                    open,
                    close,
                    connect,
                    disconnect,
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

    struct EvdiDevice {
        handle: EvdiHandle,
        device_id: i32,
    }

    // Safety: EvdiDevice contains an opaque handle pointer.
    // All access is synchronized through the MANAGER mutex.
    unsafe impl Send for EvdiDevice {}

    impl EvdiDevice {
        fn disconnect_and_close_fns(
            &self,
            disconnect_fn: FnEvdiDisconnect,
            close_fn: FnEvdiClose,
        ) {
            unsafe {
                (disconnect_fn)(self.handle);
                (close_fn)(self.handle);
            }
            log::info!("EVDI: device {} disconnected and closed", self.device_id);
        }
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

    lazy_static::lazy_static! {
        static ref MANAGER: Arc<Mutex<VirtualDisplayManager>> =
            Arc::new(Mutex::new(VirtualDisplayManager::default()));
    }

    pub fn is_supported() -> bool {
        // Check if kernel module is loaded AND libevdi is available
        let module_loaded = std::path::Path::new("/sys/module/evdi").exists();
        let lib_available = MANAGER.lock().unwrap().lib.is_some();
        if lib_available && !module_loaded {
            log::info!("EVDI: libevdi found but kernel module not loaded (try: sudo modprobe evdi)");
        }
        lib_available && module_loaded
    }

    pub fn plug_in_headless() -> ResultType<()> {
        let mut manager = MANAGER.lock().unwrap();
        let lib = manager
            .lib
            .as_ref()
            .ok_or_else(|| hbb_common::anyhow::anyhow!("EVDI library not loaded"))?;
        let add_device_fn = lib.add_device;
        let open_fn = lib.open;
        let connect_fn = lib.connect;

        if manager.headless.is_some() {
            log::debug!("EVDI: headless display already exists");
            return Ok(());
        }

        // Add a new EVDI device
        let device_id = unsafe { (add_device_fn)() };
        if device_id < 0 {
            bail!("EVDI: failed to add device (requires root/video group permissions)");
        }

        // Open the device
        let handle = unsafe { (open_fn)(device_id) };
        if handle.is_null() {
            bail!("EVDI: failed to open device {}", device_id);
        }

        // Generate EDID for 1920x1080@60Hz and connect
        let edid = generate_edid(1920, 1080, 60);
        let area_limit = 1920u32 * 1080u32;

        unsafe {
            (connect_fn)(handle, edid.as_ptr(), edid.len() as c_uint, area_limit);
        }

        log::info!(
            "EVDI: headless virtual display created (device {}, 1920x1080@60Hz)",
            device_id
        );
        manager.headless = Some(EvdiDevice { handle, device_id });

        Ok(())
    }

    pub fn plug_in_monitor(idx: u32, modes: &[super::MonitorMode]) -> ResultType<()> {
        let mut manager = MANAGER.lock().unwrap();
        let lib = manager
            .lib
            .as_ref()
            .ok_or_else(|| hbb_common::anyhow::anyhow!("EVDI library not loaded"))?;
        let add_device_fn = lib.add_device;
        let open_fn = lib.open;
        let connect_fn = lib.connect;

        if manager.peers.contains_key(&idx) {
            return Ok(());
        }

        let (width, height, refresh) = if let Some(m) = modes.first() {
            (m.width, m.height, m.sync)
        } else {
            (1920, 1080, 60)
        };

        let device_id = unsafe { (add_device_fn)() };
        if device_id < 0 {
            bail!("EVDI: failed to add device");
        }

        let handle = unsafe { (open_fn)(device_id) };
        if handle.is_null() {
            bail!("EVDI: failed to open device {}", device_id);
        }

        let edid = generate_edid(width, height, refresh);
        let area_limit = width * height;

        unsafe {
            (connect_fn)(handle, edid.as_ptr(), edid.len() as c_uint, area_limit);
        }

        log::info!(
            "EVDI: virtual display {} created (device {}, {}x{}@{}Hz)",
            idx,
            device_id,
            width,
            height,
            refresh
        );
        manager.peers.insert(idx, EvdiDevice { handle, device_id });

        Ok(())
    }

    pub fn plug_in_peer_request(
        modes: Vec<Vec<super::MonitorMode>>,
    ) -> ResultType<Vec<u32>> {
        let mut manager = MANAGER.lock().unwrap();
        let lib = manager
            .lib
            .as_ref()
            .ok_or_else(|| hbb_common::anyhow::anyhow!("EVDI library not loaded"))?;
        // Copy function pointers to avoid borrow conflict with manager mutation
        let add_device_fn = lib.add_device;
        let open_fn = lib.open;
        let connect_fn = lib.connect;

        let mut indices = Vec::new();

        for mode_set in &modes {
            // Find next available index
            let mut idx = manager.next_peer_index;
            while manager.peers.contains_key(&idx) {
                idx += 1;
            }

            let (width, height, refresh) = if let Some(m) = mode_set.first() {
                (m.width, m.height, m.sync)
            } else {
                (1920, 1080, 60)
            };

            let device_id = unsafe { (add_device_fn)() };
            if device_id < 0 {
                log::error!("EVDI: failed to add device for peer index {}", idx);
                continue;
            }

            let handle = unsafe { (open_fn)(device_id) };
            if handle.is_null() {
                log::error!(
                    "EVDI: failed to open device {} for peer index {}",
                    device_id,
                    idx
                );
                continue;
            }

            let edid = generate_edid(width, height, refresh);
            let area_limit = width * height;

            unsafe {
                (connect_fn)(handle, edid.as_ptr(), edid.len() as c_uint, area_limit);
            }

            log::info!(
                "EVDI: peer virtual display {} created (device {}, {}x{}@{}Hz)",
                idx,
                device_id,
                width,
                height,
                refresh
            );
            manager.peers.insert(idx, EvdiDevice { handle, device_id });
            indices.push(idx);
            manager.next_peer_index = idx + 1;
        }

        Ok(indices)
    }

    pub fn plug_out_monitor(index: i32) -> ResultType<()> {
        let mut manager = MANAGER.lock().unwrap();
        let lib = manager
            .lib
            .as_ref()
            .ok_or_else(|| hbb_common::anyhow::anyhow!("EVDI library not loaded"))?;
        // Copy function pointers to avoid borrow conflict
        let disconnect_fn = lib.disconnect;
        let close_fn = lib.close;

        if index < 0 {
            // Plug out all
            let devices: Vec<EvdiDevice> = manager.peers.drain().map(|(_, d)| d).collect();
            for device in devices {
                device.disconnect_and_close_fns(disconnect_fn, close_fn);
            }
            if let Some(headless) = manager.headless.take() {
                headless.disconnect_and_close_fns(disconnect_fn, close_fn);
            }
        } else {
            let idx = index as u32;
            if let Some(device) = manager.peers.remove(&idx) {
                device.disconnect_and_close_fns(disconnect_fn, close_fn);
            }
        }

        Ok(())
    }

    pub fn plug_out_monitor_indices(indices: &[u32]) -> ResultType<()> {
        let mut manager = MANAGER.lock().unwrap();
        let lib = manager
            .lib
            .as_ref()
            .ok_or_else(|| hbb_common::anyhow::anyhow!("EVDI library not loaded"))?;
        let disconnect_fn = lib.disconnect;
        let close_fn = lib.close;

        for &idx in indices {
            if let Some(device) = manager.peers.remove(&idx) {
                device.disconnect_and_close_fns(disconnect_fn, close_fn);
            }
        }

        Ok(())
    }

    pub fn reset_all() -> ResultType<()> {
        let mut manager = MANAGER.lock().unwrap();
        if let Some(lib) = &manager.lib {
            let disconnect_fn = lib.disconnect;
            let close_fn = lib.close;
            let devices: Vec<EvdiDevice> = manager.peers.drain().map(|(_, d)| d).collect();
            for device in devices {
                device.disconnect_and_close_fns(disconnect_fn, close_fn);
            }
            if let Some(headless) = manager.headless.take() {
                headless.disconnect_and_close_fns(disconnect_fn, close_fn);
            }
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
        let manager = MANAGER.lock().unwrap();
        let vds: Vec<u32> = manager.peers.keys().cloned().collect();
        if !vds.is_empty() {
            map.insert(
                "evdi_virtual_displays".into(),
                serde_json::json!(vds),
            );
        }
        if manager.headless.is_some() {
            map.insert("evdi_headless".into(), serde_json::json!(true));
        }
        map
    }

    // =========================================================================
    // EDID generation
    // =========================================================================

    /// Generate a valid 128-byte EDID for the given resolution.
    /// Manufacturer ID: "RHZ" (Rust Horizon)
    fn generate_edid(width: u32, height: u32, refresh: u32) -> [u8; 128] {
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

        // Standard timings (bytes 38-53): all unused (0x01 0x01)
        for i in (38..54).step_by(2) {
            edid[i] = 0x01;
            edid[i + 1] = 0x01;
        }

        // DTD 1 (bytes 54-71): preferred timing descriptor
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

    /// Write a Detailed Timing Descriptor (18 bytes) for the given resolution.
    fn write_dtd(buf: &mut [u8], width: u32, height: u32, refresh: u32) {
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
        let pixel_clock_10khz = (h_total as u64 * v_total as u64 * refresh as u64) / 10_000;

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
        let h_blank: u32 = match width {
            w if w <= 1920 => 280,
            w if w <= 2560 => 160,
            _ => 560,
        };
        let v_blank: u32 = match height {
            h if h <= 1080 => 45,
            h if h <= 1440 => 41,
            _ => 90,
        };
        ((width + h_blank) as u64 * (height + v_blank) as u64 * refresh as u64)
            .min(u32::MAX as u64) as u32
    }
}
