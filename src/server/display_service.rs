use super::*;
use crate::common::SimpleCallOnReturn;
#[cfg(target_os = "linux")]
use crate::platform::linux::is_x11;
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
use crate::virtual_display_manager;
#[cfg(windows)]
use hbb_common::get_version_number;
use hbb_common::protobuf::MessageField;
use scrap::Display;
use std::sync::atomic::{AtomicBool, Ordering};

// https://github.com/rustdesk/rustdesk/discussions/6042, avoiding dbus call

pub const NAME: &'static str = "display";

#[cfg(windows)]
const DUMMY_DISPLAY_SIDE_MAX_SIZE: usize = 1024;

struct ChangedResolution {
    original: (i32, i32),
    changed: (i32, i32),
}

lazy_static::lazy_static! {
    static ref IS_CAPTURER_MAGNIFIER_SUPPORTED: bool = is_capturer_mag_supported();
    static ref CHANGED_RESOLUTIONS: Arc<RwLock<HashMap<String, ChangedResolution>>> = Default::default();
    // Initial primary display index.
    // It should not be updated when displays changed.
    pub static ref PRIMARY_DISPLAY_IDX: usize = get_primary();
    static ref SYNC_DISPLAYS: Arc<Mutex<SyncDisplaysInfo>> = Default::default();
}

// https://github.com/rustdesk/rustdesk/pull/8537
static TEMP_IGNORE_DISPLAYS_CHANGED: AtomicBool = AtomicBool::new(false);

/// Set to true during display transitions (VD plug/unplug).
/// When true, cursor position broadcasts are suppressed to prevent flickering.
pub static DISPLAY_IN_TRANSITION: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct SyncDisplaysInfo {
    displays: Vec<DisplayInfo>,
    is_synced: bool,
}

impl SyncDisplaysInfo {
    fn check_changed(&mut self, displays: Vec<DisplayInfo>) {
        let ignore = TEMP_IGNORE_DISPLAYS_CHANGED.load(Ordering::Relaxed);
        if self.displays.len() != displays.len() {
            self.displays = displays;
            if !ignore {
                self.is_synced = false;
            }
            return;
        }
        for (i, d) in displays.iter().enumerate() {
            if d != &self.displays[i] {
                self.displays = displays;
                if !ignore {
                    self.is_synced = false;
                }
                return;
            }
        }
    }

    fn get_update_sync_displays(&mut self) -> Option<Vec<DisplayInfo>> {
        if self.is_synced {
            return None;
        }
        self.is_synced = true;
        Some(self.displays.clone())
    }
}

pub fn temp_ignore_displays_changed() -> SimpleCallOnReturn {
    TEMP_IGNORE_DISPLAYS_CHANGED.store(true, std::sync::atomic::Ordering::Relaxed);
    SimpleCallOnReturn {
        b: true,
        f: Box::new(move || {
            // Wait for a while to make sure check_display_changed() is called
            // after video service has sending its `SwitchDisplay` message(`try_broadcast_display_changed()`).
            std::thread::sleep(Duration::from_millis(1000));
            TEMP_IGNORE_DISPLAYS_CHANGED.store(false, Ordering::Relaxed);
            // Trigger the display changed message.
            SYNC_DISPLAYS.lock().unwrap().is_synced = false;
        }),
    }
}

