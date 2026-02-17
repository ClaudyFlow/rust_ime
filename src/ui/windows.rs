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
static mut CURRENT_CONFIG: Option<Arc<RwLock<Config>>> = None;

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
    word: String,
    hint: String,
    last_update: std::time::Instant,
}

pub fn start_gui(rx: Receiver<GuiEvent>, initial_config: Config) {
    println!("[GUI] Starting Windows GUI thread...");
    let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap() };
    let window_class = PCWSTR("RustImeGui\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
    let ks_class = PCWSTR("RustImeKeystroke\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
    let learn_class = PCWSTR("RustImeLearning\0".encode_utf16().collect::<Vec<u16>>().as_ptr());

    unsafe {
        // 加载本地字体，让 GDI 可以识别到 Noto Sans SC
        let root = crate::find_project_root();
        let font_path = root.join("fonts/NotoSansSC-Bold.ttf");
        println!("[GUI] Loading font from: {:?}", font_path);
        if font_path.exists() {
            let path_u16: Vec<u16> = font_path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
            let res = AddFontResourceExW(PCWSTR(path_u16.as_ptr()), FR_PRIVATE, None);
            println!("[GUI] AddFontResourceExW result: {}", res);
        } else {
            println!("[GUI] Warning: Font file not found!");
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
            hbrBackground: CreateSolidBrush(COLORREF(0x000000)), // Black for keystrokes
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

        CURRENT_CONFIG = Some(Arc::new(RwLock::new(initial_config)));

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            window_class, PCWSTR(std::ptr::null()), WS_POPUP,
            100, 100, 400, 120, None, None, instance, None,
        );
        SetLayeredWindowAttributes(hwnd, COLORREF(0xFFFFFF), 255, LWA_COLORKEY);

        let hwnd_ks = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            ks_class, PCWSTR(std::ptr::null()), WS_POPUP,
            0, 0, 800, 100, None, None, instance, None,
        );
        SetLayeredWindowAttributes(hwnd_ks, COLORREF(0x000000), 200, LWA_ALPHA | LWA_COLORKEY);

        let hwnd_learn = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            learn_class, PCWSTR(std::ptr::null()), WS_POPUP,
            0, 0, 400, 80, None, None, instance, None,
        );
        SetLayeredWindowAttributes(hwnd_learn, COLORREF(0x000000), 200, LWA_ALPHA | LWA_COLORKEY);

        std::thread::spawn(move || {
            println!("[GUI Thread] Event receiver thread started.");
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
                                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                                InvalidateRect(hwnd, None, BOOL(1));
                                UpdateWindow(hwnd); 
                            }
                        }
                    }
                    GuiEvent::MoveTo { x, y } => {
                        let state_ptr = std::ptr::addr_of_mut!(WINDOW_STATE);
                        if let Some(ref mut state) = *state_ptr { state.x = x; state.y = y; }
                        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y + 25, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                    }
                    GuiEvent::Keystroke(key) => {
                        let show = if let Some(ref arc) = CURRENT_CONFIG { arc.read().unwrap().appearance.show_keystrokes } else { false };
                        if !show { continue; }

                        let ks_ptr = std::ptr::addr_of_mut!(KEYSTROKE_STATE);
                        if let Some(ref mut state) = *ks_ptr {
                            state.keys.push(key);
                            if state.keys.len() > 10 { state.keys.remove(0); }
                            state.last_update = std::time::Instant::now();
                        } else {
                            *ks_ptr = Some(KeystrokeState { keys: vec![key], last_update: std::time::Instant::now() });
                        }
                        ShowWindow(hwnd_ks, SW_SHOWNOACTIVATE);
                        InvalidateRect(hwnd_ks, None, BOOL(1));
                        
                        // 定时清除
                        let timeout = if let Some(ref arc) = CURRENT_CONFIG { arc.read().unwrap().appearance.keystroke_timeout_ms } else { 1500 };
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(timeout));
                            unsafe {
                                let ks_ptr = std::ptr::addr_of_mut!(KEYSTROKE_STATE);
                                if let Some(ref state) = *ks_ptr {
                                    if state.last_update.elapsed().as_millis() >= timeout as u128 {
                                        ShowWindow(hwnd_ks, SW_HIDE);
                                    }
                                }
                            }
                        });
                    }
                    GuiEvent::ClearKeystrokes => {
                        unsafe {
                            let ks_ptr = std::ptr::addr_of_mut!(KEYSTROKE_STATE);
                            *ks_ptr = None;
                            ShowWindow(hwnd_ks, SW_HIDE);
                        }
                    }
                    GuiEvent::ShowLearning(word, hint) => {
                        let show = if let Some(ref arc) = CURRENT_CONFIG { arc.read().unwrap().appearance.learning_mode } else { false };
                        if !show { continue; }

                        let ln_ptr = std::ptr::addr_of_mut!(LEARNING_STATE);
                        *ln_ptr = Some(LearningState { word, hint, last_update: std::time::Instant::now() });
                        ShowWindow(hwnd_learn, SW_SHOWNOACTIVATE);
                        InvalidateRect(hwnd_learn, None, BOOL(1));

                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            unsafe {
                                let ln_ptr = std::ptr::addr_of_mut!(LEARNING_STATE);
                                if let Some(ref state) = *ln_ptr {
                                    if state.last_update.elapsed().as_secs() >= 3 {
                                        ShowWindow(hwnd_learn, SW_HIDE);
                                    }
                                }
                            }
                        });
                    }
                    GuiEvent::ApplyConfig(conf) => {
                        if let Some(ref arc) = CURRENT_CONFIG {
                            if let Ok(mut w) = arc.write() { *w = conf; }
                        }
                        InvalidateRect(hwnd, None, BOOL(1));
                        InvalidateRect(hwnd_ks, None, BOOL(1));
                        InvalidateRect(hwnd_learn, None, BOOL(1));
                    }
                    _ => {}
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
            
            let brush = CreateSolidBrush(COLORREF(0xFFFFFF));
            let _ = FillRect(mem_dc, &rect, brush);
            let _ = DeleteObject(brush);

            if let Some(ref state) = WINDOW_STATE {
                if let Some(ref arc) = CURRENT_CONFIG {
                    if let Ok(conf) = arc.read() {
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
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            
            let brush = CreateSolidBrush(COLORREF(0x000000));
            let _ = FillRect(hdc, &rect, brush);
            let _ = DeleteObject(brush);

            if let Some(ref state) = KEYSTROKE_STATE {
                if let Some(ref arc) = CURRENT_CONFIG {
                    if let Ok(conf) = arc.read() {
                        draw_keystrokes(hdc, hwnd, state, &conf);
                    }
                }
            }
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
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            
            let brush = CreateSolidBrush(COLORREF(0x000000));
            let _ = FillRect(hdc, &rect, brush);
            let _ = DeleteObject(brush);

            if let Some(ref state) = LEARNING_STATE {
                if let Some(ref arc) = CURRENT_CONFIG {
                    if let Ok(conf) = arc.read() {
                        draw_learning(hdc, hwnd, state, &conf);
                    }
                }
            }
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn draw_keystrokes(hdc: HDC, hwnd: HWND, state: &KeystrokeState, conf: &Config) {
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, COLORREF(0xFFFFFF));
    
    let font_name = HSTRING::from(&conf.appearance.candidate_text.font_family);
    let font_size = conf.appearance.keystroke_font_size as i32;
    let h_font = CreateFontW(
        -(font_size * 96 / 72), 0, 0, 0, 700, 0, 0, 0, DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32, PCWSTR(font_name.as_ptr())
    );
    SelectObject(hdc, h_font);

    let text = state.keys.join("  ");
    let text_u16: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    GetTextExtentPoint32W(hdc, &text_u16, &mut size);

    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);
    let margin_x = conf.appearance.keystroke_margin_x;
    let margin_y = conf.appearance.keystroke_margin_y;

    let (win_x, win_y) = match conf.appearance.keystroke_anchor.as_str() {
        "bottom_right" => (screen_w - size.cx - margin_x - 40, screen_h - size.cy - margin_y - 60),
        "bottom_left" => (margin_x, screen_h - size.cy - margin_y - 60),
        "top_right" => (screen_w - size.cx - margin_x - 40, margin_y),
        _ => (margin_x, margin_y),
    };

    let final_w = size.cx + 40;
    let final_h = size.cy + 20;
    let _ = SetWindowPos(hwnd, HWND_TOPMOST, win_x, win_y, final_w, final_h, SWP_NOACTIVATE);
    
    TextOutW(hdc, 20, 10, &text_u16);
    let _ = DeleteObject(h_font);
}

unsafe fn draw_learning(hdc: HDC, hwnd: HWND, state: &LearningState, conf: &Config) {
    SetBkMode(hdc, TRANSPARENT);
    
    let font_name = HSTRING::from(&conf.appearance.candidate_text.font_family);
    let h_font_word = CreateFontW(
        -(conf.appearance.learning_font_size as i32 * 96 / 72), 0, 0, 0, 700, 0, 0, 0, DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32, PCWSTR(font_name.as_ptr())
    );

    let h_font_hint = CreateFontW(
        -16, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32, PCWSTR(font_name.as_ptr())
    );

    SelectObject(hdc, h_font_word);
    SetTextColor(hdc, COLORREF(0x00FFFF)); // Cyan for word
    let word_u16: Vec<u16> = state.word.encode_utf16().collect();
    let mut word_size = SIZE::default();
    GetTextExtentPoint32W(hdc, &word_u16, &mut word_size);

    SelectObject(hdc, h_font_hint);
    SetTextColor(hdc, COLORREF(0xCCCCCC)); // Gray for hint
    let hint_u16: Vec<u16> = state.hint.encode_utf16().collect();
    let mut hint_size = SIZE::default();
    GetTextExtentPoint32W(hdc, &hint_u16, &mut hint_size);

    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let margin_x = conf.appearance.learning_margin_x;
    let margin_y = conf.appearance.learning_margin_y;
    
    let final_w = word_size.cx.max(hint_size.cx) + 40;
    let final_h = word_size.cy + hint_size.cy + 30;

    let (win_x, win_y) = match conf.appearance.learning_anchor.as_str() {
        "top_right" => (screen_w - final_w - margin_x, margin_y),
        "top_left" => (margin_x, margin_y),
        _ => (screen_w - final_w - margin_x, margin_y),
    };

    let _ = SetWindowPos(hwnd, HWND_TOPMOST, win_x, win_y, final_w, final_h, SWP_NOACTIVATE);
    
    SelectObject(hdc, h_font_word);
    TextOutW(hdc, 20, 10, &word_u16);
    SelectObject(hdc, h_font_hint);
    TextOutW(hdc, 20, 15 + word_size.cy, &hint_u16);

    let _ = DeleteObject(h_font_word);
    let _ = DeleteObject(h_font_hint);
}

unsafe fn draw_content(hdc: HDC, hwnd: HWND, state: &WindowState, conf: &Config) {
    SetBkMode(hdc, TRANSPARENT);
    
    let pad_x = conf.appearance.window_padding_x;
    let pad_y = conf.appearance.window_padding_y;
    let row_space = conf.appearance.row_spacing as i32;

    // 尝试加载本地粗体字，如果名字匹配不上，系统会自动回退到类似字体的 Bold 版本
    let py_font_name = HSTRING::from(&conf.appearance.pinyin_text.font_family);
    let h_font_py = CreateFontW(
        -(conf.appearance.pinyin_text.font_size as i32 * 96 / 72),
        0, 0, 0, 700, // 700 = Bold
        0, 0, 0, DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32,
        PCWSTR(py_font_name.as_ptr())
    );

    let cand_font_name = HSTRING::from(&conf.appearance.candidate_text.font_family);
    let h_font_cand = CreateFontW(
        -(conf.appearance.candidate_text.font_size as i32 * 96 / 72),
        0, 0, 0, 700, // 700 = Bold
        0, 0, 0, DEFAULT_CHARSET.0 as u32,
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

    // --- 开始绘制拼音行 ---
    SelectObject(hdc, h_font_py);
    let py_color = parse_color_win(&conf.appearance.pinyin_text.color);
    SetTextColor(hdc, py_color);
    let py_u16: Vec<u16> = state.pinyin.encode_utf16().collect();
    TextOutW(hdc, pad_x, pad_y, &py_u16);

    let mut py_size = SIZE::default();
    GetTextExtentPoint32W(hdc, &py_u16, &mut py_size);

    // --- 开始绘制候选词行 ---
    let cand_y = pad_y + py_size.cy + row_space;
    let mut x_cursor = pad_x;
    
    let cand_color = parse_color_win(&conf.appearance.candidate_text.color);
    let hint_color = parse_color_win(&conf.appearance.hint_text.color);
    let highlight_color = parse_color_win(&conf.appearance.window_highlight_color);
    
    let page_size = conf.appearance.page_size;
    let start = (state.selected / page_size) * page_size;
    let end = (start + page_size).min(state.candidates.len());

    let mut max_row_height = 0;

    for i in start..end {
        let is_selected = i == state.selected;
        
        // A. 绘制序号
        SelectObject(hdc, h_font_cand);
        let idx_text = format!("{}.", i - start + 1);
        let idx_u16: Vec<u16> = idx_text.encode_utf16().collect();
        let mut idx_size = SIZE::default();
        GetTextExtentPoint32W(hdc, &idx_u16, &mut idx_size);
        
        SetTextColor(hdc, if is_selected { highlight_color } else { cand_color });
        TextOutW(hdc, x_cursor, cand_y, &idx_u16);
        x_cursor += idx_size.cx + 4;

        // B. 绘制词语
        let cand_text = &state.candidates[i];
        let cand_u16: Vec<u16> = cand_text.encode_utf16().collect();
        let mut text_size = SIZE::default();
        GetTextExtentPoint32W(hdc, &cand_u16, &mut text_size);
        
        if is_selected {
            // 选中项背景色
            let h_brush = CreateSolidBrush(COLORREF(0xF0F0F0)); // 浅灰色高亮
            let r = RECT { left: x_cursor - 2, top: cand_y, right: x_cursor + text_size.cx + 2, bottom: cand_y + text_size.cy };
            let _ = FillRect(hdc, &r, h_brush);
            let _ = DeleteObject(h_brush);
        }
        
        TextOutW(hdc, x_cursor, cand_y, &cand_u16);
        x_cursor += text_size.cx;

        // C. 绘制提示 (辅助码)
        if let Some(hint) = state.hints.get(i) {
            if !hint.is_empty() {
                SelectObject(hdc, h_font_hint);
                let hint_u16: Vec<u16> = hint.encode_utf16().collect();
                let mut hint_size = SIZE::default();
                GetTextExtentPoint32W(hdc, &hint_u16, &mut hint_size);
                
                SetTextColor(hdc, if is_selected { highlight_color } else { hint_color });
                TextOutW(hdc, x_cursor + 4, cand_y + (text_size.cy - hint_size.cy), &hint_u16);
                x_cursor += hint_size.cx + 8;
            }
        }
        
        x_cursor += conf.appearance.item_spacing as i32;
        max_row_height = max_row_height.max(text_size.cy);
    }

    // 清理
    let _ = DeleteObject(h_font_py);
    let _ = DeleteObject(h_font_cand);
    let _ = DeleteObject(h_font_hint);
    
    // 动态调整窗口尺寸
    let final_w = (x_cursor + pad_x).max(300);
    let final_h = cand_y + max_row_height + pad_y;
    
    let mut current_rect = RECT::default();
    let _ = GetWindowRect(hwnd, &mut current_rect);
    let cur_w = current_rect.right - current_rect.left;
    let cur_h = current_rect.bottom - current_rect.top;
    
    if final_w != cur_w || final_h != cur_h {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, final_w, final_h, SWP_NOMOVE | SWP_NOACTIVATE);
    }
}

fn parse_color_win(s: &str) -> COLORREF {
    if s.starts_with('#') && s.len() == 7 {
        let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(0);
        let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(0);
        let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(0);
        return COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16));
    }
    COLORREF(0)
}