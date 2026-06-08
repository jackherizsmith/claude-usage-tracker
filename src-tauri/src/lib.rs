use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

// ---- Serde types ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitInfo {
    pub pct: Option<i32>,
    pub reset_at: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FallbackInfo {
    pub available: bool,
    pub pct: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageData {
    pub session: LimitInfo,
    pub weekly: LimitInfo,
    pub overage: LimitInfo,
    pub fallback: FallbackInfo,
    pub updated_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenStats {
    pub input: i64,
    pub output: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsData {
    pub by_project: HashMap<String, TokenStats>,
    pub by_branch: HashMap<String, TokenStats>,
    pub by_hour: Vec<i64>,
    pub by_day: HashMap<String, i64>,
    pub by_tool: HashMap<String, i64>,
    pub by_skill: HashMap<String, i64>,
    pub days: i32,
}

// ---- App State ----

pub struct AppState {
    pub cached_data: Mutex<Option<UsageData>>,
    pub analytics_cache: Mutex<HashMap<i32, (AnalyticsData, u64)>>,
    pub client: reqwest::Client,
}

// ---- Tray icon ----

fn create_tray_icon() -> tauri::image::Image<'static> {
    let w = 36u32;
    let h = 36u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];

    let fill = |rgba: &mut Vec<u8>, x: u32, y: u32, fw: u32, fh: u32| {
        for dy in 0..fh {
            for dx in 0..fw {
                let px = x + dx;
                let py = y + dy;
                if px < w && py < h {
                    let i = ((py * w + px) * 4) as usize;
                    rgba[i] = 0;
                    rgba[i + 1] = 0;
                    rgba[i + 2] = 0;
                    rgba[i + 3] = 255;
                }
            }
        }
    };

    fill(&mut rgba, 0, 0, 4, 36);   // spine
    fill(&mut rgba, 4, 6, 24, 10);  // top bar
    fill(&mut rgba, 4, 20, 14, 10); // bottom bar

    tauri::image::Image::new_owned(rgba, w, h)
}

// ---- Token reading ----

fn extract_access_token(blob: &str) -> Option<String> {
    let blob = blob.trim();
    if blob.is_empty() {
        return None;
    }
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(blob) {
        if let Some(token) = data.get("accessToken").and_then(|v| v.as_str()) {
            return Some(token.to_string());
        }
        if let Some(obj) = data.as_object() {
            for val in obj.values() {
                if let Some(token) = val.get("accessToken").and_then(|v| v.as_str()) {
                    return Some(token.to_string());
                }
            }
        }
    }
    // Regex fallback
    if let Some(start) = blob.find("\"accessToken\"") {
        let rest = &blob[start + 13..];
        if let Some(colon) = rest.find(':') {
            let after = rest[colon + 1..].trim();
            if after.starts_with('"') {
                let inner = &after[1..];
                if let Some(end) = inner.find('"') {
                    return Some(inner[..end].to_string());
                }
            }
        }
    }
    None
}

fn read_token() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let username = std::env::var("USER").unwrap_or_default();
        if !username.is_empty() {
            if let Ok(output) = std::process::Command::new("security")
                .args([
                    "find-generic-password",
                    "-s",
                    "Claude Code-credentials",
                    "-a",
                    &username,
                    "-w",
                ])
                .output()
            {
                if output.status.success() {
                    let blob = String::from_utf8_lossy(&output.stdout);
                    if let Some(token) = extract_access_token(&blob) {
                        return Some(token);
                    }
                }
            }
        }
    }

    // Fallback: read ~/.claude/.credentials.json
    if let Some(home) = dirs::home_dir() {
        let cred_path = home.join(".claude").join(".credentials.json");
        if let Ok(contents) = std::fs::read_to_string(cred_path) {
            return extract_access_token(&contents);
        }
    }
    None
}

// ---- API polling ----