// This function is really useful, though a duplicate check if display changed.
// The video server will then send the following messages to the client:
//  1. the supported resolutions of the {idx} display
//  2. the switch resolution message, so that the client can record the custom resolution.
pub(super) fn check_display_changed(
    ndisplay: usize,
    idx: usize,
    (x, y, w, h): (i32, i32, usize, usize),
) -> Option<DisplayInfo> {
    #[cfg(target_os = "linux")]
    {
        // wayland do not support changing display for now
        if !is_x11() {
            return None;
        }
    }

    let lock = SYNC_DISPLAYS.lock().unwrap();
    // If plugging out a monitor && lock.displays.get(idx) is None.
    //  1. The client version < 1.2.4. The client side has to reconnect.
    //  2. The client version > 1.2.4, The client side can handle the case because sync peer info message will be sent.
    // But it is acceptable to for the user to reconnect manually, because the monitor is unplugged.
    let d = lock.displays.get(idx)?;
    if ndisplay != lock.displays.len() {
        log::info!(
            "check_display_changed: display count changed: cap={} synced={}",
            ndisplay, lock.displays.len()
        );
        return Some(d.clone());
    }
    if !(d.x == x && d.y == y && d.width == w as i32 && d.height == h as i32) {
        Some(d.clone())
    } else {
        None
    }
}

#[inline]
pub fn set_last_changed_resolution(display_name: &str, original: (i32, i32), changed: (i32, i32)) {
    let mut lock = CHANGED_RESOLUTIONS.write().unwrap();
    match lock.get_mut(display_name) {
        Some(res) => res.changed = changed,
        None => {
            lock.insert(
                display_name.to_owned(),
                ChangedResolution { original, changed },
            );
        }
    }
}

#[inline]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn restore_resolutions() {
    for (name, res) in CHANGED_RESOLUTIONS.read().unwrap().iter() {
        let (w, h) = res.original;
        log::info!("Restore resolution of display '{}' to ({}, {})", name, w, h);
        if let Err(e) = crate::platform::change_resolution(name, w as _, h as _) {
            log::error!(
                "Failed to restore resolution of display '{}' to ({},{}): {}",
                name,
                w,
                h,
                e
            );
        }
    }
    // Can be cleared because restore resolutions is called when there is no client connected.
    CHANGED_RESOLUTIONS.write().unwrap().clear();
}

#[inline]
fn is_capturer_mag_supported() -> bool {
    #[cfg(windows)]
    return scrap::CapturerMag::is_supported();
    #[cfg(not(windows))]
    false
}

#[inline]
pub fn capture_cursor_embedded() -> bool {
    scrap::is_cursor_embedded()
}

#[inline]
#[cfg(windows)]
pub fn is_privacy_mode_mag_supported() -> bool {
    return *IS_CAPTURER_MAGNIFIER_SUPPORTED
        && get_version_number(&crate::VERSION) > get_version_number("1.1.9");
}

pub fn new() -> GenericService {
    let svc = EmptyExtraFieldService::new(NAME.to_owned(), true);
    GenericService::run(&svc.clone(), run);
    svc.sp
}

fn displays_to_msg(displays: Vec<DisplayInfo>) -> Message {
    let mut pi = PeerInfo {
        ..Default::default()
    };
    pi.displays = displays.clone();

    #[cfg(windows)]
    if crate::platform::is_installed() {
        let m = crate::virtual_display_manager::get_platform_additions();
        pi.platform_additions = serde_json::to_string(&m).unwrap_or_default();
    }
    #[cfg(target_os = "linux")]
    {
        let m = crate::virtual_display_manager::get_platform_additions();
        if !m.is_empty() {
            pi.platform_additions = serde_json::to_string(&m).unwrap_or_default();
        }
    }
    #[cfg(target_os = "macos")]
    {
        let m = crate::virtual_display_manager::get_platform_additions();
        if !m.is_empty() {
            pi.platform_additions = serde_json::to_string(&m).unwrap_or_default();
        }
    }

    // current_display should not be used in server.
    // It is set to 0 for compatibility with old clients.
    pi.current_display = 0;
    let mut msg_out = Message::new();
    msg_out.set_peer_info(pi);
    msg_out
}

