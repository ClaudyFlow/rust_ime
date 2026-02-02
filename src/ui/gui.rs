use gtk4::prelude::*;
use gtk4::{Window, Label, Box, Orientation, CssProvider};
use gdk4::Display;
use gtk4_layer_shell::{LayerShell, Layer, Edge, KeyboardMode};
use std::sync::mpsc::Receiver;
use glib::MainContext;
use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
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

use std::rc::Rc;
use std::cell::RefCell;
use glib::SourceId;

#[derive(Debug)]
struct DisplayedKey {
    label: Label,
    last_active: std::time::Instant,
}

struct KeystrokeController {
    box_: Box,
    window: Window,
    displayed_keys: RefCell<Vec<DisplayedKey>>,
    timeout_ms: RefCell<u64>,
    max_keys: usize,
}

impl KeystrokeController {
    fn new(box_: Box, window: Window, initial_timeout: u64) -> Rc<Self> {
        let controller = Rc::new(Self {
            box_,
            window,
            displayed_keys: RefCell::new(Vec::new()),
            timeout_ms: RefCell::new(initial_timeout),
            max_keys: 15,
        });
        
        // 启动全局清理定时器
        controller.clone().start_cleanup_timer();
        
        controller
    }

    fn show_key(&self, key: &str) {
        // 安全检查：防止空键名
        if key.is_empty() {
            return;
        }
        
        let mut keys = self.displayed_keys.borrow_mut();
        
        // 创建新的按键显示
        let label = Label::new(Some(key));
        label.add_css_class("key-label");
        self.box_.append(&label);
        
        let displayed = DisplayedKey {
            label,
            last_active: std::time::Instant::now(),
        };
        
        keys.push(displayed);
        
        // 保持最多max_keys个按键
        while keys.len() > self.max_keys {
            let old = keys.remove(0);
            self.box_.remove(&old.label);
        }
        
        // 确保窗口可见
        self.window.set_opacity(1.0);
    }

    fn remove_expired(&self) {
        let mut keys = self.displayed_keys.borrow_mut();
        let timeout_ms = *self.timeout_ms.borrow();
        let now = std::time::Instant::now();
        
        // 收集过期的索引（从后往前，避免索引错位）
        let mut expired_indices = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            if now.duration_since(key.last_active) > std::time::Duration::from_millis(timeout_ms) {
                expired_indices.push(i);
            }
        }
        
        // 移除过期的按键（从后往前）
        for i in expired_indices.into_iter().rev() {
            let removed = keys.remove(i);
            self.box_.remove(&removed.label);
        }
        
        // 如果没有按键了，隐藏窗口
        if keys.is_empty() {
            self.window.set_opacity(0.0);
        }
    }

    fn clear(&self) {
        // 清除所有按键显示
        let mut keys = self.displayed_keys.borrow_mut();
        
        // 强制移除所有子元素
        while let Some(child) = self.box_.first_child() {
            self.box_.remove(&child);
        }
        
        // 清空按键数组
        keys.clear();
        
        // 强制隐藏窗口
        self.window.set_opacity(0.0);
        self.window.hide();
        
        // 重新显示窗口（确保状态重置）
        self.window.show();
    }

    fn update_config(&self, timeout_ms: u64) {
        *self.timeout_ms.borrow_mut() = timeout_ms;
    }
    
    fn start_cleanup_timer(self: Rc<Self>) {
        glib::timeout_add_local(
            std::time::Duration::from_millis(100),
            move || {
                self.remove_expired();
                glib::Continue(true)
            },
        );
    }
}

struct LearningController {
    window: Window,
    word_label: Label,
    hint_label: Label,
    timeout: RefCell<Option<SourceId>>,
}

impl LearningController {
    fn new(window: Window, word_label: Label, hint_label: Label, _interval_sec: u64) -> Rc<Self> {
        Rc::new(Self {
            window,
            word_label,
            hint_label,
            timeout: RefCell::new(None),
        })
    }

    fn show(&self, word: &str, hint: &str) {
        self.word_label.set_text(word);
        self.hint_label.set_text(hint);
        self.window.show();
        self.window.set_opacity(1.0);
        
        // 只清除引用，不调用remove避免panic
        *self.timeout.borrow_mut() = None;

        let win_weak = self.window.downgrade();
        let id = glib::timeout_add_local(
            std::time::Duration::from_secs(5),
            move || {
                if let Some(w) = win_weak.upgrade() {
                    w.set_opacity(0.0);
                    w.hide();
                }
                glib::Continue(false)
            }
        );
        *self.timeout.borrow_mut() = Some(id);
    }

