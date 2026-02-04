use axum::{
    routing::{get, post},
    extract::{State, Json},
    response::{IntoResponse, Html},
    http::{StatusCode, Uri},
    Router,
};
use serde::{Serialize};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use crate::config::Config;
use crate::engine::trie::Trie;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

pub struct WebServer {
    pub port: u16,
    pub config: Arc<RwLock<Config>>,
    pub tries: Arc<RwLock<HashMap<String, Trie>>>,
    pub tray_tx: std::sync::mpsc::Sender<crate::ui::tray::TrayEvent>,
}

type WebState = (
    Arc<RwLock<Config>>, 
    Arc<RwLock<HashMap<String, Trie>>>, 
    std::sync::mpsc::Sender<crate::ui::tray::TrayEvent>
);

impl WebServer {
    pub fn new(
        port: u16, 
        config: Arc<RwLock<Config>>, 
        tries: Arc<RwLock<HashMap<String, Trie>>>,
        tray_tx: std::sync::mpsc::Sender<crate::ui::tray::TrayEvent>
    ) -> Self {
        Self { port, config, tries, tray_tx }
    }

    pub async fn start(self) {
        let state: WebState = (self.config, self.tries, self.tray_tx);
        let app = Router::new()
            .route("/", get(index_handler))
            .route("/api/config", get(get_config).post(update_config))
            .route("/api/config/reset", post(reset_config))
            .route("/api/dicts", get(list_dicts))
            .route("/api/dicts/compile", post(compile_dicts_handler))
            .route("/api/dicts/reload", post(reload_dicts))
            .route("/api/dicts/toggle", post(toggle_dict))
            .route("/api/dict/search", get(search_dict))
            .route("/api/dict/update", post(update_dict_entry))
            .route("/static/*file", get(static_handler))
            .fallback(index_handler)
            .with_state(state);

        let addr = format!("127.0.0.1:{}", self.port);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                println!("[Web] 服务器启动在 http://{}", addr);
                if let Err(e) = axum::serve(listener, app).await {
                    eprintln!("[Web] Server error: {}", e);
                }
            }
            Err(e) => {
                eprintln!("[Web] Failed to bind to {}: {}", addr, e);
            }
        }
    }
}

