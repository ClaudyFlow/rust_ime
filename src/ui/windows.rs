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
static mut CURRENT_CONFIG: Option<Arc<RwLock<Config>>> = None;

struct WindowState {
    pinyin: String,
    candidates: Vec<String>,
    hints: Vec<String>,
    selected: usize,
    x: i32,
    y: i32,
}

pub fn start_gui(rx: Receiver<GuiEvent>, initial_config: Config) {
    let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap() };
    let window_class = PCWSTR("RustImeGui\0".encode_utf16().collect::<Vec<u16>>().as_ptr());

    unsafe {
        // 加载本地字体，让 GDI 可以识别到 Noto Sans SC
        let root = crate::find_project_root();
        let font_path = root.join("fonts/NotoSansSC-Bold.ttf");
        if font_path.exists() {
            let path_u16: Vec<u16> = font_path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AddFontResourceExW(PCWSTR(path_u16.as_ptr()), FR_PRIVATE, None);
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

        CURRENT_CONFIG = Some(Arc::new(RwLock::new(initial_config)));

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            window_class, PCWSTR(std::ptr::null()), WS_POPUP | WS_BORDER,
            100, 100, 400, 120, None, None, instance, None,
        );

        std::thread::spawn(move || {
            while let Ok(mut event) = rx.recv() {
                if matches!(event, GuiEvent::Update { .. }) {
                    while let Ok(next) = rx.try_recv() {
                        if matches!(next, GuiEvent::Update { .. }) { event = next; } else { break; }
                    }
                }

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
                    GuiEvent::ApplyConfig(conf) => {
                        if let Some(ref arc) = CURRENT_CONFIG {
                            if let Ok(mut w) = arc.write() { *w = conf; }
                        }
                        InvalidateRect(hwnd, None, BOOL(1));
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