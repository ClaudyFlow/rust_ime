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
    Restart,
    Exit,
    ToggleGui,
    ToggleModernGui,
    ToggleNotify,
    ToggleKeystroke,
    ToggleLearning,
    ToggleAntiTypo,
    SwitchCommitMode,
    ReloadConfig,
    CyclePreview,
}

#[cfg(target_os = "linux")]
pub struct ImeTray {
    pub chinese_enabled: bool,
    pub active_profile: String,
    pub show_candidates: bool,
    pub show_modern_candidates: bool,
    pub show_notifications: bool,
    pub show_keystrokes: bool,
    pub learning_mode: bool,
    pub anti_typo: bool,
    pub commit_mode: String,
    pub preview_mode: String,
    pub tx: Sender<TrayEvent>,
}

#[cfg(target_os = "linux")]
impl Tray for ImeTray {
    fn icon_name(&self) -> String {
        // 动态变更名称，强制部分桌面环境（如 GNOME）刷新像素缓存
        if self.chinese_enabled { "rust-ime-zh".into() } else { "rust-ime-en".into() }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let size = 22;
        let mut pixmap = Pixmap::new(size, size).unwrap();
        
        let mut paint = Paint::default();
        if self.chinese_enabled {
            paint.set_color_rgba8(255, 128, 0, 255); // 标准橙色
        } else {
            paint.set_color_rgba8(60, 60, 60, 255); // 深灰色
        }
        paint.anti_alias = true;

        // 1. 绘制圆角背景
        let bg_path = {
            let mut pb = PathBuilder::new();
            let r = 4.0;
            let rect = Rect::from_xywh(2.0, 2.0, 18.0, 18.0).unwrap();
            pb.move_to(rect.left() + r, rect.top());
            pb.line_to(rect.right() - r, rect.top());
            pb.quad_to(rect.right(), rect.top(), rect.right(), rect.top() + r);
            pb.line_to(rect.right(), rect.bottom() - r);
            pb.quad_to(rect.right(), rect.bottom(), rect.right() - r, rect.bottom());
            pb.line_to(rect.left() + r, rect.bottom());
            pb.quad_to(rect.left(), rect.bottom(), rect.left(), rect.bottom() - r);
            pb.line_to(rect.left(), rect.top() + r);
            pb.quad_to(rect.left(), rect.top(), rect.left() + r, rect.top());
            pb.finish().unwrap()
        };
        pixmap.fill_path(&bg_path, &paint, FillRule::Winding, Transform::identity(), None);

        // 2. 绘制内容
        let mut icon_paint = Paint::default();
        icon_paint.set_color_rgba8(255, 255, 255, 255);
        icon_paint.anti_alias = true;

        if self.chinese_enabled {
            // 用 5 个矩形拼出一个结实的“中”字
            let p = &icon_paint;
            // 矩形的上下左右边
            pixmap.fill_rect(Rect::from_xywh(6.0, 8.5, 10.0, 1.5).unwrap(), p, Transform::identity(), None);   // 上
            pixmap.fill_rect(Rect::from_xywh(6.0, 13.0, 10.0, 1.5).unwrap(), p, Transform::identity(), None);  // 下
            pixmap.fill_rect(Rect::from_xywh(6.0, 8.5, 1.5, 6.0).unwrap(), p, Transform::identity(), None);    // 左
            pixmap.fill_rect(Rect::from_xywh(14.5, 8.5, 1.5, 6.0).unwrap(), p, Transform::identity(), None);  // 右
            // 中间那一竖
            pixmap.fill_rect(Rect::from_xywh(10.25, 5.0, 1.5, 12.0).unwrap(), p, Transform::identity(), None);
        } else {
            // 键盘网格 (3x2)
            for y in 0..2 {
                for x in 0..3 {
                    let k_rect = Rect::from_xywh(6.0 + x as f32 * 4.0, 9.0 + y as f32 * 4.0, 2.5, 2.5).unwrap();
                    pixmap.fill_rect(k_rect, &icon_paint, Transform::identity(), None);
                }
            }
        }

        let rgba = pixmap.data().to_vec();
        let mut argb_data = Vec::with_capacity(rgba.len());
        for chunk in rgba.chunks_exact(4) {
            argb_data.push(chunk[3]); // A
            argb_data.push(chunk[0]); // R
            argb_data.push(chunk[1]); // G
            argb_data.push(chunk[2]); // B
        }

        vec![ksni::Icon {
            width: size as i32,
            height: size as i32,
            data: argb_data,
        }]
    }

