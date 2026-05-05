use crate::config;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Gear {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Weight_oz", default)]
    pub weight_oz: String,
    #[serde(rename = "Parent Container", default)]
    pub parent_container: String,
    #[serde(rename = "Type", default)]
    pub r#type: String,
    #[serde(rename = "Category", default)]
    pub category: String,
    #[serde(rename = "Subcategory", default)]
    pub subcategory: String,
    #[serde(rename = "Priority", default)]
    pub priority: String,
    #[serde(rename = "Waterproof", default)]
    pub waterproof: String,
    #[serde(rename = "Worn", default)]
    pub worn: String,
    #[serde(rename = "Qty", default)]
    pub qty: String,
    #[serde(rename = "Consumable", default)]
    pub consumable: String,
    #[serde(rename = "Star", default)]
    pub star: String,
    #[serde(rename = "Notes", default)]
    pub notes: String,
    #[serde(rename = "Tags", default)]
    pub tags: String,
    #[serde(rename = "Condition", default)]
    pub condition: String,
    #[serde(rename = "Amazon_URL", default)]
    pub amazon_url: String,
    #[serde(rename = "Last_Used", default)]
    pub last_used: String,
    #[serde(rename = "Purchase_Date", default)]
    pub purchase_date: String,
    #[serde(rename = "Purchase_Price", default)]
    pub purchase_price: String,
    #[serde(rename = "Photo_URL", default)]
    pub photo_url: String,
    #[serde(rename = "Scan_3D_URL", default)]
    pub scan_3d_url: String,
    #[serde(rename = "Serial_Number", default)]
    pub serial_number: String,
    #[serde(rename = "Model_Number", default)]
    pub model_number: String,
    #[serde(rename = "Brand", default)]
    pub brand: String,
    #[serde(rename = "Location_Room", default)]
    pub location_room: String,
    #[serde(rename = "Location_Detail", default)]
    pub location_detail: String,
}

fn repo_path() -> Result<PathBuf, String> {
    let cfg = config::get()?;
    let target = cfg
        .publish_targets
        .iter()
        .find(|t| t.is_default)
        .or_else(|| cfg.publish_targets.first())
        .cloned()
        .ok_or_else(|| "no publish target configured".to_string())?;
    Ok(PathBuf::from(&target.repo_path))
}

fn csv_path() -> Result<PathBuf, String> {
    Ok(repo_path()?.join("data").join("gear.csv"))
}

fn read_all() -> Result<Vec<Gear>, String> {
    let path = csv_path()?;
    let mut rdr =
        csv::Reader::from_path(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let mut out = Vec::new();
    for r in rdr.deserialize::<Gear>() {
        out.push(r.map_err(|e| format!("parse: {}", e))?);
    }
    Ok(out)
}

fn write_all(items: &[Gear]) -> Result<(), String> {
    let path = csv_path()?;
    let mut wtr =
        csv::Writer::from_path(&path).map_err(|e| format!("write {}: {}", path.display(), e))?;
    for item in items {
        wtr.serialize(item)
            .map_err(|e| format!("serialize: {}", e))?;
    }
    wtr.flush().map_err(|e| format!("flush: {}", e))?;
    Ok(())
}

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

#[tauri::command]
pub fn list_gear() -> Result<Vec<Gear>, String> {
    read_all()
}

#[tauri::command]
pub fn mark_gear_used(names: Vec<String>) -> Result<usize, String> {
    let mut items = read_all()?;
    let stamp = today();
    let mut hits = 0;
    for it in items.iter_mut() {
        if names.iter().any(|n| n == &it.name) {
            it.last_used = stamp.clone();
            hits += 1;
        }
    }
    write_all(&items)?;
    Ok(hits)
}

#[tauri::command]
pub fn update_gear_location(name: String, room: String, detail: String) -> Result<(), String> {
    let mut items = read_all()?;
    let it = items
        .iter_mut()
        .find(|it| it.name == name)
        .ok_or_else(|| format!("not found: {}", name))?;
    it.location_room = room;
    it.location_detail = detail;
    write_all(&items)
}

#[tauri::command]
pub fn set_gear_scan_url(name: String, url: String) -> Result<(), String> {
    let mut items = read_all()?;
    let it = items
        .iter_mut()
        .find(|it| it.name == name)
        .ok_or_else(|| format!("not found: {}", name))?;
    it.scan_3d_url = url;
    write_all(&items)
}

#[derive(Serialize)]
pub struct GearPending {
    pub dirty: bool,
    pub diff_stat: String,
}

#[tauri::command]
pub fn gear_pending_changes() -> Result<GearPending, String> {
    let repo = repo_path()?;
    let out = Command::new(crate::bin_paths::git())
        .args(["diff", "--stat", "--", "data/gear.csv"])
        .current_dir(&repo)
        .output()
        .map_err(|e| format!("git diff: {}", e))?;
    let stat = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(GearPending {
        dirty: !stat.is_empty(),
        diff_stat: stat,
    })
}

#[tauri::command]
pub fn commit_gear_changes(message: Option<String>) -> Result<String, String> {
    let repo = repo_path()?;
    let msg = message.unwrap_or_else(|| format!("gear: hygiene pass {}", today()));

    let add = Command::new(crate::bin_paths::git())
        .args(["add", "data/gear.csv"])
        .current_dir(&repo)
        .output()
        .map_err(|e| format!("git add: {}", e))?;
    if !add.status.success() {
        return Err(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        ));
    }

    let staged = Command::new(crate::bin_paths::git())
        .args(["diff", "--cached", "--stat", "--", "data/gear.csv"])
        .current_dir(&repo)
        .output()
        .map_err(|e| format!("git diff cached: {}", e))?;
    if String::from_utf8_lossy(&staged.stdout).trim().is_empty() {
        return Ok("nothing to commit".into());
    }

    let commit = Command::new(crate::bin_paths::git())
        .args(["commit", "-m", &msg])
        .current_dir(&repo)
        .output()
        .map_err(|e| format!("git commit: {}", e))?;
    if !commit.status.success() {
        return Err(format!(
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        ));
    }
    Ok(msg)
}
