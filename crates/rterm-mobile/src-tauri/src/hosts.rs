use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostProfile {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String, // "password" or "key"
    pub password: Option<String>,
    /// Optional group/folder name for organization.
    pub group: Option<String>,
}

fn app_data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn load_hosts_from_path(path: &Path) -> Result<Vec<HostProfile>, String> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn save_hosts_to_path(path: &Path, hosts: &[HostProfile]) -> Result<(), String> {
    let content = serde_json::to_string_pretty(hosts).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_hosts(app: tauri::AppHandle) -> Result<Vec<HostProfile>, String> {
    let path = app_data_dir(&app)?.join("hosts.json");
    load_hosts_from_path(&path)
}

#[tauri::command]
pub async fn save_host(app: tauri::AppHandle, host: HostProfile) -> Result<(), String> {
    let data_dir = app_data_dir(&app)?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let path = data_dir.join("hosts.json");

    let mut hosts = load_hosts_from_path(&path)?;
    if let Some(existing) = hosts.iter_mut().find(|h| h.id == host.id) {
        *existing = host;
    } else {
        hosts.push(host);
    }

    save_hosts_to_path(&path, &hosts)
}

#[tauri::command]
pub async fn delete_host(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let data_dir = app_data_dir(&app)?;
    let path = data_dir.join("hosts.json");

    let mut hosts = load_hosts_from_path(&path)?;
    hosts.retain(|h| h.id != id);
    save_hosts_to_path(&path, &hosts)
}

/// List all stored SSH key IDs (file names without .pem extension).
#[allow(dead_code)]
#[tauri::command]
pub async fn list_keys(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let data_dir = app_data_dir(&app)?.join("keys");
    if !data_dir.exists() {
        return Ok(vec![]);
    }
    let mut keys = vec![];
    for entry in std::fs::read_dir(&data_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().into_string().unwrap_or_default();
        if name.ends_with(".pem") {
            keys.push(name.trim_end_matches(".pem").to_string());
        }
    }
    Ok(keys)
}

/// Import a private key PEM. Stores it to keys/{key_id}.pem.
#[allow(dead_code)]
#[tauri::command]
pub async fn import_key(app: tauri::AppHandle, key_id: String, pem: String) -> Result<(), String> {
    let data_dir = app_data_dir(&app)?;
    let keys_dir = data_dir.join("keys");
    std::fs::create_dir_all(&keys_dir).map_err(|e| e.to_string())?;
    let key_path = keys_dir.join(format!("{}.pem", key_id));
    std::fs::write(&key_path, &pem).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read a private key PEM from keys/{key_id}.pem.
#[allow(dead_code)]
pub fn load_key_pem(app: &tauri::AppHandle, key_id: &str) -> Result<String, String> {
    let data_dir = app_data_dir(app)?;
    let key_path = data_dir.join("keys").join(format!("{}.pem", key_id));
    std::fs::read_to_string(&key_path).map_err(|e| e.to_string())
}

/// Delete a stored key.
#[allow(dead_code)]
#[tauri::command]
pub async fn delete_key(app: tauri::AppHandle, key_id: String) -> Result<(), String> {
    let data_dir = app_data_dir(&app)?;
    let key_path = data_dir.join("keys").join(format!("{}.pem", key_id));
    if key_path.exists() {
        std::fs::remove_file(&key_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