async fn index_handler() -> impl IntoResponse {
    match Assets::get("index.html") {
        Some(content) => Html(String::from_utf8_lossy(&content.data).to_string()).into_response(),
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches("/static/").trim_start_matches("/");
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(axum::http::header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

async fn get_config(State((config, _, _)): State<WebState>) -> impl IntoResponse {
    match config.read() {
        Ok(c) => Json(c.clone()).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn update_config(
    State((config, _, tray_tx)): State<WebState>,
    Json(new_config): Json<Config>
) -> StatusCode {
    {
        let mut w = match config.write() {
            Ok(w) => w,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
        };
        
        // 处理自启逻辑变化
        if w.input.autostart != new_config.input.autostart {
            if new_config.input.autostart {
                let _ = crate::setup_autostart();
            } else {
                let _ = crate::remove_autostart();
            }
        }

        *w = new_config.clone();
    }
    if let Err(_e) = crate::save_config(&new_config) { return StatusCode::INTERNAL_SERVER_ERROR; }
    let _ = tray_tx.send(crate::ui::tray::TrayEvent::ReloadConfig);
    StatusCode::OK
}

async fn reset_config(
    State((config, _, tray_tx)): State<WebState>,
) -> StatusCode {
    let default_conf = Config::default_config();
    {
        let mut w = match config.write() {
            Ok(w) => w,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
        };
        *w = default_conf.clone();
    }
    if let Err(_e) = crate::save_config(&default_conf) { return StatusCode::INTERNAL_SERVER_ERROR; }
    let _ = tray_tx.send(crate::ui::tray::TrayEvent::ReloadConfig);
    StatusCode::OK
}

#[derive(Serialize)]
struct DictFile {
    name: String,
    path: String,
    size: u64,
    entry_count: u64,
    enabled: bool,
}

async fn list_dicts() -> Json<Vec<DictFile>> {
    let mut list = Vec::new();
    let root = "dicts";
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                    for sub_entry in sub_entries.flatten() {
                        let path = sub_entry.path();
                        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        if filename.ends_with(".json") || filename.ends_with(".json.disabled") {
                            list.push(process_dict_entry(path));
                        }
                    }
                }
            } else {
                let path = entry.path();
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                if filename.ends_with(".json") || filename.ends_with(".json.disabled") {
                    list.push(process_dict_entry(path));
                }
            }
        }
    }
    Json(list)
}

fn process_dict_entry(path: std::path::PathBuf) -> DictFile {
    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let enabled = filename.ends_with(".json");
    let metadata = path.metadata();
    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
    
    let mut entry_count = 0;
    if let Ok(f) = std::fs::File::open(&path) {
        if let Ok(json) = serde_json::from_reader::<_, serde_json::Value>(std::io::BufReader::new(f)) {
            if let Some(obj) = json.as_object() {
                for val in obj.values() {
                    if let Some(arr) = val.as_array() {
                        entry_count += arr.len() as u64;
                    } else {
                        entry_count += 1;
                    }
                }
            } else if let Some(arr) = json.as_array() {
                entry_count = arr.len() as u64;
            }
        }
    }

    DictFile {
        name: filename,
        path: path.to_string_lossy().to_string(),
        size,
        entry_count,
        enabled,
    }
}

#[derive(serde::Deserialize)]
struct ToggleRequest {
    path: String,
}

async fn toggle_dict(Json(req): Json<ToggleRequest>) -> StatusCode {
    let path = std::path::Path::new(&req.path);
    if !path.exists() { return StatusCode::NOT_FOUND; }

    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let new_path = if filename.ends_with(".json") {
        path.with_file_name(format!("{}.disabled", filename))
    } else if filename.ends_with(".json.disabled") {
        path.with_file_name(filename.replace(".json.disabled", ".json"))
    } else {
        return StatusCode::BAD_REQUEST;
    };

    if std::fs::rename(path, new_path).is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn compile_dicts_handler() -> StatusCode {
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("run").arg("--bin").arg("compile_dict");
    match cmd.status() {
        Ok(s) if s.success() => StatusCode::OK,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn reload_dicts(State((_, _, tray_tx)): State<WebState>) -> StatusCode {
    let _ = tray_tx.send(crate::ui::tray::TrayEvent::ReloadConfig);
    StatusCode::OK
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
}

#[derive(Serialize)]
struct SearchResult {
    pinyin: String,
    word: String,
    hint: String,
    file: String,
}

async fn search_dict(axum::extract::Query(query): axum::extract::Query<SearchQuery>) -> Json<Vec<SearchResult>> {
    let mut results = Vec::new();
    let q = query.q.to_lowercase();
    
    // 遍历 dicts 目录下所有有效的 json
    let root = "dicts";
    let entries = walkdir::WalkDir::new(root);
    for entry in entries.into_iter().filter_map(|e| e.ok()) {
        if entry.path().extension().map_or(false, |ext| ext == "json") {
            let path_str = entry.path().to_string_lossy().to_string();
            if let Ok(f) = std::fs::File::open(entry.path()) {
                if let Ok(json) = serde_json::from_reader::<_, serde_json::Value>(std::io::BufReader::new(f)) {
                    if let Some(obj) = json.as_object() {
                        for (pinyin, val) in obj {
                            if let Some(arr) = val.as_array() {
                                for v in arr {
                                    let word = v.get("char").and_then(|c| c.as_str()).unwrap_or("");
                                    let hint = v.get("en").and_then(|e| e.as_str()).unwrap_or("");
                                    if pinyin.to_lowercase().contains(&q) || word.contains(&q) {
                                        results.push(SearchResult {
                                            pinyin: pinyin.clone(),
                                            word: word.to_string(),
                                            hint: hint.to_string(),
                                            file: path_str.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if results.len() > 100 { break; } // 限制结果数量
    }
    Json(results)
}

#[derive(serde::Deserialize)]
struct UpdateEntryRequest {
    pinyin: String,
    word: String,
    new_hint: String,
    file: String,
}

async fn update_dict_entry(Json(req): Json<UpdateEntryRequest>) -> StatusCode {
    let path = std::path::Path::new(&req.file);
    if !path.exists() { return StatusCode::NOT_FOUND; }

    let mut data: serde_json::Value = match std::fs::File::open(path) {
        Ok(f) => serde_json::from_reader(std::io::BufReader::new(f)).unwrap_or(serde_json::Value::Null),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    let mut success = false;
    if let Some(obj) = data.as_object_mut() {
        if let Some(entries) = obj.get_mut(&req.pinyin).and_then(|v| v.as_array_mut()) {
            for entry in entries {
                if entry.get("char").and_then(|c| c.as_str()) == Some(&req.word) {
                    entry["en"] = serde_json::Value::String(req.new_hint.clone());
                    success = true;
                    break;
                }
            }
        }
    }

    if success {
        if let Ok(f) = std::fs::File::create(path) {
            if serde_json::to_writer_pretty(f, &data).is_ok() {
                return StatusCode::OK;
            }
        }
    }

    StatusCode::INTERNAL_SERVER_ERROR
}
