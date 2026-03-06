use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, InputEvent, Key, Device, EventType};
use std::thread;
use std::time::Duration;
use std::process::Command;
use zbus::blocking::Connection;


#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PasteMode {
    CtrlV,
    #[allow(dead_code)]
    CtrlShiftV,
    #[allow(dead_code)]
    ShiftInsert,
    #[allow(dead_code)]
    UnicodeHex, // Ctrl+Shift+U method
    Fcitx5,     // Native D-Bus CommitString method
}

pub struct Vkbd {
    pub dev: VirtualDevice,
    pub paste_mode: PasteMode,
    pub clipboard_delay_ms: u64,
    dbus_conn: Option<Connection>,
}

impl Vkbd {
    pub fn new(phys_dev: &Device) -> Result<Self, Box<dyn std::error::Error>> {
        let mut keys = AttributeSet::new();
        
        if let Some(supported) = phys_dev.supported_keys() {
            for k in supported.iter() {
                keys.insert(k);
            }
        }
        
        // Ensure keys required for all paste modes are available
        keys.insert(Key::KEY_LEFTCTRL);
        keys.insert(Key::KEY_RIGHTCTRL);
        keys.insert(Key::KEY_LEFTSHIFT);
        keys.insert(Key::KEY_RIGHTSHIFT);
        keys.insert(Key::KEY_LEFTALT);
        keys.insert(Key::KEY_RIGHTALT);
        keys.insert(Key::KEY_LEFTMETA);
        keys.insert(Key::KEY_V);
        keys.insert(Key::KEY_INSERT); 
        keys.insert(Key::KEY_U); 
        keys.insert(Key::KEY_ENTER);
        keys.insert(Key::KEY_KPENTER);
        keys.insert(Key::KEY_BACKSPACE);
        keys.insert(Key::KEY_SPACE);
        keys.insert(Key::KEY_ESC);
        keys.insert(Key::KEY_TAB);

        let dev = VirtualDeviceBuilder::new()? 
            .name("rust-ime-v2")
            .with_keys(&keys)?
            // 关键：声明支持同步物理扫描码事件
            .with_relative_axes(&AttributeSet::new())? 
            .build()?;

        // 尝试建立 D-Bus 连接 (Fcitx5)
        let dbus_conn = Connection::session().ok();

        Ok(Self { 
            dev,
            paste_mode: PasteMode::ShiftInsert, // Standard universal mode
            clipboard_delay_ms: 50,
            dbus_conn,
        })
    }

    #[allow(dead_code)]
    pub fn cycle_paste_mode(&mut self) -> String {
        self.paste_mode = match self.paste_mode {
            PasteMode::ShiftInsert => PasteMode::CtrlV,
            PasteMode::CtrlV => PasteMode::CtrlShiftV,
            PasteMode::CtrlShiftV => PasteMode::UnicodeHex,
            PasteMode::UnicodeHex => PasteMode::Fcitx5,
            PasteMode::Fcitx5 => PasteMode::ShiftInsert,
        };
        
        println!("[Vkbd] Manually switched paste mode to: {:?}", self.paste_mode);
        
        match self.paste_mode {
            PasteMode::ShiftInsert => "通用模式 (Shift+Insert)".to_string(),
            PasteMode::CtrlV => "标准模式 (Ctrl+V)".to_string(),
            PasteMode::CtrlShiftV => "终端模式 (Ctrl+Shift+V)".to_string(),
            PasteMode::UnicodeHex => "Unicode编码输入 (Ctrl+Shift+U)".to_string(),
            PasteMode::Fcitx5 => "Fcitx5 接口".to_string(),
        }
    }

    pub fn send_text(&mut self, text: &str) {
        self.send_text_internal(text, false);
    }

    #[allow(dead_code)]
    pub fn send_text_highlighted(&mut self, text: &str) {
        self.send_text_internal(text, true);
    }

