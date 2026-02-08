use windows::{
    Win32::Foundation::*,
    Win32::UI::WindowsAndMessaging::*,
    Win32::Graphics::Gdi::*,
    core::*,
};
use crate::ui::painter::CandidatePainter;
use crate::ui::GuiEvent;
use crate::config::Config;
use std::sync::mpsc::Receiver;

static mut WINDOW_STATE: Option<WindowState> = None;
static mut KEY_WINDOW: HWND = HWND(0);
static mut LEARN_WINDOW: HWND = HWND(0);
static mut DISPLAYED_KEYS: Vec<(String, std::time::Instant)> = Vec::new();

struct WindowState {
    pinyin: String,
    candidates: Vec<String>,
    hints: Vec<String>,
    selected: usize,
    x: i32,
    y: i32,
}

pub fn start_gui(rx: Receiver<GuiEvent>, initial_config: Config) {
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap();
        let window_class = PCWSTR("RustImeGui\0".encode_utf16().collect::<Vec<u16>>().as_ptr());

        let wc = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hInstance: instance.into(),
            lpszClassName: window_class,
            lpfnWndProc: Some(wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            window_class, PCWSTR(std::ptr::null()), WS_POPUP,
            100, 100, 600, 160, None, None, instance, None,
        );

        let key_class = PCWSTR("RustImeKey\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
        let wc_key = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: key_class,
            lpfnWndProc: Some(wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&wc_key);
        KEY_WINDOW = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            key_class, PCWSTR(std::ptr::null()), WS_POPUP,
            0, 0, 0, 0, None, None, instance, None,
        );

        let learn_class = PCWSTR("RustImeLearn\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
        let wc_learn = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: learn_class,
            lpfnWndProc: Some(wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&wc_learn);
        LEARN_WINDOW = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            learn_class, PCWSTR(std::ptr::null()), WS_POPUP,
            0, 0, 0, 0, None, None, instance, None,
        );

        let painter = CandidatePainter::new();
        let current_config = Arc::new(std::sync::RwLock::new(initial_config));

        let current_config_main = current_config.clone();
        std::thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                match event {
                    GuiEvent::ApplyConfig(conf) => { 
                        if let Ok(mut w) = current_config_main.write() { *w = conf; }
                    }
                    GuiEvent::Update { pinyin, candidates, hints, selected, .. } => {
                        unsafe {
                            if let Some(ref mut state) = WINDOW_STATE {
                                state.pinyin = pinyin;
                                state.candidates = candidates;
                                state.hints = hints;
                                state.selected = selected;
                            } else {
                                WINDOW_STATE = Some(WindowState { pinyin, candidates, hints, selected, x: 100, y: 100 });
                            }
                            
                            let state = WINDOW_STATE.as_ref().unwrap();
                            if state.pinyin.is_empty() {
                                ShowWindow(hwnd, SW_HIDE);
                            } else {
                                let (page_size, config_snapshot) = {
                                    let r = current_config_main.read().unwrap();
                                    (r.appearance.page_size, r.clone())
                                };
                                
                                let start = (state.selected / page_size) * page_size;
                                let end = (start + page_size).min(state.candidates.len());
                                
                                let current_candidates = state.candidates[start..end].to_vec();
                                let current_hints = if state.hints.len() >= end {
                                    state.hints[start..end].to_vec()
                                } else {
                                    vec![String::new(); current_candidates.len()]
                                };

                                // 使用 Painter 绘图，获取准确的 w 和 h
                                let (pixels, w, h) = painter.draw(
                                    &state.pinyin, 
                                    &current_candidates, 
                                    &current_hints, 
                                    state.selected % page_size, 
                                    &config_snapshot
                                );
                                
                                update_window_pixels(hwnd, &pixels, w, h);
                                let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, w as i32, h as i32, SWP_NOMOVE | SWP_NOACTIVATE);
                                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                            }
                        }
                    }
                    GuiEvent::MoveTo { x, y } => {
                        unsafe {
                            if let Some(ref mut state) = WINDOW_STATE { state.x = x; state.y = y; }
                            // 偏移 25 像素，确保显示在文字下方
                            let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y + 25, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                        }
                    }
                    GuiEvent::Keystroke(key) => {
                        unsafe {
                            let config_snapshot = current_config_main.read().unwrap().clone();
                            if !config_snapshot.appearance.show_keystrokes { continue; }
                            DISPLAYED_KEYS.push((key, std::time::Instant::now()));
                            if DISPLAYED_KEYS.len() > 10 { DISPLAYED_KEYS.remove(0); }
                            
                            let keys: Vec<String> = DISPLAYED_KEYS.iter().map(|(k, _)| k.clone()).collect();
                            let (pixels, w, h) = painter.draw_keystrokes(&keys, &config_snapshot);
                            update_window_pixels(KEY_WINDOW, &pixels, w, h);
                            
                            let sw = GetSystemMetrics(SM_CXSCREEN);
                            let sh = GetSystemMetrics(SM_CYSCREEN);
                            let _ = SetWindowPos(KEY_WINDOW, HWND_TOPMOST, (sw - w as i32) / 2, sh - h as i32 - 100, w as i32, h as i32, SWP_NOACTIVATE);
                            ShowWindow(KEY_WINDOW, SW_SHOWNOACTIVATE);
                        }
                    }
                    GuiEvent::ClearKeystrokes => {
                        unsafe {
                            DISPLAYED_KEYS.clear();
                            ShowWindow(KEY_WINDOW, SW_HIDE);
                        }
                    }
                    GuiEvent::ShowLearning(word, hint) => {
                        unsafe {
                            let config_snapshot = current_config_main.read().unwrap().clone();
                            if !config_snapshot.appearance.learning_mode { continue; }
                            let (pixels, w, h) = painter.draw_learning(&word, &hint, &config_snapshot);
                            update_window_pixels(LEARN_WINDOW, &pixels, w, h);
                            
                            let sw = GetSystemMetrics(SM_CXSCREEN);
                            let _ = SetWindowPos(LEARN_WINDOW, HWND_TOPMOST, sw - w as i32 - 40, 40, w as i32, h as i32, SWP_NOACTIVATE);
                            ShowWindow(LEARN_WINDOW, SW_SHOWNOACTIVATE);
                        }
                    }
                    _ => {}
                }
            }
        });

        // 启动一个简单的清理定时器线程
        let painter_timer = CandidatePainter::new(); // 计时器线程专用的 painter
        let current_config_timer = current_config.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(200));
                unsafe {
                    let now = std::time::Instant::now();
                    // 清理过期按键
                    let mut changed = false;
                    DISPLAYED_KEYS.retain(|(_, time)| {
                        if now.duration_since(*time).as_millis() < 2000 { true } else { changed = true; false }
                    });
                    if changed {
                        if DISPLAYED_KEYS.is_empty() {
                            ShowWindow(KEY_WINDOW, SW_HIDE);
                        } else {
                            // 重新绘制并更新以反映按键消失
                            let keys: Vec<String> = DISPLAYED_KEYS.iter().map(|(k, _)| k.clone()).collect();
                            let config_snapshot = current_config_timer.read().unwrap().clone();
                            let (pixels, w, h) = painter_timer.draw_keystrokes(&keys, &config_snapshot);
                            update_window_pixels(KEY_WINDOW, &pixels, w, h);
                        }
                    }
                }
            }
        });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn update_window_pixels(hwnd: HWND, pixels: &[u8], width: u32, height: u32) {
    let hdc_screen = GetDC(None);
    let hdc_mem = CreateCompatibleDC(hdc_screen);
    
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits = std::ptr::null_mut();
    let h_bitmap = CreateDIBSection(hdc_screen, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
    
    if !bits.is_null() {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, (width * height * 4) as usize);
    }

    let old_bitmap = SelectObject(hdc_mem, h_bitmap);
    let size = SIZE { cx: width as i32, cy: height as i32 };
    let pt_src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
        ..Default::default()
    };

    let _ = UpdateLayeredWindow(hwnd, hdc_screen, None, Some(&size), hdc_mem, Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA);

    SelectObject(hdc_mem, old_bitmap);
    DeleteObject(h_bitmap);
    DeleteDC(hdc_mem);
    ReleaseDC(None, hdc_screen);
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}