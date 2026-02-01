use gtk4::prelude::*;
use gtk4::{Window, Label, Box, Orientation, CssProvider};
use gdk4::Display;
use gtk4_layer_shell::{LayerShell, Layer, Edge, KeyboardMode};
use std::sync::mpsc::Receiver;
use glib::MainContext;
use crate::config::Config;

#[derive(Debug)]
pub enum GuiEvent {
    Update {
        pinyin: String,
        candidates: Vec<String>,
        hints: Vec<String>,
        selected: usize,
        sentence: String,
    },
    #[allow(dead_code)]
    MoveTo { x: i32, y: i32 },
    Keystroke(String),
    #[allow(dead_code)]
    ShowLearning(String, String), // 汉字, 提示
    #[allow(dead_code)]
    ClearKeystrokes,
    ApplyConfig(Config),
    #[allow(dead_code)]
    Exit,
}

pub fn start_gui(rx: Receiver<GuiEvent>, initial_config: Config) {
    if gtk4::init().is_err() {
        eprintln!("[GUI] Failed to initialize GTK4.");
        return;
    }

    let is_layer_supported = gtk4_layer_shell::is_supported();

    // --- 1. 传统窗口 (Traditional Window) ---
    let window = Window::builder().title("Rust IME Candidates").decorated(false).can_focus(false).focusable(false).resizable(false).build();
    if is_layer_supported {
        window.init_layer_shell();
        window.set_namespace("rust-ime-candidates");
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::None);
    }
    window.add_css_class("ime-window");
    let main_box = Box::new(Orientation::Horizontal, 8);
    main_box.set_widget_name("main-container");
    window.set_child(Some(&main_box));

    // 左侧垂直容器：上方拼音，下方完整句子
    let left_box = Box::new(Orientation::Vertical, 2);
    main_box.append(&left_box);

    let pinyin_label = Label::new(None);
    pinyin_label.set_widget_name("pinyin-label");
    left_box.append(&pinyin_label);

    let sentence_label = Label::new(None);
    sentence_label.set_widget_name("sentence-label");
    left_box.append(&sentence_label);

    let candidates_box = Box::new(Orientation::Horizontal, 12);
    candidates_box.set_widget_name("candidates-box");
    main_box.append(&candidates_box);

    // --- 2. 极客窗口 (Modern Window) ---
    let modern_window = Window::builder().title("Modern Candidates").decorated(false).can_focus(false).focusable(false).resizable(false).build();
    if is_layer_supported {
        modern_window.init_layer_shell();
        modern_window.set_namespace("rust-ime-modern");
        modern_window.set_layer(Layer::Overlay);
        modern_window.set_keyboard_mode(KeyboardMode::None);
    }
    modern_window.add_css_class("modern-window");
    let modern_main_box = Box::new(Orientation::Vertical, 10);
    modern_main_box.set_widget_name("modern-container");
    modern_window.set_child(Some(&modern_main_box));
    let modern_pinyin_label = Label::new(None);
    modern_pinyin_label.set_widget_name("modern-pinyin");
    modern_main_box.append(&modern_pinyin_label);
    let modern_candidates_box = Box::new(Orientation::Vertical, 6);
    modern_candidates_box.set_widget_name("modern-candidates-box");
    modern_main_box.append(&modern_candidates_box);

    // --- 3. 按键回显窗口 ---
    let key_window = Window::builder().title("Keystroke Display").decorated(false).can_focus(false).focusable(false).resizable(false).build();
    if is_layer_supported {
        key_window.init_layer_shell();
        key_window.set_namespace("rust-ime-keystrokes");
        key_window.set_layer(Layer::Overlay);
        key_window.set_keyboard_mode(KeyboardMode::None);
    }
    key_window.add_css_class("keystroke-window");
    let key_box = Box::new(Orientation::Horizontal, 6);
    key_box.set_widget_name("keystroke-container");
    key_window.set_child(Some(&key_box));

    let css_provider = CssProvider::new();
    if let Some(display) = Display::default() {
        gtk4::style_context_add_provider_for_display(&display, &css_provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    // --- 配置应用逻辑 ---
    let apply_style = move |conf: &Config, css: &CssProvider, w: &Window, mw: &Window, kw: &Window| {
        let app = &conf.appearance;
        
        let css_data = format!(r#"
            window.ime-window, window.modern-window, window.keystroke-window {{ background-color: transparent; }}
            
            /* 传统样式 */
            #main-container {{
                background-color: {cand_bg};
                border: 1px solid rgba(255, 255, 255, 0.1);
                border-radius: 12px;
                padding: 8px 14px;
                box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
            }}
            #pinyin-label {{
                color: #0071e3;
                font-size: {cand_font}pt;
                font-weight: 700;
            }}
            #sentence-label {{
                color: rgba(255, 255, 255, 0.7);
                font-size: {s_font}pt;
                font-weight: 400;
                margin-top: 2px;
            }}
            .candidate-item {{ padding: 4px 10px; border-radius: 6px; }}
            .candidate-selected {{ background-color: #0071e3; }}
            .candidate-text {{ color: #ffffff; font-size: {cand_font}pt; font-weight: 500; }}
            .hint-text {{ color: rgba(255, 255, 255, 0.4); font-size: 10pt; margin-left: 8px; }}
            .index {{ font-size: 10pt; color: rgba(255, 255, 255, 0.4); margin-right: 6px; }}

            /* 极客(Modern)卡片样式 */
            #modern-container {{
                background-color: transparent;
                border: none;
                padding: 0;
                box-shadow: none;
            }}
            #modern-pinyin {{
                margin-bottom: 0;
                padding-bottom: 0;
            }}
            .modern-item {{
                padding: 10px 18px;
                border-radius: 12px;
                margin: 6px 0;
                background: {m_bg};
                border: 1px solid rgba(255, 255, 255, 0.08);
                box-shadow: 0 6px 16px rgba(0, 0, 0, 0.45);
            }}
            .modern-selected {{
                background-color: {m_text};
                box-shadow: 0 0 20px {m_text}66;
                border: 1px solid {m_text};
            }}
            .modern-text {{ color: {m_text}; font-size: {m_font}pt; font-weight: 700; }}
            .modern-selected .modern-text {{ color: #000000; }}
            .m-index {{ font-size: 10pt; font-weight: 900; color: rgba(255, 255, 255, 0.2); margin-right: 12px; }}
            .modern-selected .m-index {{ color: rgba(0, 0, 0, 0.4); }}

            /* 按键回显 */
            #keystroke-container {{
                background-color: {key_bg};
                border-radius: 14px;
                padding: 8px 14px;
            }}
            .key-label {{
                background: linear-gradient(to bottom, #4a4a4a, #2c2c2c);
                color: #f5f5f7;
                font-size: {key_font}pt;
                font-weight: 600;
                padding: 6px 14px;
                border-radius: 8px;
                margin: 3px;
            }}
        "#, 
        cand_bg = app.candidate_bg_color,
        cand_font = app.candidate_font_size,
        s_font = app.candidate_font_size - 2,
        m_bg = app.modern_cand_bg_color,
        m_text = app.modern_cand_text_color,
        m_font = app.modern_cand_font_size,
        key_bg = app.keystroke_bg_color,
        key_font = app.keystroke_font_size);
        
        css.load_from_data(&css_data);

        if gtk4_layer_shell::is_supported() {
            // 传统位置
            match app.candidate_anchor.as_str() {
                "top" => { w.set_anchor(Edge::Bottom, false); w.set_anchor(Edge::Top, true); }
                _ => { w.set_anchor(Edge::Top, false); w.set_anchor(Edge::Bottom, true); }
            }
            w.set_margin(Edge::Bottom, app.candidate_margin_y);
            w.set_margin(Edge::Top, app.candidate_margin_y);
            w.set_margin(Edge::Left, app.candidate_margin_x);

            // Modern 位置
            mw.set_anchor(Edge::Bottom, false); mw.set_anchor(Edge::Top, false);
            mw.set_anchor(Edge::Left, false); mw.set_anchor(Edge::Right, false);
            match app.modern_cand_anchor.as_str() {
                "top_left" => { mw.set_anchor(Edge::Top, true); mw.set_anchor(Edge::Left, true); }
                "top_right" => { mw.set_anchor(Edge::Top, true); mw.set_anchor(Edge::Right, true); }
                "bottom_right" => { mw.set_anchor(Edge::Bottom, true); mw.set_anchor(Edge::Right, true); }
                _ => { mw.set_anchor(Edge::Bottom, true); mw.set_anchor(Edge::Left, true); }
            }
            mw.set_margin(Edge::Bottom, app.modern_cand_margin_y);
            mw.set_margin(Edge::Top, app.modern_cand_margin_y);
            mw.set_margin(Edge::Left, app.modern_cand_margin_x);
            mw.set_margin(Edge::Right, app.modern_cand_margin_x);

            // 按键回显
            kw.set_anchor(Edge::Bottom, false); kw.set_anchor(Edge::Top, false);
            match app.keystroke_anchor.as_str() {
                "top_right" => { kw.set_anchor(Edge::Top, true); kw.set_anchor(Edge::Right, true); }
                _ => { kw.set_anchor(Edge::Bottom, true); kw.set_anchor(Edge::Right, true); }
            }
            kw.set_margin(Edge::Bottom, app.keystroke_margin_y);
            kw.set_margin(Edge::Right, app.keystroke_margin_x);
        }
    };

    apply_style(&initial_config, &css_provider, &window, &modern_window, &key_window);

    let (tx, gtk_rx) = MainContext::channel::<GuiEvent>(glib::Priority::default());
    std::thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            if tx.send(msg).is_err() { break; }
        }
    });

    let window_c = window.clone();
    let modern_window_c = modern_window.clone();
    let key_window_c = key_window.clone();
    let pinyin_label_c = pinyin_label.clone();
    let sentence_label_c = sentence_label.clone();
    let candidates_box_c = candidates_box.clone();
    let modern_pinyin_c = modern_pinyin_label.clone();
    let modern_candidates_c = modern_candidates_box.clone();
    let key_box_c = key_box.clone();
    let css_p_c = css_provider.clone();
    let mut current_config = initial_config;

    gtk_rx.attach(None, move |event| {
        match event {
            GuiEvent::ApplyConfig(conf) => {
                apply_style(&conf, &css_p_c, &window_c, &modern_window_c, &key_window_c);
                current_config = conf;
            }
            GuiEvent::Update { pinyin, candidates, hints, selected, sentence } => {
                let show_trad = current_config.appearance.show_candidates;
                let show_modern = current_config.appearance.show_modern_candidates;

                if pinyin.is_empty() && candidates.is_empty() {
                    window_c.set_opacity(0.0); modern_window_c.set_opacity(0.0);
                    return glib::Continue(true);
                }

                // 1. 更新传统窗口
                if show_trad {
                    window_c.set_opacity(1.0);
                    pinyin_label_c.set_text(&pinyin);
                    sentence_label_c.set_text(&sentence);
                    while let Some(child) = candidates_box_c.first_child() { candidates_box_c.remove(&child); }
                    let start = (selected / 10) * 10;
                    let end = (start + 10).min(candidates.len());
                    for i in start..end {
                        let item = Box::new(Orientation::Horizontal, 0);
                        item.add_css_class("candidate-item");
                        if i == selected { item.add_css_class("candidate-selected"); }
                        
                        let idx_lbl = Label::new(Some(&format!("{}", (i % 10) + 1)));
                        idx_lbl.add_css_class("index");
                        let txt_lbl = Label::new(Some(&candidates[i]));
                        txt_lbl.add_css_class("candidate-text");
                        
                        item.append(&idx_lbl);
                        item.append(&txt_lbl);

                        // 加入 Hints 显示
                        if let Some(hint) = hints.get(i) {
                            if !hint.is_empty() {
                                let hint_lbl = Label::new(Some(hint));
                                hint_lbl.add_css_class("hint-text");
                                item.append(&hint_lbl);
                            }
                        }
                        
                        candidates_box_c.append(&item);
                    }
                } else { window_c.set_opacity(0.0); }

                // 2. 更新“卡片式” (Modern) 窗口
                if show_modern {
                    modern_window_c.set_opacity(1.0);
                    // 隐藏拼音
                    modern_pinyin_c.set_text(""); 
                    while let Some(child) = modern_candidates_c.first_child() { modern_candidates_c.remove(&child); }
                    let start = (selected / 10) * 10;
                    let end = (start + 10).min(candidates.len());
                    for i in start..end {
                        let item = Box::new(Orientation::Horizontal, 0);
                        item.add_css_class("modern-item");
                        if i == selected { item.add_css_class("modern-selected"); }
                        
                        let idx_lbl = Label::new(Some(&format!("{}", (i % 10) + 1)));
                        idx_lbl.add_css_class("m-index");
                        let txt_lbl = Label::new(Some(&candidates[i]));
                        txt_lbl.add_css_class("modern-text");
                        
                        item.append(&idx_lbl);
                        item.append(&txt_lbl);

                        // 加入 Hints 显示
                        if let Some(hint) = hints.get(i) {
                            if !hint.is_empty() {
                                let hint_lbl = Label::new(Some(hint));
                                hint_lbl.add_css_class("hint-text");
                                item.append(&hint_lbl);
                            }
                        }
                        
                        modern_candidates_c.append(&item);
                    }
                } else { modern_window_c.set_opacity(0.0); }
            },
            GuiEvent::Keystroke(key_name) => {
                let label = Label::new(Some(&key_name));
                label.add_css_class("key-label");
                key_box_c.append(&label);
                key_window_c.set_opacity(1.0);
                let kb_weak = key_box_c.downgrade();
                let label_weak = label.downgrade();
                let kw_weak = key_window_c.downgrade();
                glib::timeout_add_local(std::time::Duration::from_millis(current_config.appearance.keystroke_timeout_ms), move || {
                    if let (Some(kb), Some(l)) = (kb_weak.upgrade(), label_weak.upgrade()) {
                        kb.remove(&l);
                        if kb.first_child().is_none() { if let Some(kw) = kw_weak.upgrade() { kw.set_opacity(0.0); } }
                    }
                    glib::Continue(false)
                });
            },
            _ => {}
        }
        glib::Continue(true)
    });

    window.present(); modern_window.present(); key_window.present();
    window.set_opacity(0.0); modern_window.set_opacity(0.0); key_window.set_opacity(0.0);
    glib::MainLoop::new(None, false).run();
}
