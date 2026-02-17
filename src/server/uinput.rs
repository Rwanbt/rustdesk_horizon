use crate::ipc::{self, new_listener, Connection, Data, DataKeyboard, DataMouse};
use enigo::{Key, KeyboardControllable, MouseButton, MouseControllable};
use evdev::{
    uinput::{VirtualDevice, VirtualDeviceBuilder},
    AttributeSet, EventType, InputEvent,
};
use hbb_common::{
    allow_err, anyhow, bail, log,
    tokio::{self, runtime::Runtime},
    ResultType,
};

static IPC_CONN_TIMEOUT: u64 = 1000;
static IPC_REQUEST_TIMEOUT: u64 = 1000;
static IPC_POSTFIX_KEYBOARD: &str = "_uinput_keyboard";
static IPC_POSTFIX_MOUSE: &str = "_uinput_mouse";
static IPC_POSTFIX_CONTROL: &str = "_uinput_control";

pub mod client {
    use super::*;

    pub struct UInputKeyboard {
        conn: Connection,
        rt: Runtime,
    }

    impl UInputKeyboard {
        pub async fn new() -> ResultType<Self> {
            let conn = ipc::connect(IPC_CONN_TIMEOUT, IPC_POSTFIX_KEYBOARD).await?;
            let rt = Runtime::new()?;
            Ok(Self { conn, rt })
        }

        fn reconnect(&mut self) -> ResultType<()> {
            log::info!("UInput keyboard: attempting IPC reconnect...");
            self.conn = self.rt.block_on(ipc::connect(IPC_CONN_TIMEOUT, IPC_POSTFIX_KEYBOARD))?;
            log::info!("UInput keyboard: IPC reconnected successfully");
            Ok(())
        }

        fn send(&mut self, data: Data) -> ResultType<()> {
            match self.rt.block_on(self.conn.send(&data)) {
                Ok(()) => Ok(()),
                Err(e) => {
                    log::warn!("UInput keyboard send failed ({}), reconnecting...", e);
                    self.reconnect()?;
                    self.rt.block_on(self.conn.send(&data))
                }
            }
        }

        fn send_get_key_state(&mut self, data: Data) -> ResultType<bool> {
            if let Err(e) = self.rt.block_on(self.conn.send(&data)) {
                log::warn!("UInput keyboard send_get_key_state failed ({}), reconnecting...", e);
                self.reconnect()?;
                self.rt.block_on(self.conn.send(&data))?;
            }

            match self
                .rt
                .block_on(self.conn.next_timeout(IPC_REQUEST_TIMEOUT))
            {
                Ok(Some(Data::KeyboardResponse(ipc::DataKeyboardResponse::GetKeyState(state)))) => {
                    Ok(state)
                }
                Ok(Some(resp)) => {
                    // FATAL error!!!
                    bail!(
                        "FATAL error, wait keyboard result other response: {:?}",
                        &resp
                    );
                }
                Ok(None) => {
                    // FATAL error!!!
                    // Maybe wait later
                    bail!("FATAL error, wait keyboard result, receive None",);
                }
                Err(e) => {
                    // FATAL error!!!
                    bail!(
                        "FATAL error, wait keyboard result timeout {}, {}",
                        &e,
                        IPC_REQUEST_TIMEOUT
                    );
                }
            }
        }
    }

    impl KeyboardControllable for UInputKeyboard {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn get_key_state(&mut self, key: Key) -> bool {
            match self.send_get_key_state(Data::Keyboard(DataKeyboard::GetKeyState(key))) {
                Ok(state) => state,
                Err(e) => {
                    // unreachable!()
                    log::error!("Failed to get key state {}", &e);
                    false
                }
            }
        }

        fn key_sequence(&mut self, sequence: &str) {
            allow_err!(self.send(Data::Keyboard(DataKeyboard::Sequence(sequence.to_string()))));
        }

        // TODO: handle error???
        fn key_down(&mut self, key: Key) -> enigo::ResultType {
            allow_err!(self.send(Data::Keyboard(DataKeyboard::KeyDown(key))));
            Ok(())
        }
        fn key_up(&mut self, key: Key) {
            allow_err!(self.send(Data::Keyboard(DataKeyboard::KeyUp(key))));
        }
        fn key_click(&mut self, key: Key) {
            allow_err!(self.send(Data::Keyboard(DataKeyboard::KeyClick(key))));
        }
    }

    pub struct UInputMouse {
        conn: Connection,
        rt: Runtime,
    }

    impl UInputMouse {
        pub async fn new() -> ResultType<Self> {
            let conn = ipc::connect(IPC_CONN_TIMEOUT, IPC_POSTFIX_MOUSE).await?;
            let rt = Runtime::new()?;
            Ok(Self { conn, rt })
        }

        fn reconnect(&mut self) -> ResultType<()> {
            log::info!("UInput mouse: attempting IPC reconnect...");
            self.conn = self.rt.block_on(ipc::connect(IPC_CONN_TIMEOUT, IPC_POSTFIX_MOUSE))?;
            log::info!("UInput mouse: IPC reconnected successfully");
            Ok(())
        }

        fn send(&mut self, data: Data) -> ResultType<()> {
            match self.rt.block_on(self.conn.send(&data)) {
                Ok(()) => Ok(()),
                Err(e) => {
                    log::warn!("UInput mouse send failed ({}), reconnecting...", e);
                    self.reconnect()?;
                    self.rt.block_on(self.conn.send(&data))
                }
            }
        }

        pub fn send_refresh(&mut self) -> ResultType<()> {
            self.send(Data::Mouse(DataMouse::Refresh))
        }
    }

    impl MouseControllable for UInputMouse {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn mouse_move_to(&mut self, x: i32, y: i32) {
            allow_err!(self.send(Data::Mouse(DataMouse::MoveTo(x, y))));
        }
        fn mouse_move_relative(&mut self, x: i32, y: i32) {
            allow_err!(self.send(Data::Mouse(DataMouse::MoveRelative(x, y))));
        }
        // TODO: handle error???
        fn mouse_down(&mut self, button: MouseButton) -> enigo::ResultType {
            allow_err!(self.send(Data::Mouse(DataMouse::Down(button))));
            Ok(())
        }
        fn mouse_up(&mut self, button: MouseButton) {
            allow_err!(self.send(Data::Mouse(DataMouse::Up(button))));
        }
        fn mouse_click(&mut self, button: MouseButton) {
            allow_err!(self.send(Data::Mouse(DataMouse::Click(button))));
        }
        fn mouse_scroll_x(&mut self, length: i32) {
            allow_err!(self.send(Data::Mouse(DataMouse::ScrollX(length))));
        }
        fn mouse_scroll_y(&mut self, length: i32) {
            allow_err!(self.send(Data::Mouse(DataMouse::ScrollY(length))));
        }
    }

    // =========================================================================
    // Direct UInput: bypass IPC, write to /dev/uinput directly via raw ioctl.
    // Only requires rw access to /dev/uinput (ACL). Does NOT need /dev/input/event* access.
    // =========================================================================

    pub struct DirectUInputKeyboard {
        uinput_file: std::fs::File,
    }