    fn send_text_internal(&mut self, text: &str, highlight: bool) {
        if text.is_empty() { return; }

        // FAST PATH: Only for supported lowercase, digits and basic punctuation
        if !highlight && text.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || " /'.,;[]\\-=`".contains(c)) {
            for c in text.chars() {
                if let Some(key) = char_to_key(c) {
                    self.tap(key);
                    // 极小延迟防止粘连
                    thread::sleep(Duration::from_micros(200));
                }
            }
            return;
        }

        println!("[IME] Emitting text via heavy path: {} (highlight={})", text, highlight);

        // If using UnicodeHex mode, skip clipboard and type directly
        if self.paste_mode == PasteMode::UnicodeHex {
            for c in text.chars() {
                self.send_char_via_unicode(c);
            }
            return;
        }

        // 0. 优先尝试 Fcitx5
        if self.paste_mode == PasteMode::Fcitx5 {
            if self.send_via_fcitx(text) {
                return;
            }
            println!("[Vkbd] Fcitx5 fallback to clipboard...");
        }

        // 1. 优先尝试剪贴板
        if self.send_via_clipboard(text) {
            if highlight {
                let count = text.chars().count();
                thread::sleep(Duration::from_millis(150));
                self.emit(Key::KEY_LEFTSHIFT, true);
                for _ in 0..count {
                    self.tap(Key::KEY_LEFT);
                    thread::sleep(Duration::from_millis(2));
                }
                self.emit(Key::KEY_LEFTSHIFT, false);
            }
            return;
        }

