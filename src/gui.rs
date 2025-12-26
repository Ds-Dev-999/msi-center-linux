mod config;
mod ec;
mod fan;
mod scenario;

use config::{AppConfig, Profile};
use ec::EmbeddedController;
use eframe::egui;
use fan::{FanController, FanCurve, FanCurvePoint, FanInfo, FanMode};
use scenario::{ScenarioManager, ScenarioSettings, ShiftMode, UserScenario};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("MSI Center Linux"),
        ..Default::default()
    };

    eframe::run_native(
        "MSI Center Linux",
        options,
        Box::new(|cc| Ok(Box::new(MsiCenterApp::new(cc)))),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Dashboard,
    FanControl,
    Scenarios,
    Profiles,
    Settings,
}

struct MsiCenterApp {
    current_tab: Tab,
    fan_info: Option<FanInfo>,
    current_scenario: UserScenario,
    current_shift_mode: ShiftMode,
    super_battery: bool,
    cooler_boost: bool,
    config: AppConfig,
    last_update: Instant,
    update_interval: Duration,
    error_message: Option<String>,
    success_message: Option<String>,
    is_root: bool,
    
    cpu_fan_speed: f32,
    gpu_fan_speed: f32,
    manual_fan_mode: bool,
    
    cpu_curve: Vec<[f32; 2]>,
    gpu_curve: Vec<[f32; 2]>,
    
    new_profile_name: String,
    selected_profile_base: usize,
}

impl MsiCenterApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load().unwrap_or_default();
        let is_root = nix::unistd::geteuid().is_root();

        let mut app = Self {
            current_tab: Tab::Dashboard,
            fan_info: None,
            current_scenario: UserScenario::Balanced,
            current_shift_mode: ShiftMode::Comfort,
            super_battery: false,
            cooler_boost: false,
            config,
            last_update: Instant::now() - Duration::from_secs(10),
            update_interval: Duration::from_secs(2),
            error_message: None,
            success_message: None,
            is_root,
            cpu_fan_speed: 50.0,
            gpu_fan_speed: 50.0,
            manual_fan_mode: false,
            cpu_curve: vec![[40.0, 0.0], [50.0, 30.0], [60.0, 50.0], [70.0, 70.0], [80.0, 90.0], [90.0, 100.0]],
            gpu_curve: vec![[40.0, 0.0], [50.0, 30.0], [60.0, 50.0], [70.0, 70.0], [80.0, 90.0], [90.0, 100.0]],
            new_profile_name: String::new(),
            selected_profile_base: 1,
        };

        app.refresh_data();
        app
    }

    fn refresh_data(&mut self) {
        if let Ok(ec) = EmbeddedController::new() {
            let mut fan_controller = FanController::new(ec);
            if let Ok(info) = fan_controller.get_fan_info() {
                self.fan_info = Some(info.clone());
                self.cooler_boost = info.cooler_boost;
            }
        }

        if let Ok(mut ec) = EmbeddedController::new() {
            if let Ok(ec2) = EmbeddedController::new() {
                let mut fan_controller = FanController::new(ec2);
                let mut manager = ScenarioManager::new(&mut ec, &mut fan_controller);
                if let Ok(info) = manager.get_current_info() {
                    self.current_scenario = info.current_scenario;
                    self.current_shift_mode = info.shift_mode;
                    self.super_battery = info.super_battery;
                }
            }
        }

        self.last_update = Instant::now();
    }

    fn set_scenario(&mut self, scenario: UserScenario) {
        if let Ok(mut ec) = EmbeddedController::new() {
            if let Ok(ec2) = EmbeddedController::new() {
                let mut fan_controller = FanController::new(ec2);
                let mut manager = ScenarioManager::new(&mut ec, &mut fan_controller);
                match manager.set_scenario(scenario) {
                    Ok(_) => {
                        self.current_scenario = scenario;
                        self.success_message = Some(format!("Scenario set to {}", scenario));
                        self.refresh_data();
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Failed to set scenario: {}", e));
                    }
                }
            }
        }
    }

    fn set_fan_mode(&mut self, mode: FanMode) {
        if let Ok(ec) = EmbeddedController::new() {
            let mut fan_controller = FanController::new(ec);
            match fan_controller.set_fan_mode(mode) {
                Ok(_) => {
                    self.success_message = Some(format!("Fan mode set to {:?}", mode));
                    self.refresh_data();
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to set fan mode: {}", e));
                }
            }
        }
    }

    fn set_cooler_boost(&mut self, enabled: bool) {
        if let Ok(ec) = EmbeddedController::new() {
            let mut fan_controller = FanController::new(ec);
            match fan_controller.set_cooler_boost(enabled) {
                Ok(_) => {
                    self.cooler_boost = enabled;
                    self.success_message = Some(format!("Cooler Boost {}", if enabled { "enabled" } else { "disabled" }));
                    self.refresh_data();
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to set cooler boost: {}", e));
                }
            }
        }
    }

    fn apply_manual_fan_speed(&mut self) {
        if let Ok(ec) = EmbeddedController::new() {
            let mut fan_controller = FanController::new(ec);
            match fan_controller.set_manual_fan_speed(self.cpu_fan_speed as u8, self.gpu_fan_speed as u8) {
                Ok(_) => {
                    self.success_message = Some(format!("Fan speed set to CPU: {}%, GPU: {}%", 
                        self.cpu_fan_speed as u8, self.gpu_fan_speed as u8));
                    self.refresh_data();
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to set fan speed: {}", e));
                }
            }
        }
    }

    fn apply_fan_curve(&mut self, is_cpu: bool) {
        let curve_points: Vec<FanCurvePoint> = if is_cpu {
            self.cpu_curve.iter().map(|p| FanCurvePoint { temp: p[0] as u8, speed: p[1] as u8 }).collect()
        } else {
            self.gpu_curve.iter().map(|p| FanCurvePoint { temp: p[0] as u8, speed: p[1] as u8 }).collect()
        };

        let curve = FanCurve { points: curve_points };

        if let Ok(ec) = EmbeddedController::new() {
            let mut fan_controller = FanController::new(ec);
            let result = if is_cpu {
                fan_controller.set_cpu_fan_curve(curve)
            } else {
                fan_controller.set_gpu_fan_curve(curve)
            };

            match result {
                Ok(_) => {
                    self.success_message = Some(format!("{} fan curve applied", if is_cpu { "CPU" } else { "GPU" }));
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to apply fan curve: {}", e));
                }
            }
        }
    }

    fn reset_fans(&mut self) {
        if let Ok(ec) = EmbeddedController::new() {
            let mut fan_controller = FanController::new(ec);
            match fan_controller.reset_to_auto() {
                Ok(_) => {
                    self.manual_fan_mode = false;
                    self.success_message = Some("Fans reset to automatic control".to_string());
                    self.refresh_data();
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to reset fans: {}", e));
                }
            }
        }
    }
}

