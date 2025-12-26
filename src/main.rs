mod config;
mod ec;
mod fan;
mod scenario;

use clap::{Parser, Subcommand};
use colored::Colorize;
use config::{AppConfig, Profile};
use ec::EmbeddedController;
use fan::{FanController, FanCurve, FanCurvePoint, FanMode};
use scenario::{ScenarioManager, ShiftMode, UserScenario};
use std::process;

#[derive(Parser)]
#[command(name = "msi-center")]
#[command(author = "MSI Center Linux")]
#[command(version = "0.1.0")]
#[command(about = "MSI Center clone for Linux - Control laptop fans and user scenarios")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show current system status (fans, temps, scenario)
    Status,

    /// Fan control commands
    Fan {
        #[command(subcommand)]
        action: FanCommands,
    },

    /// User scenario commands
    Scenario {
        #[command(subcommand)]
        action: ScenarioCommands,
    },

    /// Profile management commands
    Profile {
        #[command(subcommand)]
        action: ProfileCommands,
    },

    /// Monitor system in real-time
    Monitor {
        /// Update interval in seconds
        #[arg(short, long, default_value = "1")]
        interval: u64,
    },

    /// Apply settings from active profile
    Apply,
}

#[derive(Subcommand)]
enum FanCommands {
    /// Show current fan status
    Status,

    /// Set fan mode
    Mode {
        /// Fan mode: auto, silent, basic, advanced
        #[arg(value_parser = parse_fan_mode)]
        mode: FanMode,
    },

    /// Enable or disable cooler boost
    CoolerBoost {
        /// Enable (on) or disable (off)
        #[arg(value_parser = parse_bool)]
        enabled: bool,
    },

    /// Set manual fan speed (requires advanced mode)
    Speed {
        /// CPU fan speed percentage (0-100)
        #[arg(short, long)]
        cpu: u8,

        /// GPU fan speed percentage (0-100)
        #[arg(short, long)]
        gpu: u8,
    },

    /// Set fan curve
    Curve {
        /// Fan to configure: cpu or gpu
        #[arg(short, long)]
        fan: String,

        /// Curve preset: silent, balanced, performance, or custom
        #[arg(short, long)]
        preset: String,

        /// Custom curve points (format: temp1:speed1,temp2:speed2,...)
        #[arg(short = 'p', long)]
        points: Option<String>,
    },

    /// Reset fans to automatic control
    Reset,
}

#[derive(Subcommand)]
enum ScenarioCommands {
    /// Show current scenario
    Status,

    /// List available scenarios
    List,

    /// Set user scenario
    Set {
        /// Scenario: silent, balanced, highperf, turbo, battery
        #[arg(value_parser = parse_scenario)]
        scenario: UserScenario,
    },

    /// Set shift mode directly
    Shift {
        /// Shift mode: eco, comfort, sport, turbo
        #[arg(value_parser = parse_shift_mode)]
        mode: ShiftMode,
    },

    /// Enable or disable super battery mode
    SuperBattery {
        /// Enable (on) or disable (off)
        #[arg(value_parser = parse_bool)]
        enabled: bool,
    },
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// List all profiles
    List,

    /// Show active profile
    Active,

    /// Set active profile
    Set {
        /// Profile name
        name: String,
    },

    /// Create a new custom profile
    Create {
        /// Profile name
        name: String,

        /// Base scenario: silent, balanced, highperf, turbo
        #[arg(short, long, default_value = "balanced")]
        base: String,
    },

    /// Delete a profile
    Delete {
        /// Profile name
        name: String,
    },

    /// Save current settings to active profile
    Save,
}

fn parse_fan_mode(s: &str) -> Result<FanMode, String> {
    match s.to_lowercase().as_str() {
        "auto" | "0" => Ok(FanMode::Auto),
        "silent" | "1" => Ok(FanMode::Silent),
        "basic" | "2" => Ok(FanMode::Basic),
        "advanced" | "3" => Ok(FanMode::Advanced),
        _ => Err(format!("Invalid fan mode: {}. Use: auto, silent, basic, advanced", s)),
    }
}

