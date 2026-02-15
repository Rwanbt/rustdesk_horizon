// Mutter ScreenCast RecordMonitor implementation for automatic display capture
// Bypasses XDG Desktop Portal to avoid user dialogs
//
// This module uses org.gnome.Mutter.ScreenCast.RecordMonitor to capture
// ALL physical displays automatically without user interaction.
//
// Requires: GNOME/Mutter compositor (GNOME 40+)

use dbus::arg::{PropMap, Variant};
use dbus::blocking::SyncConnection;
use dbus::message::{MatchRule, MessageType};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PhysicalConnector {
    pub name: String,         // "eDP-1", "HDMI-1", "DP-2", etc.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub is_primary: bool,
}

#[derive(Debug)]
pub struct MutterCaptureSession {
    pub connector: String,
    pub session_path: dbus::Path<'static>,
    pub stream_path: dbus::Path<'static>,
    pub node_id: u32,
}

/// Check if Mutter ScreenCast API is available (GNOME only)
pub fn is_mutter_available() -> bool {
    let conn = match SyncConnection::new_session() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let proxy = conn.with_proxy(
        "org.gnome.Mutter.ScreenCast",
        "/org/gnome/Mutter/ScreenCast",
        Duration::from_millis(2000),
    );

    use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
    proxy
        .get::<i32>("org.gnome.Mutter.ScreenCast", "Version")
        .is_ok()
}