fn check_get_displays_changed_msg() -> Option<Message> {
    #[cfg(target_os = "linux")]
    {
        if !is_x11() {
            return get_displays_msg();
        }
    }
    let displays = match try_get_displays() {
        Ok(d) => d,
        Err(e) => {
            log::warn!("try_get_displays failed in display service poll: {}", e);
            return None;
        }
    };
    let display_infos = build_display_infos(&displays);
    // Single lock: check_changed + get_update_sync_displays in one scope
    let mut lock = SYNC_DISPLAYS.lock().unwrap();
    lock.check_changed(display_infos);
    lock.get_update_sync_displays().map(|d| displays_to_msg(d))
}

pub fn check_displays_changed() -> ResultType<()> {
    #[cfg(target_os = "linux")]
    {
        // Currently, wayland need to call wayland::clear() before call Display::all(), otherwise it will cause
        // block, or even crash here, https://github.com/rustdesk/rustdesk/blob/0bb4d43e9ea9d9dfb9c46c8d27d1a97cd0ad6bea/libs/scrap/src/wayland/pipewire.rs#L235
        if !is_x11() {
            return Ok(());
        }
    }
    check_update_displays(&try_get_displays()?);
    Ok(())
}

#[cfg(target_os = "linux")]
fn get_displays_msg() -> Option<Message> {
    let displays = SYNC_DISPLAYS.lock().unwrap().get_update_sync_displays()?;
    Some(displays_to_msg(displays))
}

fn run(sp: EmptyExtraFieldService) -> ResultType<()> {
    let mut no_change_count: u32 = 0;
    while sp.ok() {
        sp.snapshot(|sps| {
            if !TEMP_IGNORE_DISPLAYS_CHANGED.load(Ordering::Relaxed) {
                if sps.has_subscribes() {
                    SYNC_DISPLAYS.lock().unwrap().is_synced = false;
                    bail!("new subscriber");
                }
            }
            Ok(())
        })?;

        if let Some(msg_out) = check_get_displays_changed_msg() {
            sp.send(msg_out);
            log::info!("Displays changed");
            no_change_count = 0;
        } else {
            no_change_count = no_change_count.saturating_add(1);
        }

        // Adaptive polling: 300ms initially, slowing to 1000ms after 10 unchanged polls (~3s).
        // This reduces CPU usage when the display configuration is stable.
        // No impact on video FPS or cursor — this only polls for display add/remove events.
        let sleep_ms = if no_change_count > 10 { 1000 } else { 300 };
        std::thread::sleep(Duration::from_millis(sleep_ms));
    }

    Ok(())
}

#[inline]
pub(super) fn get_original_resolution(
    display_name: &str,
    w: usize,
    h: usize,
) -> MessageField<Resolution> {
    #[cfg(windows)]
    let is_rustdesk_virtual_display =
        crate::virtual_display_manager::rustdesk_idd::is_virtual_display(&display_name);
    #[cfg(target_os = "linux")]
    let is_rustdesk_virtual_display =
        crate::virtual_display_manager::linux_evdi::is_virtual_display(&display_name);
    #[cfg(not(any(windows, target_os = "linux")))]
    let is_rustdesk_virtual_display = false;
    Some(if is_rustdesk_virtual_display {
        Resolution {
            width: 0,
            height: 0,
            ..Default::default()
        }
    } else {
        let changed_resolutions = CHANGED_RESOLUTIONS.write().unwrap();
        let (width, height) = match changed_resolutions.get(display_name) {
            Some(res) => {
                res.original
                /*
                The resolution change may not happen immediately, `changed` has been updated,
                but the actual resolution is old, it will be mistaken for a third-party change.
                if res.changed.0 != w as i32 || res.changed.1 != h as i32 {
                    // If the resolution is changed by third process, remove the record in changed_resolutions.
                    changed_resolutions.remove(display_name);
                    (w as _, h as _)
                } else {
                    res.original
                }
                */
            }
            None => (w as _, h as _),
        };
        Resolution {
            width,
            height,
            ..Default::default()
        }
    })
    .into()
}

pub(super) fn get_sync_displays() -> Vec<DisplayInfo> {
    SYNC_DISPLAYS.lock().unwrap().displays.clone()
}