        // 2. 失败处理 (ydotool)
        if self.send_via_ydotool(text) {
             self.release_all();
             return;
        }
        self.release_all();
    }

    fn send_via_clipboard(&mut self, text: &str) -> bool {
        use arboard::Clipboard;
        
        let mut cb = match Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[Error] Failed to initialize clipboard (arboard): {}", e);
                return false;
            }
        };

        if let Err(e) = cb.set_text(text.to_string()) {
            eprintln!("[Error] Failed to set clipboard text: {}", e);
            return false;
        }

        // 使用配置的延迟时间，确保应用感知到剪贴板变化
        thread::sleep(Duration::from_millis(self.clipboard_delay_ms));
        
        match self.paste_mode {
            PasteMode::CtrlV => {
                println!("[Vkbd] Injecting via Ctrl+V");
                self.emit(Key::KEY_LEFTCTRL, true);
                thread::sleep(Duration::from_millis(15));
                self.tap(Key::KEY_V);
                thread::sleep(Duration::from_millis(15));
                self.emit(Key::KEY_LEFTCTRL, false);
            },
            PasteMode::CtrlShiftV => {
                println!("[Vkbd] Injecting via Ctrl+Shift+V");
                self.emit(Key::KEY_LEFTCTRL, true);
                self.emit(Key::KEY_LEFTSHIFT, true);
                thread::sleep(Duration::from_millis(15));
                self.tap(Key::KEY_V);
                thread::sleep(Duration::from_millis(15));
                self.emit(Key::KEY_LEFTSHIFT, false);
                self.emit(Key::KEY_LEFTCTRL, false);
            },
            PasteMode::ShiftInsert => {
                println!("[Vkbd] Injecting via Shift+Insert");
                // 粘贴前增加微量同步延迟
                thread::sleep(Duration::from_millis(10));
                self.emit(Key::KEY_LEFTSHIFT, true);
                thread::sleep(Duration::from_millis(15));
                self.tap(Key::KEY_INSERT);
                thread::sleep(Duration::from_millis(15));
                self.emit(Key::KEY_LEFTSHIFT, false);
            },
            PasteMode::UnicodeHex | PasteMode::Fcitx5 => {} // No-op
        }
        
        true
    }
    
    pub fn backspace(&mut self, count: usize) {
        if count == 0 { return; }
        
        // HACK: 发送一个空格再补一个退格
        // 这在 Firefox 搜索框中能强行打断自动补全建议，确保后续退格能删掉真实的字符
        self.tap(Key::KEY_SPACE);
        self.tap(Key::KEY_BACKSPACE);

        for _ in 0..count {
            self.tap(Key::KEY_BACKSPACE);
            // 物理间隔减小到 1ms
            thread::sleep(Duration::from_millis(1));
        }
        // 关键同步延迟减小
        thread::sleep(Duration::from_millis(5));
    }

    fn send_via_fcitx(&self, text: &str) -> bool {
        if let Some(ref conn) = self.dbus_conn {
            let res = conn.call_method(
                Some("org.fcitx.Fcitx5"),
                "/controller",
                Some("org.fcitx.Fcitx.Controller1"),
                "CommitString",
                &(text),
            );

            match res {
                Ok(_) => true,
                Err(e) => {
                    if e.to_string().contains("UnknownMethod") {
                        eprintln!("[Vkbd] Fcitx5 CommitString 接口未找到。请在 Fcitx5 配置中开启 'Remote Control' (远程控制) 插件。");
                    } else {
                        eprintln!("[Vkbd] Native Fcitx5 D-Bus call failed: {}", e);
                    }
                    false
                }
            }
        } else {
            false
        }
    }

    fn send_via_ydotool(&self, text: &str) -> bool {
        let status = Command::new("ydotool")
            .arg("type")
            .arg(text)
            .status();
        match status {
            Ok(s) => s.success(),
            Err(_) => false,
        }
    }

    fn send_char_via_unicode(&mut self, ch: char) -> bool {
        self.emit(Key::KEY_LEFTCTRL, true);
        self.emit(Key::KEY_LEFTSHIFT, true);
        self.tap(Key::KEY_U);
        self.emit(Key::KEY_LEFTCTRL, false);
        self.emit(Key::KEY_LEFTSHIFT, false);

        // 减小初始化延迟
        thread::sleep(Duration::from_millis(15));

        let hex_str = format!("{:x}", ch as u32);
        for hex_char in hex_str.chars() {
             if let Some(key) = hex_char_to_key(hex_char) {
                 self.tap(key);
                 // 物理延迟降至最低
                 thread::sleep(Duration::from_micros(500));
             } else {
                 return false;
             }
        }

        self.tap(Key::KEY_ENTER);
        // 减小结束延迟
        thread::sleep(Duration::from_millis(10));
        true
    }

    pub fn tap(&mut self, key: Key) {
        self.emit(key, true);
        // 保持 1ms 按下，这是 Firefox 捕获信号的生命线
        thread::sleep(Duration::from_millis(1));
        self.emit(key, false);
        thread::sleep(Duration::from_micros(500));
    }

    #[allow(dead_code)]
    pub fn send_key(&mut self, key: Key, value: i32) {
        self.emit_raw(key, value);
    }

    pub fn emit_raw(&mut self, key: Key, value: i32) {
        // MSC_SCAN 是物理键盘通常会发送的事件，许多底层应用依赖它来确认按键真实性
        let msc = InputEvent::new(EventType::MISC, 4, key.code() as i32); // 4 = MSC_SCAN
        let ev = InputEvent::new(EventType::KEY, key.code(), value);
        let syn = InputEvent::new(EventType::SYNCHRONIZATION, 0, 0); // SYN_REPORT
        let _ = self.dev.emit(&[msc, ev, syn]);
        thread::sleep(Duration::from_micros(300));
    }

    pub fn emit(&mut self, key: Key, down: bool) {
        let val = if down { 1 } else { 0 };
        self.emit_raw(key, val);
    }

    #[allow(dead_code)]
    pub fn release_all(&mut self) {
        let modifiers = [
            Key::KEY_LEFTSHIFT, Key::KEY_RIGHTSHIFT,
            Key::KEY_LEFTCTRL, Key::KEY_RIGHTCTRL,
            Key::KEY_LEFTALT, Key::KEY_RIGHTALT,
            Key::KEY_LEFTMETA, Key::KEY_RIGHTMETA,
        ];
        for k in modifiers {
            self.emit(k, false);
        }
    }

    #[allow(dead_code)]
    pub fn copy_selection(&mut self) {
        self.emit(Key::KEY_LEFTCTRL, true);
        self.tap(Key::KEY_C);
        self.emit(Key::KEY_LEFTCTRL, false);
        thread::sleep(Duration::from_millis(150)); 
    }

    #[allow(dead_code)]
    pub fn get_clipboard_text(&self) -> Option<String> {
        use arboard::Clipboard;
        let mut cb = Clipboard::new().ok()?;
        cb.get_text().ok()
    }

    pub fn apply_config(&mut self, config: &crate::config::Config) {
        self.clipboard_delay_ms = config.input.clipboard_delay_ms;
        self.paste_mode = match config.input.paste_method.as_str() {
            "ctrl_v" => PasteMode::CtrlV,
            "ctrl_shift_v" => PasteMode::CtrlShiftV,
            "unicode" => PasteMode::UnicodeHex,
            "fcitx5" => PasteMode::Fcitx5,
            _ => PasteMode::ShiftInsert,
        };
    }
}



