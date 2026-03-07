use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::config::{Config, AuxMode, AntiTypoMode, PhantomType, DoublePinyinScheme, FuzzyPinyinConfig, PunctuationEntry};

pub type UserDictData = HashMap<String, HashMap<String, Vec<(String, u32)>>>;

pub struct ConfigManager {
    pub master_config: Config,
    pub show_candidates: bool,
    pub show_english_translation: bool,
    pub show_stroke_aux: bool,
    pub page_size: usize,
    pub show_tone_hint: bool,
    pub aux_mode: AuxMode,
    
    pub anti_typo_mode: AntiTypoMode,
    pub commit_mode: String,
    pub auto_commit_unique_en_fuzhuma: bool,
    pub auto_commit_unique_full_match: bool,
    pub enable_error_sound: bool,
    pub enable_prefix_matching: bool,
    pub prefix_matching_limit: usize,
    pub enable_abbreviation_matching: bool,
    pub filter_proper_nouns_by_case: bool,
    pub profile_keys: Vec<(String, String)>,
    
    pub page_flipping_styles: Vec<String>,
    pub swap_arrow_keys: bool,
    
    pub enable_english_filter: bool,
    pub enable_caps_selection: bool,
    pub enable_number_selection: bool,

    pub enable_double_tap: bool,
    pub double_tap_timeout: Duration,
    pub double_taps: HashMap<String, String>,

    pub enable_long_press: bool,
    pub long_press_timeout: Duration,
    pub long_press_mappings: HashMap<String, String>,

    pub enable_punctuation_long_press: bool,
    pub punctuation_long_press_mappings: HashMap<String, String>,
    pub punctuations: HashMap<String, HashMap<String, Vec<PunctuationEntry>>>,
    pub keyboard_layouts: HashMap<String, HashMap<String, String>>,

    pub phantom_type: PhantomType,

    pub enable_user_dict: bool,
    pub enable_fixed_first_candidate: bool,
    pub enable_smart_backspace: bool,
    pub enable_double_pinyin: bool,
    pub double_pinyin_scheme: DoublePinyinScheme,
    pub enable_fuzzy_pinyin: bool,
    pub fuzzy_config: FuzzyPinyinConfig,
    pub enable_traditional: bool,

    // 用户个人词库相关逻辑也移至此处（可选，目前先放配置）
    pub user_dict: Arc<Mutex<UserDictData>>,
    pub user_dict_tx: Option<std::sync::mpsc::Sender<UserDictData>>,
}

impl ConfigManager {
    pub fn new() -> Self {
        let master = Config::default_config();
        Self {
            master_config: master.clone(),
            show_candidates: master.appearance.show_candidates,
            show_english_translation: master.appearance.show_english_translation,
            show_stroke_aux: master.appearance.show_stroke_aux,
            page_size: master.appearance.page_size,
            show_tone_hint: master.appearance.show_tone_hint,
            aux_mode: master.appearance.aux_mode,
            anti_typo_mode: master.input.anti_typo_mode,
            commit_mode: master.input.commit_mode.clone(),
            auto_commit_unique_en_fuzhuma: master.input.auto_commit_unique_en_fuzhuma,
            auto_commit_unique_full_match: master.input.auto_commit_unique_full_match,
            enable_error_sound: master.input.enable_error_sound,
            enable_prefix_matching: master.input.enable_prefix_matching,
            prefix_matching_limit: master.input.prefix_matching_limit,
            enable_abbreviation_matching: master.input.enable_abbreviation_matching,
            filter_proper_nouns_by_case: master.input.filter_proper_nouns_by_case,
            profile_keys: master.input.profile_keys.iter().map(|pk| (pk.key.to_lowercase(), pk.profile.to_lowercase())).collect(),
            page_flipping_styles: master.input.page_flipping_keys.iter().map(|s| s.to_lowercase()).collect(),
            swap_arrow_keys: master.input.swap_arrow_keys,
            enable_english_filter: master.input.enable_english_filter,
            enable_caps_selection: master.input.enable_caps_selection,
            enable_number_selection: master.input.enable_number_selection,
            enable_double_tap: master.input.enable_double_tap,
            double_tap_timeout: Duration::from_millis(master.input.double_tap_timeout_ms),
            double_taps: {
                let mut m = HashMap::new();
                for dt in &master.input.double_taps { m.insert(dt.trigger_key.to_lowercase(), dt.insert_text.clone()); }
                m
            },
            enable_long_press: master.input.enable_long_press,
            long_press_timeout: Duration::from_millis(master.input.long_press_timeout_ms),
            long_press_mappings: {
                let mut m = HashMap::new();
                for lm in &master.input.long_press_mappings { m.insert(lm.trigger_key.to_lowercase(), lm.insert_text.clone()); }
                m
            },
            enable_punctuation_long_press: master.input.enable_punctuation_long_press,
            punctuation_long_press_mappings: master.input.punctuation_long_press_mappings.clone(),
            punctuations: master.input.punctuations.clone(),
            keyboard_layouts: master.input.keyboard_layouts.clone(),
            phantom_type: if cfg!(target_os = "windows") { PhantomType::None } else { master.input.phantom_type },
            enable_user_dict: master.input.enable_user_dict,
            enable_fixed_first_candidate: master.input.enable_fixed_first_candidate,
            enable_smart_backspace: master.input.enable_smart_backspace,
            enable_double_pinyin: master.input.enable_double_pinyin,
            double_pinyin_scheme: master.input.double_pinyin_scheme.clone(),
            enable_fuzzy_pinyin: master.input.enable_fuzzy_pinyin,
            fuzzy_config: master.input.fuzzy_config.clone(),
            enable_traditional: master.input.enable_traditional,
            user_dict: Arc::new(Mutex::new(HashMap::new())),
            user_dict_tx: None,
        }
    }