    impl DirectUInputKeyboard {
        pub fn new() -> ResultType<Self> {
            use std::os::unix::fs::OpenOptionsExt;
            use std::os::unix::io::AsRawFd;
            use super::mouce::*;

            let file = std::fs::File::options()
                .write(true)
                .custom_flags(O_NONBLOCK)
                .open("/dev/uinput")
                .map_err(|e| anyhow::anyhow!("Cannot open /dev/uinput: {}", e))?;
            let fd = file.as_raw_fd();

            unsafe {
                // Enable EV_KEY
                ioctl(fd, UI_SET_EVBIT, EV_KEY);
                // Enable EV_MSC for scan codes
                ioctl(fd, UI_SET_EVBIT, EV_MSC);
                ioctl(fd, UI_SET_MSCBIT, MSC_SCAN);
                // Enable EV_LED
                ioctl(fd, UI_SET_EVBIT, EV_LED);
                ioctl(fd, UI_SET_LEDBIT, LED_NUML);
                ioctl(fd, UI_SET_LEDBIT, LED_CAPSL);
                ioctl(fd, UI_SET_LEDBIT, LED_SCROLLL);

                // Register all keys (KEY_ESC=1 through BTN_TRIGGER_HAPPY40=0x2e7)
                for i in 1..=0x2e7i32 {
                    ioctl(fd, UI_SET_KEYBIT, i);
                }
            }

            let mut usetup = UInputSetup {
                id: InputId {
                    bustype: BUS_USB,
                    vendor: 0x2222,
                    product: 0x4444,
                    version: 0,
                },
                name: [0; UINPUT_MAX_NAME_SIZE],
                ff_effects_max: 0,
            };
            let name = b"RustDesk Direct Keyboard";
            for (i, &ch) in name.iter().enumerate() {
                if i < UINPUT_MAX_NAME_SIZE {
                    usetup.name[i] = ch as std::os::raw::c_char;
                }
            }

            unsafe {
                ioctl(fd, UI_DEV_SETUP, &usetup);
                ioctl(fd, UI_DEV_CREATE);
            }

            std::thread::sleep(std::time::Duration::from_millis(300));
            log::info!("DirectUInput keyboard created (raw ioctl, no /dev/input/event* needed)");
            Ok(Self { uinput_file: file })
        }

        fn emit_key(&mut self, code: u16, value: i32) {
            use std::os::unix::io::AsRawFd;
            use super::mouce::*;
            let fd = self.uinput_file.as_raw_fd();
            let mut ev = InputEvent {
                time: TimeVal { tv_sec: 0, tv_usec: 0 },
                r#type: EV_KEY as u16,
                code,
                value,
            };
            unsafe {
                write(fd, &mut ev, std::mem::size_of::<InputEvent>());
            }
            // SYN_REPORT
            let mut syn = InputEvent {
                time: TimeVal { tv_sec: 0, tv_usec: 0 },
                r#type: EV_SYN as u16,
                code: 0,
                value: 0,
            };
            unsafe {
                write(fd, &mut syn, std::mem::size_of::<InputEvent>());
            }
        }
    }

    impl KeyboardControllable for DirectUInputKeyboard {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn get_key_state(&mut self, _key: Key) -> bool {
            // Without /dev/input/event* access, we cannot read key/LED state.
            // Return false — this is best-effort. The remote client tracks
            // modifier state itself.
            false
        }

        fn key_sequence(&mut self, _sequence: &str) {
            // ignored, same as IPC path
        }

        fn key_down(&mut self, key: Key) -> enigo::ResultType {
            match key {
                Key::Raw(code) => {
                    self.emit_key((code - 8) as u16, 1);
                }
                _ => {
                    if let Ok((k, is_shift)) = super::service::map_key(&key) {
                        if is_shift {
                            self.emit_key(evdev::Key::KEY_LEFTSHIFT.code(), 1);
                        }
                        self.emit_key(k.code(), 1);
                    }
                }
            }
            Ok(())
        }

        fn key_up(&mut self, key: Key) {
            match key {
                Key::Raw(code) => {
                    self.emit_key((code - 8) as u16, 0);
                }
                _ => {
                    if let Ok((k, _)) = super::service::map_key(&key) {
                        self.emit_key(k.code(), 0);
                    }
                }
            }
        }

        fn key_click(&mut self, key: Key) {
            match key {
                Key::Raw(code) => {
                    self.emit_key((code - 8) as u16, 1);
                    self.emit_key((code - 8) as u16, 0);
                }
                _ => {
                    if let Ok((k, _)) = super::service::map_key(&key) {
                        self.emit_key(k.code(), 1);
                        self.emit_key(k.code(), 0);
                    }
                }
            }
        }
    }

    pub struct DirectUInputMouse {
        mouse: super::mouce::UInputMouseManager,
    }

    impl DirectUInputMouse {
        pub fn new(rng_x: (i32, i32), rng_y: (i32, i32)) -> ResultType<Self> {
            let mouse = super::mouce::UInputMouseManager::new(rng_x, rng_y)
                .map_err(|e| anyhow::anyhow!("Failed to create direct UInput mouse: {}", e))?;
            log::info!("DirectUInput mouse created (no IPC, direct /dev/uinput)");
            Ok(Self { mouse })
        }

        pub fn refresh_resolution(
            &mut self,
            rng_x: (i32, i32),
            rng_y: (i32, i32),
        ) -> ResultType<()> {
            self.mouse = super::mouce::UInputMouseManager::new(rng_x, rng_y)
                .map_err(|e| anyhow::anyhow!("Failed to recreate direct UInput mouse: {}", e))?;
            log::info!(
                "DirectUInput mouse refreshed ({}, {}) x ({}, {})",
                rng_x.0,
                rng_x.1,
                rng_y.0,
                rng_y.1
            );
            Ok(())
        }
    }

    impl MouseControllable for DirectUInputMouse {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn mouse_move_to(&mut self, x: i32, y: i32) {
            allow_err!(self.mouse.move_to(x as usize, y as usize));
        }
        fn mouse_move_relative(&mut self, x: i32, y: i32) {
            allow_err!(self.mouse.move_relative(x, y));
        }
        fn mouse_down(&mut self, button: MouseButton) -> enigo::ResultType {
            let btn = match button {
                MouseButton::Left => super::mouce::MouseButton::Left,
                MouseButton::Middle => super::mouce::MouseButton::Middle,
                MouseButton::Right => super::mouce::MouseButton::Right,
                _ => return Ok(()),
            };
            allow_err!(self.mouse.press_button(&btn));
            Ok(())
        }
        fn mouse_up(&mut self, button: MouseButton) {
            let btn = match button {
                MouseButton::Left => super::mouce::MouseButton::Left,
                MouseButton::Middle => super::mouce::MouseButton::Middle,
                MouseButton::Right => super::mouce::MouseButton::Right,
                _ => return,
            };
            allow_err!(self.mouse.release_button(&btn));
        }
        fn mouse_click(&mut self, button: MouseButton) {
            let btn = match button {
                MouseButton::Left => super::mouce::MouseButton::Left,
                MouseButton::Middle => super::mouce::MouseButton::Middle,
                MouseButton::Right => super::mouce::MouseButton::Right,
                _ => return,
            };
            allow_err!(self.mouse.click_button(&btn));
        }
        fn mouse_scroll_x(&mut self, length: i32) {
            let scroll = if length < 0 {
                super::mouce::ScrollDirection::Left
            } else {
                super::mouce::ScrollDirection::Right
            };
            let mut length = length.abs();
            for _ in 0..length {
                allow_err!(self.mouse.scroll_wheel(&scroll));
            }
        }
        fn mouse_scroll_y(&mut self, length: i32) {
            let scroll = if length < 0 {
                super::mouce::ScrollDirection::Up
            } else {
                super::mouce::ScrollDirection::Down
            };
            let mut length = length.abs();
            for _ in 0..length {
                allow_err!(self.mouse.scroll_wheel(&scroll));
            }
        }
    }

    pub async fn set_resolution(minx: i32, maxx: i32, miny: i32, maxy: i32) -> ResultType<()> {
        let mut conn = ipc::connect(IPC_CONN_TIMEOUT, IPC_POSTFIX_CONTROL).await?;
        conn.send(&Data::Control(ipc::DataControl::Resolution {
            minx,
            maxx,
            miny,
            maxy,
        }))
        .await?;
        let _ = conn.next().await?;
        Ok(())
    }
}