    fn title(&self) -> String {
        format!("rust-IME ({})", if self.chinese_enabled { "中" } else { "直" })
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "rust-IME".to_string(),
            description: format!("Profile: {}\nGUI: {}\nPreview: {}\nLearning: {}", 
                self.active_profile,
                if self.show_candidates { "开" } else { "关" },
                self.preview_mode,
                if self.learning_mode { "开" } else { "关" }
            ),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: format!("模式: {}", if self.chinese_enabled { "中文" } else { "直通 (无输入法)" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ToggleIme); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("词库: {}", self.active_profile),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::NextProfile); }),
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: format!("传统候选窗: {}", if self.show_candidates { "显示" } else { "隐藏" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ToggleGui); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("卡片式候选窗: {}", if self.show_modern_candidates { "显示" } else { "隐藏" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ToggleModernGui); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("拼音预览: {}", if self.preview_mode == "pinyin" { "开启" } else { "关闭" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::CyclePreview); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("系统通知候选词: {}", if self.show_notifications { "开启" } else { "关闭" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ToggleNotify); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("按键显示: {}", if self.show_keystrokes { "开启" } else { "关闭" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ToggleKeystroke); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("学习模式: {}", if self.learning_mode { "开启" } else { "关闭" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ToggleLearning); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("防呆模式: {}", if self.anti_typo { "开启" } else { "关闭" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ToggleAntiTypo); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: format!("上屏模式: {}", if self.commit_mode == "single" { "词模式(单空格)" } else { "长句模式(双空格)" }),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::SwitchCommitMode); }),
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: "配置中心 (Web)".to_string(),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::OpenConfig); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: "重新加载配置".to_string(),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::ReloadConfig); }),
                ..Default::default()
            }.into(),
            StandardItem {
                label: "重启服务".to_string(),
                activate: Box::new(|this: &mut Self| { let _ = this.tx.send(TrayEvent::Restart); }),
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

#[cfg(target_os = "linux")]
pub fn start_tray(
    chinese_enabled: bool, active_profile: String, show_candidates: bool,
    show_modern_candidates: bool,
    show_notifications: bool, show_keystrokes: bool, learning_mode: bool,
    anti_typo: bool,
    commit_mode: String,
    preview_mode: String,
    event_tx: Sender<TrayEvent>
) -> Handle<ImeTray> {
    let service = ImeTray { chinese_enabled, active_profile, show_candidates, show_modern_candidates, show_notifications, show_keystrokes, learning_mode, anti_typo, commit_mode, preview_mode, tx: event_tx };
    let tray_service = TrayService::new(service);
    let handle = tray_service.handle();
    std::thread::spawn(move || { let _ = tray_service.run(); });
    handle
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
    pub show_candidates: bool,
    pub show_modern_candidates: bool,
    pub show_notifications: bool,
    pub show_keystrokes: bool,
    pub learning_mode: bool,
    pub anti_typo: bool,
    pub commit_mode: String,
    pub preview_mode: String,
}

#[cfg(target_os = "windows")]
static mut TRAY_STATE: Option<Arc<Mutex<ImeTrayStub>>> = None;
#[cfg(target_os = "windows")]
static mut TRAY_TX: Option<Sender<TrayEvent>> = None;

#[cfg(target_os = "windows")]
pub struct WindowsTrayHandle(Arc<Mutex<ImeTrayStub>>, HWND);

#[cfg(target_os = "windows")]
impl WindowsTrayHandle {
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut ImeTrayStub),
    {
        if let Ok(mut state) = self.0.lock() {
            f(&mut *state);
            // 这里可以触发 Shell_NotifyIconW 更新图标或提示，但目前简化处理
        }
    }
}