fn parse_scenario(s: &str) -> Result<UserScenario, String> {
    match s.to_lowercase().as_str() {
        "silent" | "quiet" => Ok(UserScenario::Silent),
        "balanced" | "comfort" => Ok(UserScenario::Balanced),
        "highperf" | "performance" | "sport" => Ok(UserScenario::HighPerformance),
        "turbo" | "extreme" => Ok(UserScenario::Turbo),
        "battery" | "superbattery" | "eco" => Ok(UserScenario::SuperBattery),
        _ => Err(format!("Invalid scenario: {}. Use: silent, balanced, highperf, turbo, battery", s)),
    }
}

fn parse_shift_mode(s: &str) -> Result<ShiftMode, String> {
    match s.to_lowercase().as_str() {
        "eco" | "silent" => Ok(ShiftMode::EcoSilent),
        "comfort" | "balanced" => Ok(ShiftMode::Comfort),
        "sport" | "performance" => Ok(ShiftMode::Sport),
        "turbo" | "extreme" => Ok(ShiftMode::Turbo),
        _ => Err(format!("Invalid shift mode: {}. Use: eco, comfort, sport, turbo", s)),
    }
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "on" | "true" | "1" | "yes" | "enable" => Ok(true),
        "off" | "false" | "0" | "no" | "disable" => Ok(false),
        _ => Err(format!("Invalid value: {}. Use: on/off, true/false, 1/0", s)),
    }
}

fn parse_curve_points(points_str: &str) -> Result<FanCurve, String> {
    let mut points = Vec::new();

    for pair in points_str.split(',') {
        let parts: Vec<&str> = pair.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid curve point format: {}. Use temp:speed", pair));
        }

        let temp: u8 = parts[0].parse().map_err(|_| format!("Invalid temperature: {}", parts[0]))?;
        let speed: u8 = parts[1].parse().map_err(|_| format!("Invalid speed: {}", parts[1]))?;

        if speed > 100 {
            return Err(format!("Speed must be 0-100, got: {}", speed));
        }

        points.push(FanCurvePoint { temp, speed });
    }

    points.sort_by_key(|p| p.temp);

    Ok(FanCurve { points })
}

fn check_root() {
    if !nix::unistd::geteuid().is_root() {
        eprintln!("{}", "Warning: Not running as root. Some features may not work.".yellow());
        eprintln!("{}", "Run with 'sudo' for full functionality.".yellow());
        println!();
    }
}

fn print_header(title: &str) {
    println!();
    println!("{}", format!("═══ {} ═══", title).cyan().bold());
    println!();
}

fn print_status_line(label: &str, value: &str, color: colored::Color) {
    println!("  {}: {}", label.white().bold(), value.color(color));
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    check_root();

    let result = match cli.command {
        Commands::Status => cmd_status(),
        Commands::Fan { action } => cmd_fan(action),
        Commands::Scenario { action } => cmd_scenario(action),
        Commands::Profile { action } => cmd_profile(action),
        Commands::Monitor { interval } => cmd_monitor(interval),
        Commands::Apply => cmd_apply(),
    };

    if let Err(e) = result {
        eprintln!("{}: {}", "Error".red().bold(), e);
        process::exit(1);
    }
}