pub mod service {
    use super::*;
    use hbb_common::lazy_static;
    use std::{collections::HashMap, sync::Mutex, sync::OnceLock};

    lazy_static::lazy_static! {
    static ref KEY_MAP: HashMap<enigo::Key, evdev::Key> = HashMap::from(
        [
            (enigo::Key::Alt, evdev::Key::KEY_LEFTALT),
            (enigo::Key::Backspace, evdev::Key::KEY_BACKSPACE),
            (enigo::Key::CapsLock, evdev::Key::KEY_CAPSLOCK),
            (enigo::Key::Control, evdev::Key::KEY_LEFTCTRL),
            (enigo::Key::Delete, evdev::Key::KEY_DELETE),
            (enigo::Key::DownArrow, evdev::Key::KEY_DOWN),
            (enigo::Key::End, evdev::Key::KEY_END),
            (enigo::Key::Escape, evdev::Key::KEY_ESC),
            (enigo::Key::F1, evdev::Key::KEY_F1),
            (enigo::Key::F10, evdev::Key::KEY_F10),
            (enigo::Key::F11, evdev::Key::KEY_F11),
            (enigo::Key::F12, evdev::Key::KEY_F12),
            (enigo::Key::F2, evdev::Key::KEY_F2),
            (enigo::Key::F3, evdev::Key::KEY_F3),
            (enigo::Key::F4, evdev::Key::KEY_F4),
            (enigo::Key::F5, evdev::Key::KEY_F5),
            (enigo::Key::F6, evdev::Key::KEY_F6),
            (enigo::Key::F7, evdev::Key::KEY_F7),
            (enigo::Key::F8, evdev::Key::KEY_F8),
            (enigo::Key::F9, evdev::Key::KEY_F9),
            (enigo::Key::Home, evdev::Key::KEY_HOME),
            (enigo::Key::LeftArrow, evdev::Key::KEY_LEFT),
            (enigo::Key::Meta, evdev::Key::KEY_LEFTMETA),
            (enigo::Key::Option, evdev::Key::KEY_OPTION),
            (enigo::Key::PageDown, evdev::Key::KEY_PAGEDOWN),
            (enigo::Key::PageUp, evdev::Key::KEY_PAGEUP),
            (enigo::Key::Return, evdev::Key::KEY_ENTER),
            (enigo::Key::RightArrow, evdev::Key::KEY_RIGHT),
            (enigo::Key::Shift, evdev::Key::KEY_LEFTSHIFT),
            (enigo::Key::Space, evdev::Key::KEY_SPACE),
            (enigo::Key::Tab, evdev::Key::KEY_TAB),
            (enigo::Key::UpArrow, evdev::Key::KEY_UP),
            (enigo::Key::Numpad0, evdev::Key::KEY_KP0),  // check if correct?
            (enigo::Key::Numpad1, evdev::Key::KEY_KP1),
            (enigo::Key::Numpad2, evdev::Key::KEY_KP2),
            (enigo::Key::Numpad3, evdev::Key::KEY_KP3),
            (enigo::Key::Numpad4, evdev::Key::KEY_KP4),
            (enigo::Key::Numpad5, evdev::Key::KEY_KP5),
            (enigo::Key::Numpad6, evdev::Key::KEY_KP6),
            (enigo::Key::Numpad7, evdev::Key::KEY_KP7),
            (enigo::Key::Numpad8, evdev::Key::KEY_KP8),
            (enigo::Key::Numpad9, evdev::Key::KEY_KP9),
            (enigo::Key::Cancel, evdev::Key::KEY_CANCEL),
            (enigo::Key::Clear, evdev::Key::KEY_CLEAR),
            (enigo::Key::Alt, evdev::Key::KEY_LEFTALT),
            (enigo::Key::Pause, evdev::Key::KEY_PAUSE),
            (enigo::Key::Kana, evdev::Key::KEY_KATAKANA),  // check if correct?
            (enigo::Key::Hangul, evdev::Key::KEY_HANGEUL),  // check if correct?
            // (enigo::Key::Junja, evdev::Key::KEY_JUNJA),     // map?
            // (enigo::Key::Final, evdev::Key::KEY_FINAL),     // map?
            (enigo::Key::Hanja, evdev::Key::KEY_HANJA),
            // (enigo::Key::Kanji, evdev::Key::KEY_KANJI),      // map?
            // (enigo::Key::Convert, evdev::Key::KEY_CONVERT),
            (enigo::Key::Select, evdev::Key::KEY_SELECT),
            (enigo::Key::Print, evdev::Key::KEY_PRINT),
            // (enigo::Key::Execute, evdev::Key::KEY_EXECUTE),
            (enigo::Key::Snapshot, evdev::Key::KEY_SYSRQ),
            (enigo::Key::Insert, evdev::Key::KEY_INSERT),
            (enigo::Key::Help, evdev::Key::KEY_HELP),
            (enigo::Key::Sleep, evdev::Key::KEY_SLEEP),
            // (enigo::Key::Separator, evdev::Key::KEY_SEPARATOR),
            (enigo::Key::Scroll, evdev::Key::KEY_SCROLLLOCK),
            (enigo::Key::NumLock, evdev::Key::KEY_NUMLOCK),
            (enigo::Key::RWin, evdev::Key::KEY_RIGHTMETA),
            (enigo::Key::Apps, evdev::Key::KEY_COMPOSE),    // it's a little strange that the key is mapped to KEY_COMPOSE, not KEY_MENU
            (enigo::Key::Multiply, evdev::Key::KEY_KPASTERISK),
            (enigo::Key::Add, evdev::Key::KEY_KPPLUS),
            (enigo::Key::Subtract, evdev::Key::KEY_KPMINUS),
            (enigo::Key::Decimal, evdev::Key::KEY_KPCOMMA),   // KEY_KPDOT and KEY_KPCOMMA are exchanged?
            (enigo::Key::Divide, evdev::Key::KEY_KPSLASH),
            (enigo::Key::Equals, evdev::Key::KEY_KPEQUAL),
            (enigo::Key::NumpadEnter, evdev::Key::KEY_KPENTER),
            (enigo::Key::RightAlt, evdev::Key::KEY_RIGHTALT),
            (enigo::Key::RightControl, evdev::Key::KEY_RIGHTCTRL),
            (enigo::Key::RightShift, evdev::Key::KEY_RIGHTSHIFT),
        ]);

        static ref KEY_MAP_LAYOUT: HashMap<char, (evdev::Key, bool)> = HashMap::from(
            [
                ('a', (evdev::Key::KEY_A, false)),
                ('b', (evdev::Key::KEY_B, false)),
                ('c', (evdev::Key::KEY_C, false)),
                ('d', (evdev::Key::KEY_D, false)),
                ('e', (evdev::Key::KEY_E, false)),
                ('f', (evdev::Key::KEY_F, false)),
                ('g', (evdev::Key::KEY_G, false)),
                ('h', (evdev::Key::KEY_H, false)),
                ('i', (evdev::Key::KEY_I, false)),
                ('j', (evdev::Key::KEY_J, false)),
                ('k', (evdev::Key::KEY_K, false)),
                ('l', (evdev::Key::KEY_L, false)),
                ('m', (evdev::Key::KEY_M, false)),
                ('n', (evdev::Key::KEY_N, false)),
                ('o', (evdev::Key::KEY_O, false)),
                ('p', (evdev::Key::KEY_P, false)),
                ('q', (evdev::Key::KEY_Q, false)),
                ('r', (evdev::Key::KEY_R, false)),
                ('s', (evdev::Key::KEY_S, false)),
                ('t', (evdev::Key::KEY_T, false)),
                ('u', (evdev::Key::KEY_U, false)),
                ('v', (evdev::Key::KEY_V, false)),
                ('w', (evdev::Key::KEY_W, false)),
                ('x', (evdev::Key::KEY_X, false)),
                ('y', (evdev::Key::KEY_Y, false)),
                ('z', (evdev::Key::KEY_Z, false)),
                ('0', (evdev::Key::KEY_0, false)),
                ('1', (evdev::Key::KEY_1, false)),
                ('2', (evdev::Key::KEY_2, false)),
                ('3', (evdev::Key::KEY_3, false)),
                ('4', (evdev::Key::KEY_4, false)),
                ('5', (evdev::Key::KEY_5, false)),
                ('6', (evdev::Key::KEY_6, false)),
                ('7', (evdev::Key::KEY_7, false)),
                ('8', (evdev::Key::KEY_8, false)),
                ('9', (evdev::Key::KEY_9, false)),
                ('`', (evdev::Key::KEY_GRAVE, false)),
                ('-', (evdev::Key::KEY_MINUS, false)),
                ('=', (evdev::Key::KEY_EQUAL, false)),
                ('[', (evdev::Key::KEY_LEFTBRACE, false)),
                (']', (evdev::Key::KEY_RIGHTBRACE, false)),
                ('\\', (evdev::Key::KEY_BACKSLASH, false)),
                (',', (evdev::Key::KEY_COMMA, false)),
                ('.', (evdev::Key::KEY_DOT, false)),
                ('/', (evdev::Key::KEY_SLASH, false)),
                (';', (evdev::Key::KEY_SEMICOLON, false)),
                ('\'', (evdev::Key::KEY_APOSTROPHE, false)),

                // Shift + key
                ('A', (evdev::Key::KEY_A, true)),
                ('B', (evdev::Key::KEY_B, true)),
                ('C', (evdev::Key::KEY_C, true)),
                ('D', (evdev::Key::KEY_D, true)),
                ('E', (evdev::Key::KEY_E, true)),
                ('F', (evdev::Key::KEY_F, true)),
                ('G', (evdev::Key::KEY_G, true)),
                ('H', (evdev::Key::KEY_H, true)),
                ('I', (evdev::Key::KEY_I, true)),
                ('J', (evdev::Key::KEY_J, true)),
                ('K', (evdev::Key::KEY_K, true)),
                ('L', (evdev::Key::KEY_L, true)),
                ('M', (evdev::Key::KEY_M, true)),
                ('N', (evdev::Key::KEY_N, true)),
                ('O', (evdev::Key::KEY_O, true)),
                ('P', (evdev::Key::KEY_P, true)),
                ('Q', (evdev::Key::KEY_Q, true)),
                ('R', (evdev::Key::KEY_R, true)),
                ('S', (evdev::Key::KEY_S, true)),
                ('T', (evdev::Key::KEY_T, true)),
                ('U', (evdev::Key::KEY_U, true)),
                ('V', (evdev::Key::KEY_V, true)),
                ('W', (evdev::Key::KEY_W, true)),
                ('X', (evdev::Key::KEY_X, true)),
                ('Y', (evdev::Key::KEY_Y, true)),
                ('Z', (evdev::Key::KEY_Z, true)),
                (')', (evdev::Key::KEY_0, true)),
                ('!', (evdev::Key::KEY_1, true)),
                ('@', (evdev::Key::KEY_2, true)),
                ('#', (evdev::Key::KEY_3, true)),
                ('$', (evdev::Key::KEY_4, true)),
                ('%', (evdev::Key::KEY_5, true)),
                ('^', (evdev::Key::KEY_6, true)),
                ('&', (evdev::Key::KEY_7, true)),
                ('*', (evdev::Key::KEY_8, true)),
                ('(', (evdev::Key::KEY_9, true)),
                ('~', (evdev::Key::KEY_GRAVE, true)),
                ('_', (evdev::Key::KEY_MINUS, true)),
                ('+', (evdev::Key::KEY_EQUAL, true)),
                ('{', (evdev::Key::KEY_LEFTBRACE, true)),
                ('}', (evdev::Key::KEY_RIGHTBRACE, true)),
                ('|', (evdev::Key::KEY_BACKSLASH, true)),
                ('<', (evdev::Key::KEY_COMMA, true)),
                ('>', (evdev::Key::KEY_DOT, true)),
                ('?', (evdev::Key::KEY_SLASH, true)),
                (':', (evdev::Key::KEY_SEMICOLON, true)),
                ('"', (evdev::Key::KEY_APOSTROPHE, true)),
            ]);

        // ((minx, maxx), (miny, maxy))
        static ref RESOLUTION: Mutex<((i32, i32), (i32, i32))> = Mutex::new(((0, 0), (0, 0)));
    }