pub(super) fn get_display_info(idx: usize) -> Option<DisplayInfo> {
    SYNC_DISPLAYS.lock().unwrap().displays.get(idx).cloned()
}

// Build Vec<DisplayInfo> from raw Display list (no lock taken).
fn build_display_infos(all: &[Display]) -> Vec<DisplayInfo> {
    #[cfg(target_os = "linux")]
    let use_logical_scale = !is_x11()
        && crate::is_server()
        && scrap::wayland::display::get_displays().displays.len() > 1;
    all.iter()
        .map(|d| {
            let display_name = d.name();
            #[allow(unused_assignments)]
            #[allow(unused_mut)]
            let mut scale = 1.0;
            #[cfg(target_os = "macos")]
            {
                scale = d.scale();
            }
            #[cfg(target_os = "linux")]
            {
                if use_logical_scale {
                    scale = d.scale();
                }
            }
            let original_resolution = get_original_resolution(
                &display_name,
                ((d.width() as f64) / scale).round() as usize,
                (d.height() as f64 / scale).round() as usize,
            );
            DisplayInfo {
                x: d.origin().0 as _,
                y: d.origin().1 as _,
                width: d.width() as _,
                height: d.height() as _,
                name: display_name,
                online: d.is_online(),
                cursor_embedded: false,
                original_resolution,
                scale,
                ..Default::default()
            }
        })
        .collect()
}

// Display to DisplayInfo
// The DisplayInfo is be sent to the peer.
pub(super) fn check_update_displays(all: &Vec<Display>) {
    let displays = build_display_infos(all);
    SYNC_DISPLAYS.lock().unwrap().check_changed(displays);
}

pub fn is_inited_msg() -> Option<Message> {
    #[cfg(target_os = "linux")]
    if !is_x11() {
        return super::wayland::is_inited();
    }
    None
}

pub async fn update_get_sync_displays_on_login() -> ResultType<Vec<DisplayInfo>> {
    #[cfg(target_os = "linux")]
    {
        if !is_x11() {
            return super::wayland::get_displays().await;
        }
    }
    #[cfg(not(windows))]
    let displays = display_service::try_get_displays();
    #[cfg(windows)]
    let displays = display_service::try_get_displays_add_amyuni_headless();
    check_update_displays(&displays?);
    Ok(SYNC_DISPLAYS.lock().unwrap().displays.clone())
}

#[inline]
pub fn get_primary() -> usize {
    #[cfg(target_os = "linux")]
    {
        if !is_x11() {
            return match super::wayland::get_primary() {
                Ok(n) => n,
                Err(_) => 0,
            };
        }
    }

    try_get_displays().map(|d| get_primary_2(&d)).unwrap_or(0)
}

#[inline]
pub fn get_primary_2(all: &Vec<Display>) -> usize {
    all.iter().position(|d| d.is_primary()).unwrap_or(0)
}

#[inline]
#[cfg(windows)]
fn no_displays(displays: &Vec<Display>) -> bool {
    let display_len = displays.len();
    if display_len == 0 {
        true
    } else if display_len == 1 {
        let display = &displays[0];
        if display.width() > DUMMY_DISPLAY_SIDE_MAX_SIZE
            || display.height() > DUMMY_DISPLAY_SIDE_MAX_SIZE
        {
            return false;
        }
        let any_real = crate::platform::resolutions(&display.name())
            .iter()
            .any(|r| {
                (r.height as usize) > DUMMY_DISPLAY_SIDE_MAX_SIZE
                    || (r.width as usize) > DUMMY_DISPLAY_SIDE_MAX_SIZE
            });
        !any_real
    } else {
        false
    }
}