fn cmd_status() -> Result<(), Box<dyn std::error::Error>> {
    print_header("MSI Center Linux - System Status");

    let mut ec = EmbeddedController::new()?;

    if !ec.is_msi_laptop() {
        println!("{}", "Warning: This may not be an MSI laptop.".yellow());
    }

    let mut fan_controller = FanController::new(EmbeddedController::new()?);
    let fan_info = fan_controller.get_fan_info()?;

    let mut ec2 = EmbeddedController::new()?;
    let mut scenario_manager = ScenarioManager::new(&mut ec2, &mut fan_controller);
    let scenario_info = scenario_manager.get_current_info()?;

    println!("{}", "── Temperatures ──".green());
    print_status_line("CPU Temperature", &format!("{}°C", fan_info.cpu_temp), get_temp_color(fan_info.cpu_temp));
    print_status_line("GPU Temperature", &format!("{}°C", fan_info.gpu_temp), get_temp_color(fan_info.gpu_temp));
    println!();

    println!("{}", "── Fan Status ──".green());
    print_status_line("CPU Fan", &format!("{} RPM ({}%)", fan_info.cpu_fan_rpm, fan_info.cpu_fan_percent), colored::Color::White);
    print_status_line("GPU Fan", &format!("{} RPM ({}%)", fan_info.gpu_fan_rpm, fan_info.gpu_fan_percent), colored::Color::White);
    print_status_line("Fan Mode", &format!("{:?}", fan_info.fan_mode), colored::Color::Cyan);
    print_status_line("Cooler Boost", if fan_info.cooler_boost { "ON" } else { "OFF" }, 
        if fan_info.cooler_boost { colored::Color::Red } else { colored::Color::Green });
    println!();

    println!("{}", "── Power Profile ──".green());
    print_status_line("Current Scenario", &scenario_info.current_scenario.to_string(), colored::Color::Cyan);
    print_status_line("Shift Mode", &scenario_info.shift_mode.to_string(), colored::Color::Cyan);
    print_status_line("Super Battery", if scenario_info.super_battery { "ON" } else { "OFF" },
        if scenario_info.super_battery { colored::Color::Green } else { colored::Color::White });

    println!();
    Ok(())
}

fn get_temp_color(temp: u8) -> colored::Color {
    match temp {
        0..=50 => colored::Color::Green,
        51..=70 => colored::Color::Yellow,
        71..=85 => colored::Color::Red,
        _ => colored::Color::BrightRed,
    }
}

fn cmd_fan(action: FanCommands) -> Result<(), Box<dyn std::error::Error>> {
    let ec = EmbeddedController::new()?;
    let mut fan_controller = FanController::new(ec);

    match action {
        FanCommands::Status => {
            let info = fan_controller.get_fan_info()?;
            print_header("Fan Status");
            print_status_line("CPU Fan", &format!("{} RPM ({}%)", info.cpu_fan_rpm, info.cpu_fan_percent), colored::Color::White);
            print_status_line("GPU Fan", &format!("{} RPM ({}%)", info.gpu_fan_rpm, info.gpu_fan_percent), colored::Color::White);
            print_status_line("CPU Temp", &format!("{}°C", info.cpu_temp), get_temp_color(info.cpu_temp));
            print_status_line("GPU Temp", &format!("{}°C", info.gpu_temp), get_temp_color(info.gpu_temp));
            print_status_line("Mode", &format!("{:?}", info.fan_mode), colored::Color::Cyan);
            print_status_line("Cooler Boost", if info.cooler_boost { "ON" } else { "OFF" }, colored::Color::Yellow);
            println!();
        }

        FanCommands::Mode { mode } => {
            fan_controller.set_fan_mode(mode)?;
            println!("{} Fan mode set to {:?}", "✓".green(), mode);
        }

        FanCommands::CoolerBoost { enabled } => {
            fan_controller.set_cooler_boost(enabled)?;
            println!("{} Cooler boost {}", "✓".green(), if enabled { "enabled" } else { "disabled" });
        }

        FanCommands::Speed { cpu, gpu } => {
            fan_controller.set_manual_fan_speed(cpu, gpu)?;
            println!("{} Manual fan speed set - CPU: {}%, GPU: {}%", "✓".green(), cpu, gpu);
        }

        FanCommands::Curve { fan, preset, points } => {
            let curve = match preset.as_str() {
                "silent" => FanCurve::silent(),
                "balanced" | "default" => FanCurve::default(),
                "performance" => FanCurve::performance(),
                "custom" => {
                    if let Some(pts) = points {
                        parse_curve_points(&pts)?
                    } else {
                        return Err("Custom curve requires --points argument".into());
                    }
                }
                _ => return Err(format!("Unknown preset: {}. Use: silent, balanced, performance, custom", preset).into()),
            };

            match fan.to_lowercase().as_str() {
                "cpu" => {
                    fan_controller.set_cpu_fan_curve(curve)?;
                    println!("{} CPU fan curve set to {}", "✓".green(), preset);
                }
                "gpu" => {
                    fan_controller.set_gpu_fan_curve(curve)?;
                    println!("{} GPU fan curve set to {}", "✓".green(), preset);
                }
                "both" | "all" => {
                    fan_controller.set_cpu_fan_curve(curve.clone())?;
                    fan_controller.set_gpu_fan_curve(curve)?;
                    println!("{} Both fan curves set to {}", "✓".green(), preset);
                }
                _ => return Err(format!("Unknown fan: {}. Use: cpu, gpu, both", fan).into()),
            }
        }

        FanCommands::Reset => {
            fan_controller.reset_to_auto()?;
            println!("{} Fans reset to automatic control", "✓".green());
        }
    }

    Ok(())
}