async fn poll_usage(token: &str, client: &reqwest::Client) -> Result<UsageData, String> {
    let body = serde_json::json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "hi"}]
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Content-Type", "application/json")
        .header("User-Agent", "claude-code/2.1.5")
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let headers = resp.headers().clone();

    let hdr = |name: &str| -> String {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };

    let pct = |name: &str| -> Option<i32> {
        let v: f64 = hdr(name).parse().ok()?;
        Some((v * 100.0).round() as i32)
    };

    let reset_iso = |name: &str| -> Option<String> {
        let v: f64 = hdr(name).parse().ok()?;
        let secs = v as i64;
        DateTime::from_timestamp(secs, 0).map(|dt: DateTime<Utc>| dt.to_rfc3339())
    };

    let error = if status.as_u16() >= 400 {
        Some(format!("API returned {}", status.as_u16()))
    } else {
        None
    };

    Ok(UsageData {
        session: LimitInfo {
            pct: pct("anthropic-ratelimit-unified-5h-utilization"),
            reset_at: reset_iso("anthropic-ratelimit-unified-5h-reset"),
            status: {
                let s = hdr("anthropic-ratelimit-unified-5h-status");
                if s.is_empty() { "normal".to_string() } else { s }
            },
        },
        weekly: LimitInfo {
            pct: pct("anthropic-ratelimit-unified-7d-utilization"),
            reset_at: reset_iso("anthropic-ratelimit-unified-7d-reset"),
            status: {
                let s = hdr("anthropic-ratelimit-unified-7d-status");
                if s.is_empty() { "normal".to_string() } else { s }
            },
        },
        overage: LimitInfo {
            pct: pct("anthropic-ratelimit-unified-overage-utilization"),
            reset_at: reset_iso("anthropic-ratelimit-unified-overage-reset"),
            status: {
                let s = hdr("anthropic-ratelimit-unified-overage-status");
                if s.is_empty() { "allowed".to_string() } else { s }
            },
        },
        fallback: FallbackInfo {
            available: hdr("anthropic-ratelimit-unified-fallback") == "available",
            pct: {
                let v: f64 = hdr("anthropic-ratelimit-unified-fallback-percentage")
                    .parse()
                    .unwrap_or(0.0);
                (v * 100.0).round() as i32
            },
        },
        updated_at: Utc::now().to_rfc3339(),
        error,
    })
}

// ---- fetch_and_cache ----

async fn fetch_and_cache(app: &AppHandle) {
    let state = app.state::<AppState>();
    let token = read_token();

    let result = match token {
        None => UsageData {
            session: LimitInfo { pct: None, reset_at: None, status: "normal".to_string() },
            weekly: LimitInfo { pct: None, reset_at: None, status: "normal".to_string() },
            overage: LimitInfo { pct: None, reset_at: None, status: "allowed".to_string() },
            fallback: FallbackInfo { available: false, pct: 0 },
            updated_at: Utc::now().to_rfc3339(),
            error: Some("No Claude Code token found — is Claude Code installed and signed in?".to_string()),
        },
        Some(t) => match poll_usage(&t, &state.client).await {
            Ok(data) => data,
            Err(e) => {
                let current = state.cached_data.lock().unwrap().clone();
                let base = current.unwrap_or_else(|| UsageData {
                    session: LimitInfo { pct: None, reset_at: None, status: "normal".to_string() },
                    weekly: LimitInfo { pct: None, reset_at: None, status: "normal".to_string() },
                    overage: LimitInfo { pct: None, reset_at: None, status: "allowed".to_string() },
                    fallback: FallbackInfo { available: false, pct: 0 },
                    updated_at: Utc::now().to_rfc3339(),
                    error: None,
                });
                UsageData {
                    error: Some(e),
                    updated_at: Utc::now().to_rfc3339(),
                    ..base
                }
            }
        },
    };

    // Update tray title (macOS only — set_title is not available on Windows/Linux)
    #[cfg(target_os = "macos")]
    if let Some(tray) = app.tray_by_id("main") {
        let title = match (result.session.pct, result.weekly.pct) {
            (Some(s), Some(w)) => format!(" {}% ({}%)", s, w),
            (Some(s), None) => format!(" {}%", s),
            _ => " –".to_string(),
        };
        let _ = tray.set_title(Some(&title));
    }

    let is_visible = app
        .get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);

    *state.cached_data.lock().unwrap() = Some(result);

    if is_visible {
        let _ = app.emit("refresh", ());
    }
}

