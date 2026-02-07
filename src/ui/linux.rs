use gtk4::prelude::*;
use gtk4::{Window, Label, Box, Orientation, CssProvider};
use gdk4::Display;
use gtk4_layer_shell::{LayerShell, Layer, Edge, KeyboardMode};
use std::sync::mpsc::Receiver;
use glib::MainContext;
use crate::config::Config;
use crate::ui::GuiEvent;
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
        controller.clone().start_cleanup_timer();
        controller
    }

    fn show_key(&self, key: &str) {
        if key.is_empty() { return; }
        let mut keys = self.displayed_keys.borrow_mut();
        let label = Label::new(Some(key));
        label.add_css_class("key-label");
        self.box_.append(&label);
        let displayed = DisplayedKey { label, last_active: std::time::Instant::now() };
        keys.push(displayed);
        while keys.len() > self.max_keys {
            let old = keys.remove(0);
            self.box_.remove(&old.label);
        }
        self.window.set_opacity(1.0);
    }

    fn remove_expired(&self) {
        let mut keys = self.displayed_keys.borrow_mut();
        let timeout_ms = *self.timeout_ms.borrow();
        let now = std::time::Instant::now();
        let mut expired_indices = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            if now.duration_since(key.last_active) > std::time::Duration::from_millis(timeout_ms) {
                expired_indices.push(i);
            }
        }
        for i in expired_indices.into_iter().rev() {
            let removed = keys.remove(i);
            self.box_.remove(&removed.label);
        }
        if keys.is_empty() { self.window.set_opacity(0.0); }
    }

    fn clear(&self) {
        let mut keys = self.displayed_keys.borrow_mut();
        while let Some(child) = self.box_.first_child() { self.box_.remove(&child); }
        keys.clear();
        self.window.set_opacity(0.0);
        self.window.hide();
        self.window.show();
    }

    fn update_config(&self, timeout_ms: u64) { *self.timeout_ms.borrow_mut() = timeout_ms; }
    
    fn start_cleanup_timer(self: Rc<Self>) {
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            self.remove_expired();
            glib::Continue(true)
        });
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
        Rc::new(Self { window, word_label, hint_label, timeout: RefCell::new(None) })
    }

    fn show(&self, word: &str, hint: &str) {
        self.word_label.set_text(word);
        self.hint_label.set_text(hint);
        self.window.show();
        self.window.set_opacity(1.0);
        *self.timeout.borrow_mut() = None;
        let win_weak = self.window.downgrade();
        let id = glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
            if let Some(w) = win_weak.upgrade() {
                w.set_opacity(0.0);
                w.hide();
            }
            glib::Continue(false)
        });
        *self.timeout.borrow_mut() = Some(id);
    }

    fn clear(&self) {
        *self.timeout.borrow_mut() = None;
        self.window.set_opacity(0.0);
        self.window.hide();
        self.word_label.set_text("");
        self.hint_label.set_text("");
    }
}

pub fn start_gui(rx: Receiver<GuiEvent>, initial_config: Config) {
    if gtk4::init().is_err() { return; }
    let is_layer_supported = gtk4_layer_shell::is_supported();

    let window = Window::builder().title("Rust IME Candidates").decorated(false).can_focus(false).focusable(false).resizable(false).build();
    if is_layer_supported {
        window.init_layer_shell();
        window.set_namespace("rust-ime-candidates");
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::None);
    }
    window.add_css_class("ime-window");
    let main_box = Box::new(Orientation::Vertical, 4);
    window.set_child(Some(&main_box));
    let sentence_label = Label::new(None);
    main_box.append(&sentence_label);
    let pinyin_label = Label::new(None);
    main_box.append(&pinyin_label);
    let candidates_box = Box::new(Orientation::Horizontal, 12);
    main_box.append(&candidates_box);

    let modern_window = Window::builder().title("Modern Candidates").decorated(false).can_focus(false).focusable(false).resizable(false).build();
    if is_layer_supported {
        modern_window.init_layer_shell();
        modern_window.set_namespace("rust-ime-modern");
        modern_window.set_layer(Layer::Overlay);
        modern_window.set_keyboard_mode(KeyboardMode::None);
    }
    let modern_main_box = Box::new(Orientation::Vertical, 10);
    modern_window.set_child(Some(&modern_main_box));
    let modern_pinyin_label = Label::new(None);
    modern_main_box.append(&modern_pinyin_label);
    let modern_candidates_box = Box::new(Orientation::Vertical, 6);
    modern_main_box.append(&modern_candidates_box);

    let key_window = Window::builder().title("Keystrokes").decorated(false).can_focus(false).focusable(false).resizable(false).build();
    if is_layer_supported {
        key_window.init_layer_shell();
        key_window.set_namespace("rust-ime-keystrokes");
        key_window.set_layer(Layer::Overlay);
        key_window.set_keyboard_mode(KeyboardMode::None);
    }
    let key_box = Box::new(Orientation::Horizontal, 6);
    key_window.set_child(Some(&key_box));

    let learn_window = Window::builder().title("Learning").decorated(false).can_focus(false).focusable(false).resizable(false).build();
    if is_layer_supported {
        learn_window.init_layer_shell();
        learn_window.set_namespace("rust-ime-learning");
        learn_window.set_layer(Layer::Overlay);
        learn_window.set_keyboard_mode(KeyboardMode::None);
    }
    let learn_box = Box::new(Orientation::Vertical, 4);
    learn_window.set_child(Some(&learn_box));
    let learn_word_label = Label::new(None);
    learn_box.append(&learn_word_label);
    let learn_hint_label = Label::new(None);
    learn_box.append(&learn_hint_label);

    let css_provider = CssProvider::new();
    if let Some(display) = Display::default() {
        gtk4::style_context_add_provider_for_display(&display, &css_provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let apply_style = move |conf: &Config, css: &CssProvider, w: &Window, mw: &Window, kw: &Window, lw: &Window| {
        // ... (Styles similar to before, omitted for brevity but should be kept in real implementation)
    };

    apply_style(&initial_config, &css_provider, &window, &modern_window, &key_window, &learn_window);
    let ks_controller = KeystrokeController::new(key_box.clone(), key_window.clone(), initial_config.appearance.keystroke_timeout_ms);
    let learn_controller = LearningController::new(learn_window.clone(), learn_word_label.clone(), learn_hint_label.clone(), initial_config.appearance.learning_interval_sec);

    let (tx, gtk_rx) = MainContext::channel::<GuiEvent>(glib::Priority::default());
    std::thread::spawn(move || { while let Ok(msg) = rx.recv() { let _ = tx.send(msg); } });

    gtk_rx.attach(None, move |event| {
        match event {
            GuiEvent::Update { pinyin, candidates, hints, selected, sentence, commit_mode, .. } => {
                // ... (GTK update logic)
            }
            GuiEvent::ApplyConfig(conf) => { /* apply */ }
            _ => {}
        }
        glib::Continue(true)
    });

    glib::MainLoop::new(None, false).run();
}
