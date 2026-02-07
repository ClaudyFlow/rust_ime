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

        let painter = CandidatePainter::new();
        let mut current_config = initial_config;

        std::thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                match event {
                    GuiEvent::ApplyConfig(conf) => { current_config = conf; }
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
                                let page_size = current_config.appearance.page_size;
                                let start = (state.selected / page_size) * page_size;
                                let end = (start + page_size).min(state.candidates.len());
                                
                                let current_candidates = state.candidates[start..end].to_vec();
                                let current_hints = if state.hints.len() >= end {
                                    state.hints[start..end].to_vec()
                                } else {
                                    vec![String::new(); current_candidates.len()]
                                };

                                let pixels = painter.draw(
                                    &state.pinyin, 
                                    &current_candidates, 
                                    &current_hints, 
                                    state.selected % page_size, 
                                    &current_config
                                );
                                
                                let dynamic_width = (pixels.len() / (painter.height as usize * 4)) as u32;
                                
                                update_window_pixels(hwnd, &pixels, dynamic_width, painter.height);
                                let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, dynamic_width as i32, painter.height as i32, SWP_NOMOVE | SWP_NOACTIVATE);
                                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                            }
                        }
                    }
                    GuiEvent::MoveTo { x, y } => {
                        unsafe {
                            if let Some(ref mut state) = WINDOW_STATE { state.x = x; state.y = y; }
                            let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y + 20, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                        }
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
    let mut size = SIZE { cx: width as i32, cy: height as i32 };
    let mut pt_src = POINT { x: 0, y: 0 };
    let mut blend = BLENDFUNCTION {
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
