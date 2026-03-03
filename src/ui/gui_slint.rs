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
    let random_highlight_atomic = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(config.appearance.enable_random_highlight));
    let page_size_atomic = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(config.appearance.page_size));
    
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
                    ex_style |= WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0 | WS_EX_TRANSPARENT.0;
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
                    
                    // 初始定位到右下角
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(sb) = sb_init.upgrade() {
                            let mut work_area = windows::Win32::Foundation::RECT::default();
                            if SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work_area as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() {
                                let x = work_area.right - 80; 
                                let y = work_area.bottom - 40;
                                let _ = SetWindowPos(s_hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE);
                            }
                        }
                    });
                }
            }
        });
    }

    let mut last_pinyin_was_empty = true;
    let mut current_accent_color = parse_color(&config.appearance.window_highlight_color);

    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            let h = window_handle.clone();
            let s = status_bar_handle.clone();
            let show_candidates_for_loop = show_candidates.clone();
            let random_highlight_for_loop = random_highlight_atomic.clone();
            let page_size_for_loop = page_size_atomic.clone();
            let last_pos_inner = last_pos_for_loop.clone();
            
            let _ = slint::invoke_from_event_loop(move || {
                match event {
                    GuiEvent::Update { pinyin, candidates, hints, selected, .. } => {
                        if let Some(w) = h.upgrade() {
                            if pinyin.is_empty() && candidates.is_empty() || !show_candidates_for_loop.load(std::sync::atomic::Ordering::SeqCst) {
                                w.set_is_visible(false);
                                let _ = w.window().hide();
                                last_pinyin_was_empty = true;
                            } else {
                                if last_pinyin_was_empty && !pinyin.is_empty() {
                                    if random_highlight_for_loop.load(std::sync::atomic::Ordering::SeqCst) {
                                        use std::time::{SystemTime, UNIX_EPOCH};
                                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                                        let r = (now % 150 + 50) as u8;
                                        let g = ((now >> 8) % 150 + 50) as u8;
                                        let b = ((now >> 16) % 150 + 50) as u8;
                                        current_accent_color = slint::Color::from_rgb_u8(r, g, b);
                                    }
                                    last_pinyin_was_empty = false;
                                }
                                w.set_accent_color(current_accent_color);
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
                                    data_vec.push(CandidateData {
                                        text: SharedString::from(cand),
                                        english_aux: SharedString::from(english),
                                        stroke_aux: SharedString::from(stroke),
                                    });
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
                    GuiEvent::ShowStatus(status, is_active) => {
                        if let Some(sb) = s.upgrade() {
                            sb.set_status_text(SharedString::from(status.clone()));
                            sb.set_chinese_enabled(is_active);
                            let (lx, ly) = { let pos = last_pos_inner.lock().unwrap(); (pos.0, pos.1) };
                            let _ = sb.window().set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(lx, ly - 50)));
                            slint::Timer::single_shot(std::time::Duration::from_millis(30), move || {
                                #[cfg(target_os = "windows")]
                                unsafe {
                                    let s_title = "RustImeStatusBar\0".encode_utf16().collect::<Vec<u16>>();
                                    let s_hwnd = FindWindowW(None, PCWSTR(s_title.as_ptr()));
                                    if s_hwnd.0 != 0 {
                                        let mut work_area = windows::Win32::Foundation::RECT::default();
                                        if SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work_area as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() {
                                            let win_width = sb.window().size().width as i32;
                                            let x = work_area.right - win_width - 20;
                                            let y = work_area.bottom - 40;
                                            let _ = SetWindowPos(s_hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE);
                                        }
                                    }
                                }
                            });
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
                        random_highlight_for_loop.store(new_conf.appearance.enable_random_highlight, std::sync::atomic::Ordering::SeqCst);
                        page_size_for_loop.store(new_conf.appearance.page_size, std::sync::atomic::Ordering::SeqCst);
                        if let Some(w) = h.upgrade() {
                            w.set_show_english_aux(new_conf.appearance.show_english_aux);
                            w.set_show_stroke_aux(new_conf.appearance.show_stroke_aux);
                            w.set_show_translation(new_conf.appearance.show_english_translation);
                            w.set_is_horizontal(new_conf.appearance.candidate_layout == "horizontal");
                            current_accent_color = parse_color(&new_conf.appearance.window_highlight_color);
                            w.set_bg_color(parse_color(&new_conf.appearance.window_bg_color));
                            w.set_accent_color(current_accent_color);
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
