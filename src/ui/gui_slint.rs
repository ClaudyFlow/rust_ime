use std::sync::mpsc::Receiver;
use slint::{ComponentHandle, SharedString, ModelRc, VecModel};
use crate::ui::GuiEvent;
use crate::Config;

slint::include_modules!();

#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::*;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};

pub fn start_gui(rx: Receiver<GuiEvent>, config: Config) {
    let window = CandidateWindow::new().expect("Failed to create CandidateWindow");
    let status_bar = StatusBar::new().expect("Failed to create StatusBar");
    
    let window_handle = window.as_weak();
    let status_bar_handle = status_bar.as_weak();

    // 初始设置
    window.set_is_horizontal(config.appearance.candidate_layout == "horizontal");
    window.set_show_english_aux(config.appearance.show_english_aux);
    window.set_show_stroke_aux(config.appearance.show_stroke_aux);
    
    let show_candidates = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(config.appearance.show_candidates));

    // 1. 初始化窗口特殊的系统属性 (Windows)
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            unsafe {
                // 处理候选窗
                let title = "RustImeCandidateWindow\0".encode_utf16().collect::<Vec<u16>>();
                let hwnd = FindWindowW(None, PCWSTR(title.as_ptr()));
                if hwnd.0 != 0 {
                    let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                    // WS_EX_TOOLWINDOW: 隐藏任务栏图标
                    // WS_EX_NOACTIVATE: 窗口不获取焦点
                    // WS_EX_TOPMOST: 置顶
                    // WS_EX_TRANSPARENT: 鼠标穿透
                    ex_style |= WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0 | WS_EX_TRANSPARENT.0;
                    ex_style &= !WS_EX_APPWINDOW.0;
                    let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
                }

                // 处理状态栏
                let s_title = "RustImeStatusBar\0".encode_utf16().collect::<Vec<u16>>();
                let s_hwnd = FindWindowW(None, PCWSTR(s_title.as_ptr()));
                if s_hwnd.0 != 0 {
                    let mut ex_style = GetWindowLongPtrW(s_hwnd, GWL_EXSTYLE) as u32;
                    ex_style |= WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0;
                    ex_style &= !WS_EX_APPWINDOW.0;
                    let _ = SetWindowLongPtrW(s_hwnd, GWL_EXSTYLE, ex_style as isize);
                    
                    // 固定在右下角，并强制尺寸为 32x32，防止占满屏幕
                    let screen_width = GetSystemMetrics(SM_CXSCREEN);
                    let screen_height = GetSystemMetrics(SM_CYSCREEN);
                    let _ = SetWindowPos(s_hwnd, HWND_TOPMOST, screen_width - 80, screen_height - 80, 32, 32, SWP_NOACTIVATE);
                }
            }
        });
    }

    // 2. 启动事件轮询线程
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            let h = window_handle.clone();
            let s = status_bar_handle.clone();
            let show_candidates_for_loop = show_candidates.clone();
            
            let _ = slint::invoke_from_event_loop(move || {
                match event {
                    GuiEvent::Update { pinyin, candidates, hints, selected, .. } => {
                        if let Some(w) = h.upgrade() {
                            if pinyin.is_empty() && candidates.is_empty() || !show_candidates_for_loop.load(std::sync::atomic::Ordering::SeqCst) {
                                // 真正隐藏窗口，释放所有鼠标区域
                                let _ = w.window().hide();
                            } else {
                                w.set_pinyin(SharedString::from(pinyin));
                                
                                // 获取配置中的分页大小，或者默认为 5
                                let page_size = 5; 
                                let page = (selected / page_size) * page_size;
                                let relative_selected = (selected % page_size) as i32;
                                
                                let mut data_vec = Vec::new();
                                // 只取当前页的候选词发送给 UI
                                for i in page..(page + page_size).min(candidates.len()) {
                                    let cand = &candidates[i];
                                    let hint = hints.get(i).cloned().unwrap_or_default();
                                    let mut english = String::new();
                                    let mut stroke = String::new();
                                    if !hint.is_empty() {
                                        if hint.contains('/') {
                                            let parts: Vec<&str> = hint.split('/').collect();
                                            english = parts[0].to_string();
                                            stroke = parts[1].to_string();
                                        } else if hint.chars().all(|c| c.is_ascii_alphabetic()) {
                                            english = hint.clone();
                                        } else {
                                            stroke = hint.clone();
                                        }
                                    }
                                    data_vec.push(CandidateData {
                                        text: SharedString::from(cand),
                                        english_aux: SharedString::from(english),
                                        stroke_aux: SharedString::from(stroke),
                                    });
                                }
                                
                                w.set_candidates(ModelRc::new(VecModel::from(data_vec)));
                                w.set_selected_index(relative_selected);
                                
                                // 显示窗口
                                let _ = w.window().show();
                            }
                        }
                    }
                    GuiEvent::ShowStatus(status) => {
                        if let Some(sb) = s.upgrade() {
                            sb.set_status_text(SharedString::from(status.clone()));
                            sb.set_chinese_enabled(status == "中");
                        }
                    }
                    GuiEvent::MoveTo { x, y } => {
                        if let Some(w) = h.upgrade() {
                            let mut final_x = x;
                            let mut final_y = y;
                            #[cfg(target_os = "windows")]
                            unsafe {
                                let win_size = w.window().size();
                                let width = win_size.width as i32;
                                let height = win_size.height as i32;
                                let monitor = MonitorFromPoint(windows::Win32::Foundation::POINT { x, y }, MONITOR_DEFAULTTONEAREST);
                                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                                if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                                    if final_x + width > mi.rcMonitor.right { final_x = mi.rcMonitor.right - width - 10; }
                                    if final_y + height > mi.rcMonitor.bottom { final_y = mi.rcMonitor.bottom - height - 10; }
                                    if final_x < mi.rcMonitor.left { final_x = mi.rcMonitor.left + 5; }
                                    if final_y < mi.rcMonitor.top { final_y = mi.rcMonitor.top + 5; }
                                }
                            }
                            let _ = w.window().set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(final_x, final_y)));
                        }
                    }
                    GuiEvent::ApplyConfig(new_conf) => {
                        show_candidates_for_loop.store(new_conf.appearance.show_candidates, std::sync::atomic::Ordering::SeqCst);
                        if let Some(w) = h.upgrade() {
                            w.set_is_horizontal(new_conf.appearance.candidate_layout == "horizontal");
                            w.set_show_english_aux(new_conf.appearance.show_english_aux);
                            w.set_show_stroke_aux(new_conf.appearance.show_stroke_aux);
                        }
                    }
                    GuiEvent::Exit => {
                        let _ = slint::quit_event_loop();
                    }
                    _ => {}
                }
            });
        }
    });

    status_bar.show().expect("Failed to show StatusBar");
    // window 初始不调用 show，由 Update 事件驱动
    slint::run_event_loop().expect("Failed to run Slint event loop");
}