fn char_to_key(c: char) -> Option<Key> {
    match c.to_ascii_lowercase() {
        'a' => Some(Key::KEY_A), 'b' => Some(Key::KEY_B), 'c' => Some(Key::KEY_C),
        'd' => Some(Key::KEY_D), 'e' => Some(Key::KEY_E), 'f' => Some(Key::KEY_F),
        'g' => Some(Key::KEY_G), 'h' => Some(Key::KEY_H), 'i' => Some(Key::KEY_I),
        'j' => Some(Key::KEY_J), 'k' => Some(Key::KEY_K), 'l' => Some(Key::KEY_L),
        'm' => Some(Key::KEY_M), 'n' => Some(Key::KEY_N), 'o' => Some(Key::KEY_O),
        'p' => Some(Key::KEY_P), 'q' => Some(Key::KEY_Q), 'r' => Some(Key::KEY_R),
        's' => Some(Key::KEY_S), 't' => Some(Key::KEY_T), 'u' => Some(Key::KEY_U),
        'v' => Some(Key::KEY_V), 'w' => Some(Key::KEY_W), 'x' => Some(Key::KEY_X),
        'y' => Some(Key::KEY_Y), 'z' => Some(Key::KEY_Z),
        '0' => Some(Key::KEY_0), '1' => Some(Key::KEY_1), '2' => Some(Key::KEY_2),
        '3' => Some(Key::KEY_3), '4' => Some(Key::KEY_4), '5' => Some(Key::KEY_5),
        '6' => Some(Key::KEY_6), '7' => Some(Key::KEY_7), '8' => Some(Key::KEY_8),
        '9' => Some(Key::KEY_9),
        '\'' => Some(Key::KEY_APOSTROPHE),
        ' ' => Some(Key::KEY_SPACE),
        ',' => Some(Key::KEY_COMMA),
        '.' => Some(Key::KEY_DOT),
        '/' => Some(Key::KEY_SLASH),
        ';' => Some(Key::KEY_SEMICOLON),
        '[' => Some(Key::KEY_LEFTBRACE),
        ']' => Some(Key::KEY_RIGHTBRACE),
        '\\' => Some(Key::KEY_BACKSLASH),
        '-' => Some(Key::KEY_MINUS),
        '=' => Some(Key::KEY_EQUAL),
        '`' => Some(Key::KEY_GRAVE),
        _ => None,
    }
}

fn hex_char_to_key(c: char) -> Option<Key> {
    match c.to_ascii_lowercase() {
        '0' => Some(Key::KEY_0), '1' => Some(Key::KEY_1), '2' => Some(Key::KEY_2),
        '3' => Some(Key::KEY_3), '4' => Some(Key::KEY_4), '5' => Some(Key::KEY_5),
        '6' => Some(Key::KEY_6), '7' => Some(Key::KEY_7), '8' => Some(Key::KEY_8),
        '9' => Some(Key::KEY_9),
        'a' => Some(Key::KEY_A), 'b' => Some(Key::KEY_B), 'c' => Some(Key::KEY_C),
        'd' => Some(Key::KEY_D), 'e' => Some(Key::KEY_E), 'f' => Some(Key::KEY_F),
        _ => None,
    }
}