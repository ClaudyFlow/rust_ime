#[cfg(target_os = "linux")]
use ksni::menu::{StandardItem, MenuItem};
#[cfg(target_os = "linux")]
use ksni::{Tray, ToolTip, TrayService, Handle};
use std::sync::mpsc::Sender;
#[cfg(target_os = "linux")]
use tiny_skia::*;

#[derive(Debug, Clone)]
pub enum TrayEvent {
    ToggleIme,
    NextProfile,
    OpenConfig,
    Exit,
    ReloadConfig,
    SyncStatus { chinese_enabled: bool, active_profile: String },
    ClearUserDict,
    RequestMenu { x: i32, y: i32 },
}

#[cfg(target_os = "linux")]
pub struct ImeTray {
    pub chinese_enabled: bool,
    pub active_profile: String,
    pub tx: Sender<TrayEvent>,
}

#[cfg(target_os = "linux")]
impl Tray for ImeTray {
    fn icon_name(&self) -> String {
        if self.chinese_enabled { "rust-ime-zh".into() } else { "rust-ime-en".into() }
    }

    fn title(&self) -> String {
        format!("rust-IME ({})", if self.chinese_enabled { "中" } else { "英" })
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let profile_zh = match self.active_profile.as_str() {
            "chinese" => "中文",
            "english" => "英文",
            "japanese" => "日文",
            "mixed" => "中日英混",
            other => other,
        };

        vec![
            StandardItem {
                label: format!("输入法: {}", if self.chinese_enabled { "中" } else { "英" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ToggleIme); }),    
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("词典方案: {}", profile_zh),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::NextProfile); }),  
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: "配置管理 (Web)".to_string(),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::OpenConfig); }),   
                ..Default::default()
            }.into(),
            StandardItem {
                label: "重载词库配置".to_string(),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ReloadConfig); }), 
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: "退出程序".to_string(),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::Exit); }),
                ..Default::default()
            }.into(),
        ]
    }
}

#[cfg(target_os = "windows")]
use windows::{
    Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
    Win32::Foundation::*,
    core::*,
};
#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use image;

#[cfg(target_os = "windows")]
const WM_TRAYICON: u32 = WM_USER + 100;
#[cfg(target_os = "windows")]
const TRAY_ICON_ID: u32 = 1;

#[cfg(target_os = "windows")]
pub struct ImeTrayStub {
    pub chinese_enabled: bool,
    pub active_profile: String,
}

#[cfg(target_os = "windows")]
static mut TRAY_STATE: Option<Arc<Mutex<ImeTrayStub>>> = None;
#[cfg(target_os = "windows")]
static mut TRAY_TX: Option<Sender<TrayEvent>> = None;

#[cfg(target_os = "windows")]
pub struct WindowsTrayHandle(Arc<Mutex<ImeTrayStub>>);

#[cfg(target_os = "windows")]
impl WindowsTrayHandle {
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut ImeTrayStub),
    {
        if let Ok(mut state) = self.0.lock() {
            f(&mut *state);
        }
    }
}

#[cfg(target_os = "windows")]
pub fn start_tray(
    chinese_enabled: bool, active_profile: String, _show_candidates: bool,
    _anti_typo_mode: crate::config::AntiTypoMode,
    _double_pinyin: bool,
    _commit_mode: String,
    _preview_mode: String,
    _candidate_layout: String,
    event_tx: Sender<TrayEvent>
) -> WindowsTrayHandle {
    let state = Arc::new(Mutex::new(ImeTrayStub {
        chinese_enabled, active_profile,
    }));

    unsafe {
        TRAY_STATE = Some(state.clone());
        TRAY_TX = Some(event_tx);
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let _state_clone = state.clone();
    std::thread::spawn(move || {
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
            let window_class = PCWSTR("RustImeTrayClass\0".encode_utf16().collect::<Vec<u16>>().as_ptr());

            let wc = WNDCLASSW {
                hInstance: instance.into(),
                lpszClassName: window_class,
                lpfnWndProc: Some(tray_wnd_proc),
                ..Default::default()
            };
            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW, window_class, PCWSTR(std::ptr::null()),
                WS_POPUP, 0, 0, 0, 0, None, None, instance, None
            );

            // Load custom icon
            let h_icon = if let Ok(img) = image::open("picture/rust-ime.png") {
                let img = img.resize(32, 32, image::imageops::FilterType::Lanczos3);
                let rgba = img.to_rgba8();
                let mut bgra = Vec::with_capacity(rgba.len());
                for pixel in rgba.pixels() {
                    bgra.push(pixel[2]); bgra.push(pixel[1]); bgra.push(pixel[0]); bgra.push(pixel[3]);
                }
                let and_mask = vec![0u8; 32 * 32 / 8];
                CreateIcon(None, 32, 32, 1, 32, and_mask.as_ptr(), bgra.as_ptr()).unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default())
            } else {
                LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
            };

            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: TRAY_ICON_ID,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAYICON,
                hIcon: h_icon,
                ..Default::default()
            };

            let _tip_str = "Rust IME";
            
            if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                println!("[Tray] 系统托盘初始化成功。");
            } else {
                eprintln!("[Tray] 系统托盘初始化失败。");
            }
            let _ = tx.send(hwnd);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    });

    let _hwnd = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or(HWND(0));
    WindowsTrayHandle(state)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn tray_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAYICON => {
            if lparam.0 as u32 == WM_RBUTTONUP {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                
                let h_menu = CreatePopupMenu().unwrap();
                let _ = AppendMenuW(h_menu, MF_STRING, 1001, windows::core::w!("切换中/英"));
                let _ = AppendMenuW(h_menu, MF_STRING, 1002, windows::core::w!("切换方案"));
                let _ = AppendMenuW(h_menu, MF_SEPARATOR, 0, None);
                let _ = AppendMenuW(h_menu, MF_STRING, 1011, windows::core::w!("配置管理 (Web)"));
                let _ = AppendMenuW(h_menu, MF_STRING, 1012, windows::core::w!("重载词库配置"));
                let _ = AppendMenuW(h_menu, MF_SEPARATOR, 0, None);
                let _ = AppendMenuW(h_menu, MF_STRING, 1014, windows::core::w!("退出程序"));
                
                let _ = SetForegroundWindow(hwnd);
                let _ = TrackPopupMenu(h_menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
                let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
                let _ = DestroyMenu(h_menu);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 as u32;
            if let Some(ref tx) = TRAY_TX {
                match id {
                    1001 => { let _ = tx.send(TrayEvent::ToggleIme); }
                    1002 => { let _ = tx.send(TrayEvent::NextProfile); }
                    1011 => { let _ = tx.send(TrayEvent::OpenConfig); }
                    1012 => { let _ = tx.send(TrayEvent::ReloadConfig); }
                    1014 => { let _ = tx.send(TrayEvent::Exit); }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

