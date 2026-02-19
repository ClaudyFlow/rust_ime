use windows::{
    Win32::Foundation::*,
    Win32::UI::WindowsAndMessaging::*,
    Win32::Graphics::Gdi::*,
    core::*,
};
use crate::ui::GuiEvent;
use crate::config::Config;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};

static mut WINDOW_STATE: Option<WindowState> = None;
static mut KEYSTROKE_STATE: Option<KeystrokeState> = None;
static mut LEARNING_STATE: Option<LearningState> = None;
static mut STATUS_STATE: Option<StatusState> = None;
static CURRENT_CONFIG: std::sync::OnceLock<Arc<RwLock<Config>>> = std::sync::OnceLock::new();

struct WindowState {
    pinyin: String,
    candidates: Vec<String>,
    hints: Vec<String>,
    selected: usize,
    x: i32,
    y: i32,
}

struct KeystrokeState {
    keys: Vec<String>,
    last_update: std::time::Instant,
}

struct LearningState {
    last_update: std::time::Instant,
}

struct StatusState {
    last_update: std::time::Instant,
}

pub fn start_gui(rx: Receiver<GuiEvent>, initial_config: Config) {
    println!("[GUI] Starting Windows GUI thread...");
    let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap() };
    let window_class = PCWSTR("RustImeGui\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
    let ks_class = PCWSTR("RustImeKeystroke\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
    let learn_class = PCWSTR("RustImeLearning\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
    let status_class = PCWSTR("RustImeStatus\0".encode_utf16().collect::<Vec<u16>>().as_ptr());

    unsafe {
        // ... 前面的字体加载代码保持不变 ...
        let root = crate::find_project_root();
        let font_path = root.join("fonts/NotoSansSC-Bold.ttf");
        let mut font_loaded = false;
        let mut path_u16: Vec<u16> = Vec::new();

        if font_path.exists() {
            path_u16 = font_path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
            let res = AddFontResourceExW(PCWSTR(path_u16.as_ptr()), FR_PRIVATE, None);
            if res > 0 { font_loaded = true; }
        }

        let wc = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hInstance: instance.into(),
            lpszClassName: window_class,
            lpfnWndProc: Some(wnd_proc),
            hbrBackground: CreateSolidBrush(COLORREF(0xFFFFFF)), 
            ..Default::default()
        };
        RegisterClassW(&wc);

        let wc_ks = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hInstance: instance.into(),
            lpszClassName: ks_class,
            lpfnWndProc: Some(ks_wnd_proc),
            hbrBackground: CreateSolidBrush(COLORREF(0x000000)),
            ..Default::default()
        };
        RegisterClassW(&wc_ks);

        let wc_learn = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hInstance: instance.into(),
            lpszClassName: learn_class,
            lpfnWndProc: Some(learn_wnd_proc),
            hbrBackground: CreateSolidBrush(COLORREF(0x000000)),
            ..Default::default()
        };
        RegisterClassW(&wc_learn);

        let wc_status = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hInstance: instance.into(),
            lpszClassName: status_class,
            lpfnWndProc: Some(status_wnd_proc),
            hbrBackground: CreateSolidBrush(COLORREF(0x000000)),
            ..Default::default()
        };
        RegisterClassW(&wc_status);

        let _ = CURRENT_CONFIG.set(Arc::new(RwLock::new(initial_config)));

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            window_class, PCWSTR(std::ptr::null()), WS_POPUP,
            100, 100, 400, 120, None, None, instance, None,
        );

        let hwnd_ks = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            ks_class, PCWSTR(std::ptr::null()), WS_POPUP,
            0, 0, 800, 100, None, None, instance, None,
        );
        let _ = SetLayeredWindowAttributes(hwnd_ks, COLORREF(0x000000), 200, LWA_ALPHA | LWA_COLORKEY);

        let hwnd_learn = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            learn_class, PCWSTR(std::ptr::null()), WS_POPUP,
            0, 0, 400, 80, None, None, instance, None,
        );
        let _ = SetLayeredWindowAttributes(hwnd_learn, COLORREF(0x000000), 200, LWA_ALPHA | LWA_COLORKEY);

        let hwnd_status = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            status_class, PCWSTR(std::ptr::null()), WS_POPUP,
            0, 0, 100, 100, None, None, instance, None,
        );
        let _ = SetLayeredWindowAttributes(hwnd_status, COLORREF(0x000000), 255, LWA_ALPHA | LWA_COLORKEY);

        std::thread::spawn(move || {
            let painter = crate::ui::painter::CandidatePainter::new();
            while let Ok(event) = rx.recv() {
                match event {
                    GuiEvent::Update { pinyin, candidates, hints, selected, .. } => {
                        let state_ptr = std::ptr::addr_of_mut!(WINDOW_STATE);
                        if let Some(ref mut state) = *state_ptr {
                            state.pinyin = pinyin;
                            state.candidates = candidates;
                            state.hints = hints;
                            state.selected = selected;
                        } else {
                            *state_ptr = Some(WindowState { pinyin, candidates, hints, selected, x: 100, y: 100 });
                        }
                        
                        if let Some(ref state) = *state_ptr {
                            if state.pinyin.is_empty() {
                                ShowWindow(hwnd, SW_HIDE);
                            } else {
                                ShowWindow(hwnd_status, SW_HIDE);
                                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                                InvalidateRect(hwnd, None, BOOL(1));
                                UpdateWindow(hwnd);
                            }
                        }
                    }
                    GuiEvent::MoveTo { x, y } => {
                        let state_ptr = std::ptr::addr_of_mut!(WINDOW_STATE);
                        if let Some(ref mut state) = *state_ptr { state.x = x; state.y = y; }
                        
                        let mut rect = RECT::default();
                        let _ = GetWindowRect(hwnd, &mut rect);
                        let w = rect.right - rect.left;
                        let h = rect.bottom - rect.top;

                        let screen_w = GetSystemMetrics(SM_CXSCREEN);
                        let screen_h = GetSystemMetrics(SM_CYSCREEN);

                        let anchor = if let Some(arc) = CURRENT_CONFIG.get() { 
                            arc.read().unwrap().appearance.candidate_anchor.clone() 
                        } else { 
                            "bottom".to_string() 
                        };

                        let mut final_x = x;
                        let mut final_y = if anchor == "top" { y - h - 5 } else { y + 20 };

                        if final_x + w > screen_w { final_x = screen_w - w; }
                        if anchor == "top" {
                            if final_y < 0 { final_y = y + 20; }
                        } else {
                            if final_y + h > screen_h { final_y = y - h - 5; }
                        }

                        if final_x < 0 { final_x = 0; }
                        if final_y < 0 { final_y = 0; }

                        let _ = SetWindowPos(hwnd, HWND_TOPMOST, final_x, final_y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                        
                        // 只有在 pinyin 为空时才显示/移动状态窗，避免两个框重叠
                        if let Some(ref state) = *state_ptr {
                            if state.pinyin.is_empty() {
                                let _ = SetWindowPos(hwnd_status, HWND_TOPMOST, final_x, final_y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                            } else {
                                ShowWindow(hwnd_status, SW_HIDE);
                            }
                        }
                    }
                    GuiEvent::ShowStatus(text) => {
                        let status_ptr = std::ptr::addr_of_mut!(STATUS_STATE);
                        *status_ptr = Some(StatusState { last_update: std::time::Instant::now() });
                        
                        let (data, w, h) = {
                            let conf = CURRENT_CONFIG.get().unwrap().read().unwrap();
                            painter.draw_status(&text, &conf)
                        };
                        
                        // 使用 UpdateLayeredWindow 实现真正的透明和渲染
                        update_layered_window(hwnd_status, &data, w, h);
                        ShowWindow(hwnd_status, SW_SHOWNOACTIVATE);

                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(800));
                            let status_ptr = std::ptr::addr_of_mut!(STATUS_STATE);
                            if let Some(ref state) = *status_ptr {
                                if state.last_update.elapsed().as_millis() >= 800 {
                                    ShowWindow(hwnd_status, SW_HIDE);
                                }
                            }
                        });
                    }
                    GuiEvent::Keystroke(key) => {
                        let show = if let Some(arc) = CURRENT_CONFIG.get() { arc.read().unwrap().appearance.show_keystrokes } else { false };
                        if !show { continue; }

                        let ks_ptr = std::ptr::addr_of_mut!(KEYSTROKE_STATE);
                        if let Some(ref mut state) = *ks_ptr {
                            state.keys.push(key);
                            if state.keys.len() > 10 { state.keys.remove(0); }
                            state.last_update = std::time::Instant::now();
                        } else {
                            *ks_ptr = Some(KeystrokeState { keys: vec![key], last_update: std::time::Instant::now() });
                        }
                        
                        if let Some(ref state) = *ks_ptr {
                            let (data, w, h) = {
                                let conf = CURRENT_CONFIG.get().unwrap().read().unwrap();
                                painter.draw_keystrokes(&state.keys, &conf)
                            };
                            update_layered_window(hwnd_ks, &data, w, h);
                            ShowWindow(hwnd_ks, SW_SHOWNOACTIVATE);
                        }
                        
                        let timeout = if let Some(arc) = CURRENT_CONFIG.get() { arc.read().unwrap().appearance.keystroke_timeout_ms } else { 1500 };
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(timeout));
                            let ks_ptr = std::ptr::addr_of_mut!(KEYSTROKE_STATE);
                            if let Some(ref state) = *ks_ptr {
                                if state.last_update.elapsed().as_millis() >= timeout as u128 {
                                    ShowWindow(hwnd_ks, SW_HIDE);
                                }
                            }
                        });
                    }
                    GuiEvent::ClearKeystrokes => {
                        let ks_ptr = std::ptr::addr_of_mut!(KEYSTROKE_STATE);
                        *ks_ptr = None;
                        ShowWindow(hwnd_ks, SW_HIDE);
                    }
                    GuiEvent::ShowLearning(word, hint) => {
                        let show = if let Some(arc) = CURRENT_CONFIG.get() { arc.read().unwrap().appearance.learning_mode } else { false };
                        if !show { continue; }

                        let ln_ptr = std::ptr::addr_of_mut!(LEARNING_STATE);
                        *ln_ptr = Some(LearningState { last_update: std::time::Instant::now() });
                        
                        let (data, w, h) = {
                            let conf = CURRENT_CONFIG.get().unwrap().read().unwrap();
                            painter.draw_learning(&word, &hint, &conf)
                        };
                        update_layered_window(hwnd_learn, &data, w, h);
                        ShowWindow(hwnd_learn, SW_SHOWNOACTIVATE);

                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            let ln_ptr = std::ptr::addr_of_mut!(LEARNING_STATE);
                            if let Some(ref state) = *ln_ptr {
                                if state.last_update.elapsed().as_secs() >= 3 {
                                    ShowWindow(hwnd_learn, SW_HIDE);
                                }
                            }
                        });
                    }
                    GuiEvent::ApplyConfig(conf) => {
                        if let Some(arc) = CURRENT_CONFIG.get() {
                            if let Ok(mut w) = arc.write() { *w = conf; }
                        }
                        
                        let state_ptr = std::ptr::addr_of_mut!(WINDOW_STATE);
                        if let Some(ref state) = *state_ptr {
                            if !state.pinyin.is_empty() {
                                let (data, w, h) = {
                                    let conf = CURRENT_CONFIG.get().unwrap().read().unwrap();
                                    painter.draw(&state.pinyin, &state.candidates, &state.hints, state.selected, &conf)
                                };
                                update_layered_window(hwnd, &data, w, h);
                            }
                        }

                        InvalidateRect(hwnd, None, BOOL(1));
                        InvalidateRect(hwnd_ks, None, BOOL(1));
                        InvalidateRect(hwnd_learn, None, BOOL(1));
                    }
                    GuiEvent::Exit => {
                        PostQuitMessage(0);
                    }
                }
            }
        });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if font_loaded {
            let res = RemoveFontResourceExW(PCWSTR(path_u16.as_ptr()), FR_PRIVATE.0, None);
            println!("[GUI] RemoveFontResourceExW result: {}", res.as_bool());
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bm = CreateCompatibleBitmap(hdc, rect.right, rect.bottom);
            let old_bm = SelectObject(mem_dc, mem_bm);
            
            if let Some(ref state) = WINDOW_STATE {
                if let Some(arc) = CURRENT_CONFIG.get() {
                    if let Ok(conf) = arc.read() {
                        // 使用配置背景色填充背景，避免残留
                        let bg_color = parse_color_win(&conf.appearance.window_bg_color);
                        let brush = CreateSolidBrush(bg_color);
                        let _ = FillRect(mem_dc, &rect, brush);
                        let _ = DeleteObject(brush);
                        
                        draw_content(mem_dc, hwnd, state, &conf);
                    }
                }
            }
            
            let _ = BitBlt(hdc, 0, 0, rect.right, rect.bottom, mem_dc, 0, 0, SRCCOPY);
            
            SelectObject(mem_dc, old_bm);
            let _ = DeleteObject(mem_bm);
            let _ = DeleteDC(mem_dc);
            
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn ks_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _hdc = BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn learn_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _hdc = BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn status_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn draw_content(hdc: HDC, hwnd: HWND, state: &WindowState, conf: &Config) {
    SetBkMode(hdc, TRANSPARENT);
    
    let pad_x = conf.appearance.window_padding_x;
    let pad_y = conf.appearance.window_padding_y;
    let row_space = conf.appearance.row_spacing as i32;
    let item_space = conf.appearance.item_spacing as i32;

    let bg_color = parse_color_win(&conf.appearance.window_bg_color);
    let border_color = parse_color_win(&conf.appearance.window_border_color);
    let radius = (conf.appearance.corner_radius as i32) * 2;
    let border_width = 2;

    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    
    // 1. 设置物理裁切区域 (严格对应当前窗口物理尺寸)
    let hrgn = CreateRoundRectRgn(rect.left, rect.top, rect.right, rect.bottom, radius, radius);
    let _ = SetWindowRgn(hwnd, hrgn, BOOL(1));

    let bg_brush = CreateSolidBrush(bg_color);
    let border_pen = CreatePen(PS_SOLID, border_width, border_color);
    let old_brush = SelectObject(hdc, bg_brush);
    let old_pen = SelectObject(hdc, border_pen);
    
    // 2. 绘制圆角矩形。
    // 在 GDI 中，RoundRect 的右/底坐标是包含的。
    // 我们绘制在 (0, 0) 到 (width-1, height-1) 范围内，这会紧贴裁切边缘内侧。
    RoundRect(hdc, rect.left, rect.top, rect.right - 1, rect.bottom - 1, radius, radius);
    
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    let _ = DeleteObject(bg_brush);
    let _ = DeleteObject(border_pen);

    // 字体和其余绘制逻辑（GDI 路径下极快）
    let py_font_name = HSTRING::from(&conf.appearance.pinyin_text.font_family);
    let h_font_py = CreateFontW(
        -(conf.appearance.pinyin_text.font_size as i32 * 96 / 72),
        0, 0, 0, 700, 0, 0, 0, DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32,
        PCWSTR(py_font_name.as_ptr())
    );

    let cand_font_name = HSTRING::from(&conf.appearance.candidate_text.font_family);
    let h_font_cand = CreateFontW(
        -(conf.appearance.candidate_text.font_size as i32 * 96 / 72),
        0, 0, 0, 700, 0, 0, 0, DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32,
        PCWSTR(cand_font_name.as_ptr())
    );

    let hint_font_name = HSTRING::from(&conf.appearance.hint_text.font_family);
    let h_font_hint = CreateFontW(
        -(conf.appearance.hint_text.font_size as i32 * 96 / 72),
        0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32,
        PCWSTR(hint_font_name.as_ptr())
    );

    // --- 绘制拼音行 ---
    SelectObject(hdc, h_font_py);
    SetTextColor(hdc, parse_color_win(&conf.appearance.pinyin_text.color));
    let py_u16: Vec<u16> = state.pinyin.encode_utf16().collect();
    TextOutW(hdc, pad_x, pad_y, &py_u16);
    let mut py_size = SIZE::default();
    GetTextExtentPoint32W(hdc, &py_u16, &mut py_size);

    // --- 绘制候选词行 ---
    let cand_y = pad_y + py_size.cy + row_space;
    let mut x_cursor = pad_x;
    let cand_color = parse_color_win(&conf.appearance.candidate_text.color);
    let hint_color = parse_color_win(&conf.appearance.hint_text.color);
    let page_size = conf.appearance.page_size;
    let start = (state.selected / page_size) * page_size;
    let end = (start + page_size).min(state.candidates.len());
    let mut max_row_height = 0;

    for i in start..end {
        let is_selected = i == state.selected;
        SelectObject(hdc, h_font_cand);
        let idx_text = format!("{}.", i - start + 1);
        let idx_u16: Vec<u16> = idx_text.encode_utf16().collect();
        let mut idx_size = SIZE::default();
        GetTextExtentPoint32W(hdc, &idx_u16, &mut idx_size);

        let cand_text = &state.candidates[i];
        let cand_u16: Vec<u16> = cand_text.encode_utf16().collect();
        let mut text_size = SIZE::default();
        GetTextExtentPoint32W(hdc, &cand_u16, &mut text_size);

        let mut hint_w = 0;
        let mut h_size = SIZE::default();
        if let Some(hint) = state.hints.get(i) {
            if !hint.is_empty() {
                SelectObject(hdc, h_font_hint);
                let hint_u16: Vec<u16> = hint.encode_utf16().collect();
                GetTextExtentPoint32W(hdc, &hint_u16, &mut h_size);
                hint_w = h_size.cx + 8;
            }
        }

        let item_total_w = idx_size.cx + 4 + text_size.cx + hint_w;
        if is_selected {
            // 高亮背景使用 AlphaBlend (GDI+ 效果)
            let h_brush = CreateSolidBrush(parse_color_win(&conf.appearance.window_highlight_color));
            let r = RECT { 
                left: x_cursor - 6, 
                top: cand_y - 2, 
                right: x_cursor + item_total_w + 6, 
                bottom: cand_y + text_size.cy + 2 
            };
            let h_pen = CreatePen(PS_NULL, 0, COLORREF(0));
            let old_b = SelectObject(hdc, h_brush);
            let old_p = SelectObject(hdc, h_pen);
            RoundRect(hdc, r.left, r.top, r.right, r.bottom, 8, 8);
            SelectObject(hdc, old_b);
            SelectObject(hdc, old_p);
            let _ = DeleteObject(h_brush);
            let _ = DeleteObject(h_pen);
        }
        
        SelectObject(hdc, h_font_cand);
        SetTextColor(hdc, if is_selected { COLORREF(0xFFFFFF) } else { cand_color });
        TextOutW(hdc, x_cursor, cand_y, &idx_u16);
        x_cursor += idx_size.cx + 4;
        TextOutW(hdc, x_cursor, cand_y, &cand_u16);
        x_cursor += text_size.cx;

        if hint_w > 0 {
            SelectObject(hdc, h_font_hint);
            SetTextColor(hdc, if is_selected { COLORREF(0xDDDDDD) } else { hint_color });
            let hint_u16: Vec<u16> = state.hints[i].encode_utf16().collect();
            TextOutW(hdc, x_cursor + 4, cand_y + (text_size.cy - h_size.cy), &hint_u16);
            x_cursor += hint_w;
        }
        x_cursor += item_space;
        max_row_height = max_row_height.max(text_size.cy);
    }

    let _ = DeleteObject(h_font_py);
    let _ = DeleteObject(h_font_cand);
    let _ = DeleteObject(h_font_hint);
    
    // 动态调整窗口尺寸
    let candidates_width = x_cursor + pad_x - item_space;
    let pinyin_width = py_size.cx + pad_x * 2;
    let final_w = (candidates_width.max(pinyin_width) + 25).max(200); 
    let final_h = cand_y + max_row_height + pad_y;

    let mut current_rect = RECT::default();
    let _ = GetWindowRect(hwnd, &mut current_rect);
    let cur_w = current_rect.right - current_rect.left;
    let cur_h = current_rect.bottom - current_rect.top;

    if final_w != cur_w || final_h != cur_h {
        // 1. 立即调整窗口物理大小
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, current_rect.left, current_rect.top, final_w, final_h, SWP_NOACTIVATE);
        // 2. 立即同步更新裁切区域，确保新区域不被截断
        let hrgn = CreateRoundRectRgn(0, 0, final_w, final_h, radius, radius);
        let _ = SetWindowRgn(hwnd, hrgn, BOOL(1));
    }
}

