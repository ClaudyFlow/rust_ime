use std::sync::mpsc::{Receiver, Sender};
use slint::{ComponentHandle, SharedString, ModelRc, VecModel};
use crate::ui::GuiEvent;
use crate::Config;
use crate::ui::tray::TrayEvent;

slint::include_modules!();

#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::*;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};

#[cfg(target_os = "windows")]
unsafe fn hide_window_from_taskbar(title_str: &str) {
    let mut title_w: Vec<u16> = title_str.encode_utf16().collect();
    title_w.push(0);
    let hwnd = FindWindowW(None, PCWSTR(title_w.as_ptr()));
    if hwnd.0 != 0 {
        let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if (ex_style & WS_EX_TOOLWINDOW.0) == 0 {
            ex_style |= WS_EX_TOOLWINDOW.0;
            ex_style &= !WS_EX_APPWINDOW.0;
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
            // 刷新窗口样式以生效
            let _ = SetWindowPos(hwnd, HWND(0), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE);
        }
    }
}

pub fn start_gui(rx: Receiver<GuiEvent>, config: Config, tray_tx: Sender<TrayEvent>) {
    let window = CandidateWindow::new().expect("Failed to create CandidateWindow");
    let status_bar = StatusBar::new().expect("Failed to create StatusBar");
    let tray_menu = TrayMenu::new().expect("Failed to create TrayMenu");
    
    let window_handle = window.as_weak();
    let status_bar_handle = status_bar.as_weak();
    let tray_menu_handle = tray_menu.as_weak();

    let last_active_hwnd = std::sync::Arc::new(std::sync::atomic::AtomicIsize::new(0));
    
    // 绑定托盘菜单回调
    {
        let lah = last_active_hwnd.clone();
        let tx = tray_tx.clone();
        let tm = tray_menu_handle.clone();
        tray_menu.on_toggle_ime(move || { 
            let _ = tx.send(TrayEvent::ToggleIme); 
            if let Some(m) = tm.upgrade() { 
                let _ = m.window().hide(); 
                #[cfg(target_os = "windows")]
                unsafe { let prev = lah.load(std::sync::atomic::Ordering::SeqCst); if prev != 0 { let _ = SetForegroundWindow(HWND(prev as isize)); } }
            } 
        });
        
        let lah = last_active_hwnd.clone();
        let tx = tray_tx.clone();
        let tm = tray_menu_handle.clone();
        tray_menu.on_next_profile(move || { 
            let _ = tx.send(TrayEvent::NextProfile); 
            if let Some(m) = tm.upgrade() { 
                let _ = m.window().hide(); 
                #[cfg(target_os = "windows")]
                unsafe { let prev = lah.load(std::sync::atomic::Ordering::SeqCst); if prev != 0 { let _ = SetForegroundWindow(HWND(prev as isize)); } }
            } 
        });
        
        let lah = last_active_hwnd.clone();
        let tx = tray_tx.clone();
        let tm = tray_menu_handle.clone();
        tray_menu.on_open_config(move || { 
            let _ = tx.send(TrayEvent::OpenConfig); 
            if let Some(m) = tm.upgrade() { 
                let _ = m.window().hide(); 
                #[cfg(target_os = "windows")]
                unsafe { let prev = lah.load(std::sync::atomic::Ordering::SeqCst); if prev != 0 { let _ = SetForegroundWindow(HWND(prev as isize)); } }
            } 
        });
        
        let lah = last_active_hwnd.clone();
        let tx = tray_tx.clone();
        let tm = tray_menu_handle.clone();
        tray_menu.on_reload_config(move || { 
            let _ = tx.send(TrayEvent::ReloadConfig); 
            if let Some(m) = tm.upgrade() { 
                let _ = m.window().hide(); 
                #[cfg(target_os = "windows")]
                unsafe { let prev = lah.load(std::sync::atomic::Ordering::SeqCst); if prev != 0 { let _ = SetForegroundWindow(HWND(prev as isize)); } }
            } 
        });
        
        let lah = last_active_hwnd.clone();
        let tx = tray_tx.clone();
        let tm = tray_menu_handle.clone();
        tray_menu.on_restart(move || { 
            let _ = tx.send(TrayEvent::Restart); 
            if let Some(m) = tm.upgrade() { 
                let _ = m.window().hide(); 
                #[cfg(target_os = "windows")]
                unsafe { let prev = lah.load(std::sync::atomic::Ordering::SeqCst); if prev != 0 { let _ = SetForegroundWindow(HWND(prev as isize)); } }
            } 
        });
        
        let lah = last_active_hwnd.clone();
        let tx = tray_tx.clone();
        let tm = tray_menu_handle.clone();
        tray_menu.on_exit(move || { 
            let _ = tx.send(TrayEvent::Exit); 
            if let Some(m) = tm.upgrade() { 
                let _ = m.window().hide(); 
                #[cfg(target_os = "windows")]
                unsafe { let prev = lah.load(std::sync::atomic::Ordering::SeqCst); if prev != 0 { let _ = SetForegroundWindow(HWND(prev as isize)); } }
            } 
        });
    }

    // 初始设置
    window.set_show_english_aux(config.appearance.show_english_aux);
    window.set_show_stroke_aux(config.appearance.show_stroke_aux);
    window.set_show_translation(config.appearance.show_english_translation);
    window.set_is_horizontal(config.appearance.candidate_layout == "horizontal");
    
    let parse_color = |s: &str| -> slint::Color {
        if s.starts_with('#') && s.len() == 7 {
            let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(255);
            let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(255);
            let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(255);
            slint::Color::from_rgb_u8(r, g, b)
        } else {
            slint::Color::from_rgb_u8(9, 105, 218)
        }
    };

    window.set_bg_color(parse_color(&config.appearance.window_bg_color));
    window.set_accent_color(parse_color(&config.appearance.window_highlight_color));
    window.set_border_color(parse_color(&config.appearance.window_border_color));
    window.set_text_color(parse_color(&config.appearance.candidate_text.color));
    window.set_highlight_text_color(parse_color(&config.appearance.window_bg_color));
    
    window.set_pinyin_font_size(config.appearance.pinyin_text.font_size as f32);
    window.set_pinyin_font_family(SharedString::from(config.appearance.pinyin_text.font_family.clone()));
    window.set_pinyin_font_weight(config.appearance.pinyin_text.font_weight as i32);
    window.set_candidate_font_size(config.appearance.candidate_text.font_size as f32);
    window.set_candidate_font_family(SharedString::from(config.appearance.candidate_text.font_family.clone()));
    window.set_candidate_font_weight(config.appearance.candidate_text.font_weight as i32);

    let show_candidates = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(config.appearance.show_candidates));
    let show_status_bar_atomic = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(config.appearance.show_status_bar));
    let random_highlight_atomic = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(config.appearance.enable_random_highlight));
    let page_size_atomic = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(config.appearance.page_size));
    
    // 共享颜色状态，解决随机色不更新问题
    let current_color_shared = std::sync::Arc::new(std::sync::Mutex::new(parse_color(&config.appearance.window_highlight_color)));
    
    let last_pos = std::sync::Arc::new(std::sync::Mutex::new((0i32, 0i32)));
    let last_pos_for_loop = last_pos.clone();

    #[cfg(target_os = "windows")]
    {
        let sb_init = status_bar_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            unsafe {
                let title = "RustImeCandidateWindow\0".encode_utf16().collect::<Vec<u16>>();
                let hwnd = FindWindowW(None, PCWSTR(title.as_ptr()));
                if hwnd.0 != 0 {
                    let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                    ex_style |= WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0;
                    ex_style &= !WS_EX_APPWINDOW.0;
                    let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
                }

                let s_title = "RustImeStatusBar\0".encode_utf16().collect::<Vec<u16>>();
                let s_hwnd = FindWindowW(None, PCWSTR(s_title.as_ptr()));
                if s_hwnd.0 != 0 {
                    let mut ex_style = GetWindowLongPtrW(s_hwnd, GWL_EXSTYLE) as u32;
                    ex_style |= WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0;
                    ex_style &= !WS_EX_APPWINDOW.0;
                    let _ = SetWindowLongPtrW(s_hwnd, GWL_EXSTYLE, ex_style as isize);
                    
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(_sb) = sb_init.upgrade() {
                            let mut work_area = windows::Win32::Foundation::RECT::default();
                            if SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work_area as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() {
                                let x = work_area.right - 100; 
                                let y = work_area.bottom - 40;
                                let _ = SetWindowPos(s_hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE);
                            }
                        }
                    });
                }

                let t_title = "RustImeTrayMenu\0".encode_utf16().collect::<Vec<u16>>();
                let t_hwnd = FindWindowW(None, PCWSTR(t_title.as_ptr()));
                if t_hwnd.0 != 0 {
                    let mut ex_style = GetWindowLongPtrW(t_hwnd, GWL_EXSTYLE) as u32;
                    ex_style |= WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0;
                    ex_style &= !WS_EX_APPWINDOW.0;
                    let _ = SetWindowLongPtrW(t_hwnd, GWL_EXSTYLE, ex_style as isize);
                }
            }
        });
    }

    let window_was_visible = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let open_menu_time = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let open_menu_time_for_timer = open_menu_time.clone();

    // 定时器：检测托盘菜单失去焦点自动隐藏
    let tm_for_timer = tray_menu_handle.clone();
    slint::Timer::default().start(slint::TimerMode::Repeated, std::time::Duration::from_millis(150), move || {
        if let Some(tm) = tm_for_timer.upgrade() {
            if tm.window().is_visible() {
                #[cfg(target_os = "windows")]
                unsafe {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                    let open_time = open_menu_time_for_timer.load(std::sync::atomic::Ordering::SeqCst);
                    if now - open_time < 600 { return; } 

                    let active_hwnd = GetForegroundWindow();
                    let title = "RustImeTrayMenu\0".encode_utf16().collect::<Vec<u16>>();
                    let menu_hwnd = FindWindowW(None, PCWSTR(title.as_ptr()));
                    
                    if menu_hwnd.0 != 0 && active_hwnd.0 != 0 && active_hwnd != menu_hwnd {
                        // 检查活动窗口是否是菜单的子窗口或属于同一线程（处理某些特殊 UI 情况）
                        let active_thread_id = GetWindowThreadProcessId(active_hwnd, None);
                        let menu_thread_id = GetWindowThreadProcessId(menu_hwnd, None);
                        if active_thread_id != menu_thread_id {
                            let _ = tm.window().hide();
                        }
                    } else if active_hwnd.0 == 0 {
                        // 桌面或任务栏等特殊区域，没有明确的 Foreground Window 时也尝试隐藏
                        // 只要距离打开时间够久且当前没焦点
                        let _ = tm.window().hide();
                    }
                }
            }
        }
    });

    let open_menu_time_for_thread = open_menu_time.clone();

    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            let h = window_handle.clone();
            let s = status_bar_handle.clone();
            let tm_handle = tray_menu_handle.clone();
            let show_candidates_for_loop = show_candidates.clone();
            let show_status_bar_for_loop = show_status_bar_atomic.clone();
            let random_highlight_for_loop = random_highlight_atomic.clone();
            let page_size_for_loop = page_size_atomic.clone();
            let last_pos_inner = last_pos_for_loop.clone();
            let was_visible_atomic = window_was_visible.clone();
            let color_shared = current_color_shared.clone();
            let open_menu_time_inner = open_menu_time_for_thread.clone();
            let lah_inner = last_active_hwnd.clone();
            
            let _ = slint::invoke_from_event_loop(move || {
                match event {
                    GuiEvent::Update { pinyin, candidates, hints, selected, .. } => {
                        if let Some(w) = h.upgrade() {
                            #[cfg(target_os = "windows")]
                            unsafe { hide_window_from_taskbar("RustImeCandidateWindow"); }
                            let should_be_visible = !(pinyin.is_empty() && candidates.is_empty()) && show_candidates_for_loop.load(std::sync::atomic::Ordering::SeqCst);
                            
                            if !should_be_visible {
                                w.set_is_visible(false);
                                let _ = w.window().hide();
                                was_visible_atomic.store(false, std::sync::atomic::Ordering::SeqCst);
                            } else {
                                if !was_visible_atomic.load(std::sync::atomic::Ordering::SeqCst) {
                                    if random_highlight_for_loop.load(std::sync::atomic::Ordering::SeqCst) {
                                        use std::time::{SystemTime, UNIX_EPOCH};
                                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                                        let r = (now % 150 + 50) as u8;
                                        let g = ((now >> 8) % 150 + 50) as u8;
                                        let b = ((now >> 16) % 150 + 50) as u8;
                                        let mut c = color_shared.lock().unwrap();
                                        *c = slint::Color::from_rgb_u8(r, g, b);
                                    }
                                    was_visible_atomic.store(true, std::sync::atomic::Ordering::SeqCst);
                                }
                                
                                {
                                    let c = color_shared.lock().unwrap();
                                    w.set_accent_color(*c);
                                }
                                
                                w.set_pinyin(SharedString::from(&pinyin));
                                let page_size = page_size_for_loop.load(std::sync::atomic::Ordering::SeqCst); 
                                let page = (selected / page_size) * page_size;
                                let relative_selected = (selected % page_size) as i32;
                                let mut data_vec = Vec::new();
                                for i in page..(page + page_size).min(candidates.len()) {
                                    let cand = &candidates[i];
                                    let hint = hints.get(i).cloned().unwrap_or_default();
                                    let mut english = String::new();
                                    let mut stroke = String::new();
                                    if !hint.is_empty() {
                                        if hint.contains('/') {
                                            let parts: Vec<&str> = hint.split('/').collect();
                                            english = parts[0].trim().to_string();
                                            stroke = parts[1].trim().to_string();
                                        } else { english = hint.clone(); }
                                    }
                                    data_vec.push(CandidateData { text: SharedString::from(cand), english_aux: SharedString::from(english), stroke_aux: SharedString::from(stroke) });
                                }
                                w.set_candidates(ModelRc::new(VecModel::from(data_vec)));
                                w.set_selected_index(relative_selected);
                                w.set_is_visible(true);
                                let (lx, ly) = { let pos = last_pos_inner.lock().unwrap(); (pos.0, pos.1) };
                                if lx != 0 || ly != 0 {
                                    let mut final_x = lx; let mut final_y = ly;
                                    #[cfg(target_os = "windows")]
                                    unsafe {
                                        let win_size = w.window().size();
                                        let monitor = MonitorFromPoint(windows::Win32::Foundation::POINT { x: lx, y: ly }, MONITOR_DEFAULTTONEAREST);
                                        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                                        if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                                            if final_x + win_size.width as i32 > mi.rcMonitor.right { final_x = mi.rcMonitor.right - win_size.width as i32 - 10; }
                                            if final_y + win_size.height as i32 > mi.rcMonitor.bottom { final_y = mi.rcMonitor.bottom - win_size.height as i32 - 10; }
                                        }
                                    }
                                    let _ = w.window().set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(final_x, final_y)));
                                }
                                if !w.window().is_visible() { let _ = w.window().show(); }
                            }
                        }
                    }
                    GuiEvent::OpenTrayMenu { x, y, chinese_enabled, active_profile } => {
                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                        open_menu_time_inner.store(now, std::sync::atomic::Ordering::SeqCst);

                        #[cfg(target_os = "windows")]
                        unsafe {
                            let active = GetForegroundWindow();
                            if active.0 != 0 {
                                lah_inner.store(active.0 as isize, std::sync::atomic::Ordering::SeqCst);
                            }
                        }

                        if let Some(tm) = tm_handle.upgrade() {
                            #[cfg(target_os = "windows")]
                            unsafe { hide_window_from_taskbar("RustImeTrayMenu"); }
                            tm.set_chinese_enabled(chinese_enabled);
                            tm.set_active_profile(SharedString::from(active_profile));
                            
                            let mut final_x = x;
                            let mut final_y = y;
                            let win_width = 200; 
                            
                            #[cfg(target_os = "windows")]
                            unsafe {
                                let monitor = MonitorFromPoint(windows::Win32::Foundation::POINT { x, y }, MONITOR_DEFAULTTONEAREST);
                                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                                if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                                    if final_x + win_width > mi.rcMonitor.right { final_x = mi.rcMonitor.right - win_width - 10; }
                                    let win_height = tm.window().size().height as i32;
                                    final_y = y - win_height;
                                    if final_y < mi.rcMonitor.top { final_y = y; }
                                }
                            }
                            
                            let _ = tm.window().set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(final_x, final_y)));
                            let _ = tm.window().show();
                            tm.invoke_request_focus();
                            
                            #[cfg(target_os = "windows")]
                            unsafe {
                                let title = "RustImeTrayMenu\0".encode_utf16().collect::<Vec<u16>>();
                                let hwnd = FindWindowW(None, PCWSTR(title.as_ptr()));
                                if hwnd.0 != 0 {
                                    // 确保窗口置顶并获取焦点
                                    let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
                                    let _ = SetForegroundWindow(hwnd);
                                }
                            }
                        }
                    }
                    GuiEvent::ShowStatus(status, is_chinese) => {
                        if let Some(sb) = s.upgrade() {
                            #[cfg(target_os = "windows")]
                            unsafe { hide_window_from_taskbar("RustImeStatusBar"); }
                            sb.set_status_text(SharedString::from(status.clone()));
                            sb.set_chinese_enabled(is_chinese);
                            
                            let show_pref = show_status_bar_for_loop.load(std::sync::atomic::Ordering::SeqCst);
                            if show_pref && !sb.window().is_visible() {
                                let _ = sb.window().show();
                            }
                        }
                        // 同步更新托盘菜单界面
                        if let Some(tm) = tm_handle.upgrade() {
                            tm.set_chinese_enabled(is_chinese);
                        }
                    }
                    GuiEvent::SetVisible(visible) => {
                        if let Some(sb) = s.upgrade() {
                            let show_pref = show_status_bar_for_loop.load(std::sync::atomic::Ordering::SeqCst);
                            if visible && show_pref {
                                #[cfg(target_os = "windows")]
                                unsafe { hide_window_from_taskbar("RustImeStatusBar"); }
                                let _ = sb.window().show();
                            } else {
                                let _ = sb.window().hide();
                            }
                        }
                        if !visible {
                            if let Some(w) = h.upgrade() {
                                let _ = w.window().hide();
                            }
                        }
                    }
                    GuiEvent::MoveTo { x, y } => {
                        if x == 0 && y == 0 { return; }
                        if let Ok(mut pos) = last_pos_inner.lock() { *pos = (x, y); }
                        if let Some(w) = h.upgrade() {
                            let mut final_x = x; let mut final_y = y;
                            #[cfg(target_os = "windows")]
                            unsafe {
                                let win_size = w.window().size();
                                let monitor = MonitorFromPoint(windows::Win32::Foundation::POINT { x, y }, MONITOR_DEFAULTTONEAREST);
                                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                                if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                                    if final_x + win_size.width as i32 > mi.rcMonitor.right { final_x = mi.rcMonitor.right - win_size.width as i32 - 10; }
                                    if final_y + win_size.height as i32 > mi.rcMonitor.bottom { final_y = mi.rcMonitor.bottom - win_size.height as i32 - 10; }
                                }
                            }
                            let _ = w.window().set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(final_x, final_y)));
                        }
                    }
                    GuiEvent::ApplyConfig(new_conf) => {
                        show_candidates_for_loop.store(new_conf.appearance.show_candidates, std::sync::atomic::Ordering::SeqCst);
                        show_status_bar_for_loop.store(new_conf.appearance.show_status_bar, std::sync::atomic::Ordering::SeqCst);
                        random_highlight_for_loop.store(new_conf.appearance.enable_random_highlight, std::sync::atomic::Ordering::SeqCst);
                        page_size_for_loop.store(new_conf.appearance.page_size, std::sync::atomic::Ordering::SeqCst);
                        if let Some(w) = h.upgrade() {
                            w.set_show_english_aux(new_conf.appearance.show_english_aux);
                            w.set_show_stroke_aux(new_conf.appearance.show_stroke_aux);
                            w.set_show_translation(new_conf.appearance.show_english_translation);
                            w.set_is_horizontal(new_conf.appearance.candidate_layout == "horizontal");
                            {
                                let mut c = color_shared.lock().unwrap();
                                *c = parse_color(&new_conf.appearance.window_highlight_color);
                                w.set_accent_color(*c);
                            }
                            w.set_bg_color(parse_color(&new_conf.appearance.window_bg_color));
                            w.set_border_color(parse_color(&new_conf.appearance.window_border_color));
                            w.set_text_color(parse_color(&new_conf.appearance.candidate_text.color));
                            w.set_highlight_text_color(parse_color(&new_conf.appearance.window_bg_color));
                            w.set_pinyin_font_size(new_conf.appearance.pinyin_text.font_size as f32);
                            w.set_pinyin_font_family(SharedString::from(new_conf.appearance.pinyin_text.font_family.clone()));
                            w.set_pinyin_font_weight(new_conf.appearance.pinyin_text.font_weight as i32);
                            w.set_candidate_font_size(new_conf.appearance.candidate_text.font_size as f32);
                            w.set_candidate_font_family(SharedString::from(new_conf.appearance.candidate_text.font_family.clone()));
                            w.set_candidate_font_weight(new_conf.appearance.candidate_text.font_weight as i32);
                            if let Some(sb) = s.upgrade() {
                                if new_conf.appearance.show_status_bar { let _ = sb.show(); } else { let _ = sb.hide(); } 
                            }
                        }
                    }
                    GuiEvent::Exit => { let _ = slint::quit_event_loop(); }
                }
            });
        }
    });

    if config.appearance.show_status_bar { status_bar.show().expect("Failed to show StatusBar"); }
    slint::run_event_loop().expect("Failed to run Slint event loop");
}