    /// Cached dynamic layout map built from the system's XKB keymap.
    /// `None` means the system uses US QWERTY or detection failed (fall back to KEY_MAP_LAYOUT).
    static DYNAMIC_LAYOUT_MAP: OnceLock<Option<HashMap<char, (evdev::Key, bool)>>> = OnceLock::new();

    /// Convert an XKB keysym name to an ASCII char.
    /// Returns None for dead keys, non-ASCII symbols, or unknown keysyms.
    fn xkb_sym_to_char(sym: &str) -> Option<char> {
        // Single ASCII character keysyms (e.g., "a", "A", "1")
        if sym.len() == 1 {
            let ch = sym.chars().next()?;
            if ch.is_ascii() {
                return Some(ch);
            }
        }
        // Named keysyms → ASCII char
        match sym {
            "space" => Some(' '),
            "exclam" => Some('!'),
            "at" => Some('@'),
            "numbersign" => Some('#'),
            "dollar" => Some('$'),
            "percent" => Some('%'),
            "asciicircum" => Some('^'),
            "ampersand" => Some('&'),
            "asterisk" => Some('*'),
            "parenleft" => Some('('),
            "parenright" => Some(')'),
            "minus" => Some('-'),
            "underscore" => Some('_'),
            "equal" => Some('='),
            "plus" => Some('+'),
            "bracketleft" => Some('['),
            "bracketright" => Some(']'),
            "braceleft" => Some('{'),
            "braceright" => Some('}'),
            "backslash" => Some('\\'),
            "bar" => Some('|'),
            "semicolon" => Some(';'),
            "colon" => Some(':'),
            "apostrophe" => Some('\''),
            "quotedbl" => Some('"'),
            "grave" => Some('`'),
            "asciitilde" => Some('~'),
            "comma" => Some(','),
            "period" => Some('.'),
            "slash" => Some('/'),
            "less" => Some('<'),
            "greater" => Some('>'),
            "question" => Some('?'),
            _ => None, // dead keys, accented chars, etc. — skip
        }
    }

