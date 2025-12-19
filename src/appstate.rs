use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{create_dir_all, File},
    path::PathBuf,
};
use log::debug;

fn get_config_dir() -> Result<PathBuf> {
    Ok(dirs_next::data_local_dir()
        .ok_or_else(|| anyhow!("Can't get local dir"))?
        .join("oculante"))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PersistentSettings {
    pub vsync: bool,
    pub show_checker_background: bool,
}

impl Default for PersistentSettings {
    fn default() -> Self {
        Self {
            vsync: true,
            show_checker_background: false,
        }
    }
}

impl PersistentSettings {
    pub fn load() -> Result<Self> {
        let config_path = get_config_dir()?.join("config.json");
        debug!("Loading persistent settings from: {}", config_path.display());
        let file = File::open(config_path)?;
        Ok(serde_json::from_reader(file)?)
    }

    pub fn save_blocking(&self) -> Result<()> {
        let config_dir = get_config_dir()?;
        if !config_dir.exists() {
            create_dir_all(&config_dir)?;
        }
        let config_path = config_dir.join("config.json");
        let f = File::create(&config_path)?;
        serde_json::to_writer_pretty(f, self)?;
        debug!("Saved persistent settings to: {}", config_path.display());
        Ok(())
    }
}