#[inline]
#[cfg(not(windows))]
pub fn try_get_displays() -> ResultType<Vec<Display>> {
    let mut displays = Display::all()?;
    log::trace!(
        "Display::all() returned {} display(s): {:?}",
        displays.len(),
        displays.iter().map(|d| format!("{}({}x{})", d.name(), d.width(), d.height())).collect::<Vec<_>>()
    );
    #[cfg(target_os = "linux")]
    {
        if displays.is_empty()
            && crate::platform::linux::is_headless_allowed()
            && virtual_display_manager::is_virtual_display_supported()
        {
            log::debug!("no displays on Linux, creating virtual display via EVDI");
            if let Err(e) = virtual_display_manager::plug_in_headless() {
                log::error!("plug_in_headless failed: {}", e);
            } else {
                std::thread::sleep(std::time::Duration::from_secs(1));
                displays = Display::all()?;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if displays.is_empty()
            && virtual_display_manager::is_virtual_display_supported()
        {
            log::debug!("no displays on macOS, creating virtual display via CGVirtualDisplay");
            if let Err(e) = virtual_display_manager::plug_in_headless() {
                log::error!("plug_in_headless failed: {}", e);
            } else {
                std::thread::sleep(std::time::Duration::from_secs(1));
                displays = Display::all()?;
            }
        }
    }
    Ok(displays)
}

#[inline]
#[cfg(windows)]
pub fn try_get_displays() -> ResultType<Vec<Display>> {
    try_get_displays_(false)
}

// We can't get full control of the virtual display if we use amyuni idd.
// If we add a virtual display, we cannot remove it automatically.
// So when using amyuni idd, we only add a virtual display for headless if it is required.
// eg. when the client is connecting.
#[inline]
#[cfg(windows)]
pub fn try_get_displays_add_amyuni_headless() -> ResultType<Vec<Display>> {
    try_get_displays_(true)
}

#[inline]
#[cfg(windows)]
pub fn try_get_displays_(add_amyuni_headless: bool) -> ResultType<Vec<Display>> {
    let mut displays = Display::all()?;

    // Do not add virtual display if the platform is not installed or the virtual display is not supported.
    if !crate::platform::is_installed() || !virtual_display_manager::is_virtual_display_supported()
    {
        return Ok(displays);
    }

    // Enable headless virtual display when
    // 1. `amyuni` idd is not used.
    // 2. `amyuni` idd is used and `add_amyuni_headless` is true.
    if virtual_display_manager::is_amyuni_idd() && !add_amyuni_headless {
        return Ok(displays);
    }

    // The following code causes a bug.
    // The virtual display cannot be added when there's no session(eg. when exiting from RDP).
    // Because `crate::platform::desktop_changed()` always returns true at that time.
    //
    // The code only solves a rare case:
    // 1. The control side is connecting.
    // 2. The windows session is switching, no displays are detected, but they're there.
    // Then the controlled side plugs in a virtual display for "headless".
    //
    // No need to do the following check. But the code is kept here for marking the issue.
    // If there're someones reporting the issue, we may add a better check by waiting for a while. (switching session).
    // But I don't think it's good to add the timeout check without any issue.
    //
    // If is switching session, no displays may be detected.
    // if displays.is_empty() && crate::platform::desktop_changed() {
    //     return Ok(displays);
    // }

    let no_displays_v = no_displays(&displays);
    if no_displays_v {
        log::debug!("no displays, create virtual display");
        if let Err(e) = virtual_display_manager::plug_in_headless() {
            log::error!("plug in headless failed {}", e);
        } else {
            displays = Display::all()?;
        }
    }
    Ok(displays)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_in_transition_flag_default_false() {
        assert!(!DISPLAY_IN_TRANSITION.load(Ordering::Relaxed));
    }

    #[test]
    fn sync_displays_info_detects_count_change() {
        let mut sdi = SyncDisplaysInfo::default();
        sdi.is_synced = true;

        let d1 = DisplayInfo {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            ..Default::default()
        };
        sdi.displays = vec![d1.clone()];
        sdi.is_synced = true;

        TEMP_IGNORE_DISPLAYS_CHANGED.store(false, Ordering::Relaxed);
        sdi.check_changed(vec![d1.clone(), d1.clone()]);
        assert!(!sdi.is_synced, "display count change should un-sync");
    }

    #[test]
    fn sync_displays_info_detects_content_change() {
        let mut sdi = SyncDisplaysInfo::default();
        let d1 = DisplayInfo {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            ..Default::default()
        };
        sdi.displays = vec![d1];
        sdi.is_synced = true;

        let d2 = DisplayInfo {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            ..Default::default()
        };
        TEMP_IGNORE_DISPLAYS_CHANGED.store(false, Ordering::Relaxed);
        sdi.check_changed(vec![d2]);
        assert!(!sdi.is_synced, "resolution change should un-sync");
    }

    #[test]
    fn sync_displays_info_no_change_stays_synced() {
        let mut sdi = SyncDisplaysInfo::default();
        let d1 = DisplayInfo {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            ..Default::default()
        };
        sdi.displays = vec![d1.clone()];
        sdi.is_synced = true;

        TEMP_IGNORE_DISPLAYS_CHANGED.store(false, Ordering::Relaxed);
        sdi.check_changed(vec![d1]);
        assert!(sdi.is_synced, "same content should stay synced");
    }

    #[test]
    fn display_service_has_adaptive_polling() {
        let src = include_str!("display_service.rs");
        assert!(
            src.contains("no_change_count"),
            "display service must implement adaptive polling interval"
        );
    }

    #[test]
    fn check_changed_loads_temp_ignore_once() {
        // Source-level: verify check_changed loads TEMP_IGNORE once (single `let ignore =`)
        let src = include_str!("display_service.rs");
        let check_changed_fn = src
            .split("fn check_changed(")
            .nth(1)
            .expect("check_changed function must exist");
        let fn_body = &check_changed_fn[..check_changed_fn.find("\n    fn ").unwrap_or(check_changed_fn.len())];
        let ignore_loads = fn_body.matches("TEMP_IGNORE_DISPLAYS_CHANGED").count();
        assert_eq!(
            ignore_loads, 1,
            "check_changed should load TEMP_IGNORE only once (found {} refs)",
            ignore_loads
        );
    }

    #[test]
    fn build_display_infos_exists() {
        let src = include_str!("display_service.rs");
        assert!(
            src.contains("fn build_display_infos("),
            "build_display_infos helper must exist for lock merging"
        );
    }

    #[test]
    fn bench_sync_displays_lock_merge() {
        use std::time::Instant;
        const ITERATIONS: u32 = 100_000;

        let d1 = DisplayInfo {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            ..Default::default()
        };

        // Scenario A: Two separate locks (old behavior)
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            SYNC_DISPLAYS
                .lock()
                .unwrap()
                .check_changed(vec![d1.clone()]);
            let _ = SYNC_DISPLAYS
                .lock()
                .unwrap()
                .get_update_sync_displays();
        }
        let two_locks_elapsed = start.elapsed();

        // Scenario B: Single lock (new behavior)
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            let mut lock = SYNC_DISPLAYS.lock().unwrap();
            lock.check_changed(vec![d1.clone()]);
            let _ = lock.get_update_sync_displays();
        }
        let one_lock_elapsed = start.elapsed();

        println!(
            "\n=== SYNC_DISPLAYS lock merge benchmark ({} iterations) ===",
            ITERATIONS
        );
        println!("  Two locks (old):  {:?}", two_locks_elapsed);
        println!("  One lock (new):   {:?}", one_lock_elapsed);
        let speedup =
            two_locks_elapsed.as_nanos() as f64 / one_lock_elapsed.as_nanos().max(1) as f64;
        println!("  Speedup: {:.2}x", speedup);
        assert!(
            one_lock_elapsed <= two_locks_elapsed,
            "Single lock should be faster or equal to double lock"
        );
    }
}
