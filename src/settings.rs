use eyre::{eyre, Result};
use directories::ProjectDirs;
use serde::Deserialize;
use std::collections::HashMap;
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
    #[serde(deserialize_with = "de_map")]
    pub map: HashMap<String, String>,
}

fn de_map<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>
{
    let map: HashMap<String, Vec<String>> = Deserialize::deserialize(deserializer)?;

    let result = map.iter().fold(HashMap::new(), |mut acc, item| {
        item.1.iter().for_each(|name| {
            let _ = acc.insert(name.clone().to_lowercase(), item.0.clone());
        });

        acc
    });

    Ok(result)
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

    pub fn get(&self, key: &str) -> Option<&String> {
        self.map.get(&key.to_lowercase())
    }
}
