use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EcError {
    #[error("Failed to open EC port: {0}")]
    OpenError(#[from] std::io::Error),
    #[error("Permission denied. Run as root or add user to appropriate group")]
    PermissionDenied,
    #[error("EC not found or not supported")]
    NotSupported,
    #[error("Invalid EC address: {0:#x}")]
    InvalidAddress(u16),
    #[error("EC read/write failed")]
    IoFailed,
}

pub type Result<T> = std::result::Result<T, EcError>;

const EC_SC: u16 = 0x66;
const EC_DATA: u16 = 0x62;
const EC_SC_READ_CMD: u8 = 0x80;
const EC_SC_WRITE_CMD: u8 = 0x81;
const EC_SC_IBF: u8 = 0x02;
const EC_SC_OBF: u8 = 0x01;

pub const MSI_ADDRESS_CPU_FAN_SPEED: u8 = 0xC8;
pub const MSI_ADDRESS_GPU_FAN_SPEED: u8 = 0xCA;
pub const MSI_ADDRESS_CPU_TEMP: u8 = 0x68;
pub const MSI_ADDRESS_GPU_TEMP: u8 = 0x80;
pub const MSI_ADDRESS_FAN_MODE: u8 = 0xD4;
pub const MSI_ADDRESS_COOLER_BOOST: u8 = 0x98;
pub const MSI_ADDRESS_SHIFT_MODE: u8 = 0xD2;
pub const MSI_ADDRESS_SUPER_BATTERY: u8 = 0xEB;
pub const MSI_ADDRESS_FAN1_BASE: u8 = 0x72;
pub const MSI_ADDRESS_FAN2_BASE: u8 = 0x8A;

pub struct EmbeddedController {
    port_file: Option<File>,
    use_acpi: bool,
    acpi_path: Option<String>,
}

impl EmbeddedController {
    pub fn new() -> Result<Self> {
        if let Ok(ec) = Self::try_direct_port_access() {
            return Ok(ec);
        }

        if let Ok(ec) = Self::try_acpi_access() {
            return Ok(ec);
        }

        if let Ok(ec) = Self::try_msi_ec_driver() {
            return Ok(ec);
        }

        Err(EcError::NotSupported)
    }

    fn try_direct_port_access() -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/port")
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    EcError::PermissionDenied
                } else {
                    EcError::OpenError(e)
                }
            })?;

        Ok(Self {
            port_file: Some(file),
            use_acpi: false,
            acpi_path: None,
        })
    }

    fn try_acpi_access() -> Result<Self> {
        let acpi_path = "/sys/kernel/debug/ec/ec0/io";
        if std::path::Path::new(acpi_path).exists() {
            return Ok(Self {
                port_file: None,
                use_acpi: true,
                acpi_path: Some(acpi_path.to_string()),
            });
        }
        Err(EcError::NotSupported)
    }

    fn try_msi_ec_driver() -> Result<Self> {
        let msi_ec_path = "/sys/devices/platform/msi-ec";
        if std::path::Path::new(msi_ec_path).exists() {
            return Ok(Self {
                port_file: None,
                use_acpi: true,
                acpi_path: Some(msi_ec_path.to_string()),
            });
        }
        Err(EcError::NotSupported)
    }

    fn wait_ec_ibf_clear(&mut self) -> Result<()> {
        if let Some(ref mut file) = self.port_file {
            for _ in 0..10000 {
                file.seek(SeekFrom::Start(EC_SC as u64))?;
                let mut buf = [0u8; 1];
                file.read_exact(&mut buf)?;
                if (buf[0] & EC_SC_IBF) == 0 {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_micros(10));
            }
        }
        Err(EcError::IoFailed)
    }

    fn wait_ec_obf_set(&mut self) -> Result<()> {
        if let Some(ref mut file) = self.port_file {
            for _ in 0..10000 {
                file.seek(SeekFrom::Start(EC_SC as u64))?;
                let mut buf = [0u8; 1];
                file.read_exact(&mut buf)?;
                if (buf[0] & EC_SC_OBF) != 0 {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_micros(10));
            }
        }
        Err(EcError::IoFailed)
    }

    fn write_port(&mut self, port: u16, value: u8) -> Result<()> {
        if let Some(ref mut file) = self.port_file {
            file.seek(SeekFrom::Start(port as u64))?;
            file.write_all(&[value])?;
            Ok(())
        } else {
            Err(EcError::IoFailed)
        }
    }

    fn read_port(&mut self, port: u16) -> Result<u8> {
        if let Some(ref mut file) = self.port_file {
            file.seek(SeekFrom::Start(port as u64))?;
            let mut buf = [0u8; 1];
            file.read_exact(&mut buf)?;
            Ok(buf[0])
        } else {
            Err(EcError::IoFailed)
        }
    }

    pub fn read_byte(&mut self, address: u8) -> Result<u8> {
        if self.use_acpi {
            return self.read_byte_acpi(address);
        }

        self.wait_ec_ibf_clear()?;
        self.write_port(EC_SC, EC_SC_READ_CMD)?;
        self.wait_ec_ibf_clear()?;
        self.write_port(EC_DATA, address)?;
        self.wait_ec_obf_set()?;
        self.read_port(EC_DATA)
    }

    pub fn write_byte(&mut self, address: u8, value: u8) -> Result<()> {
        if self.use_acpi {
            return self.write_byte_acpi(address, value);
        }

        self.wait_ec_ibf_clear()?;
        self.write_port(EC_SC, EC_SC_WRITE_CMD)?;
        self.wait_ec_ibf_clear()?;
        self.write_port(EC_DATA, address)?;
        self.wait_ec_ibf_clear()?;
        self.write_port(EC_DATA, value)?;
        Ok(())
    }

    fn read_byte_acpi(&self, address: u8) -> Result<u8> {
        if let Some(ref path) = self.acpi_path {
            if path.contains("msi-ec") {
                return self.read_msi_ec_driver(address);
            }
            let mut file = OpenOptions::new().read(true).open(path)?;
            file.seek(SeekFrom::Start(address as u64))?;
            let mut buf = [0u8; 1];
            file.read_exact(&mut buf)?;
            return Ok(buf[0]);
        }
        Err(EcError::NotSupported)
    }

    fn write_byte_acpi(&self, address: u8, value: u8) -> Result<()> {
        if let Some(ref path) = self.acpi_path {
            if path.contains("msi-ec") {
                return self.write_msi_ec_driver(address, value);
            }
            let mut file = OpenOptions::new().write(true).open(path)?;
            file.seek(SeekFrom::Start(address as u64))?;
            file.write_all(&[value])?;
            return Ok(());
        }
        Err(EcError::NotSupported)
    }

    fn read_msi_ec_driver(&self, address: u8) -> Result<u8> {
        let sysfs_map = self.get_sysfs_mapping(address);
        if let Some(path) = sysfs_map {
            let content = std::fs::read_to_string(path)?;
            let value: u8 = content.trim().parse().unwrap_or(0);
            return Ok(value);
        }
        Err(EcError::NotSupported)
    }

    fn write_msi_ec_driver(&self, address: u8, value: u8) -> Result<()> {
        let sysfs_map = self.get_sysfs_mapping(address);
        if let Some(path) = sysfs_map {
            std::fs::write(path, format!("{}", value))?;
            return Ok(());
        }
        Err(EcError::NotSupported)
    }

    fn get_sysfs_mapping(&self, address: u8) -> Option<String> {
        let base = "/sys/devices/platform/msi-ec";
        match address {
            MSI_ADDRESS_SHIFT_MODE => Some(format!("{}/shift_mode", base)),
            MSI_ADDRESS_SUPER_BATTERY => Some(format!("{}/super_battery", base)),
            MSI_ADDRESS_COOLER_BOOST => Some(format!("{}/cooler_boost", base)),
            MSI_ADDRESS_FAN_MODE => Some(format!("{}/fan_mode", base)),
            _ => None,
        }
    }

    pub fn is_msi_laptop(&mut self) -> bool {
        if let Ok(vendor) = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor") {
            return vendor.to_lowercase().contains("micro-star") || 
                   vendor.to_lowercase().contains("msi");
        }
        false
    }
}

impl Default for EmbeddedController {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            port_file: None,
            use_acpi: false,
            acpi_path: None,
        })
    }
}
