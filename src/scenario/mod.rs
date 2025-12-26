use crate::ec::{EcError, EmbeddedController, MSI_ADDRESS_SHIFT_MODE, MSI_ADDRESS_SUPER_BATTERY};
use crate::fan::{FanController, FanCurve, FanError, FanMode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScenarioError {
    #[error("EC error: {0}")]
    EcError(#[from] EcError),
    #[error("Fan error: {0}")]
    FanError(#[from] FanError),
    #[error("Invalid scenario: {0}")]
    InvalidScenario(String),
}

pub type Result<T> = std::result::Result<T, ScenarioError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShiftMode {
    EcoSilent = 0xC2,
    Comfort = 0xC1,
    Sport = 0xC0,
    Turbo = 0xC4,
}

impl From<u8> for ShiftMode {
    fn from(value: u8) -> Self {
        match value {
            0xC2 => ShiftMode::EcoSilent,
            0xC1 => ShiftMode::Comfort,
            0xC0 => ShiftMode::Sport,
            0xC4 => ShiftMode::Turbo,
            _ => ShiftMode::Comfort,
        }
    }
}

impl std::fmt::Display for ShiftMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShiftMode::EcoSilent => write!(f, "Eco/Silent"),
            ShiftMode::Comfort => write!(f, "Comfort/Balanced"),
            ShiftMode::Sport => write!(f, "Sport/Performance"),
            ShiftMode::Turbo => write!(f, "Turbo/Extreme"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserScenario {
    Silent,
    Balanced,
    HighPerformance,
    Turbo,
    SuperBattery,
    Custom,
}

impl std::fmt::Display for UserScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserScenario::Silent => write!(f, "Silent"),
            UserScenario::Balanced => write!(f, "Balanced"),
            UserScenario::HighPerformance => write!(f, "High Performance"),
            UserScenario::Turbo => write!(f, "Turbo"),
            UserScenario::SuperBattery => write!(f, "Super Battery"),
            UserScenario::Custom => write!(f, "Custom"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioSettings {
    pub shift_mode: ShiftMode,
    pub fan_mode: FanMode,
    pub cooler_boost: bool,
    pub super_battery: bool,
    pub cpu_fan_curve: Option<FanCurve>,
    pub gpu_fan_curve: Option<FanCurve>,
}

impl ScenarioSettings {
    pub fn silent() -> Self {
        Self {
            shift_mode: ShiftMode::EcoSilent,
            fan_mode: FanMode::Silent,
            cooler_boost: false,
            super_battery: false,
            cpu_fan_curve: Some(FanCurve::silent()),
            gpu_fan_curve: Some(FanCurve::silent()),
        }
    }

    pub fn balanced() -> Self {
        Self {
            shift_mode: ShiftMode::Comfort,
            fan_mode: FanMode::Auto,
            cooler_boost: false,
            super_battery: false,
            cpu_fan_curve: Some(FanCurve::default()),
            gpu_fan_curve: Some(FanCurve::default()),
        }
    }

    pub fn high_performance() -> Self {
        Self {
            shift_mode: ShiftMode::Sport,
            fan_mode: FanMode::Basic,
            cooler_boost: false,
            super_battery: false,
            cpu_fan_curve: Some(FanCurve::performance()),
            gpu_fan_curve: Some(FanCurve::performance()),
        }
    }

    pub fn turbo() -> Self {
        Self {
            shift_mode: ShiftMode::Turbo,
            fan_mode: FanMode::Advanced,
            cooler_boost: true,
            super_battery: false,
            cpu_fan_curve: Some(FanCurve::performance()),
            gpu_fan_curve: Some(FanCurve::performance()),
        }
    }

    pub fn super_battery() -> Self {
        Self {
            shift_mode: ShiftMode::EcoSilent,
            fan_mode: FanMode::Silent,
            cooler_boost: false,
            super_battery: true,
            cpu_fan_curve: Some(FanCurve::silent()),
            gpu_fan_curve: Some(FanCurve::silent()),
        }
    }
}

#[derive(Debug)]
pub struct ScenarioInfo {
    pub current_scenario: UserScenario,
    pub shift_mode: ShiftMode,
    pub super_battery: bool,
}

pub struct ScenarioManager<'a> {
    ec: &'a mut EmbeddedController,
    fan_controller: &'a mut FanController,
    current_scenario: UserScenario,
}

impl<'a> ScenarioManager<'a> {
    pub fn new(ec: &'a mut EmbeddedController, fan_controller: &'a mut FanController) -> Self {
        Self {
            ec,
            fan_controller,
            current_scenario: UserScenario::Balanced,
        }
    }

    pub fn get_current_info(&mut self) -> Result<ScenarioInfo> {
        let shift_mode_raw = self.ec.read_byte(MSI_ADDRESS_SHIFT_MODE).unwrap_or(0xC1);
        let super_battery_raw = self.ec.read_byte(MSI_ADDRESS_SUPER_BATTERY).unwrap_or(0);

        let shift_mode = ShiftMode::from(shift_mode_raw);
        let super_battery = (super_battery_raw & 0x01) != 0;

        let current_scenario = self.detect_scenario(shift_mode, super_battery);

        Ok(ScenarioInfo {
            current_scenario,
            shift_mode,
            super_battery,
        })
    }

    fn detect_scenario(&self, shift_mode: ShiftMode, super_battery: bool) -> UserScenario {
        if super_battery {
            return UserScenario::SuperBattery;
        }

        match shift_mode {
            ShiftMode::EcoSilent => UserScenario::Silent,
            ShiftMode::Comfort => UserScenario::Balanced,
            ShiftMode::Sport => UserScenario::HighPerformance,
            ShiftMode::Turbo => UserScenario::Turbo,
        }
    }

    pub fn set_scenario(&mut self, scenario: UserScenario) -> Result<()> {
        let settings = match scenario {
            UserScenario::Silent => ScenarioSettings::silent(),
            UserScenario::Balanced => ScenarioSettings::balanced(),
            UserScenario::HighPerformance => ScenarioSettings::high_performance(),
            UserScenario::Turbo => ScenarioSettings::turbo(),
            UserScenario::SuperBattery => ScenarioSettings::super_battery(),
            UserScenario::Custom => return Ok(()),
        };

        self.apply_settings(&settings)?;
        self.current_scenario = scenario;
        
        Ok(())
    }

    pub fn apply_settings(&mut self, settings: &ScenarioSettings) -> Result<()> {
        self.ec.write_byte(MSI_ADDRESS_SHIFT_MODE, settings.shift_mode as u8)?;

        let super_battery_value = if settings.super_battery { 0x01 } else { 0x00 };
        self.ec.write_byte(MSI_ADDRESS_SUPER_BATTERY, super_battery_value)?;

        self.fan_controller.set_fan_mode(settings.fan_mode)?;
        self.fan_controller.set_cooler_boost(settings.cooler_boost)?;

        if let Some(ref curve) = settings.cpu_fan_curve {
            self.fan_controller.set_cpu_fan_curve(curve.clone())?;
        }

        if let Some(ref curve) = settings.gpu_fan_curve {
            self.fan_controller.set_gpu_fan_curve(curve.clone())?;
        }

        Ok(())
    }

    pub fn set_shift_mode(&mut self, mode: ShiftMode) -> Result<()> {
        self.ec.write_byte(MSI_ADDRESS_SHIFT_MODE, mode as u8)?;
        Ok(())
    }

    pub fn set_super_battery(&mut self, enabled: bool) -> Result<()> {
        let value = if enabled { 0x01 } else { 0x00 };
        self.ec.write_byte(MSI_ADDRESS_SUPER_BATTERY, value)?;
        Ok(())
    }

    pub fn get_available_scenarios() -> Vec<UserScenario> {
        vec![
            UserScenario::Silent,
            UserScenario::Balanced,
            UserScenario::HighPerformance,
            UserScenario::Turbo,
            UserScenario::SuperBattery,
        ]
    }
}

pub fn apply_scenario_standalone(scenario: UserScenario) -> Result<()> {
    let mut ec = EmbeddedController::new()?;
    let mut fan_controller = FanController::new(EmbeddedController::new()?);
    let mut manager = ScenarioManager::new(&mut ec, &mut fan_controller);
    manager.set_scenario(scenario)
}