// ---- Analytics ----

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn compute_analytics(days: i32) -> Option<AnalyticsData> {
    let home = dirs::home_dir()?;
    let base = home.join(".claude").join("projects");

    if !base.exists() {
        return None;
    }

    let now_ms = now_unix_ms();
    let cutoff = now_ms.saturating_sub(days as u64 * 86_400_000);

    let mut by_project: HashMap<String, TokenStats> = HashMap::new();
    let mut by_branch: HashMap<String, TokenStats> = HashMap::new();
    let mut by_hour: Vec<i64> = vec![0; 24];
    let mut by_day: HashMap<String, i64> = HashMap::new();
    let mut by_tool: HashMap<String, i64> = HashMap::new();
    let mut by_skill: HashMap<String, i64> = HashMap::new();
    let mut seen_leafs: HashSet<String> = HashSet::new();

    let dirs_iter = match std::fs::read_dir(&base) {
        Ok(d) => d,
        Err(_) => return None,
    };

    for dir_entry in dirs_iter.flatten() {
        let dir_path = dir_entry.path();
        if !dir_path.is_dir() {
            continue;
        }

        let files_iter = match std::fs::read_dir(&dir_path) {
            Ok(f) => f,
            Err(_) => continue,
        };

        for file_entry in files_iter.flatten() {
            let file_path = file_entry.path();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "jsonl" {
                continue;
            }

            // Skip files older than cutoff based on mtime
            if let Ok(meta) = std::fs::metadata(&file_path) {
                if let Ok(modified) = meta.modified() {
                    let mtime_ms = modified
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    if mtime_ms < cutoff {
                        continue;
                    }
                }
            }

            let text = match std::fs::read_to_string(&file_path) {
                Ok(t) => t,
                Err(_) => continue,
            };

            let lines: Vec<&str> = text.lines().collect();

            // First pass: build uuid→timestamp map
            let mut uuid_ts: HashMap<String, u64> = HashMap::new();
            for line in &lines {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(e) = serde_json::from_str::<serde_json::Value>(line) {
                    if let (Some(uuid), Some(ts_str)) = (
                        e.get("uuid").and_then(|v| v.as_str()),
                        e.get("timestamp").and_then(|v| v.as_str()),
                    ) {
                        if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str) {
                            uuid_ts.insert(uuid.to_string(), dt.timestamp_millis() as u64);
                        }
                    }
                }
            }

            let skill_re_prefix = "/";

            // Second pass: process entries
            for line in &lines {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let e: serde_json::Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let entry_type = e.get("type").and_then(|v| v.as_str()).unwrap_or("");

                if entry_type == "last-prompt" {
                    if let Some(leaf_uuid) = e.get("leafUuid").and_then(|v| v.as_str()) {
                        let ts = uuid_ts.get(leaf_uuid).copied();
                        if let Some(t) = ts {
                            if t < cutoff {
                                continue;
                            }
                        }
                        let last_prompt = e
                            .get("lastPrompt")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim_start();
                        if last_prompt.starts_with(skill_re_prefix) && last_prompt.len() > 1 {
                            let rest = &last_prompt[1..];
                            let skill_name: &str = rest
                                .split(|c: char| c == '/' || c.is_whitespace())
                                .next()
                                .unwrap_or("");
                            if !skill_name.is_empty() && !seen_leafs.contains(leaf_uuid) {
                                seen_leafs.insert(leaf_uuid.to_string());
                                *by_skill.entry(skill_name.to_string()).or_insert(0) += 1;
                            }
                        }
                    }
                    continue;
                }

                if entry_type != "assistant" {
                    continue;
                }

                let ts_str = match e.get("timestamp").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => continue,
                };

                let dt = match DateTime::parse_from_rfc3339(ts_str) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let ts_ms = dt.timestamp_millis() as u64;
                if ts_ms < cutoff {
                    continue;
                }

                let project = e
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .and_then(|p| std::path::Path::new(p).file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let branch = e
                    .get("gitBranch")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                if let Some(usage) = e.get("message").and_then(|m| m.get("usage")) {
                    let input = usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0)
                        + usage
                            .get("cache_creation_input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)
                        + usage
                            .get("cache_read_input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                    let output = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);

                    let proj_entry = by_project.entry(project).or_default();
                    proj_entry.input += input;
                    proj_entry.output += output;

                    let branch_entry = by_branch.entry(branch).or_default();
                    branch_entry.input += input;
                    branch_entry.output += output;

                    let hour = dt.naive_local().time().hour() as usize;
                    if hour < 24 {
                        by_hour[hour] += output;
                    }

                    let day = dt.format("%Y-%m-%d").to_string();
                    *by_day.entry(day).or_insert(0) += output;
                }

                if let Some(content) = e
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for block in content {
                        if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                            if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                                *by_tool.entry(name.to_string()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    Some(AnalyticsData {
        by_project,
        by_branch,
        by_hour,
        by_day,
        by_tool,
        by_skill,
        days,
    })
}

// ---- Tauri commands ----

#[tauri::command]
async fn get_usage(state: State<'_, AppState>) -> Result<Option<UsageData>, String> {
    Ok(state.cached_data.lock().unwrap().clone())
}

#[tauri::command]
async fn refresh_now(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<UsageData>, String> {
    fetch_and_cache(&app).await;
    Ok(state.cached_data.lock().unwrap().clone())
}

#[tauri::command]
async fn get_analytics(
    state: State<'_, AppState>,
    days: Option<i32>,
) -> Result<Option<AnalyticsData>, String> {
    let days = days.unwrap_or(30);
    let cache_ttl_secs: u64 = 5 * 60;
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    {
        let cache = state.analytics_cache.lock().unwrap();
        if let Some((data, cached_at)) = cache.get(&days) {
            if now_secs.saturating_sub(*cached_at) < cache_ttl_secs {
                return Ok(Some(data.clone()));
            }
        }
    }

    let result = tokio::task::spawn_blocking(move || compute_analytics(days))
        .await
        .map_err(|e| e.to_string())?;

    if let Some(ref data) = result {
        state
            .analytics_cache
            .lock()
            .unwrap()
            .insert(days, (data.clone(), now_secs));
    }

    Ok(result)
}

#[tauri::command]
async fn set_height(app: AppHandle, height: f64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let current_size = win.inner_size().map_err(|e| e.to_string())?;
        let scale = win.scale_factor().map_err(|e| e.to_string())?;
        let logical_width = current_size.width as f64 / scale;
        win.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: logical_width,
            height,
        }))
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn quit(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

// ---- run() ----

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            cached_data: Mutex::new(None),
            analytics_cache: Mutex::new(HashMap::new()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("failed to build reqwest client"),
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let tray_builder = tauri::tray::TrayIconBuilder::with_id("main")
                .icon(create_tray_icon());

            // set_title / icon_as_template are macOS-only in Tauri's tray API
            #[cfg(target_os = "macos")]
            let tray_builder = tray_builder.icon_as_template(true).title(" –");

            let tray = tray_builder.on_tray_icon_event(move |tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        position,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                            } else {
                                if let Ok(scale) = win.scale_factor() {
                                    let lx = position.x / scale - 190.0;
                                    let ly = position.y / scale + 10.0;
                                    let _ = win.set_position(tauri::Position::Logical(
                                        tauri::LogicalPosition { x: lx, y: ly },
                                    ));
                                }
                                let _ = win.show();
                                let _ = win.set_focus();
                                let _ = app.emit("refresh", ());
                            }
                        }
                    }
                })
                .build(app)?;

            drop(tray);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                fetch_and_cache(&handle).await;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    fetch_and_cache(&handle).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_usage,
            refresh_now,
            get_analytics,
            set_height,
            quit
        ])
        .run(tauri::generate_context!())
        .expect("error running app");
}
