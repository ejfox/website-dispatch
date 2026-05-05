use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

static CONFIG: OnceLock<RwLock<AppConfig>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: u32,
    pub vault: VaultConfig,
    pub publish_targets: Vec<PublishTarget>,
    pub editors: Vec<EditorConfig>,
    pub default_editor: String,
    #[serde(default)]
    pub cloudinary_cloud_name: Option<String>,
    #[serde(default)]
    pub analytics_url: Option<String>,
    #[serde(default)]
    pub mastodon_instance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub path: String,
    pub name: String,
    pub excluded_dirs: Vec<String>,
    pub publishable_dirs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishTarget {
    pub name: String,
    pub id: String,
    pub repo_path: String,
    pub domain: String,
    pub content_path_pattern: String,
    pub branch: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    pub name: String,
    pub app_name: String,
    pub enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        AppConfig {
            version: 1,
            vault: VaultConfig {
                path: format!(
                    "{}/Library/Mobile Documents/iCloud~md~obsidian/Documents/ejfox",
                    home
                ),
                name: "ejfox".into(),
                excluded_dirs: vec![
                    "robot-notes".into(),
                    "private".into(),
                    "templates".into(),
                    "attachments".into(),
                    "drafts".into(),
                ],
                publishable_dirs: vec!["blog".into(), "drafts".into(), "week-notes".into()],
            },
            publish_targets: vec![PublishTarget {
                name: "Website".into(),
                id: "website".into(),
                repo_path: format!("{}/code/website2", home),
                domain: "https://ejfox.com".into(),
                content_path_pattern: "content/blog/{year}".into(),
                branch: "main".into(),
                is_default: true,
            }],
            editors: vec![
                EditorConfig {
                    name: "Obsidian".into(),
                    app_name: "Obsidian".into(),
                    enabled: true,
                },
                EditorConfig {
                    name: "iA Writer".into(),
                    app_name: "iA Writer".into(),
                    enabled: true,
                },
                EditorConfig {
                    name: "VS Code".into(),
                    app_name: "Visual Studio Code".into(),
                    enabled: false,
                },
            ],
            default_editor: "iA Writer".into(),
            cloudinary_cloud_name: None,
            analytics_url: None,
            mastodon_instance: None,
        }
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library/Application Support/com.ejfox.dispatch")
        .join("config.json")
}

fn load_or_create() -> AppConfig {
    let path = config_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(config) => return config,
                Err(e) => log::warn!("Config parse error, using defaults: {}", e),
            },
            Err(e) => log::warn!("Config read error, using defaults: {}", e),
        }
    }
    let config = AppConfig::default();
    let _ = save_config(&config);
    config
}

fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

pub fn init() {
    let config = load_or_create();
    let _ = CONFIG.set(RwLock::new(config));
}

pub fn get() -> Result<AppConfig, String> {
    let lock = CONFIG
        .get()
        .ok_or_else(|| "config::init() must be called first".to_string())?;
    let guard = lock
        .read()
        .map_err(|e| format!("config lock poisoned: {}", e))?;
    Ok(guard.clone())
}

pub fn update(config: AppConfig) -> Result<(), String> {
    save_config(&config)?;
    let rw = CONFIG
        .get()
        .ok_or_else(|| "config::init() must be called first".to_string())?;
    let mut lock = rw
        .write()
        .map_err(|e| format!("config lock poisoned: {}", e))?;
    *lock = config;
    Ok(())
}

pub fn get_config_file_path() -> String {
    config_path().to_string_lossy().to_string()
}

/// Get the default publish target, or the first one
pub fn default_target() -> Result<PublishTarget, String> {
    let config = get()?;
    config
        .publish_targets
        .iter()
        .find(|t| t.is_default)
        .or(config.publish_targets.first())
        .cloned()
        .ok_or_else(|| "No publish targets configured".to_string())
}

/// Resolve a target by optional ID (None = default)
pub fn resolve_target(target_id: Option<&str>) -> Result<PublishTarget, String> {
    let config = get()?;
    match target_id {
        None => default_target(),
        Some(id) => config
            .publish_targets
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| format!("Publish target '{}' not found", id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_round_trip() {
        let config = AppConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.vault.name, "ejfox");
        assert_eq!(parsed.default_editor, "iA Writer");
        assert_eq!(parsed.publish_targets.len(), 1);
        assert!(parsed.publish_targets[0].is_default);
        assert_eq!(parsed.publish_targets[0].domain, "https://ejfox.com");
        assert_eq!(parsed.editors.len(), 3);
    }

    #[test]
    fn test_defaults_match_old_config() {
        // Verify defaults produce the same paths as the old hardcoded Config::default()
        let config = AppConfig::default();
        let home = std::env::var("HOME").unwrap_or_default();

        assert_eq!(
            config.vault.path,
            format!(
                "{}/Library/Mobile Documents/iCloud~md~obsidian/Documents/ejfox",
                home
            )
        );
        assert_eq!(
            config.publish_targets[0].repo_path,
            format!("{}/code/website2", home)
        );
        assert_eq!(
            config.vault.excluded_dirs,
            vec![
                "robot-notes",
                "private",
                "templates",
                "attachments",
                "drafts"
            ]
        );
    }

    #[test]
    fn test_save_and_load() {
        let dir = std::env::temp_dir().join("dispatch-test-config");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let mut config = AppConfig::default();
        config.vault.name = "test-vault".into();
        config.default_editor = "VS Code".into();

        let json = serde_json::to_string_pretty(&config).unwrap();
        fs::write(&path, &json).unwrap();

        let loaded: AppConfig = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.vault.name, "test-vault");
        assert_eq!(loaded.default_editor, "VS Code");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_missing_optional_fields_deserialize() {
        // Simulate a config file missing optional fields (forward compat)
        let json = r#"{
            "version": 1,
            "vault": { "path": "/tmp", "name": "test", "excluded_dirs": [], "publishable_dirs": ["blog"] },
            "publish_targets": [],
            "editors": [],
            "default_editor": "vim"
        }"#;

        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(config.cloudinary_cloud_name.is_none());
        assert!(config.analytics_url.is_none());
    }
}
