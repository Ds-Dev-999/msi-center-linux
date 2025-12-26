use crate::fan::FanCurve;
use crate::scenario::{ScenarioSettings, ShiftMode, UserScenario};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Config directory not found")]
    ConfigDirNotFound,
}

pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub scenario: UserScenario,
    pub settings: ScenarioSettings,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            scenario: UserScenario::Balanced,
            settings: ScenarioSettings::balanced(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub active_profile: String,
    pub profiles: Vec<Profile>,
    pub auto_start: bool,
    pub apply_on_boot: bool,
    pub show_notifications: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_profile: "Balanced".to_string(),
            profiles: vec![
                Profile {
                    name: "Silent".to_string(),
                    scenario: UserScenario::Silent,
                    settings: ScenarioSettings::silent(),
                },
                Profile {
                    name: "Balanced".to_string(),
                    scenario: UserScenario::Balanced,
                    settings: ScenarioSettings::balanced(),
                },
                Profile {
                    name: "High Performance".to_string(),
                    scenario: UserScenario::HighPerformance,
                    settings: ScenarioSettings::high_performance(),
                },
                Profile {
                    name: "Turbo".to_string(),
                    scenario: UserScenario::Turbo,
                    settings: ScenarioSettings::turbo(),
                },
                Profile {
                    name: "Super Battery".to_string(),
                    scenario: UserScenario::SuperBattery,
                    settings: ScenarioSettings::super_battery(),
                },
            ],
            auto_start: false,
            apply_on_boot: true,
            show_notifications: true,
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or(ConfigError::ConfigDirNotFound)?
            .join("msi-center-linux");
        
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)?;
        }
        
        Ok(config_dir)
    }

    pub fn config_file() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.json"))
    }

    pub fn load() -> Result<Self> {
        let config_file = Self::config_file()?;
        
        if !config_file.exists() {
            let default_config = Self::default();
            default_config.save()?;
            return Ok(default_config);
        }
        
        let content = fs::read_to_string(&config_file)?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_file = Self::config_file()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&config_file, content)?;
        Ok(())
    }

    pub fn get_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn get_active_profile(&self) -> Option<&Profile> {
        self.get_profile(&self.active_profile)
    }

    pub fn set_active_profile(&mut self, name: &str) -> bool {
        if self.profiles.iter().any(|p| p.name == name) {
            self.active_profile = name.to_string();
            true
        } else {
            false
        }
    }

    pub fn add_profile(&mut self, profile: Profile) {
        if !self.profiles.iter().any(|p| p.name == profile.name) {
            self.profiles.push(profile);
        }
    }

    pub fn remove_profile(&mut self, name: &str) -> bool {
        if let Some(pos) = self.profiles.iter().position(|p| p.name == name) {
            if self.profiles.len() > 1 {
                self.profiles.remove(pos);
                if self.active_profile == name {
                    self.active_profile = self.profiles[0].name.clone();
                }
                return true;
            }
        }
        false
    }

    pub fn create_custom_profile(&mut self, name: &str, cpu_curve: FanCurve, gpu_curve: FanCurve, shift_mode: ShiftMode) {
        let settings = ScenarioSettings {
            shift_mode,
            fan_mode: crate::fan::FanMode::Advanced,
            cooler_boost: false,
            super_battery: false,
            cpu_fan_curve: Some(cpu_curve),
            gpu_fan_curve: Some(gpu_curve),
        };

        let profile = Profile {
            name: name.to_string(),
            scenario: UserScenario::Custom,
            settings,
        };

        self.add_profile(profile);
    }
}