fn parse_color_win(s: &str) -> COLORREF {
    if s.starts_with('#') && s.len() == 7 {
        let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(0);
        let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(0);
        let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(0);
        return COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16));
    } else if s.starts_with("rgba") {
        let parts: Vec<&str> = s.trim_start_matches("rgba(").trim_end_matches(')').split(',').map(|p| p.trim()).collect();
        if parts.len() >= 3 {
            let r = parts[0].parse::<u8>().unwrap_or(0);
            let g = parts[1].parse::<u8>().unwrap_or(0);
            let b = parts[2].parse::<u8>().unwrap_or(0);
            return COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16));
        }
    }
    COLORREF(0)
}

unsafe fn update_layered_window(hwnd: HWND, data: &[u8], w: u32, h: u32) {
    let mut bgra_data = data.to_vec();
    for pixel in bgra_data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    
    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(screen_dc);
    let h_bitmap = CreateCompatibleBitmap(screen_dc, w as i32, h as i32);
    let old_bitmap = SelectObject(mem_dc, h_bitmap);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // Top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    let _ = SetDIBitsToDevice(
        mem_dc, 0, 0, w, h, 0, 0, 0, h,
        bgra_data.as_ptr() as *const _, &bmi, DIB_RGB_COLORS
    );

    let mut pt_dst = POINT::default();
    let mut rect = RECT::default();
    let _ = GetWindowRect(hwnd, &mut rect);
    let mut final_x = rect.left;
    let mut final_y = rect.top;
    let final_w = w as i32;
    let final_h = h as i32;

    let h_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut monitor_info = MONITORINFO::default();
    monitor_info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(h_monitor, &mut monitor_info).as_bool() {
        let rc_work = monitor_info.rcWork;
        if final_x + final_w > rc_work.right { final_x = rc_work.right - final_w; }
        if final_y + final_h > rc_work.bottom { final_y = rc_work.bottom - final_h; }
        if final_x < rc_work.left { final_x = rc_work.left; }
        if final_y < rc_work.top { final_y = rc_work.top; }
    }
    pt_dst.x = final_x;
    pt_dst.y = final_y;

    let size_src = SIZE { cx: w as i32, cy: h as i32 };
    let pt_src = POINT::default();
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let _ = UpdateLayeredWindow(
        hwnd, screen_dc, Some(&pt_dst), Some(&size_src),
        mem_dc, Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA
    );

    SelectObject(mem_dc, old_bitmap);
    let _ = DeleteObject(h_bitmap);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
}