#[cfg(target_os = "windows")]
pub fn start_tray(
    chinese_enabled: bool, active_profile: String, show_candidates: bool,
    show_modern_candidates: bool,
    show_notifications: bool, show_keystrokes: bool, learning_mode: bool,
    anti_typo: bool,
    commit_mode: String,
    preview_mode: String,
    event_tx: Sender<TrayEvent>
) -> WindowsTrayHandle {
    let state = Arc::new(Mutex::new(ImeTrayStub {
        chinese_enabled, active_profile, show_candidates, show_modern_candidates,
        show_notifications, show_keystrokes, learning_mode, anti_typo,
        commit_mode, preview_mode,
    }));
    
    unsafe {
        TRAY_STATE = Some(state.clone());
        TRAY_TX = Some(event_tx);
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let state_clone = state.clone();
    std::thread::spawn(move || {
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
            let window_class = PCWSTR("RustImeTrayClass\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
            
            let wc = WNDCLASSW {
                hInstance: instance.into(),
                lpszClassName: window_class,
                lpfnWndProc: Some(tray_wnd_proc),
                hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
                ..Default::default()
            };
            RegisterClassW(&wc);

            // 使用 WS_POPUP 创建一个完全不可见的后台窗口
            let hwnd = CreateWindowExW(
                Default::default(), window_class, PCWSTR(std::ptr::null()), 
                WS_POPUP, 0, 0, 0, 0, None, None, instance, None
            );
            
            if hwnd.0 == 0 {
                eprintln!("[Tray] 无法创建托盘隐藏窗口: {:?}", GetLastError());
                return;
            }

            // Load custom icon
            let h_icon = if let Ok(img) = image::open("picture/rust-ime.png") {
                let img = img.resize(32, 32, image::imageops::FilterType::Lanczos3);
                let rgba = img.to_rgba8();
                let mut bgra = Vec::with_capacity(rgba.len());
                for pixel in rgba.pixels() {
                    bgra.push(pixel[2]); // B
                    bgra.push(pixel[1]); // G
                    bgra.push(pixel[0]); // R
                    bgra.push(pixel[3]); // A
                }
                let and_mask = vec![0u8; 32 * 32 / 8];
                CreateIcon(None, 32, 32, 1, 32, and_mask.as_ptr(), bgra.as_ptr()).unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default())
            } else {
                LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
            };

            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: TRAY_ICON_ID,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAYICON,
                hIcon: h_icon,
                ..Default::default()
            };
            
            let tip_str = "Rust IME (运行中)";
            let tip_w: Vec<u16> = tip_str.encode_utf16().collect();
            let len = tip_w.len().min(nid.szTip.len() - 1);
            nid.szTip[..len].copy_from_slice(&tip_w[..len]);
            
            if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                eprintln!("[Tray] 注册托盘图标失败");
            }

            let _ = tx.send(hwnd);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            
            Shell_NotifyIconW(NIM_DELETE, &nid);
            DestroyIcon(h_icon);
        }
    });

    // 等待窗口句柄返回，如果超时或失败则返回空句柄
    let hwnd = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or(HWND(0));
    WindowsTrayHandle(state_clone, hwnd)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn tray_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAYICON => {
            if lparam.0 as u32 == WM_RBUTTONUP {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                show_context_menu(hwnd, pt.x, pt.y);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 as u32;
            if let Some(ref tx) = TRAY_TX {
                match id {
                    1001 => { let _ = tx.send(TrayEvent::ToggleIme); }
                    1002 => { let _ = tx.send(TrayEvent::NextProfile); }
                    1003 => { let _ = tx.send(TrayEvent::ToggleGui); }
                    1004 => { let _ = tx.send(TrayEvent::ToggleModernGui); }
                    1005 => { let _ = tx.send(TrayEvent::CyclePreview); }
                    1006 => { let _ = tx.send(TrayEvent::ToggleNotify); }
                    1007 => { let _ = tx.send(TrayEvent::ToggleKeystroke); }
                    1008 => { let _ = tx.send(TrayEvent::ToggleLearning); }
                    1009 => { let _ = tx.send(TrayEvent::ToggleAntiTypo); }
                    1010 => { let _ = tx.send(TrayEvent::SwitchCommitMode); }
                    1011 => { let _ = tx.send(TrayEvent::OpenConfig); }
                    1012 => { let _ = tx.send(TrayEvent::ReloadConfig); }
                    1013 => { let _ = tx.send(TrayEvent::Restart); }
                    1014 => { let _ = tx.send(TrayEvent::Exit); }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(target_os = "windows")]
unsafe fn show_context_menu(hwnd: HWND, x: i32, y: i32) {
    let h_menu = CreatePopupMenu().unwrap();
    if let Some(ref state_arc) = TRAY_STATE {
        if let Ok(state) = state_arc.lock() {
            let mode_label = format!("模式: {}", if state.chinese_enabled { "中文" } else { "直通" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1001, PCWSTR(HSTRING::from(&mode_label).as_ptr()));
            let profile_label = format!("词库: {}", state.active_profile);
            let _ = AppendMenuW(h_menu, MF_STRING, 1002, PCWSTR(HSTRING::from(&profile_label).as_ptr()));
            let _ = AppendMenuW(h_menu, MF_SEPARATOR, 0, PCWSTR(std::ptr::null()));
            
            let gui_label = format!("传统候选窗: {}", if state.show_candidates { "显示" } else { "隐藏" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1003, PCWSTR(HSTRING::from(&gui_label).as_ptr()));
            let modern_label = format!("卡片式候选窗: {}", if state.show_modern_candidates { "显示" } else { "隐藏" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1004, PCWSTR(HSTRING::from(&modern_label).as_ptr()));
            let preview_label = format!("拼音预览: {}", if state.preview_mode == "pinyin" { "开启" } else { "关闭" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1005, PCWSTR(HSTRING::from(&preview_label).as_ptr()));
            let notify_label = format!("系统通知候选词: {}", if state.show_notifications { "开启" } else { "关闭" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1006, PCWSTR(HSTRING::from(&notify_label).as_ptr()));
            let key_label = format!("按键显示: {}", if state.show_keystrokes { "开启" } else { "关闭" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1007, PCWSTR(HSTRING::from(&key_label).as_ptr()));
            let learn_label = format!("学习模式: {}", if state.learning_mode { "开启" } else { "关闭" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1008, PCWSTR(HSTRING::from(&learn_label).as_ptr()));
            let anti_label = format!("防呆模式: {}", if state.anti_typo { "开启" } else { "关闭" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1009, PCWSTR(HSTRING::from(&anti_label).as_ptr()));
            let commit_label = format!("上屏模式: {}", if state.commit_mode == "single" { "词模式" } else { "长句模式" });
            let _ = AppendMenuW(h_menu, MF_STRING, 1010, PCWSTR(HSTRING::from(&commit_label).as_ptr()));
            
            let _ = AppendMenuW(h_menu, MF_SEPARATOR, 0, PCWSTR(std::ptr::null()));
            let _ = AppendMenuW(h_menu, MF_STRING, 1011, PCWSTR(HSTRING::from("配置中心 (Web)").as_ptr()));
            let _ = AppendMenuW(h_menu, MF_STRING, 1012, PCWSTR(HSTRING::from("重新加载配置").as_ptr()));
            let _ = AppendMenuW(h_menu, MF_STRING, 1013, PCWSTR(HSTRING::from("重启服务").as_ptr()));
            let _ = AppendMenuW(h_menu, MF_SEPARATOR, 0, PCWSTR(std::ptr::null()));
            let _ = AppendMenuW(h_menu, MF_STRING, 1014, PCWSTR(HSTRING::from("退出程序").as_ptr()));
        }
    }
    
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(h_menu, TPM_RIGHTBUTTON, x, y, 0, hwnd, None);
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(h_menu);
}
