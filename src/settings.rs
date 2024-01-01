use eyre::{eyre, Result};
use directories::ProjectDirs;
use serde::Deserialize;
use std::fs::File;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct NotionSettings {
    pub api_key: String,
    pub database_id: notion::ids::DatabaseId,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub notion: NotionSettings,
}

impl Settings {
    pub fn new() -> Result<Self> {
        Self::config_path()
            .and_then(|path| File::open(path).map_err(eyre::Error::new))
            .and_then(|file| serde_yaml::from_reader(file).map_err(eyre::Error::new))
    }

    pub fn config_path() -> Result<PathBuf> {
        let config_path = ProjectDirs::from("", "", "notion")
            .ok_or_else(|| eyre!("Couldn't retrive project dirs"))
            .map(|prj_dirs| prj_dirs.config_dir().join("config.yaml"))?;

        if !config_path.exists() {
            File::create(&config_path)?;
        }

        Ok(config_path)
    }
}
