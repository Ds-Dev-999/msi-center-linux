use crate::ec::{
    EcError, EmbeddedController, MSI_ADDRESS_COOLER_BOOST, MSI_ADDRESS_CPU_FAN_SPEED,
    MSI_ADDRESS_CPU_TEMP, MSI_ADDRESS_FAN1_BASE, MSI_ADDRESS_FAN2_BASE, MSI_ADDRESS_FAN_MODE,
    MSI_ADDRESS_GPU_FAN_SPEED, MSI_ADDRESS_GPU_TEMP,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FanError {
    #[error("EC error: {0}")]
    EcError(#[from] EcError),
    #[error("Invalid fan speed: {0}")]
    InvalidSpeed(u8),
    #[error("Fan not found: {0}")]
    FanNotFound(String),
    #[error("Hwmon interface error: {0}")]
    HwmonError(String),
}

pub type Result<T> = std::result::Result<T, FanError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FanMode {
    Auto = 0,
    Silent = 1,
    Basic = 2,
    Advanced = 3,
}

impl From<u8> for FanMode {
    fn from(value: u8) -> Self {
        match value {
            0 => FanMode::Auto,
            1 => FanMode::Silent,
            2 => FanMode::Basic,
            3 => FanMode::Advanced,
            _ => FanMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanCurvePoint {
    pub temp: u8,
    pub speed: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanCurve {
    pub points: Vec<FanCurvePoint>,
}

impl Default for FanCurve {
    fn default() -> Self {
        Self {
            points: vec![
                FanCurvePoint { temp: 40, speed: 0 },
                FanCurvePoint { temp: 50, speed: 30 },
                FanCurvePoint { temp: 60, speed: 50 },
                FanCurvePoint { temp: 70, speed: 70 },
                FanCurvePoint { temp: 80, speed: 90 },
                FanCurvePoint { temp: 90, speed: 100 },
            ],
        }
    }
}

impl FanCurve {
    pub fn silent() -> Self {
        Self {
            points: vec![
                FanCurvePoint { temp: 50, speed: 0 },
                FanCurvePoint { temp: 60, speed: 20 },
                FanCurvePoint { temp: 70, speed: 40 },
                FanCurvePoint { temp: 80, speed: 60 },
                FanCurvePoint { temp: 90, speed: 80 },
                FanCurvePoint { temp: 95, speed: 100 },
            ],
        }
    }

    pub fn performance() -> Self {
        Self {
            points: vec![
                FanCurvePoint { temp: 35, speed: 30 },
                FanCurvePoint { temp: 45, speed: 50 },
                FanCurvePoint { temp: 55, speed: 70 },
                FanCurvePoint { temp: 65, speed: 85 },
                FanCurvePoint { temp: 75, speed: 100 },
                FanCurvePoint { temp: 85, speed: 100 },
            ],
        }
    }

    pub fn get_speed_for_temp(&self, temp: u8) -> u8 {
        if self.points.is_empty() {
            return 50;
        }

        if temp <= self.points[0].temp {
            return self.points[0].speed;
        }

        if temp >= self.points.last().unwrap().temp {
            return self.points.last().unwrap().speed;
        }

        for i in 0..self.points.len() - 1 {
            let p1 = &self.points[i];
            let p2 = &self.points[i + 1];

            if temp >= p1.temp && temp <= p2.temp {
                let temp_range = (p2.temp - p1.temp) as f32;
                let speed_range = (p2.speed as i16 - p1.speed as i16) as f32;
                let temp_offset = (temp - p1.temp) as f32;

                let interpolated = p1.speed as f32 + (temp_offset / temp_range) * speed_range;
                return interpolated.clamp(0.0, 100.0) as u8;
            }
        }

        50
    }
}

#[derive(Debug, Clone)]
pub struct FanInfo {
    pub cpu_fan_rpm: u32,
    pub gpu_fan_rpm: u32,
    pub cpu_fan_percent: u8,
    pub gpu_fan_percent: u8,
    pub cpu_temp: u8,
    pub gpu_temp: u8,
    pub fan_mode: FanMode,
    pub cooler_boost: bool,
}

pub struct FanController {
    ec: EmbeddedController,
    cpu_curve: FanCurve,
    gpu_curve: FanCurve,
    coretemp_path: Option<String>,
}

impl FanController {
    pub fn new(ec: EmbeddedController) -> Self {
        let coretemp_path = Self::find_coretemp_path();
        Self {
            ec,
            cpu_curve: FanCurve::default(),
            gpu_curve: FanCurve::default(),
            coretemp_path,
        }
    }

    fn find_coretemp_path() -> Option<String> {
        let hwmon_base = "/sys/class/hwmon";
        if let Ok(entries) = fs::read_dir(hwmon_base) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name_file = path.join("name");
                if let Ok(name) = fs::read_to_string(&name_file) {
                    if name.trim() == "coretemp" {
                        return Some(path.to_string_lossy().to_string());
                    }
                }
            }
        }
        None
    }

    fn read_cpu_temp_from_hwmon(&self) -> Option<u8> {
        if let Some(ref path) = self.coretemp_path {
            let temp_path = format!("{}/temp1_input", path);
            if let Ok(content) = fs::read_to_string(&temp_path) {
                if let Ok(millidegrees) = content.trim().parse::<i32>() {
                    return Some((millidegrees / 1000) as u8);
                }
            }
        }
        
        for i in 0..3 {
            let tz_path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
            if let Ok(content) = fs::read_to_string(&tz_path) {
                if let Ok(millidegrees) = content.trim().parse::<i32>() {
                    let temp = (millidegrees / 1000) as u8;
                    if temp > 20 && temp < 110 {
                        return Some(temp);
                    }
                }
            }
        }
        None
    }

    fn read_gpu_temp_from_hwmon(&self) -> Option<u8> {
        let hwmon_base = "/sys/class/hwmon";
        if let Ok(entries) = fs::read_dir(hwmon_base) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name_file = path.join("name");
                if let Ok(name) = fs::read_to_string(&name_file) {
                    let name = name.trim().to_lowercase();
                    if name.contains("nvidia") || name.contains("amdgpu") || name.contains("nouveau") {
                        let temp_path = path.join("temp1_input");
                        if let Ok(content) = fs::read_to_string(&temp_path) {
                            if let Ok(millidegrees) = content.trim().parse::<i32>() {
                                return Some((millidegrees / 1000) as u8);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn read_ec_byte(&self, address: u8) -> Option<u8> {
        let ec_path = "/sys/kernel/debug/ec/ec0/io";
        if let Ok(mut file) = fs::File::open(ec_path) {
            let mut buf = [0u8; 1];
            if file.seek(SeekFrom::Start(address as u64)).is_ok() {
                if file.read_exact(&mut buf).is_ok() {
                    return Some(buf[0]);
                }
            }
        }
        None
    }

    fn write_ec_byte(&mut self, address: u8, value: u8) -> Result<()> {
        use std::io::Write;
        let ec_path = "/sys/kernel/debug/ec/ec0/io";
        if let Ok(mut file) = fs::OpenOptions::new().write(true).open(ec_path) {
            if file.seek(SeekFrom::Start(address as u64)).is_ok() {
                if file.write_all(&[value]).is_ok() {
                    return Ok(());
                }
            }
        }
        self.ec.write_byte(address, value)?;
        Ok(())
    }

    fn read_fan_rpm_from_ec(&self, fan_num: u8) -> (u32, u8) {
        let address = if fan_num == 1 { 0xC8 } else { 0xCA };
        
        if let Some(raw) = self.read_ec_byte(address) {
            if raw > 0 {
                let rpm = (raw as u32) * 100;
                let percent = ((raw as f32 / 150.0) * 100.0).clamp(0.0, 100.0) as u8;
                return (rpm, percent);
            }
        }
        
        let realtime_addr = if fan_num == 1 { 0xC9 } else { 0xCB };
        if let Some(raw) = self.read_ec_byte(realtime_addr) {
            if raw > 0 {
                let rpm = (raw as u32) * 100;
                let percent = ((raw as f32 / 150.0) * 100.0).clamp(0.0, 100.0) as u8;
                return (rpm, percent);
            }
        }
        
        (0, 0)
    }

    pub fn get_fan_info(&mut self) -> Result<FanInfo> {
        let cpu_temp = self.read_cpu_temp_from_hwmon()
            .or_else(|| self.read_ec_byte(MSI_ADDRESS_CPU_TEMP))
            .or_else(|| self.ec.read_byte(MSI_ADDRESS_CPU_TEMP).ok())
            .unwrap_or(0);

        let gpu_temp = self.read_gpu_temp_from_hwmon()
            .or_else(|| self.read_ec_byte(MSI_ADDRESS_GPU_TEMP))
            .or_else(|| self.ec.read_byte(MSI_ADDRESS_GPU_TEMP).ok())
            .unwrap_or(0);

        let (cpu_fan_rpm, cpu_fan_percent) = self.read_fan_rpm_from_ec(1);
        let (gpu_fan_rpm, gpu_fan_percent) = self.read_fan_rpm_from_ec(2);

        let fan_mode_raw = self.read_ec_byte(MSI_ADDRESS_FAN_MODE)
            .or_else(|| self.ec.read_byte(MSI_ADDRESS_FAN_MODE).ok())
            .unwrap_or(0);

        let cooler_boost_raw = self.read_ec_byte(MSI_ADDRESS_COOLER_BOOST)
            .or_else(|| self.ec.read_byte(MSI_ADDRESS_COOLER_BOOST).ok())
            .unwrap_or(0);

        Ok(FanInfo {
            cpu_fan_rpm,
            gpu_fan_rpm,
            cpu_fan_percent,
            gpu_fan_percent,
            cpu_temp,
            gpu_temp,
            fan_mode: FanMode::from(fan_mode_raw & 0x0F),
            cooler_boost: (cooler_boost_raw & 0x80) != 0,
        })
    }

    fn calculate_rpm(&self, raw_value: u8) -> u32 {
        if raw_value == 0 {
            return 0;
        }
        (raw_value as u32) * 100
    }

    pub fn set_fan_mode(&mut self, mode: FanMode) -> Result<()> {
        let mode_value = mode as u8;
        self.write_ec_byte(MSI_ADDRESS_FAN_MODE, mode_value)?;
        Ok(())
    }

    pub fn set_cooler_boost(&mut self, enabled: bool) -> Result<()> {
        let current = self.read_ec_byte(MSI_ADDRESS_COOLER_BOOST).unwrap_or(0);
        let new_value = if enabled {
            current | 0x80
        } else {
            current & 0x7F
        };
        self.write_ec_byte(MSI_ADDRESS_COOLER_BOOST, new_value)?;
        Ok(())
    }

    pub fn set_cpu_fan_curve(&mut self, curve: FanCurve) -> Result<()> {
        self.apply_fan_curve(MSI_ADDRESS_FAN1_BASE, &curve)?;
        self.cpu_curve = curve;
        Ok(())
    }

    pub fn set_gpu_fan_curve(&mut self, curve: FanCurve) -> Result<()> {
        self.apply_fan_curve(MSI_ADDRESS_FAN2_BASE, &curve)?;
        self.gpu_curve = curve;
        Ok(())
    }

    fn apply_fan_curve(&mut self, base_address: u8, curve: &FanCurve) -> Result<()> {
        let num_points = curve.points.len().min(6);
        
        for (i, point) in curve.points.iter().take(num_points).enumerate() {
            let temp_addr = base_address + (i as u8 * 2);
            let speed_addr = temp_addr + 1;
            
            self.write_ec_byte(temp_addr, point.temp)?;
            let speed_value = ((point.speed as u16 * 255) / 100) as u8;
            self.write_ec_byte(speed_addr, speed_value)?;
        }

        Ok(())
    }

    pub fn set_manual_fan_speed(&mut self, cpu_percent: u8, gpu_percent: u8) -> Result<()> {
        if cpu_percent > 100 || gpu_percent > 100 {
            return Err(FanError::InvalidSpeed(cpu_percent.max(gpu_percent)));
        }

        self.set_fan_mode(FanMode::Advanced)?;

        let cpu_value = ((cpu_percent as u16 * 255) / 100) as u8;
        let gpu_value = ((gpu_percent as u16 * 255) / 100) as u8;

        for i in 0..6u8 {
            self.write_ec_byte(MSI_ADDRESS_FAN1_BASE + (i * 2), 0)?;
            self.write_ec_byte(MSI_ADDRESS_FAN1_BASE + (i * 2) + 1, cpu_value)?;
            self.write_ec_byte(MSI_ADDRESS_FAN2_BASE + (i * 2), 0)?;
            self.write_ec_byte(MSI_ADDRESS_FAN2_BASE + (i * 2) + 1, gpu_value)?;
        }

        Ok(())
    }

    pub fn get_cpu_curve(&self) -> &FanCurve {
        &self.cpu_curve
    }

    pub fn get_gpu_curve(&self) -> &FanCurve {
        &self.gpu_curve
    }

    pub fn reset_to_auto(&mut self) -> Result<()> {
        self.set_fan_mode(FanMode::Auto)?;
        self.set_cooler_boost(false)?;
        Ok(())
    }
}