impl eframe::App for MsiCenterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_update.elapsed() > self.update_interval {
            self.refresh_data();
        }

        ctx.request_repaint_after(Duration::from_millis(500));

        self.render_top_panel(ctx);
        self.render_side_panel(ctx);
        self.render_central_panel(ctx);
        self.render_notifications(ctx);
    }
}

impl MsiCenterApp {
    fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("🖥 MSI Center Linux").size(24.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.is_root {
                        ui.label(egui::RichText::new("⚠ Not running as root").color(egui::Color32::YELLOW));
                    } else {
                        ui.label(egui::RichText::new("✓ Root access").color(egui::Color32::GREEN));
                    }
                });
            });
            ui.add_space(8.0);
        });
    }

    fn render_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("side_panel")
            .resizable(false)
            .default_width(180.0)
            .show(ctx, |ui| {
                ui.add_space(20.0);

                let tabs = [
                    (Tab::Dashboard, "📊", "Dashboard"),
                    (Tab::FanControl, "🌀", "Fan Control"),
                    (Tab::Scenarios, "⚡", "Scenarios"),
                    (Tab::Profiles, "👤", "Profiles"),
                    (Tab::Settings, "⚙", "Settings"),
                ];

                for (tab, icon, label) in tabs {
                    let is_selected = self.current_tab == tab;
                    let text = format!("{} {}", icon, label);

                    let button = egui::Button::new(
                        egui::RichText::new(&text)
                            .size(16.0)
                            .color(if is_selected { egui::Color32::WHITE } else { egui::Color32::LIGHT_GRAY })
                    )
                    .fill(if is_selected { egui::Color32::from_rgb(60, 60, 100) } else { egui::Color32::TRANSPARENT })
                    .min_size(egui::vec2(160.0, 40.0));

                    if ui.add(button).clicked() {
                        self.current_tab = tab;
                    }
                    ui.add_space(4.0);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    if ui.button("🔄 Refresh").clicked() {
                        self.refresh_data();
                        self.success_message = Some("Data refreshed".to_string());
                    }
                    ui.add_space(10.0);
                });
            });
    }

    fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.current_tab {
                    Tab::Dashboard => self.render_dashboard(ui),
                    Tab::FanControl => self.render_fan_control(ui),
                    Tab::Scenarios => self.render_scenarios(ui),
                    Tab::Profiles => self.render_profiles(ui),
                    Tab::Settings => self.render_settings(ui),
                }
            });
        });
    }

    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("System Dashboard");
        ui.add_space(20.0);

        ui.columns(2, |columns| {
            columns[0].group(|ui| {
                ui.heading("🌡 Temperatures");
                ui.add_space(10.0);

                if let Some(ref info) = self.fan_info {
                    self.render_temp_gauge(ui, "CPU", info.cpu_temp);
                    ui.add_space(10.0);
                    self.render_temp_gauge(ui, "GPU", info.gpu_temp);
                } else {
                    ui.label("No data available");
                }
            });

            columns[1].group(|ui| {
                ui.heading("🌀 Fan Speeds");
                ui.add_space(10.0);

                if let Some(ref info) = self.fan_info {
                    self.render_fan_gauge(ui, "CPU Fan", info.cpu_fan_rpm, info.cpu_fan_percent);
                    ui.add_space(10.0);
                    self.render_fan_gauge(ui, "GPU Fan", info.gpu_fan_rpm, info.gpu_fan_percent);
                } else {
                    ui.label("No data available");
                }
            });
        });

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("⚡ Current Status");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Scenario:");
                ui.label(egui::RichText::new(self.current_scenario.to_string()).strong().color(egui::Color32::LIGHT_BLUE));
            });

            ui.horizontal(|ui| {
                ui.label("Shift Mode:");
                ui.label(egui::RichText::new(self.current_shift_mode.to_string()).strong());
            });

            ui.horizontal(|ui| {
                ui.label("Cooler Boost:");
                let (text, color) = if self.cooler_boost {
                    ("ON", egui::Color32::RED)
                } else {
                    ("OFF", egui::Color32::GREEN)
                };
                ui.label(egui::RichText::new(text).strong().color(color));
            });

            ui.horizontal(|ui| {
                ui.label("Super Battery:");
                let (text, color) = if self.super_battery {
                    ("ON", egui::Color32::GREEN)
                } else {
                    ("OFF", egui::Color32::GRAY)
                };
                ui.label(egui::RichText::new(text).strong().color(color));
            });
        });

        ui.add_space(20.0);

        ui.horizontal(|ui| {
            ui.heading("Quick Actions");
        });
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            if ui.button("🔇 Silent Mode").clicked() {
                self.set_scenario(UserScenario::Silent);
            }
            if ui.button("⚖ Balanced").clicked() {
                self.set_scenario(UserScenario::Balanced);
            }
            if ui.button("🚀 Performance").clicked() {
                self.set_scenario(UserScenario::HighPerformance);
            }
            if ui.button("🔥 Turbo").clicked() {
                self.set_scenario(UserScenario::Turbo);
            }
            if ui.button("🔋 Battery").clicked() {
                self.set_scenario(UserScenario::SuperBattery);
            }
        });
    }

    fn render_temp_gauge(&self, ui: &mut egui::Ui, label: &str, temp: u8) {
        let color = match temp {
            0..=50 => egui::Color32::GREEN,
            51..=70 => egui::Color32::YELLOW,
            71..=85 => egui::Color32::from_rgb(255, 165, 0),
            _ => egui::Color32::RED,
        };

        ui.horizontal(|ui| {
            ui.label(format!("{}: ", label));
            ui.label(egui::RichText::new(format!("{}°C", temp)).size(20.0).color(color).strong());
        });

        let progress = temp as f32 / 100.0;
        let progress_bar = egui::ProgressBar::new(progress)
            .fill(color)
            .show_percentage();
        ui.add(progress_bar);
    }

    fn render_fan_gauge(&self, ui: &mut egui::Ui, label: &str, rpm: u32, percent: u8) {
        ui.horizontal(|ui| {
            ui.label(format!("{}: ", label));
            ui.label(egui::RichText::new(format!("{} RPM", rpm)).size(18.0).strong());
            ui.label(format!("({}%)", percent));
        });

        let progress = percent as f32 / 100.0;
        let progress_bar = egui::ProgressBar::new(progress)
            .fill(egui::Color32::from_rgb(100, 150, 255))
            .show_percentage();
        ui.add(progress_bar);
    }

    fn render_fan_control(&mut self, ui: &mut egui::Ui) {
        ui.heading("Fan Control");
        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Fan Mode");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if ui.button("🔄 Auto").clicked() {
                    self.set_fan_mode(FanMode::Auto);
                }
                if ui.button("🔇 Silent").clicked() {
                    self.set_fan_mode(FanMode::Silent);
                }
                if ui.button("📊 Basic").clicked() {
                    self.set_fan_mode(FanMode::Basic);
                }
                if ui.button("⚙ Advanced").clicked() {
                    self.set_fan_mode(FanMode::Advanced);
                }
            });
        });

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Cooler Boost");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Cooler Boost: ");
                let mut cb = self.cooler_boost;
                let label = if cb { "🔥 ON" } else { "OFF" };
                if ui.toggle_value(&mut cb, label).changed() {
                    self.set_cooler_boost(cb);
                }
            });
            ui.label(egui::RichText::new("Maximum fan speed for cooling").small().color(egui::Color32::GRAY));
        });

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Manual Fan Speed");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("CPU Fan: ");
                ui.add(egui::Slider::new(&mut self.cpu_fan_speed, 0.0..=100.0).suffix("%"));
            });

            ui.horizontal(|ui| {
                ui.label("GPU Fan: ");
                ui.add(egui::Slider::new(&mut self.gpu_fan_speed, 0.0..=100.0).suffix("%"));
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("✓ Apply Manual Speed").clicked() {
                    self.apply_manual_fan_speed();
                }
                if ui.button("🔄 Reset to Auto").clicked() {
                    self.reset_fans();
                }
            });
        });

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Fan Curves");
            ui.add_space(10.0);

            ui.label("CPU Fan Curve:");
            self.render_fan_curve_editor(ui, true);

            ui.add_space(10.0);

            ui.label("GPU Fan Curve:");
            self.render_fan_curve_editor(ui, false);
        });
    }

    fn render_fan_curve_editor(&mut self, ui: &mut egui::Ui, is_cpu: bool) {
        let curve = if is_cpu { &mut self.cpu_curve } else { &mut self.gpu_curve };

        ui.horizontal(|ui| {
            if ui.button("Silent").clicked() {
                *curve = vec![[50.0, 0.0], [60.0, 20.0], [70.0, 40.0], [80.0, 60.0], [90.0, 80.0], [95.0, 100.0]];
            }
            if ui.button("Balanced").clicked() {
                *curve = vec![[40.0, 0.0], [50.0, 30.0], [60.0, 50.0], [70.0, 70.0], [80.0, 90.0], [90.0, 100.0]];
            }
            if ui.button("Performance").clicked() {
                *curve = vec![[35.0, 30.0], [45.0, 50.0], [55.0, 70.0], [65.0, 85.0], [75.0, 100.0], [85.0, 100.0]];
            }
        });

        egui::Grid::new(if is_cpu { "cpu_curve_grid" } else { "gpu_curve_grid" })
            .num_columns(7)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Point");
                for i in 0..curve.len() {
                    ui.label(format!("{}", i + 1));
                }
                ui.end_row();

                ui.label("Temp °C");
                for point in curve.iter_mut() {
                    ui.add(egui::DragValue::new(&mut point[0]).range(0.0..=100.0).speed(1.0));
                }
                ui.end_row();

                ui.label("Speed %");
                for point in curve.iter_mut() {
                    ui.add(egui::DragValue::new(&mut point[1]).range(0.0..=100.0).speed(1.0));
                }
                ui.end_row();
            });

        if ui.button(format!("Apply {} Curve", if is_cpu { "CPU" } else { "GPU" })).clicked() {
            self.apply_fan_curve(is_cpu);
        }
    }

    fn render_scenarios(&mut self, ui: &mut egui::Ui) {
        ui.heading("User Scenarios");
        ui.add_space(20.0);

        let scenarios = [
            (UserScenario::Silent, "🔇 Silent", "Low noise, reduced performance. Perfect for quiet work.", egui::Color32::from_rgb(100, 150, 100)),
            (UserScenario::Balanced, "⚖ Balanced", "Default balanced mode for everyday use.", egui::Color32::from_rgb(100, 150, 200)),
            (UserScenario::HighPerformance, "🚀 High Performance", "Maximum CPU/GPU performance for demanding tasks.", egui::Color32::from_rgb(200, 150, 100)),
            (UserScenario::Turbo, "🔥 Turbo", "Extreme performance with Cooler Boost enabled.", egui::Color32::from_rgb(200, 100, 100)),
            (UserScenario::SuperBattery, "🔋 Super Battery", "Maximum battery life for extended mobility.", egui::Color32::from_rgb(100, 200, 100)),
        ];

        for (scenario, name, desc, color) in scenarios {
            let is_selected = self.current_scenario == scenario;

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    let radio = ui.radio(is_selected, "");
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(name).size(18.0).color(color).strong());
                        ui.label(egui::RichText::new(desc).small().color(egui::Color32::GRAY));
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Apply").clicked() || radio.clicked() {
                            self.set_scenario(scenario);
                        }
                    });
                });
            });
            ui.add_space(5.0);
        }

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Shift Mode");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                let modes = [
                    (ShiftMode::EcoSilent, "Eco"),
                    (ShiftMode::Comfort, "Comfort"),
                    (ShiftMode::Sport, "Sport"),
                    (ShiftMode::Turbo, "Turbo"),
                ];

                for (mode, name) in modes {
                    let is_selected = self.current_shift_mode == mode;
                    if ui.selectable_label(is_selected, name).clicked() {
                        if let Ok(mut ec) = EmbeddedController::new() {
                            if let Ok(ec2) = EmbeddedController::new() {
                                let mut fan_controller = FanController::new(ec2);
                                let mut manager = ScenarioManager::new(&mut ec, &mut fan_controller);
                                if manager.set_shift_mode(mode).is_ok() {
                                    self.current_shift_mode = mode;
                                    self.success_message = Some(format!("Shift mode set to {}", mode));
                                }
                            }
                        }
                    }
                }
            });
        });
    }

    fn render_profiles(&mut self, ui: &mut egui::Ui) {
        ui.heading("Profile Management");
        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Saved Profiles");
            ui.add_space(10.0);

            let active_profile = self.config.active_profile.clone();
            let profiles: Vec<_> = self.config.profiles.iter().cloned().collect();

            for profile in profiles {
                let is_active = profile.name == active_profile;

                ui.horizontal(|ui| {
                    if is_active {
                        ui.label(egui::RichText::new("►").color(egui::Color32::GREEN));
                    } else {
                        ui.label("  ");
                    }

                    ui.label(egui::RichText::new(&profile.name).strong());
                    ui.label(format!("({})", profile.scenario));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !is_active {
                            if ui.small_button("🗑").clicked() {
                                self.config.remove_profile(&profile.name);
                                let _ = self.config.save();
                            }
                        }
                        if ui.small_button("Apply").clicked() {
                            self.config.set_active_profile(&profile.name);
                            let _ = self.config.save();
                            
                            if let Ok(mut ec) = EmbeddedController::new() {
                                if let Ok(ec2) = EmbeddedController::new() {
                                    let mut fan_controller = FanController::new(ec2);
                                    let mut manager = ScenarioManager::new(&mut ec, &mut fan_controller);
                                    if manager.apply_settings(&profile.settings).is_ok() {
                                        self.success_message = Some(format!("Applied profile: {}", profile.name));
                                        self.refresh_data();
                                    }
                                }
                            }
                        }
                    });
                });
                ui.separator();
            }
        });

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Create New Profile");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.new_profile_name);
            });

            ui.horizontal(|ui| {
                ui.label("Base:");
                egui::ComboBox::from_label("")
                    .selected_text(match self.selected_profile_base {
                        0 => "Silent",
                        1 => "Balanced",
                        2 => "High Performance",
                        3 => "Turbo",
                        _ => "Super Battery",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_profile_base, 0, "Silent");
                        ui.selectable_value(&mut self.selected_profile_base, 1, "Balanced");
                        ui.selectable_value(&mut self.selected_profile_base, 2, "High Performance");
                        ui.selectable_value(&mut self.selected_profile_base, 3, "Turbo");
                        ui.selectable_value(&mut self.selected_profile_base, 4, "Super Battery");
                    });
            });

            ui.add_space(10.0);

            if ui.button("➕ Create Profile").clicked() && !self.new_profile_name.is_empty() {
                let scenario = match self.selected_profile_base {
                    0 => UserScenario::Silent,
                    1 => UserScenario::Balanced,
                    2 => UserScenario::HighPerformance,
                    3 => UserScenario::Turbo,
                    _ => UserScenario::SuperBattery,
                };

                let settings = match scenario {
                    UserScenario::Silent => ScenarioSettings::silent(),
                    UserScenario::Balanced => ScenarioSettings::balanced(),
                    UserScenario::HighPerformance => ScenarioSettings::high_performance(),
                    UserScenario::Turbo => ScenarioSettings::turbo(),
                    UserScenario::SuperBattery => ScenarioSettings::super_battery(),
                    UserScenario::Custom => ScenarioSettings::balanced(),
                };

                let profile = Profile {
                    name: self.new_profile_name.clone(),
                    scenario,
                    settings,
                };

                self.config.add_profile(profile);
                let _ = self.config.save();
                self.success_message = Some(format!("Profile '{}' created", self.new_profile_name));
                self.new_profile_name.clear();
            }
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Application Settings");
            ui.add_space(10.0);

            ui.checkbox(&mut self.config.auto_start, "Start on boot");
            ui.checkbox(&mut self.config.apply_on_boot, "Apply profile on startup");
            ui.checkbox(&mut self.config.show_notifications, "Show notifications");

            ui.add_space(10.0);
            if ui.button("💾 Save Settings").clicked() {
                if self.config.save().is_ok() {
                    self.success_message = Some("Settings saved".to_string());
                }
            }
        });

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("Refresh Interval");
            ui.add_space(10.0);

            let mut interval_secs = self.update_interval.as_secs() as f32;
            if ui.add(egui::Slider::new(&mut interval_secs, 1.0..=10.0).suffix("s")).changed() {
                self.update_interval = Duration::from_secs_f32(interval_secs);
            }
        });

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("About");
            ui.add_space(10.0);

            ui.label(egui::RichText::new("MSI Center Linux").size(18.0).strong().color(egui::Color32::from_rgb(100, 180, 255)));
            ui.label("Version 1.0.0");
            ui.add_space(5.0);
            ui.label("A powerful MSI laptop control center for Linux");
            ui.label(egui::RichText::new("Fan control • User scenarios • Performance profiles").small().color(egui::Color32::GRAY));
            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(egui::RichText::new("👨‍💻 Developer").strong());
            ui.label(egui::RichText::new("Dasun Sanching").size(16.0).color(egui::Color32::from_rgb(255, 200, 100)));
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Built with ❤️ using Rust & egui").small().color(egui::Color32::GRAY));
            ui.add_space(5.0);
            ui.label(egui::RichText::new("© 2025 Dasun Sanching. MIT License").small().color(egui::Color32::DARK_GRAY));
        });

        ui.add_space(20.0);

        ui.group(|ui| {
            ui.heading("System Info");
            ui.add_space(10.0);

            ui.label(format!("Running as root: {}", if self.is_root { "Yes" } else { "No" }));

            if let Ok(vendor) = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor") {
                ui.label(format!("Vendor: {}", vendor.trim()));
            }
            if let Ok(product) = std::fs::read_to_string("/sys/class/dmi/id/product_name") {
                ui.label(format!("Product: {}", product.trim()));
            }
        });
    }

    fn render_notifications(&mut self, ctx: &egui::Context) {
        if let Some(ref msg) = self.success_message.clone() {
            egui::TopBottomPanel::bottom("success_notification").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("✓ {}", msg)).color(egui::Color32::GREEN));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            self.success_message = None;
                        }
                    });
                });
            });

            ctx.request_repaint_after(Duration::from_secs(3));
            if self.last_update.elapsed() > Duration::from_secs(3) {
                self.success_message = None;
            }
        }

        if let Some(ref msg) = self.error_message.clone() {
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(msg).color(egui::Color32::RED));
                    if ui.button("OK").clicked() {
                        self.error_message = None;
                    }
                });
        }
    }
}