    /// Sync the XWayland keyboard layout with the system layout.
    /// This prevents mismatches that can occur when other software
    /// (e.g., Docker Desktop) resets the XWayland keymap.
    pub fn sync_xwayland_layout() {
        let localectl = match std::process::Command::new("localectl")
            .arg("status")
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                log::warn!("Failed to run localectl: {}", e);
                return;
            }
        };
        let out = String::from_utf8_lossy(&localectl.stdout);
        let mut layout = String::new();
        let mut variant = String::new();
        for line in out.lines() {
            let line = line.trim();
            if line.starts_with("X11 Layout:") {
                layout = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("X11 Variant:") {
                variant = line.split(':').nth(1).unwrap_or("").trim().to_string();
            }
        }
        if layout.is_empty() {
            return;
        }
        let mut args = vec!["-layout".to_string(), layout.clone()];
        if !variant.is_empty() {
            args.push("-variant".to_string());
            args.push(variant.clone());
        }
        match std::process::Command::new("setxkbmap").args(&args).output() {
            Ok(_) => log::info!(
                "Synced XWayland layout to {} {}",
                layout,
                variant
            ),
            Err(e) => log::warn!("Failed to run setxkbmap: {}", e),
        }
    }

    /// Build a dynamic char→(evdev::Key, is_shift) map by parsing the system's
    /// XKB keymap via `xkbcomp`. This correctly handles non-US layouts (AZERTY,
    /// QWERTZ, etc.) where the character positions differ from QWERTY.
    fn build_dynamic_layout_map() -> Option<HashMap<char, (evdev::Key, bool)>> {
        // XKB key name → evdev keycode mapping (standard for evdev-based systems)
        let xkb_to_evdev: &[(&str, u16)] = &[
            // Grave / tilde
            ("TLDE", 41),
            // Number row
            ("AE01", 2), ("AE02", 3), ("AE03", 4), ("AE04", 5), ("AE05", 6),
            ("AE06", 7), ("AE07", 8), ("AE08", 9), ("AE09", 10), ("AE10", 11),
            ("AE11", 12), ("AE12", 13),
            // Top alpha row (QWERTY: Q W E R T Y U I O P [ ])
            ("AD01", 16), ("AD02", 17), ("AD03", 18), ("AD04", 19), ("AD05", 20),
            ("AD06", 21), ("AD07", 22), ("AD08", 23), ("AD09", 24), ("AD10", 25),
            ("AD11", 26), ("AD12", 27),
            // Home alpha row (QWERTY: A S D F G H J K L ; ')
            ("AC01", 30), ("AC02", 31), ("AC03", 32), ("AC04", 33), ("AC05", 34),
            ("AC06", 35), ("AC07", 36), ("AC08", 37), ("AC09", 38), ("AC10", 39),
            ("AC11", 40),
            // Bottom alpha row (QWERTY: Z X C V B N M , . /)
            ("AB01", 44), ("AB02", 45), ("AB03", 46), ("AB04", 47), ("AB05", 48),
            ("AB06", 49), ("AB07", 50), ("AB08", 51), ("AB09", 52), ("AB10", 53),
            // Extra keys
            ("BKSL", 43), ("LSGT", 86),
        ];
        let xkb_map: HashMap<&str, u16> = xkb_to_evdev.iter().copied().collect();

        // Check system layout
        let localectl = std::process::Command::new("localectl")
            .arg("status")
            .output()
            .ok()?;
        let out = String::from_utf8_lossy(&localectl.stdout);
        let mut layout = String::new();
        for line in out.lines() {
            let line = line.trim();
            if line.starts_with("X11 Layout:") {
                layout = line.split(':').nth(1).unwrap_or("").trim().to_string();
            }
        }

        if layout.is_empty() || layout == "us" {
            log::info!("System layout is US QWERTY, using static KEY_MAP_LAYOUT");
            return None;
        }

        log::info!("Non-US layout detected ({}), building dynamic keymap", layout);

        // Get the compiled keymap from xkbcomp
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("xkbcomp {} - 2>/dev/null", display))
            .output()
            .ok()?;
        let xkb_text = String::from_utf8_lossy(&output.stdout);

        if xkb_text.is_empty() {
            log::warn!("xkbcomp returned empty output, falling back to static map");
            return None;
        }

        let mut result: HashMap<char, (evdev::Key, bool)> = HashMap::new();

        // Parse each key block:
        //   key <NAME> { ... symbols[Group1]= [ sym0, Sym1, sym2, sym3 ] ... };
        // sym0 = unshifted, sym1 = shifted
        for (key_name, evdev_code) in xkb_to_evdev {
            let pattern = format!("key <{}>", key_name);
            if let Some(start) = xkb_text.find(&pattern) {
                let block = &xkb_text[start..];
                if let Some(end) = block.find("};") {
                    let block = &block[..end];
                    if let Some(sym_start) = block.find("symbols[Group1]= [") {
                        let after = &block[sym_start + "symbols[Group1]= [".len()..];
                        if let Some(sym_end) = after.find(']') {
                            let syms: Vec<&str> = after[..sym_end]
                                .split(',')
                                .map(|s| s.trim())
                                .collect();

                            let evdev_key = evdev::Key::new(*evdev_code);

                            // Level 0: unshifted
                            if let Some(sym0) = syms.get(0) {
                                if let Some(ch) = xkb_sym_to_char(sym0) {
                                    result.entry(ch).or_insert((evdev_key, false));
                                }
                            }
                            // Level 1: shifted
                            if let Some(sym1) = syms.get(1) {
                                if let Some(ch) = xkb_sym_to_char(sym1) {
                                    result.entry(ch).or_insert((evdev_key, true));
                                }
                            }
                        }
                    }
                }
            }
        }

        if result.is_empty() {
            log::warn!("Dynamic layout map is empty, falling back to static map");
            return None;
        }

        log::info!("Built dynamic layout map with {} entries", result.len());
        Some(result)
    }

    /// Get or build the dynamic layout map. Returns None if US QWERTY or on failure.
    fn get_dynamic_layout_map() -> &'static Option<HashMap<char, (evdev::Key, bool)>> {
        DYNAMIC_LAYOUT_MAP.get_or_init(|| build_dynamic_layout_map())
    }

    pub fn create_uinput_keyboard() -> ResultType<VirtualDevice> {
        // TODO: ensure keys here
        let mut keys = AttributeSet::<evdev::Key>::new();
        for i in evdev::Key::KEY_ESC.code()..(evdev::Key::BTN_TRIGGER_HAPPY40.code() + 1) {
            let key = evdev::Key::new(i);
            if !format!("{:?}", &key).contains("unknown key") {
                keys.insert(key);
            }
        }
        let mut leds = AttributeSet::<evdev::LedType>::new();
        leds.insert(evdev::LedType::LED_NUML);
        leds.insert(evdev::LedType::LED_CAPSL);
        leds.insert(evdev::LedType::LED_SCROLLL);
        let mut miscs = AttributeSet::<evdev::MiscType>::new();
        miscs.insert(evdev::MiscType::MSC_SCAN);
        let keyboard = VirtualDeviceBuilder::new()?
            .name("RustDesk UInput Keyboard")
            .with_keys(&keys)?
            .with_leds(&leds)?
            .with_miscs(&miscs)?
            .build()?;
        Ok(keyboard)
    }

    pub fn map_key(key: &enigo::Key) -> ResultType<(evdev::Key, bool)> {
        if let Some(k) = KEY_MAP.get(&key) {
            log::trace!("mapkey {:?}, get {:?}", &key, &k);
            return Ok((k.clone(), false));
        } else {
            match key {
                enigo::Key::Layout(c) => {
                    // Try dynamic layout map first (handles AZERTY, QWERTZ, etc.)
                    if let Some(dynamic_map) = get_dynamic_layout_map() {
                        if let Some((k, is_shift)) = dynamic_map.get(&c) {
                            log::trace!("mapkey {:?}, dynamic get {:?}", &key, k);
                            return Ok((k.clone(), is_shift.clone()));
                        }
                    }
                    // Fall back to static QWERTY map
                    if let Some((k, is_shift)) = KEY_MAP_LAYOUT.get(&c) {
                        log::trace!("mapkey {:?}, static get {:?}", &key, k);
                        return Ok((k.clone(), is_shift.clone()));
                    }
                }
                _ => {}
            }
        }
        bail!("Failed to map key {:?}", &key);
    }

    async fn ipc_send_data(stream: &mut Connection, data: &Data) {
        allow_err!(stream.send(data).await);
    }

    async fn handle_keyboard(
        stream: &mut Connection,
        keyboard: &mut VirtualDevice,
        data: &DataKeyboard,
    ) {
        log::trace!("handle_keyboard {:?}", &data);
        match data {
            DataKeyboard::Sequence(_seq) => {
                // ignore
            }
            DataKeyboard::KeyDown(enigo::Key::Raw(code)) => {
                let down_event = InputEvent::new(EventType::KEY, *code - 8, 1);
                allow_err!(keyboard.emit(&[down_event]));
            }
            DataKeyboard::KeyUp(enigo::Key::Raw(code)) => {
                let up_event = InputEvent::new(EventType::KEY, *code - 8, 0);
                allow_err!(keyboard.emit(&[up_event]));
            }
            DataKeyboard::KeyDown(key) => {
                if let Ok((k, is_shift)) = map_key(key) {
                    if is_shift {
                        let down_event =
                            InputEvent::new(EventType::KEY, evdev::Key::KEY_LEFTSHIFT.code(), 1);
                        allow_err!(keyboard.emit(&[down_event]));
                    }
                    let down_event = InputEvent::new(EventType::KEY, k.code(), 1);
                    allow_err!(keyboard.emit(&[down_event]));
                }
            }
            DataKeyboard::KeyUp(key) => {
                if let Ok((k, _)) = map_key(key) {
                    let up_event = InputEvent::new(EventType::KEY, k.code(), 0);
                    allow_err!(keyboard.emit(&[up_event]));
                }
            }
            DataKeyboard::KeyClick(key) => {
                if let Ok((k, _)) = map_key(key) {
                    let down_event = InputEvent::new(EventType::KEY, k.code(), 1);
                    let up_event = InputEvent::new(EventType::KEY, k.code(), 0);
                    allow_err!(keyboard.emit(&[down_event, up_event]));
                }
            }
            DataKeyboard::GetKeyState(key) => {
                let key_state = if enigo::Key::CapsLock == *key {
                    match keyboard.get_led_state() {
                        Ok(leds) => leds.contains(evdev::LedType::LED_CAPSL),
                        Err(_e) => {
                            // log::debug!("Failed to get led state {}", &_e);
                            false
                        }
                    }
                } else if enigo::Key::NumLock == *key {
                    match keyboard.get_led_state() {
                        Ok(leds) => leds.contains(evdev::LedType::LED_NUML),
                        Err(_e) => {
                            // log::debug!("Failed to get led state {}", &_e);
                            false
                        }
                    }
                } else {
                    match keyboard.get_key_state() {
                        Ok(keys) => match key {
                            enigo::Key::Shift => {
                                keys.contains(evdev::Key::KEY_LEFTSHIFT)
                                    || keys.contains(evdev::Key::KEY_RIGHTSHIFT)
                            }
                            enigo::Key::Control => {
                                keys.contains(evdev::Key::KEY_LEFTCTRL)
                                    || keys.contains(evdev::Key::KEY_RIGHTCTRL)
                            }
                            enigo::Key::Alt => {
                                keys.contains(evdev::Key::KEY_LEFTALT)
                                    || keys.contains(evdev::Key::KEY_RIGHTALT)
                            }
                            enigo::Key::Meta => {
                                keys.contains(evdev::Key::KEY_LEFTMETA)
                                    || keys.contains(evdev::Key::KEY_RIGHTMETA)
                            }
                            _ => false,
                        },
                        Err(_e) => {
                            // log::debug!("Failed to get key state: {}", &_e);
                            false
                        }
                    }
                };
                ipc_send_data(
                    stream,
                    &Data::KeyboardResponse(ipc::DataKeyboardResponse::GetKeyState(key_state)),
                )
                .await;
            }
        }
    }

    fn handle_mouse(mouse: &mut mouce::UInputMouseManager, data: &DataMouse) {
        log::trace!("handle_mouse {:?}", &data);
        match data {
            DataMouse::MoveTo(x, y) => {
                allow_err!(mouse.move_to(*x as _, *y as _))
            }
            DataMouse::MoveRelative(x, y) => {
                allow_err!(mouse.move_relative(*x, *y))
            }
            DataMouse::Down(button) => {
                let btn = match button {
                    enigo::MouseButton::Left => mouce::MouseButton::Left,
                    enigo::MouseButton::Middle => mouce::MouseButton::Middle,
                    enigo::MouseButton::Right => mouce::MouseButton::Right,
                    _ => {
                        return;
                    }
                };
                allow_err!(mouse.press_button(&btn))
            }
            DataMouse::Up(button) => {
                let btn = match button {
                    enigo::MouseButton::Left => mouce::MouseButton::Left,
                    enigo::MouseButton::Middle => mouce::MouseButton::Middle,
                    enigo::MouseButton::Right => mouce::MouseButton::Right,
                    _ => {
                        return;
                    }
                };
                allow_err!(mouse.release_button(&btn))
            }
            DataMouse::Click(button) => {
                let btn = match button {
                    enigo::MouseButton::Left => mouce::MouseButton::Left,
                    enigo::MouseButton::Middle => mouce::MouseButton::Middle,
                    enigo::MouseButton::Right => mouce::MouseButton::Right,
                    _ => {
                        return;
                    }
                };
                allow_err!(mouse.click_button(&btn))
            }
            DataMouse::ScrollX(_length) => {
                // TODO: not supported for now
            }
            DataMouse::ScrollY(length) => {
                let mut length = *length;

                let scroll = if length < 0 {
                    mouce::ScrollDirection::Up
                } else {
                    mouce::ScrollDirection::Down
                };

                if length < 0 {
                    length = -length;
                }

                for _ in 0..length {
                    allow_err!(mouse.scroll_wheel(&scroll))
                }
            }
            DataMouse::Refresh => {
                // unreachable!()
            }
        }
    }

    fn spawn_keyboard_handler(mut stream: Connection) {
        tokio::spawn(async move {
            let mut keyboard = match create_uinput_keyboard() {
                Ok(keyboard) => keyboard,
                Err(e) => {
                    log::error!("Failed to create keyboard {}", e);
                    return;
                }
            };
            loop {
                tokio::select! {
                    res = stream.next() => {
                        match res {
                            Err(err) => {
                                log::info!("UInput keyboard ipc connection closed: {}", err);
                                break;
                            }
                            Ok(Some(data)) => {
                                match data {
                                    Data::Keyboard(data) => {
                                        handle_keyboard(&mut stream, &mut keyboard, &data).await;
                                    }
                                    _ => {
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }

    fn spawn_mouse_handler(mut stream: ipc::Connection) {
        let resolution = RESOLUTION.lock().unwrap();
        if resolution.0 .0 == resolution.0 .1 || resolution.1 .0 == resolution.1 .1 {
            return;
        }
        let rng_x = resolution.0.clone();
        let rng_y = resolution.1.clone();
        tokio::spawn(async move {
            log::info!(
                "Create uinput mouce with rng_x: ({}, {}), rng_y: ({}, {})",
                rng_x.0,
                rng_x.1,
                rng_y.0,
                rng_y.1
            );
            let mut mouse = match mouce::UInputMouseManager::new(rng_x, rng_y) {
                Ok(mouse) => mouse,
                Err(e) => {
                    log::error!("Failed to create mouse, {}", e);
                    return;
                }
            };
            loop {
                tokio::select! {
                    res = stream.next() => {
                        match res {
                            Err(err) => {
                                log::info!("UInput mouse ipc connection closed: {}", err);
                                break;
                            }
                            Ok(Some(data)) => {
                                match data {
                                    Data::Mouse(data) => {
                                        if let DataMouse::Refresh = data {
                                            let resolution = RESOLUTION.lock().unwrap();
                                            let rng_x = resolution.0.clone();
                                            let rng_y = resolution.1.clone();
                                            log::info!(
                                                "Refresh uinput mouce with rng_x: ({}, {}), rng_y: ({}, {})",
                                                rng_x.0,
                                                rng_x.1,
                                                rng_y.0,
                                                rng_y.1
                                            );
                                            mouse = match mouce::UInputMouseManager::new(rng_x, rng_y) {
                                                Ok(mouse) => mouse,
                                                Err(e) => {
                                                    log::error!("Failed to create mouse, {}", e);
                                                    return;
                                                }
                                            }
                                        } else {
                                            handle_mouse(&mut mouse, &data);
                                        }
                                    }
                                    _ => {
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }

    fn spawn_controller_handler(mut stream: ipc::Connection) {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    res = stream.next() => {
                        match res {
                            Err(_err) => {
                                // log::info!("UInput controller ipc connection closed: {}", err);
                                break;
                            }
                            Ok(Some(data)) => {
                                match data {
                                    Data::Control(data) => match data {
                                        ipc::DataControl::Resolution{
                                            minx,
                                            maxx,
                                            miny,
                                            maxy,
                                        } => {
                                            *RESOLUTION.lock().unwrap() = ((minx, maxx), (miny, maxy));
                                            allow_err!(stream.send(&Data::Empty).await);
                                        }
                                    }
                                    _ => {
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }

    /// Start uinput service.
    async fn start_service<F: FnOnce(ipc::Connection) + Copy>(postfix: &str, handler: F) {
        match new_listener(postfix).await {
            Ok(mut incoming) => {
                while let Some(result) = incoming.next().await {
                    match result {
                        Ok(stream) => {
                            log::debug!("Got new connection of uinput ipc {}", postfix);
                            handler(Connection::new(stream));
                        }
                        Err(err) => {
                            log::error!("Couldn't get uinput mouse client: {:?}", err);
                        }
                    }
                }
            }
            Err(err) => {
                log::error!("Failed to start uinput mouse ipc service: {}", err);
            }
        }
    }

    /// Start uinput keyboard service.
    #[tokio::main(flavor = "current_thread")]
    pub async fn start_service_keyboard() {
        log::info!("start uinput keyboard service");
        start_service(IPC_POSTFIX_KEYBOARD, spawn_keyboard_handler).await;
    }

    /// Start uinput mouse service.
    #[tokio::main(flavor = "current_thread")]
    pub async fn start_service_mouse() {
        log::info!("start uinput mouse service");
        start_service(IPC_POSTFIX_MOUSE, spawn_mouse_handler).await;
    }

    /// Start uinput mouse service.
    #[tokio::main(flavor = "current_thread")]
    pub async fn start_service_control() {
        log::info!("start uinput control service");
        start_service(IPC_POSTFIX_CONTROL, spawn_controller_handler).await;
    }

    pub fn stop_service_keyboard() {
        log::info!("stop uinput keyboard service");
    }
    pub fn stop_service_mouse() {
        log::info!("stop uinput mouse service");
    }
    pub fn stop_service_control() {
        log::info!("stop uinput control service");
    }
}

// https://github.com/emrebicer/mouce
pub(crate) mod mouce {
    use std::{
        fs::File,
        io::{Error, ErrorKind, Result},
        mem::size_of,
        os::{
            raw::{c_char, c_int, c_long, c_uint, c_ulong, c_ushort},
            unix::{fs::OpenOptionsExt, io::AsRawFd},
        },
        thread,
        time::Duration,
    };

    pub const O_NONBLOCK: c_int = 2048;

    /// ioctl and uinput definitions
    pub(super) const UI_ABS_SETUP: c_ulong = 1075598596;
    pub(super) const UI_SET_EVBIT: c_ulong = 1074025828;
    pub(super) const UI_SET_KEYBIT: c_ulong = 1074025829;
    pub(super) const UI_SET_RELBIT: c_ulong = 1074025830;
    pub(super) const UI_SET_ABSBIT: c_ulong = 1074025831;
    pub(super) const UI_SET_MSCBIT: c_ulong = 1074025832;
    pub(super) const UI_SET_LEDBIT: c_ulong = 1074025841;
    pub(super) const UI_DEV_SETUP: c_ulong = 1079792899;
    pub(super) const UI_DEV_CREATE: c_ulong = 21761;
    pub(super) const UI_DEV_DESTROY: c_uint = 21762;

    pub const EV_KEY: c_int = 0x01;
    pub const EV_REL: c_int = 0x02;
    pub const EV_ABS: c_int = 0x03;
    pub(super) const EV_MSC: c_int = 0x04;
    pub(super) const EV_LED: c_int = 0x11;
    pub const REL_X: c_uint = 0x00;
    pub const REL_Y: c_uint = 0x01;
    pub const ABS_X: c_uint = 0x00;
    pub const ABS_Y: c_uint = 0x01;
    pub const REL_WHEEL: c_uint = 0x08;
    pub const REL_HWHEEL: c_uint = 0x06;
    pub const BTN_LEFT: c_int = 0x110;
    pub const BTN_RIGHT: c_int = 0x111;
    pub const BTN_MIDDLE: c_int = 0x112;
    pub const BTN_SIDE: c_int = 0x113;
    pub const BTN_EXTRA: c_int = 0x114;
    pub const BTN_FORWARD: c_int = 0x115;
    pub const BTN_BACK: c_int = 0x116;
    pub const BTN_TASK: c_int = 0x117;
    pub(super) const MSC_SCAN: c_int = 0x04;
    pub(super) const LED_NUML: c_int = 0x00;
    pub(super) const LED_CAPSL: c_int = 0x01;
    pub(super) const LED_SCROLLL: c_int = 0x02;
    const SYN_REPORT: c_int = 0x00;
    pub(super) const EV_SYN: c_int = 0x00;
    pub(super) const BUS_USB: c_ushort = 0x03;

    /// uinput types
    #[repr(C)]
    pub(super) struct UInputSetup {
        pub id: InputId,
        pub name: [c_char; UINPUT_MAX_NAME_SIZE],
        pub ff_effects_max: c_ulong,
    }

    #[repr(C)]
    pub(super) struct InputId {
        pub bustype: c_ushort,
        pub vendor: c_ushort,
        pub product: c_ushort,
        pub version: c_ushort,
    }

    #[repr(C)]
    pub(super) struct InputEvent {
        pub time: TimeVal,
        pub r#type: c_ushort,
        pub code: c_ushort,
        pub value: c_int,
    }

    #[repr(C)]
    pub(super) struct TimeVal {
        pub tv_sec: c_ulong,
        pub tv_usec: c_ulong,
    }

    #[repr(C)]
    pub struct UinputAbsSetup {
        pub code: c_ushort,
        pub absinfo: InputAbsinfo,
    }

    #[repr(C)]
    pub struct InputAbsinfo {
        pub value: c_int,
        pub minimum: c_int,
        pub maximum: c_int,
        pub fuzz: c_int,
        pub flat: c_int,
        pub resolution: c_int,
    }

    extern "C" {
        pub(super) fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
        pub(super) fn write(fd: c_int, buf: *mut InputEvent, count: usize) -> c_long;
    }

    #[derive(Debug, Copy, Clone)]
    pub enum MouseButton {
        Left,
        Middle,
        Side,
        Extra,
        Right,
        Back,
        Forward,
        Task,
    }

    #[derive(Debug, Copy, Clone)]
    pub enum ScrollDirection {
        Up,
        Down,
        Right,
        Left,
    }

    pub(super) const UINPUT_MAX_NAME_SIZE: usize = 80;

    pub struct UInputMouseManager {
        uinput_file: File,
    }

    impl UInputMouseManager {
        pub fn new(rng_x: (i32, i32), rng_y: (i32, i32)) -> Result<Self> {
            let manager = UInputMouseManager {
                uinput_file: File::options()
                    .write(true)
                    .custom_flags(O_NONBLOCK)
                    .open("/dev/uinput")?,
            };
            let fd = manager.uinput_file.as_raw_fd();
            unsafe {
                // For press events (also needed for mouse movement)
                ioctl(fd, UI_SET_EVBIT, EV_KEY);
                ioctl(fd, UI_SET_KEYBIT, BTN_LEFT);
                ioctl(fd, UI_SET_KEYBIT, BTN_RIGHT);
                ioctl(fd, UI_SET_KEYBIT, BTN_MIDDLE);

                // For mouse movement
                ioctl(fd, UI_SET_EVBIT, EV_ABS);
                ioctl(fd, UI_SET_ABSBIT, ABS_X);
                ioctl(
                    fd,
                    UI_ABS_SETUP,
                    &UinputAbsSetup {
                        code: ABS_X as _,
                        absinfo: InputAbsinfo {
                            value: 0,
                            minimum: rng_x.0,
                            maximum: rng_x.1,
                            fuzz: 0,
                            flat: 0,
                            resolution: 0,
                        },
                    },
                );
                ioctl(fd, UI_SET_ABSBIT, ABS_Y);
                ioctl(
                    fd,
                    UI_ABS_SETUP,
                    &UinputAbsSetup {
                        code: ABS_Y as _,
                        absinfo: InputAbsinfo {
                            value: 0,
                            minimum: rng_y.0,
                            maximum: rng_y.1,
                            fuzz: 0,
                            flat: 0,
                            resolution: 0,
                        },
                    },
                );

                ioctl(fd, UI_SET_EVBIT, EV_REL);
                ioctl(fd, UI_SET_RELBIT, REL_X);
                ioctl(fd, UI_SET_RELBIT, REL_Y);
                ioctl(fd, UI_SET_RELBIT, REL_WHEEL);
                ioctl(fd, UI_SET_RELBIT, REL_HWHEEL);
            }

            let mut usetup = UInputSetup {
                id: InputId {
                    bustype: BUS_USB,
                    // Random vendor and product
                    vendor: 0x2222,
                    product: 0x3333,
                    version: 0,
                },
                name: [0; UINPUT_MAX_NAME_SIZE],
                ff_effects_max: 0,
            };

            let mut device_bytes: Vec<c_char> = "mouce-library-fake-mouse"
                .chars()
                .map(|ch| ch as c_char)
                .collect();

            // Fill the rest of the name buffer with empty chars
            for _ in 0..UINPUT_MAX_NAME_SIZE - device_bytes.len() {
                device_bytes.push('\0' as c_char);
            }

            usetup.name.copy_from_slice(&device_bytes);

            unsafe {
                ioctl(fd, UI_DEV_SETUP, &usetup);
                ioctl(fd, UI_DEV_CREATE);
            }

            // On UI_DEV_CREATE the kernel will create the device node for this
            // device. We are inserting a pause here so that userspace has time
            // to detect, initialize the new device, and can start listening to
            // the event, otherwise it will not notice the event we are about to send.
            thread::sleep(Duration::from_millis(300));

            Ok(manager)
        }

        /// Write the given event to the uinput file
        fn emit(&self, r#type: c_int, code: c_int, value: c_int) -> Result<()> {
            let mut event = InputEvent {
                time: TimeVal {
                    tv_sec: 0,
                    tv_usec: 0,
                },
                r#type: r#type as c_ushort,
                code: code as c_ushort,
                value,
            };
            let fd = self.uinput_file.as_raw_fd();

            unsafe {
                let count = size_of::<InputEvent>();
                let written_bytes = write(fd, &mut event, count);
                if written_bytes == -1 || written_bytes != count as c_long {
                    return Err(Error::new(
                        ErrorKind::Other,
                        format!("failed while trying to write to a file"),
                    ));
                }
            }

            Ok(())
        }

        /// Syncronize the device
        fn syncronize(&self) -> Result<()> {
            self.emit(EV_SYN, SYN_REPORT, 0)?;
            // Give uinput some time to update the mouse location,
            // otherwise it fails to move the mouse on release mode
            // A delay of 1 milliseconds seems to be enough for it
            thread::sleep(Duration::from_millis(1));
            Ok(())
        }

        /// Move the mouse relative to the current position
        fn move_relative_(&self, x: i32, y: i32) -> Result<()> {
            // uinput does not move the mouse in pixels but uses `units`. I couldn't
            // find information regarding to this uinput `unit`, but according to
            // my findings 1 unit corresponds to exactly 2 pixels.
            //
            // To achieve the expected behavior; divide the parameters by 2
            //
            // This seems like there is a bug in this crate, but the
            // behavior is the same on other projects that make use of
            // uinput. e.g. `ydotool`. When you try to move your mouse,
            // it will move 2x further pixels
            self.emit(EV_REL, REL_X as c_int, (x as f32 / 2.).ceil() as c_int)?;
            self.emit(EV_REL, REL_Y as c_int, (y as f32 / 2.).ceil() as c_int)?;
            self.syncronize()
        }

        fn map_btn(button: &MouseButton) -> c_int {
            match button {
                MouseButton::Left => BTN_LEFT,
                MouseButton::Right => BTN_RIGHT,
                MouseButton::Middle => BTN_MIDDLE,
                MouseButton::Side => BTN_SIDE,
                MouseButton::Extra => BTN_EXTRA,
                MouseButton::Forward => BTN_FORWARD,
                MouseButton::Back => BTN_BACK,
                MouseButton::Task => BTN_TASK,
            }
        }

        pub fn move_to(&self, x: usize, y: usize) -> Result<()> {
            // // For some reason, absolute mouse move events are not working on uinput
            // // (as I understand those events are intended for touch events)
            // //
            // // As a work around solution; first set the mouse to top left, then
            // // call relative move function to simulate an absolute move event
            //self.move_relative(i32::MIN, i32::MIN)?;
            //self.move_relative(x as i32, y as i32)

            self.emit(EV_ABS, ABS_X as c_int, x as c_int)?;
            self.emit(EV_ABS, ABS_Y as c_int, y as c_int)?;
            self.syncronize()
        }

        pub fn move_relative(&self, x_offset: i32, y_offset: i32) -> Result<()> {
            self.move_relative_(x_offset, y_offset)
        }

        pub fn press_button(&self, button: &MouseButton) -> Result<()> {
            self.emit(EV_KEY, Self::map_btn(button), 1)?;
            self.syncronize()
        }

        pub fn release_button(&self, button: &MouseButton) -> Result<()> {
            self.emit(EV_KEY, Self::map_btn(button), 0)?;
            self.syncronize()
        }

        pub fn click_button(&self, button: &MouseButton) -> Result<()> {
            self.press_button(button)?;
            self.release_button(button)
        }

        pub fn scroll_wheel(&self, direction: &ScrollDirection) -> Result<()> {
            let (code, scroll_value) = match direction {
                ScrollDirection::Up => (REL_WHEEL, 1),
                ScrollDirection::Down => (REL_WHEEL, -1),
                ScrollDirection::Left => (REL_HWHEEL, -1),
                ScrollDirection::Right => (REL_HWHEEL, 1),
            };
            self.emit(EV_REL, code as c_int, scroll_value)?;
            self.syncronize()
        }
    }

    impl Drop for UInputMouseManager {
        fn drop(&mut self) {
            let fd = self.uinput_file.as_raw_fd();
            unsafe {
                // Destroy the device, the file is closed automatically by the File module
                ioctl(fd, UI_DEV_DESTROY as c_ulong);
            }
        }
    }
}