fn cmd_scenario(action: ScenarioCommands) -> Result<(), Box<dyn std::error::Error>> {
    let mut ec = EmbeddedController::new()?;
    let mut fan_controller = FanController::new(EmbeddedController::new()?);
    let mut manager = ScenarioManager::new(&mut ec, &mut fan_controller);

    match action {
        ScenarioCommands::Status => {
            let info = manager.get_current_info()?;
            print_header("Current Scenario");
            print_status_line("Scenario", &info.current_scenario.to_string(), colored::Color::Cyan);
            print_status_line("Shift Mode", &info.shift_mode.to_string(), colored::Color::Yellow);
            print_status_line("Super Battery", if info.super_battery { "ON" } else { "OFF" }, colored::Color::Green);
            println!();
        }

        ScenarioCommands::List => {
            print_header("Available Scenarios");
            for scenario in ScenarioManager::get_available_scenarios() {
                println!("  • {}", scenario.to_string().cyan());
            }
            println!();
        }

        ScenarioCommands::Set { scenario } => {
            manager.set_scenario(scenario)?;
            println!("{} Scenario set to {}", "✓".green(), scenario);
        }

        ScenarioCommands::Shift { mode } => {
            manager.set_shift_mode(mode)?;
            println!("{} Shift mode set to {}", "✓".green(), mode);
        }

        ScenarioCommands::SuperBattery { enabled } => {
            manager.set_super_battery(enabled)?;
            println!("{} Super battery {}", "✓".green(), if enabled { "enabled" } else { "disabled" });
        }
    }

    Ok(())
}

fn cmd_profile(action: ProfileCommands) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AppConfig::load()?;

    match action {
        ProfileCommands::List => {
            print_header("Profiles");
            for profile in &config.profiles {
                let marker = if profile.name == config.active_profile { "►" } else { " " };
                println!("  {} {} ({})", marker.green(), profile.name.cyan(), profile.scenario);
            }
            println!();
        }

        ProfileCommands::Active => {
            if let Some(profile) = config.get_active_profile() {
                print_header("Active Profile");
                print_status_line("Name", &profile.name, colored::Color::Cyan);
                print_status_line("Scenario", &profile.scenario.to_string(), colored::Color::Yellow);
                print_status_line("Shift Mode", &profile.settings.shift_mode.to_string(), colored::Color::White);
                print_status_line("Fan Mode", &format!("{:?}", profile.settings.fan_mode), colored::Color::White);
                print_status_line("Cooler Boost", if profile.settings.cooler_boost { "ON" } else { "OFF" }, colored::Color::White);
                println!();
            } else {
                println!("{}", "No active profile found".yellow());
            }
        }

        ProfileCommands::Set { name } => {
            if config.set_active_profile(&name) {
                config.save()?;
                println!("{} Active profile set to {}", "✓".green(), name.cyan());
            } else {
                println!("{} Profile '{}' not found", "✗".red(), name);
            }
        }

        ProfileCommands::Create { name, base } => {
            let scenario = parse_scenario(&base)?;
            let settings = match scenario {
                UserScenario::Silent => scenario::ScenarioSettings::silent(),
                UserScenario::Balanced => scenario::ScenarioSettings::balanced(),
                UserScenario::HighPerformance => scenario::ScenarioSettings::high_performance(),
                UserScenario::Turbo => scenario::ScenarioSettings::turbo(),
                UserScenario::SuperBattery => scenario::ScenarioSettings::super_battery(),
                UserScenario::Custom => scenario::ScenarioSettings::balanced(),
            };

            let profile = Profile {
                name: name.clone(),
                scenario,
                settings,
            };

            config.add_profile(profile);
            config.save()?;
            println!("{} Profile '{}' created based on {}", "✓".green(), name.cyan(), base);
        }

        ProfileCommands::Delete { name } => {
            if config.remove_profile(&name) {
                config.save()?;
                println!("{} Profile '{}' deleted", "✓".green(), name);
            } else {
                println!("{} Cannot delete profile '{}' (not found or last profile)", "✗".red(), name);
            }
        }

        ProfileCommands::Save => {
            println!("{} Current settings saved to active profile", "✓".green());
            config.save()?;
        }
    }

    Ok(())
}