    fn clear(&self) {
        // 清除定时器引用，让GLib自动管理
        *self.timeout.borrow_mut() = None;
        // 立即隐藏窗口
        self.window.set_opacity(0.0);
        self.window.hide();
        // 清空文本
        self.word_label.set_text("");
        self.hint_label.set_text("");
    }
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

    let pinyin_label = Label::new(None);
    pinyin_label.set_widget_name("pinyin-label");
    main_box.append(&pinyin_label);

    let sentence_label = Label::new(None);
    sentence_label.set_widget_name("sentence-label");
    main_box.append(&sentence_label);

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

    // --- 3. On-Screen Keystrokes (按键显示) 窗口 ---
    let key_window = Window::builder().title("On-Screen Keystrokes (按键显示)").decorated(false).can_focus(false).focusable(false).resizable(false).build();
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

    // --- 4. 学习模式 (Learning Mode) 窗口 ---
    let learn_window = Window::builder().title("Learning Mode").decorated(false).can_focus(false).focusable(false).resizable(false).build();
    if is_layer_supported {
        learn_window.init_layer_shell();
        learn_window.set_namespace("rust-ime-learning");
        learn_window.set_layer(Layer::Overlay);
        learn_window.set_keyboard_mode(KeyboardMode::None);
    }
    learn_window.add_css_class("learning-window");
    let learn_box = Box::new(Orientation::Vertical, 4);
    learn_box.set_widget_name("learning-container");
    learn_window.set_child(Some(&learn_box));
    
    let learn_word_label = Label::new(None);
    learn_word_label.set_widget_name("learning-word");
    learn_box.append(&learn_word_label);
    
    let learn_hint_label = Label::new(None);
    learn_hint_label.set_widget_name("learning-hint");
    learn_box.append(&learn_hint_label);