/// Get all physical display connectors from Mutter DisplayConfig
/// Returns connectors with their positions and dimensions
pub fn get_all_physical_connectors() -> Result<Vec<PhysicalConnector>, String> {
    let conn = SyncConnection::new_session()
        .map_err(|e| format!("Cannot connect to session D-Bus: {}", e))?;

    let proxy = conn.with_proxy(
        "org.gnome.Mutter.DisplayConfig",
        "/org/gnome/Mutter/DisplayConfig",
        Duration::from_millis(5000),
    );

    // GetCurrentState returns:
    // (serial, monitors_info, logical_monitors, properties)
    type StateResult = (
        u32,
        Vec<(
            (String, String, String, String),                              // connector info
            Vec<(String, i32, i32, f64, f64, Vec<f64>, PropMap)>,         // modes
            PropMap,                                                       // properties
        )>,
        Vec<(i32, i32, f64, u32, bool, Vec<(String, String, String, String)>, PropMap)>, // logical monitors
        PropMap,
    );

    let (_, monitors, logical_monitors, _): StateResult = proxy
        .method_call(
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
            (),
        )
        .map_err(|e| format!("GetCurrentState failed: {}", e))?;

    let mut connectors = Vec::new();

    // Parse logical monitors to get positions and dimensions
    for (x, y, scale, _transform, is_primary, monitor_specs, _props) in logical_monitors {
        for (connector, _vendor, _product, _serial) in monitor_specs {
            // Skip virtual displays (Meta-0, Meta-1, etc.)
            if connector.starts_with("Meta-") {
                continue;
            }

            // Find width/height from monitors info
            let mut width = 0;
            let mut height = 0;

            for (mon_info, modes, _props) in &monitors {
                if mon_info.0 == connector {
                    // Find current mode (marked with refresh_rate > 0)
                    for (_id, w, h, _refresh, _pref_scale, _scales, _props) in modes {
                        if *w > 0 && *h > 0 {
                            width = *w;
                            height = *h;
                            break;
                        }
                    }
                    break;
                }
            }

            if width > 0 && height > 0 {
                connectors.push(PhysicalConnector {
                    name: connector,
                    x,
                    y,
                    width,
                    height,
                    scale,
                    is_primary,
                });
            }
        }
    }

    if connectors.is_empty() {
        return Err("No physical displays found".to_string());
    }

    tracing::info!(
        "Mutter: found {} physical connectors: {:?}",
        connectors.len(),
        connectors.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    Ok(connectors)
}

/// Create a Mutter ScreenCast session for a specific monitor
/// This uses RecordMonitor instead of the portal, so NO user dialog
pub fn create_capture_session(connector: &str) -> Result<MutterCaptureSession, String> {
    let conn = SyncConnection::new_session()
        .map_err(|e| format!("Cannot connect to session D-Bus: {}", e))?;

    tracing::info!("Mutter: creating capture session for {}", connector);

    // Step 1: CreateSession
    let sc = conn.with_proxy(
        "org.gnome.Mutter.ScreenCast",
        "/org/gnome/Mutter/ScreenCast",
        Duration::from_millis(5000),
    );

    let (session_path,): (dbus::Path<'static>,) = sc
        .method_call(
            "org.gnome.Mutter.ScreenCast",
            "CreateSession",
            (PropMap::new(),),
        )
        .map_err(|e| format!("CreateSession failed: {}", e))?;

    tracing::debug!("Mutter: session created at {}", session_path);

    // Step 2: RecordMonitor for this specific connector
    let session_proxy = conn.with_proxy(
        "org.gnome.Mutter.ScreenCast",
        session_path.clone(),
        Duration::from_millis(5000),
    );

    let mut props = PropMap::new();
    props.insert(
        "cursor-mode".to_string(),
        Variant(Box::new(1u32)), // 1 = embedded cursor
    );

    let (stream_path,): (dbus::Path<'static>,) = session_proxy
        .method_call(
            "org.gnome.Mutter.ScreenCast.Session",
            "RecordMonitor",
            (connector.to_string(), props),
        )
        .map_err(|e| format!("RecordMonitor failed for {}: {}", connector, e))?;

    tracing::debug!("Mutter: stream created at {}", stream_path);

    // Step 3: Listen for PipeWireStreamAdded signal to get node_id
    let node_id: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
    let node_id_cb = node_id.clone();

    let mut rule = MatchRule::new();
    rule.path = Some(stream_path.clone());
    rule.msg_type = Some(MessageType::Signal);
    rule.interface = Some("org.gnome.Mutter.ScreenCast.Stream".into());
    rule.member = Some("PipeWireStreamAdded".into());

    conn.add_match(rule, move |_: (), _, msg| {
        if let Some(nid) = msg.get1::<u32>() {
            *node_id_cb.lock().unwrap() = Some(nid);
            tracing::info!("Mutter: PipeWire node_id = {}", nid);
        }
        true
    })
    .map_err(|e| format!("Failed to add match rule: {}", e))?;

    // Step 4: Start the session
    session_proxy
        .method_call::<(), _, _, _>("org.gnome.Mutter.ScreenCast.Session", "Start", ())
        .map_err(|e| format!("Session Start failed: {}", e))?;

    tracing::debug!("Mutter: session started, waiting for node_id...");

    // Wait for PipeWireStreamAdded signal (timeout 5s)
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        conn.process(Duration::from_millis(100))
            .map_err(|e| format!("D-Bus processing error: {}", e))?;

        if let Some(nid) = *node_id.lock().unwrap() {
            tracing::info!("Mutter: capture session ready for {} (node_id={})", connector, nid);
            return Ok(MutterCaptureSession {
                connector: connector.to_string(),
                session_path,
                stream_path,
                node_id: nid,
            });
        }

        if std::time::Instant::now() > deadline {
            return Err(format!(
                "Timeout waiting for PipeWireStreamAdded signal for {}",
                connector
            ));
        }
    }
}

/// Create capture sessions for ALL physical displays
/// Returns a list of sessions, one per display
pub fn create_sessions_for_all_displays() -> Result<Vec<MutterCaptureSession>, String> {
    if !is_mutter_available() {
        return Err("Mutter ScreenCast not available (requires GNOME)".to_string());
    }

    let connectors = get_all_physical_connectors()?;
    let mut sessions = Vec::new();

    for connector in connectors {
        match create_capture_session(&connector.name) {
            Ok(session) => {
                tracing::info!("✓ Capture session created for {}", connector.name);
                sessions.push(session);
            }
            Err(e) => {
                tracing::error!("✗ Failed to create session for {}: {}", connector.name, e);
                // Continue with other displays even if one fails
            }
        }
    }

    if sessions.is_empty() {
        return Err("Failed to create any capture sessions".to_string());
    }

    tracing::info!(
        "Mutter: created {} capture sessions (no user dialog required)",
        sessions.len()
    );

    Ok(sessions)
}

/// Create RecordMonitor sessions for ALL physical displays on a SINGLE shared D-Bus connection.
/// This fixes the node_id duplication bug: with separate connections per session,
/// the PipeWireStreamAdded signal handler was shared and all sessions got the same node_id.
/// With a shared connection, each session's signal is properly dispatched.
/// Returns (connection, sessions) — the connection MUST be kept alive for sessions to persist.
pub fn create_all_sessions_on_shared_conn() -> Result<(SyncConnection, Vec<MutterCaptureSession>), String> {
    let conn = SyncConnection::new_session()
        .map_err(|e| format!("Cannot connect to session D-Bus: {}", e))?;

    let connectors = get_all_physical_connectors()?;
    let mut sessions = Vec::new();

    for connector in &connectors {
        // Skip Meta- virtual outputs
        if connector.name.starts_with("Meta-") {
            continue;
        }

        match create_capture_session_on(&conn, &connector.name) {
            Ok(session) => {
                tracing::info!("Capture session created for {} (node_id={})", connector.name, session.node_id);
                sessions.push(session);
            }
            Err(e) => {
                tracing::error!("Failed to create session for {}: {}", connector.name, e);
            }
        }
    }

    if sessions.is_empty() {
        return Err("Failed to create any capture sessions".to_string());
    }

    tracing::info!("Mutter: created {} sessions on shared D-Bus connection", sessions.len());
    Ok((conn, sessions))
}

/// Create a RecordMonitor session on an existing shared D-Bus connection.
/// The connection MUST stay alive for the session to persist.
fn create_capture_session_on(conn: &SyncConnection, connector: &str) -> Result<MutterCaptureSession, String> {
    tracing::info!("Mutter: creating capture session for {} (shared conn)", connector);

    let sc = conn.with_proxy(
        "org.gnome.Mutter.ScreenCast",
        "/org/gnome/Mutter/ScreenCast",
        Duration::from_millis(5000),
    );

    let (session_path,): (dbus::Path<'static>,) = sc
        .method_call(
            "org.gnome.Mutter.ScreenCast",
            "CreateSession",
            (PropMap::new(),),
        )
        .map_err(|e| format!("CreateSession failed: {}", e))?;

    let session_proxy = conn.with_proxy(
        "org.gnome.Mutter.ScreenCast",
        session_path.clone(),
        Duration::from_millis(5000),
    );

    let mut props = PropMap::new();
    props.insert(
        "cursor-mode".to_string(),
        Variant(Box::new(1u32)), // 1 = embedded cursor
    );

    let (stream_path,): (dbus::Path<'static>,) = session_proxy
        .method_call(
            "org.gnome.Mutter.ScreenCast.Session",
            "RecordMonitor",
            (connector.to_string(), props),
        )
        .map_err(|e| format!("RecordMonitor failed for {}: {}", connector, e))?;

    let node_id: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
    let node_id_cb = node_id.clone();

    let mut rule = MatchRule::new();
    rule.path = Some(stream_path.clone());
    rule.msg_type = Some(MessageType::Signal);
    rule.interface = Some("org.gnome.Mutter.ScreenCast.Stream".into());
    rule.member = Some("PipeWireStreamAdded".into());

    conn.add_match(rule, move |_: (), _, msg| {
        if let Some(nid) = msg.get1::<u32>() {
            *node_id_cb.lock().unwrap() = Some(nid);
            tracing::info!("Mutter: PipeWire node_id = {} (shared conn)", nid);
        }
        true
    })
    .map_err(|e| format!("Failed to add match rule: {}", e))?;

    session_proxy
        .method_call::<(), _, _, _>("org.gnome.Mutter.ScreenCast.Session", "Start", ())
        .map_err(|e| format!("Session Start failed: {}", e))?;

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        conn.process(Duration::from_millis(100))
            .map_err(|e| format!("D-Bus processing error: {}", e))?;

        if let Some(nid) = *node_id.lock().unwrap() {
            return Ok(MutterCaptureSession {
                connector: connector.to_string(),
                session_path,
                stream_path,
                node_id: nid,
            });
        }

        if std::time::Instant::now() > deadline {
            return Err(format!(
                "Timeout waiting for PipeWireStreamAdded signal for {}",
                connector
            ));
        }
    }
}

/// Stop a Mutter capture session
pub fn stop_session(session_path: &dbus::Path<'static>) -> Result<(), String> {
    let conn = SyncConnection::new_session()
        .map_err(|e| format!("Cannot connect to session D-Bus: {}", e))?;

    let proxy = conn.with_proxy(
        "org.gnome.Mutter.ScreenCast",
        session_path.clone(),
        Duration::from_millis(3000),
    );

    proxy
        .method_call::<(), _, _, _>("org.gnome.Mutter.ScreenCast.Session", "Stop", ())
        .map_err(|e| format!("Session Stop failed: {}", e))?;

    Ok(())
}

/// Open PipeWire socket and return file descriptor
/// This allows creating PipeWire capturers without the XDG portal
pub fn open_pipewire_socket() -> Result<std::os::unix::io::OwnedFd, String> {
    use std::os::unix::io::{AsRawFd, FromRawFd};

    let xdg_runtime = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| {
            let uid = unsafe { hbb_common::libc::getuid() };
            format!("/run/user/{}", uid)
        });

    let socket_path = format!("{}/pipewire-0", xdg_runtime);

    tracing::debug!("Opening PipeWire socket: {}", socket_path);

    let socket = std::os::unix::net::UnixStream::connect(&socket_path)
        .map_err(|e| format!("Failed to connect to PipeWire socket {}: {}", socket_path, e))?;

    let fd = unsafe { std::os::unix::io::OwnedFd::from_raw_fd(socket.as_raw_fd()) };
    std::mem::forget(socket); // Don't close the socket, transfer ownership to OwnedFd

    tracing::info!("PipeWire socket opened successfully (fd={})", fd.as_raw_fd());
    Ok(fd)
}