fn cmd_monitor(interval: u64) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "Starting real-time monitoring. Press Ctrl+C to stop.".yellow());
    println!();

    loop {
        print!("\x1B[2J\x1B[1;1H");

        print_header("MSI Center Linux - Live Monitor");

        if let Ok(mut fan_controller) = EmbeddedController::new().map(FanController::new) {
            if let Ok(info) = fan_controller.get_fan_info() {
                println!("{}", "── System Status ──".green());
                println!();

                let cpu_bar = create_progress_bar(info.cpu_temp as f32, 100.0, 20);
                let gpu_bar = create_progress_bar(info.gpu_temp as f32, 100.0, 20);

                println!("  CPU Temp: {:>3}°C {}", info.cpu_temp, cpu_bar);
                println!("  GPU Temp: {:>3}°C {}", info.gpu_temp, gpu_bar);
                println!();

                let cpu_fan_bar = create_progress_bar(info.cpu_fan_percent as f32, 100.0, 20);
                let gpu_fan_bar = create_progress_bar(info.gpu_fan_percent as f32, 100.0, 20);

                println!("  CPU Fan:  {:>5} RPM {:>3}% {}", info.cpu_fan_rpm, info.cpu_fan_percent, cpu_fan_bar);
                println!("  GPU Fan:  {:>5} RPM {:>3}% {}", info.gpu_fan_rpm, info.gpu_fan_percent, gpu_fan_bar);
                println!();

                println!("  Mode: {:?}  |  Cooler Boost: {}", 
                    info.fan_mode,
                    if info.cooler_boost { "ON".red() } else { "OFF".green() }
                );
            }
        }

        println!();
        println!("{}", format!("Refreshing every {}s...", interval).dimmed());

        std::thread::sleep(std::time::Duration::from_secs(interval));
    }
}

fn create_progress_bar(value: f32, max: f32, width: usize) -> String {
    let ratio = (value / max).clamp(0.0, 1.0);
    let filled = (ratio * width as f32) as usize;
    let empty = width - filled;

    let color = if ratio < 0.5 {
        colored::Color::Green
    } else if ratio < 0.75 {
        colored::Color::Yellow
    } else {
        colored::Color::Red
    };

    format!(
        "[{}{}]",
        "█".repeat(filled).color(color),
        "░".repeat(empty).dimmed()
    )
}

fn cmd_apply() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load()?;

    if let Some(profile) = config.get_active_profile() {
        let mut ec = EmbeddedController::new()?;
        let mut fan_controller = FanController::new(EmbeddedController::new()?);
        let mut manager = ScenarioManager::new(&mut ec, &mut fan_controller);

        manager.apply_settings(&profile.settings)?;

        println!("{} Applied profile: {}", "✓".green(), profile.name.cyan());
        println!("  Scenario: {}", profile.scenario);
        println!("  Shift Mode: {}", profile.settings.shift_mode);
        println!("  Fan Mode: {:?}", profile.settings.fan_mode);
        println!("  Cooler Boost: {}", if profile.settings.cooler_boost { "ON" } else { "OFF" });
    } else {
        println!("{} No active profile found", "✗".red());
    }

    Ok(())
}