    let css_provider = CssProvider::new();
    if let Some(display) = Display::default() {
        gtk4::style_context_add_provider_for_display(&display, &css_provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    // --- 配置应用逻辑 ---
    let apply_style = move |conf: &Config, css: &CssProvider, w: &Window, mw: &Window, kw: &Window, lw: &Window| {
        let app = &conf.appearance;
        
        let css_data = format!(r#"
            window.ime-window, window.modern-window, window.keystroke-window, window.learning-window {{ background-color: transparent; }}
            
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
                margin-right: 4px;
            }}
            #sentence-label {{
                color: rgba(255, 255, 255, 0.7);
                font-size: {s_font}pt;
                font-weight: 400;
                margin-right: 12px;
                padding-right: 12px;
                border-right: 1px solid rgba(255, 255, 255, 0.1);
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

            /* 学习模式样式 */
            window.learning-window {{
            }}
            #learning-container {{
                background-color: {key_bg};
                border-radius: 16px;
                padding: 12px 20px;
                box-shadow: 0 10px 40px rgba(0,0,0,0.5);
                border: 1px solid rgba(255,255,255,0.1);
            }}
            #learning-word {{
                color: #f5f5f7;
                font-size: 24pt;
                font-weight: 800;
            }}
            #learning-hint {{
                color: rgba(255,255,255,0.6);
                font-size: 10pt;
                font-weight: 400;
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

            // 学习模式
            lw.set_anchor(Edge::Top, false); lw.set_anchor(Edge::Bottom, false);
            lw.set_anchor(Edge::Left, false); lw.set_anchor(Edge::Right, false);
            match app.learning_anchor.as_str() {
                "top_left" => { lw.set_anchor(Edge::Top, true); lw.set_anchor(Edge::Left, true); }
                "bottom_left" => { lw.set_anchor(Edge::Bottom, true); lw.set_anchor(Edge::Left, true); }
                "bottom_right" => { lw.set_anchor(Edge::Bottom, true); lw.set_anchor(Edge::Right, true); }
                _ => { lw.set_anchor(Edge::Top, true); lw.set_anchor(Edge::Right, true); }
            }
            lw.set_margin(Edge::Top, app.learning_margin_y);
            lw.set_margin(Edge::Bottom, app.learning_margin_y);
            lw.set_margin(Edge::Right, app.learning_margin_x);
            lw.set_margin(Edge::Left, app.learning_margin_x);
        }
        lw.set_opacity(0.0);
    };

    apply_style(&initial_config, &css_provider, &window, &modern_window, &key_window, &learn_window);

    let ks_controller = KeystrokeController::new(key_box.clone(), key_window.clone(), initial_config.appearance.keystroke_timeout_ms);
    let learn_controller = LearningController::new(learn_window.clone(), learn_word_label.clone(), learn_hint_label.clone(), initial_config.appearance.learning_interval_sec);

    let (tx, gtk_rx) = MainContext::channel::<GuiEvent>(glib::Priority::default());
    std::thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            if tx.send(msg).is_err() { break; }
        }
    });

    let window_c = window.clone();
    let modern_window_c = modern_window.clone();
    let pinyin_label_c = pinyin_label.clone();
    let sentence_label_c = sentence_label.clone();
    let candidates_box_c = candidates_box.clone();
    let modern_pinyin_c = modern_pinyin_label.clone();
    let modern_candidates_c = modern_candidates_box.clone();

    let css_p_c = css_provider.clone();
    let mut current_config = initial_config;

    let ks_c = ks_controller.clone();
    let learn_c = learn_controller.clone();

    gtk_rx.attach(None, move |event| {
        match event {
            GuiEvent::ApplyConfig(conf) => {
                apply_style(&conf, &css_p_c, &window_c, &modern_window_c, &ks_c.window, &learn_c.window);
                ks_c.update_config(conf.appearance.keystroke_timeout_ms);
                current_config = conf;
                if !current_config.appearance.learning_mode {
                    learn_c.clear();
                }
            }
            GuiEvent::ShowLearning(word, hint) => {
                if current_config.appearance.learning_mode {
                    learn_c.show(&word, &hint);
                }
            }
            GuiEvent::Keystroke(key_name) => {
                ks_c.show_key(&key_name);
            }
            GuiEvent::ClearKeystrokes => {
                ks_c.clear();
            }
            GuiEvent::Update { pinyin, candidates, hints, selected, sentence } => {
                let show_trad = current_config.appearance.show_candidates;
                let show_modern = current_config.appearance.show_modern_candidates;

                if pinyin.is_empty() && candidates.is_empty() {
                    window_c.set_opacity(0.0); modern_window_c.set_opacity(0.0);
                    return glib::Continue(true);
                }

                if show_trad {
                    window_c.set_opacity(1.0);
                    pinyin_label_c.set_text(&pinyin);
                    sentence_label_c.set_text(&sentence);
                    if sentence.is_empty() { sentence_label_c.set_opacity(0.0); } else { sentence_label_c.set_opacity(1.0); }

                    while let Some(child) = candidates_box_c.first_child() { candidates_box_c.remove(&child); }
                    let start = (selected / current_config.appearance.page_size) * current_config.appearance.page_size;
                    let end = (start + current_config.appearance.page_size).min(candidates.len());
                    for i in start..end {
                        let item = Box::new(Orientation::Horizontal, 0);
                        item.add_css_class("candidate-item");
                        if i == selected { item.add_css_class("candidate-selected"); }
                        let idx_lbl = Label::new(Some(&format!("{}", (i % current_config.appearance.page_size) + 1)));
                        idx_lbl.add_css_class("index");
                        let txt_lbl = Label::new(Some(&candidates[i]));
                        txt_lbl.add_css_class("candidate-text");
                        item.append(&idx_lbl);
                        item.append(&txt_lbl);
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

                if show_modern {
                    modern_window_c.set_opacity(1.0);
                    modern_pinyin_c.set_text(""); 
                    while let Some(child) = modern_candidates_c.first_child() { modern_candidates_c.remove(&child); }
                    let start = (selected / current_config.appearance.page_size) * current_config.appearance.page_size;
                    let end = (start + current_config.appearance.page_size).min(candidates.len());
                    for i in start..end {
                        let item = Box::new(Orientation::Horizontal, 0);
                        item.add_css_class("modern-item");
                        if i == selected { item.add_css_class("modern-selected"); }
                        let idx_lbl = Label::new(Some(&format!("{}", (i % current_config.appearance.page_size) + 1)));
                        idx_lbl.add_css_class("m-index");
                        let txt_lbl = Label::new(Some(&candidates[i]));
                        txt_lbl.add_css_class("modern-text");
                        item.append(&idx_lbl);
                        item.append(&txt_lbl);
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
            _ => {}
        }
        glib::Continue(true)
    });

    window.present(); modern_window.present(); ks_controller.window.present(); learn_controller.window.present();
    window.set_opacity(0.0); modern_window.set_opacity(0.0); ks_controller.window.set_opacity(0.0); learn_controller.window.set_opacity(0.0);
    glib::MainLoop::new(None, false).run();
}