    pub fn apply_config(&mut self, conf: &Config) {
        self.master_config = conf.clone();
        self.enable_user_dict = conf.input.enable_user_dict;
        self.enable_fixed_first_candidate = conf.input.enable_fixed_first_candidate;
        self.enable_smart_backspace = conf.input.enable_smart_backspace;
        self.enable_double_pinyin = conf.input.enable_double_pinyin;
        self.double_pinyin_scheme = conf.input.double_pinyin_scheme.clone();
        self.enable_fuzzy_pinyin = conf.input.enable_fuzzy_pinyin;
        self.fuzzy_config = conf.input.fuzzy_config.clone();
        self.enable_traditional = conf.input.enable_traditional;
        
        self.show_candidates = conf.appearance.show_candidates;
        self.show_english_translation = conf.appearance.show_english_translation;
        self.show_stroke_aux = conf.appearance.show_stroke_aux;
        self.page_size = conf.appearance.page_size;
        self.show_tone_hint = conf.appearance.show_tone_hint;
        self.aux_mode = conf.appearance.aux_mode;
        
        self.anti_typo_mode = conf.input.anti_typo_mode;
        self.commit_mode = conf.input.commit_mode.clone();
        self.auto_commit_unique_en_fuzhuma = conf.input.auto_commit_unique_en_fuzhuma;
        self.auto_commit_unique_full_match = conf.input.auto_commit_unique_full_match;
        self.enable_error_sound = conf.input.enable_error_sound;
        self.enable_prefix_matching = conf.input.enable_prefix_matching;
        self.prefix_matching_limit = conf.input.prefix_matching_limit;
        self.enable_abbreviation_matching = conf.input.enable_abbreviation_matching;
        self.filter_proper_nouns_by_case = conf.input.filter_proper_nouns_by_case;
        self.profile_keys = conf.input.profile_keys.iter().map(|pk| (pk.key.to_lowercase(), pk.profile.to_lowercase())).collect();
        
        self.page_flipping_styles = conf.input.page_flipping_keys.iter().map(|s| s.to_lowercase()).collect();
        self.swap_arrow_keys = conf.input.swap_arrow_keys;
        
        self.enable_english_filter = conf.input.enable_english_filter;
        self.enable_caps_selection = conf.input.enable_caps_selection;
        self.enable_number_selection = conf.input.enable_number_selection;

        self.enable_double_tap = conf.input.enable_double_tap;
        self.double_tap_timeout = Duration::from_millis(conf.input.double_tap_timeout_ms);
        self.double_taps.clear();
        for dt in &conf.input.double_taps {
            self.double_taps.insert(dt.trigger_key.to_lowercase(), dt.insert_text.clone());
        }

        self.enable_long_press = conf.input.enable_long_press;
        self.long_press_timeout = Duration::from_millis(conf.input.long_press_timeout_ms);
        self.long_press_mappings.clear();
        for lm in &conf.input.long_press_mappings {
            self.long_press_mappings.insert(lm.trigger_key.to_lowercase(), lm.insert_text.clone());
        }

        self.enable_punctuation_long_press = conf.input.enable_punctuation_long_press;
        self.punctuation_long_press_mappings = conf.input.punctuation_long_press_mappings.clone();
        self.punctuations = conf.input.punctuations.clone();
        self.keyboard_layouts = conf.input.keyboard_layouts.clone();

        self.phantom_type = conf.input.phantom_type;
        if cfg!(target_os = "windows") && self.phantom_type != PhantomType::None {
            self.phantom_type = PhantomType::None;
        }

        if self.enable_user_dict && self.user_dict.lock().is_ok_and(|d| d.is_empty()) {
            self.load_user_dict();
        }
    }

    pub fn load_user_dict(&mut self) {
        let path = std::path::Path::new("data/user_dict.json");
        if path.exists() {
            if let Ok(file) = std::fs::File::open(path) {
                if let Ok(dict) = serde_json::from_reader(std::io::BufReader::new(file)) {
                    if let Ok(mut d) = self.user_dict.lock() {
                        *d = dict;
                    }
                }
            }
        }
        if self.user_dict_tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<UserDictData>();
            self.user_dict_tx = Some(tx);
            std::thread::spawn(move || {
                let path = std::path::PathBuf::from("data/user_dict.json");
                while let Ok(dict_clone) = rx.recv() {
                    let mut latest = dict_clone;
                    while let Ok(next) = rx.try_recv() { latest = next; }
                    if let Ok(file) = std::fs::File::create(&path) {
                        let _ = serde_json::to_writer_pretty(std::io::BufWriter::new(file), &latest);
                    }
                }
            });
        }
    }

    pub fn save_user_dict(&self) {
        if let Some(ref tx) = self.user_dict_tx {
            if let Ok(dict) = self.user_dict.lock() {
                let _ = tx.send(dict.clone());
            }
        }
    }
}
