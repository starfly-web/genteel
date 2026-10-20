use crate::audio;
use crate::frontend::{self, InputMapping};
use crate::input::InputScript;
use crate::Emulator;
#[cfg(feature = "gilrs")]
use gilrs::{Axis, Button, EventType, Gilrs};
#[cfg(feature = "gui")]
use pixels::{wgpu, SurfaceTexture};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
#[cfg(feature = "gui")]
use winit::{
    event::{ElementState, Event, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};
#[cfg(feature = "gilrs")]
pub fn init_gilrs_with_builder<F, R, E>(builder: F) -> Option<R>
where
    F: FnOnce() -> Result<R, E>,
    E: std::fmt::Display,
{
    match builder() {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("Warning: Failed to initialize gilrs: {}", e);
            None
        }
    }
}

pub const SLOT_NAMES: [&str; 10] = [
    "Slot 0", "Slot 1", "Slot 2", "Slot 3", "Slot 4", "Slot 5", "Slot 6", "Slot 7", "Slot 8",
    "Slot 9",
];

pub const SLOT_EXTS: [&str; 10] = ["s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9"];

/// Maximum GUI configuration size in bytes (1MB) to prevent OOM
const MAX_GUI_CONFIG_SIZE: u64 = 1024 * 1024;

pub const HEX_LOOKUP: [&str; 256] = [
    "00", "01", "02", "03", "04", "05", "06", "07", "08", "09", "0A", "0B", "0C", "0D", "0E", "0F",
    "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "1A", "1B", "1C", "1D", "1E", "1F",
    "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "2A", "2B", "2C", "2D", "2E", "2F",
    "30", "31", "32", "33", "34", "35", "36", "37", "38", "39", "3A", "3B", "3C", "3D", "3E", "3F",
    "40", "41", "42", "43", "44", "45", "46", "47", "48", "49", "4A", "4B", "4C", "4D", "4E", "4F",
    "50", "51", "52", "53", "54", "55", "56", "57", "58", "59", "5A", "5B", "5C", "5D", "5E", "5F",
    "60", "61", "62", "63", "64", "65", "66", "67", "68", "69", "6A", "6B", "6C", "6D", "6E", "6F",
    "70", "71", "72", "73", "74", "75", "76", "77", "78", "79", "7A", "7B", "7C", "7D", "7E", "7F",
    "80", "81", "82", "83", "84", "85", "86", "87", "88", "89", "8A", "8B", "8C", "8D", "8E", "8F",
    "90", "91", "92", "93", "94", "95", "96", "97", "98", "99", "9A", "9B", "9C", "9D", "9E", "9F",
    "A0", "A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "AA", "AB", "AC", "AD", "AE", "AF",
    "B0", "B1", "B2", "B3", "B4", "B5", "B6", "B7", "B8", "B9", "BA", "BB", "BC", "BD", "BE", "BF",
    "C0", "C1", "C2", "C3", "C4", "C5", "C6", "C7", "C8", "C9", "CA", "CB", "CC", "CD", "CE", "CF",
    "D0", "D1", "D2", "D3", "D4", "D5", "D6", "D7", "D8", "D9", "DA", "DB", "DC", "DD", "DE", "DF",
    "E0", "E1", "E2", "E3", "E4", "E5", "E6", "E7", "E8", "E9", "EA", "EB", "EC", "ED", "EE", "EF",
    "F0", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "FA", "FB", "FC", "FD", "FE", "FF",
];

#[cfg(feature = "gui")]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum PlaneTab {
    PlaneA,
    PlaneB,
}

#[cfg(feature = "gui")]
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct WindowState {
    pub open: bool,
}

#[cfg(feature = "gui")]
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct GuiState {
    pub windows: HashMap<String, WindowState>,
    pub input_mapping: InputMapping,
    /// Per-button key overrides ("button" -> winit key name). Empty = preset layout.
    #[serde(default)]
    pub custom_keymap: HashMap<String, String>,
    pub integer_scaling: bool,
    pub force_red: bool,
    pub paused: bool,
    /// Emit 68k disassembly in GNU/AT&T syntax instead of Motorola.
    #[serde(default)]
    pub gnu_syntax: bool,
    pub recent_roms: Vec<PathBuf>,
    pub auto_save_load: bool,
    pub scroll_plane_tab: PlaneTab,
    #[serde(skip)]
    pub single_step: bool,
    #[serde(skip)]
    pub show_about: bool,
    #[serde(skip)]
    pub reset_requested: bool,
    #[serde(skip)]
    pub close_requested: bool,
    #[serde(skip)]
    pub pick_rom_requested: bool,
    #[serde(skip)]
    pub save_requested: Option<u8>,
    #[serde(skip)]
    pub load_requested: Option<u8>,
    #[serde(skip)]
    pub delete_state_requested: Option<u8>,
}

fn frame_duration_for_region(is_pal: bool) -> std::time::Duration {
    let fps = if is_pal {
        crate::audio::PAL_FPS
    } else {
        crate::audio::FPS
    };
    std::time::Duration::from_secs_f64(1.0 / fps as f64)
}

fn trim_pending_audio_to_limit(pending_audio: &mut Vec<i16>, max_samples: usize) {
    if pending_audio.len() <= max_samples {
        return;
    }
    let to_drop = ((pending_audio.len() - max_samples) + 1) & !1;
    pending_audio.drain(..to_drop.min(pending_audio.len() & !1));
}

fn frames_due(
    now: std::time::Instant,
    last_frame_inst: std::time::Instant,
    frame_duration: std::time::Duration,
    max_frames: usize,
) -> usize {
    if now < last_frame_inst + frame_duration {
        return 0;
    }
    let mut frames_to_run = 1usize;
    while frames_to_run < max_frames
        && now >= last_frame_inst + frame_duration.mul_f64((frames_to_run + 1) as f64)
    {
        frames_to_run += 1;
    }
    frames_to_run
}

fn max_pending_audio_samples(emulator: &Emulator) -> usize {
    let bus = emulator.bus.borrow();
    audio::samples_per_frame_for_rate_and_region(bus.sample_rate, bus.vdp.is_pal) * 2 * 8
}

fn flush_pending_audio_upload(
    audio_buffer: &audio::SharedAudioBuffer,
    pending_audio_upload: &mut Vec<i16>,
) {
    if pending_audio_upload.is_empty() {
        return;
    }
    if let Ok(mut buf) = audio_buffer.try_lock() {
        buf.push(pending_audio_upload);
        pending_audio_upload.clear();
    }
}

fn run_emulation_frames(
    emulator: &mut Emulator,
    input: &crate::input::FrameInput,
    pending_audio_upload: &mut Vec<i16>,
    frames_to_run: usize,
) {
    if frames_to_run == 0 {
        return;
    }
    for _ in 0..frames_to_run {
        emulator.step_frame(Some(input));
        pending_audio_upload.extend_from_slice(&emulator.audio_buffer);
        emulator.audio_buffer.clear();
    }
    trim_pending_audio_to_limit(pending_audio_upload, max_pending_audio_samples(emulator));
}

#[cfg(feature = "gui")]
fn get_gui_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("GENTEEL_GUI_CONFIG") {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(app_data) = std::env::var("APPDATA") {
            return PathBuf::from(app_data)
                .join("genteel")
                .join("gui_config.json");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("genteel")
                .join("gui_config.json");
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
            if !config_home.is_empty() {
                return PathBuf::from(config_home)
                    .join("genteel")
                    .join("gui_config.json");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("genteel")
                .join("gui_config.json");
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("gui_config.json");
        }
    }

    PathBuf::from("gui_config.json")
}

#[cfg(feature = "gui")]
impl GuiState {
    pub fn new(input_mapping: InputMapping) -> Self {
        let mut state = Self {
            windows: HashMap::new(),
            input_mapping,
            custom_keymap: HashMap::new(),
            integer_scaling: true,
            force_red: false,
            paused: false,
            gnu_syntax: false,
            recent_roms: Vec::new(),
            auto_save_load: false,
            scroll_plane_tab: PlaneTab::PlaneA,
            single_step: false,
            show_about: false,
            reset_requested: false,
            close_requested: false,
            pick_rom_requested: false,
            save_requested: None,
            load_requested: None,
            delete_state_requested: None,
        };
        state.register_default_windows();
        state
    }

    pub fn load_or_default(input_mapping: InputMapping) -> Self {
        let config_path = get_gui_config_path();
        if let Ok(file) = std::fs::File::open(&config_path) {
            if let Ok(metadata) = file.metadata() {
                if metadata.len() <= MAX_GUI_CONFIG_SIZE {
                    use std::io::Read;
                    let mut content = String::new();
                    if file
                        .take(MAX_GUI_CONFIG_SIZE + 1)
                        .read_to_string(&mut content)
                        .is_ok()
                        && content.len() as u64 <= MAX_GUI_CONFIG_SIZE
                    {
                        if let Ok(mut state) = serde_json::from_str::<Self>(&content) {
                            state.register_default_windows();
                            return state;
                        }
                    }
                }
            }
        }
        Self::new(input_mapping)
    }

    fn register_default_windows(&mut self) {
        let defaults = [
            "Settings",
            "Input Mapping",
            "Performance & Debug",
            "M68k Status",
            "Z80 Status",
            "Disassembly",
            "Execution Control",
            "Palette Viewer",
            "Tile Viewer",
            "Sprite Viewer",
            "Scroll Plane Viewer",
            "VDP Memory Hex",
            "Memory Viewer",
            "Sound Chip Visualizer",
            "Audio Channel Waveforms",
            "Controller Viewer",
            "Expansion Status",
            "State Browser",
        ];
        for &name in &defaults {
            if !self.windows.contains_key(name) {
                self.windows
                    .insert(name.to_string(), WindowState { open: false });
            }
        }
    }

    pub fn save(&self) {
        let state = self.clone();
        // Use a background thread to prevent blocking the GUI thread during disk I/O.
        // Note: This may lead to out-of-order writes if save() is called frequently,
        // and may be interrupted if the application exits before the thread completes.
        std::thread::spawn(move || {
            if let Ok(json) = serde_json::to_string_pretty(&state) {
                let path = get_gui_config_path();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(path, json);
            }
        });
    }

    pub fn is_window_open(&self, name: &str) -> bool {
        self.windows.get(name).map(|w| w.open).unwrap_or(false)
    }

    pub fn set_window_open(&mut self, name: &str, open: bool) {
        if let Some(window) = self.windows.get_mut(name) {
            window.open = open;
        } else {
            self.windows.insert(name.to_string(), WindowState { open });
        }
        self.save();
    }

    pub fn toggle_window(&mut self, name: &str) {
        let open = self.is_window_open(name);
        self.set_window_open(name, !open);
    }
}

#[cfg(feature = "gui")]
pub struct DebugInfo {
    pub m68k_pc: u32,
    pub m68k_d: [u32; 8],
    pub m68k_a: [u32; 8],
    pub m68k_sr: u16,
    pub m68k_usp: u32,
    pub m68k_ssp: u32,
    // (address, decoded instruction, raw words: opcode + up to 4 extension words)
    pub m68k_disasm: [(u32, crate::cpu::instructions::Instruction, [u16; 5]); 10],
    pub z80_pc: u16,
    pub z80_a: u8,
    pub z80_f: u8,
    pub z80_b: u8,
    pub z80_c: u8,
    pub z80_d: u8,
    pub z80_e: u8,
    pub z80_h: u8,
    pub z80_l: u8,
    pub z80_ix: u16,
    pub z80_iy: u16,
    pub z80_sp: u16,
    pub z80_i: u8,
    pub z80_r: u8,
    pub z80_memptr: u16,
    pub z80_iff1: bool,
    pub z80_im: u8,
    pub z80_disasm: [(u16, u8); 10],
    pub frame_count: u64,
    pub vdp_status: u16,
    pub vdp_registers: [u8; 24],
    pub display_enabled: bool,
    pub bg_color_index: u8,
    pub cram: [u16; 64],
    pub cram_raw: [u16; 64],
    pub vram: Option<[u8; 0x10000]>,
    pub vsram: [u8; 80],
    pub wram: Option<[u8; 0x10000]>,
    pub z80_ram: Option<[u8; 0x2000]>,
    pub ym2612_regs: [[u8; 256]; 2],
    pub psg_tone: [crate::apu::psg::ToneChannel; 3],
    pub psg_noise: crate::apu::psg::NoiseChannel,
    pub channel_waveforms: [[i16; 128]; 10],
    pub port1_state: crate::io::ControllerState,
    pub port1_type: crate::io::ControllerType,
    pub port2_state: crate::io::ControllerState,
    pub port2_type: crate::io::ControllerType,
    pub has_rom: bool,
    pub current_rom_path: Option<PathBuf>,
}

#[cfg(feature = "gui")]
#[cfg(feature = "gilrs")]
fn init_gilrs() -> Option<Gilrs> {
    #[cfg(test)]
    if std::env::var("GENTEEL_TEST_FAIL_GILRS").is_ok() {
        return None;
    }
    init_gilrs_with_builder(Gilrs::new)
}

#[cfg(feature = "gui")]
pub struct Framework {
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    pub screen_descriptor: egui_wgpu::ScreenDescriptor,
    pub renderer: egui_wgpu::Renderer,
    pub gui_state: GuiState,
    pub tile_texture: Option<egui::TextureHandle>,
    pub tile_viewer_image: std::sync::Arc<egui::ColorImage>,
    pub plane_a_viewer_image: std::sync::Arc<egui::ColorImage>,
    pub plane_b_viewer_image: std::sync::Arc<egui::ColorImage>,
    pub plane_a_texture: Option<egui::TextureHandle>,
    pub plane_b_texture: Option<egui::TextureHandle>,
    pub pending_rom_path: Arc<Mutex<Option<PathBuf>>>,
    #[cfg(feature = "gilrs")]
    pub gilrs: Option<Gilrs>,
    pub label_buffer: String,
    pub waveform_points: Vec<egui::Pos2>,
}

#[cfg(feature = "gui")]
impl Framework {
    pub fn new(
        event_loop: &winit::event_loop::EventLoopWindowTarget<()>,
        width: u32,
        height: u32,
        scale_factor: f32,
        pixels: &pixels::Pixels,
        input_mapping: InputMapping,
    ) -> Self {
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::viewport::ViewportId::ROOT,
            event_loop,
            Some(scale_factor),
            None,
        );
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: scale_factor,
        };
        let renderer =
            egui_wgpu::Renderer::new(pixels.device(), pixels.render_texture_format(), None, 1);
        let gui_state = GuiState::load_or_default(input_mapping);
        Self {
            egui_ctx,
            egui_state,
            screen_descriptor,
            renderer,
            gui_state,
            tile_texture: None,
            tile_viewer_image: std::sync::Arc::new(egui::ColorImage::new(
                [128, 1024],
                egui::Color32::TRANSPARENT,
            )),
            plane_a_viewer_image: std::sync::Arc::new(egui::ColorImage::new(
                [0, 0],
                egui::Color32::TRANSPARENT,
            )),
            plane_b_viewer_image: std::sync::Arc::new(egui::ColorImage::new(
                [0, 0],
                egui::Color32::TRANSPARENT,
            )),
            plane_a_texture: None,
            plane_b_texture: None,
            pending_rom_path: Arc::new(Mutex::new(None)),
            #[cfg(feature = "gilrs")]
            gilrs: init_gilrs(),
            label_buffer: String::with_capacity(64),
            waveform_points: Vec::with_capacity(128),
        }
    }
    pub fn handle_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) {
        let _ = self.egui_state.on_window_event(window, event);
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.screen_descriptor.size_in_pixels = [width, height];
        }
    }
    pub fn scale_factor(&mut self, scale_factor: f32) {
        self.screen_descriptor.pixels_per_point = scale_factor;
    }

    fn label_fmt(buffer: &mut String, ui: &mut egui::Ui, args: std::fmt::Arguments) {
        buffer.clear();
        let _ = buffer.write_fmt(args);
        ui.label(&*buffer);
    }

    fn colored_label_fmt(
        buffer: &mut String,
        ui: &mut egui::Ui,
        color: egui::Color32,
        args: std::fmt::Arguments,
    ) {
        buffer.clear();
        let _ = buffer.write_fmt(args);
        ui.colored_label(color, &*buffer);
    }

    fn on_hover_text_fmt(
        buffer: &mut String,
        response: &egui::Response,
        args: std::fmt::Arguments,
    ) {
        buffer.clear();
        let _ = buffer.write_fmt(args);
        response.clone().on_hover_text(&*buffer);
    }

    pub fn pick_rom(&mut self) {
        let pending = self.pending_rom_path.clone();
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .add_filter("Genesis ROMs", &["bin", "md", "gen", "zip"][..])
                .add_filter("All Files", &["*"][..])
                .pick_file();
            if let Some(path) = file {
                if let Ok(mut lock) = pending.lock() {
                    *lock = Some(path);
                } else {
                    eprintln!("Failed to acquire pending_rom_path lock");
                }
            }
        });
    }

    #[cfg(feature = "gilrs")]
    pub fn poll_gamepads(&mut self, state: &mut crate::io::ControllerState) {
        if let Some(gilrs) = &mut self.gilrs {
            while let Some(gilrs::Event { event, .. }) = gilrs.next_event() {
                match event {
                    EventType::ButtonPressed(button, _) => {
                        Self::handle_gamepad_button_pressed(button, state);
                    }
                    EventType::ButtonReleased(button, _) => {
                        Self::handle_gamepad_button_released(button, state);
                    }
                    EventType::AxisChanged(axis, value, _) => {
                        Self::handle_gamepad_axis_changed(axis, value, state);
                    }
                    _ => {}
                }
            }
        }
    }

    #[cfg(feature = "gilrs")]
    fn handle_gamepad_button_pressed(button: Button, state: &mut crate::io::ControllerState) {
        match button {
            Button::DPadUp => state.up = true,
            Button::DPadDown => state.down = true,
            Button::DPadLeft => state.left = true,
            Button::DPadRight => state.right = true,
            Button::South => state.b = true,
            Button::East => state.c = true,
            Button::West => state.a = true,
            Button::North => state.x = true,
            Button::LeftTrigger => state.y = true,
            Button::RightTrigger => state.z = true,
            Button::Select => state.mode = true,
            Button::Start => state.start = true,
            _ => {}
        }
    }

    #[cfg(feature = "gilrs")]
    fn handle_gamepad_button_released(button: Button, state: &mut crate::io::ControllerState) {
        match button {
            Button::DPadUp => state.up = false,
            Button::DPadDown => state.down = false,
            Button::DPadLeft => state.left = false,
            Button::DPadRight => state.right = false,
            Button::South => state.b = false,
            Button::East => state.c = false,
            Button::West => state.a = false,
            Button::North => state.x = false,
            Button::LeftTrigger => state.y = false,
            Button::RightTrigger => state.z = false,
            Button::Select => state.mode = false,
            Button::Start => state.start = false,
            _ => {}
        }
    }

    #[cfg(feature = "gilrs")]
    fn handle_gamepad_axis_changed(axis: Axis, value: f32, state: &mut crate::io::ControllerState) {
        let threshold = 0.5;
        match axis {
            Axis::LeftStickX => {
                if value > threshold {
                    state.right = true;
                    state.left = false;
                } else if value < -threshold {
                    state.left = true;
                    state.right = false;
                } else {
                    state.left = false;
                    state.right = false;
                }
            }
            Axis::LeftStickY => {
                if value > threshold {
                    state.up = true;
                    state.down = false;
                } else if value < -threshold {
                    state.down = true;
                    state.up = false;
                } else {
                    state.up = false;
                    state.down = false;
                }
            }
            _ => {}
        }
    }

    pub fn prepare(&mut self, window: &winit::window::Window, debug_info: &DebugInfo) {
        let ctx = self.egui_ctx.clone();
        let raw_input = self.egui_state.take_egui_input(window);
        ctx.begin_frame(raw_input);

        // Global shortcuts
        let ctrl = ctx.input(|i| i.modifiers.command);
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::O)) {
            self.gui_state.pick_rom_requested = true;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::R)) && debug_info.has_rom {
            self.gui_state.reset_requested = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) && debug_info.has_rom {
            self.gui_state.save_requested = Some(0); // Default to slot 0
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F8)) && debug_info.has_rom {
            self.gui_state.load_requested = Some(0); // Default to slot 0
        }

        self.render_top_menu_bar(debug_info);
        self.render_about_window();
        self.render_performance_debug_window(debug_info);
        self.render_settings_window();
        self.render_input_mapping_window();
        self.render_execution_control_window();
        self.render_m68k_status_window(debug_info);
        self.render_z80_status_window(debug_info);
        self.render_disassembly_window(debug_info);
        self.render_palette_viewer_window(debug_info);
        self.render_tile_viewer_window(debug_info);
        self.render_sprite_viewer_window(debug_info);
        self.render_scroll_plane_viewer_window(debug_info);
        self.render_vdp_memory_hex_window(debug_info);
        self.render_memory_viewer_window(debug_info);
        self.render_sound_chip_visualizer_window(debug_info);
        self.render_audio_channel_waveforms_window(debug_info);
        self.render_controller_viewer_window(debug_info);
        self.render_expansion_status_window();
        self.render_state_browser_window(debug_info);
    }

    fn render_top_menu_bar(&mut self, debug_info: &DebugInfo) {
        // Draw the GUI

        egui::TopBottomPanel::top("menubar_container").show(&self.egui_ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui
                        .add(egui::Button::new("Open...").shortcut_text("Ctrl+O"))
                        .clicked()
                    {
                        self.gui_state.pick_rom_requested = true;
                        ui.close_menu();
                    }
                    ui.menu_button("Open Recent", |ui| {
                        if self.gui_state.recent_roms.is_empty() {
                            ui.label("No recent ROMs");
                        } else {
                            for path in &self.gui_state.recent_roms {
                                let filename = path
                                    .file_name()
                                    .and_then(|f| f.to_str())
                                    .unwrap_or("Unknown");
                                if ui.button(filename).clicked() {
                                    let mut lock = self.pending_rom_path.lock().unwrap();
                                    *lock = Some(path.clone());
                                    ui.close_menu();
                                }
                            }
                        }
                    });
                    ui.separator();
                    if ui
                        .add_enabled(
                            debug_info.has_rom,
                            egui::Button::new("Reset ROM").shortcut_text("Ctrl+R"),
                        )
                        .clicked()
                    {
                        self.gui_state.reset_requested = true;
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(debug_info.has_rom, egui::Button::new("Close ROM"))
                        .clicked()
                    {
                        self.gui_state.close_requested = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("Save State", |ui| {
                        for slot in 0..10 {
                            let btn = if slot == 0 {
                                egui::Button::new(SLOT_NAMES[slot as usize]).shortcut_text("F5")
                            } else {
                                egui::Button::new(SLOT_NAMES[slot as usize])
                            };
                            if ui.add_enabled(debug_info.has_rom, btn).clicked() {
                                self.gui_state.save_requested = Some(slot);
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Load State", |ui| {
                        for slot in 0..10 {
                            let btn = if slot == 0 {
                                egui::Button::new(SLOT_NAMES[slot as usize]).shortcut_text("F8")
                            } else {
                                egui::Button::new(SLOT_NAMES[slot as usize])
                            };
                            if ui.add_enabled(debug_info.has_rom, btn).clicked() {
                                self.gui_state.load_requested = Some(slot);
                                ui.close_menu();
                            }
                        }
                    });
                    if ui
                        .add_enabled(debug_info.has_rom, egui::Button::new("State Browser..."))
                        .clicked()
                    {
                        self.gui_state.set_window_open("State Browser", true);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        std::process::exit(0);
                    }
                });
                ui.menu_button("Settings", |ui| {
                    if ui.button("Video").clicked() {
                        self.gui_state.set_window_open("Settings", true);
                        ui.close_menu();
                    }
                    if ui.button("Input Mapping...").clicked() {
                        self.gui_state.set_window_open("Input Mapping", true);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Debug", |ui| {
                    let pause_label = if self.gui_state.paused {
                        "▶ Resume"
                    } else {
                        "⏸ Pause"
                    };
                    if ui.button(pause_label).clicked() {
                        self.gui_state.paused = !self.gui_state.paused;
                        self.gui_state.save();
                        ui.close_menu();
                    }
                    if ui.button("⏭ Single Step").clicked() {
                        self.gui_state.single_step = true;
                        self.gui_state.paused = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    let mut names: Vec<String> = self.gui_state.windows.keys().cloned().collect();
                    names.sort(); // Keep menu consistent
                    for name in names {
                        if name == "Settings" {
                            continue;
                        } // Settings is in Settings menu
                        let mut open = self.gui_state.is_window_open(&name);
                        if ui.checkbox(&mut open, &name).changed() {
                            self.gui_state.set_window_open(&name, open);
                        }
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.gui_state.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    fn render_about_window(&mut self) {
        if self.gui_state.show_about {
            egui::Window::new("About Genteel")
                .open(&mut self.gui_state.show_about)
                .show(&self.egui_ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("Genteel");
                        ui.label(format!("Version: {}", genteel::VERSION));
                    });
                    ui.separator();

                    ui.label("An instrumentable Sega Mega Drive/Genesis emulator architected for the intersection of human and machine intelligence.");
                    ui.add_space(8.0);

                    ui.group(|ui| {
                        ui.label(egui::RichText::new("🛠 Comprehensive Debugging").strong());
                        ui.label("Integrated multi-window suite for real-time VDP, CPU, Memory, and Audio analysis with GDB support.");
                    });

                    ui.group(|ui| {
                        ui.label(egui::RichText::new("🧪 CI/CD Instrumentation").strong());
                        ui.label("Headless validation, deterministic execution, and massive automated test coverage for ROM verification.");
                    });

                    ui.group(|ui| {
                        ui.label(egui::RichText::new("🤖 AI-Driven Development").strong());
                        ui.label("Serialization-first design and instrumentable APIs tailored for AI agents and machine observation.");
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.hyperlink_to("GitHub Repository", "https://github.com/segin/genteel");
                        ui.label("•");
                        ui.label("MIT License");
                    });
                });
        }
    }

    fn render_performance_debug_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Performance & Debug") {
            let mut open = true;

            egui::Window::new("Performance & Debug")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    let dt = ui.ctx().input(|i| i.stable_dt);
                    let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("Frontend FPS: {:.1}", fps),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("Frame Time: {:.2}ms", dt * 1000.0),
                    );
                    ui.separator();
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("Internal Frames: {}", debug_info.frame_count),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("M68k PC: {:06X}", debug_info.m68k_pc),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("Z80 PC: {:04X}", debug_info.z80_pc),
                    );
                    ui.separator();
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!(
                            "VDP Display: {}",
                            if debug_info.display_enabled {
                                "ENABLED"
                            } else {
                                "DISABLED"
                            }
                        ),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("VDP Status: {:04X}", debug_info.vdp_status),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("BG Color Index: {}", debug_info.bg_color_index),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("CRAM[0] (RGB565): {:04X}", debug_info.cram[0]),
                    );
                    if ui
                        .checkbox(&mut self.gui_state.force_red, "Force Red BG (Debug)")
                        .changed()
                    {
                        self.gui_state.save();
                    }

                    #[cfg(feature = "gilrs")]
                    {
                        ui.separator();
                        ui.heading("Connected Gamepads");
                        if let Some(gilrs) = &self.gilrs {
                            let gamepads: Vec<_> = gilrs
                                .gamepads()
                                .map(|(id, g)| (id, g.name().to_string()))
                                .collect();
                            if gamepads.is_empty() {
                                ui.label("No gamepads connected");
                            } else {
                                for (id, name) in gamepads {
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!("{}: {}", id, name),
                                    );
                                }
                            }
                        } else {
                            ui.label("Gamepad support disabled or failed to initialize");
                        }
                    }
                });
            if !open {
                self.gui_state.set_window_open("Performance & Debug", false);
            }
        }
    }

    fn render_settings_window(&mut self) {
        if self.gui_state.is_window_open("Settings") {
            let mut open = true;

            egui::Window::new("Settings")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    ui.heading("Video");
                    if ui
                        .checkbox(&mut self.gui_state.integer_scaling, "Integer Pixel Scaling")
                        .changed()
                    {
                        self.gui_state.save();
                    }
                    ui.separator();
                    ui.heading("Input");
                    ui.label("Input Mapping:");
                    if ui
                        .radio_value(
                            &mut self.gui_state.input_mapping,
                            InputMapping::Original,
                            "Original",
                        )
                        .changed()
                    {
                        self.gui_state.save();
                    }
                    if ui
                        .radio_value(
                            &mut self.gui_state.input_mapping,
                            InputMapping::Ergonomic,
                            "Ergonomic",
                        )
                        .changed()
                    {
                        self.gui_state.save();
                    }
                    ui.separator();
                    ui.heading("System");
                    if ui
                        .checkbox(&mut self.gui_state.auto_save_load, "Auto-Save/Load State")
                        .changed()
                    {
                        self.gui_state.save();
                    }
                });
            if !open {
                self.gui_state.set_window_open("Settings", false);
            }
        }
    }

    fn render_input_mapping_window(&mut self) {
        if !self.gui_state.is_window_open("Input Mapping") {
            return;
        }
        let mut open = true;
        egui::Window::new("Input Mapping")
            .open(&mut open)
            .default_width(300.0)
            .show(&self.egui_ctx, |ui| {
                ui.label("Choose the keyboard key for each controller button.");
                ui.label("Entries showing '(preset)' use the active layout preset;");
                ui.label("custom entries override the preset. Esc always quits.");
                ui.separator();
                let buttons = [
                    ("up", "D-pad Up"),
                    ("down", "D-pad Down"),
                    ("left", "D-pad Left"),
                    ("right", "D-pad Right"),
                    ("a", "A"),
                    ("b", "B"),
                    ("c", "C"),
                    ("x", "X (6-button)"),
                    ("y", "Y (6-button)"),
                    ("z", "Z (6-button)"),
                    ("start", "Start"),
                    ("mode", "Mode"),
                ];
                let key_options = frontend::bindable_keys();
                for (button, label) in buttons {
                    ui.horizontal(|ui| {
                        ui.label(format!("{label:>14}:"));
                        let current = self.gui_state.custom_keymap.get(button).cloned();
                        let preset =
                            frontend::preset_key_display(button, self.gui_state.input_mapping);
                        let mut sel = current.clone();
                        let selected_text = match &sel {
                            Some(k) => key_options
                                .iter()
                                .find(|(n, _)| n == k)
                                .map(|(_, d)| d.clone())
                                .unwrap_or_else(|| k.clone()),
                            None => format!("{preset} (preset)"),
                        };
                        egui::ComboBox::from_id_source(format!("keymap_{button}"))
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut sel, None, format!("{preset} (preset)"));
                                for (name, display) in &key_options {
                                    ui.selectable_value(
                                        &mut sel,
                                        Some(name.clone()),
                                        display.clone(),
                                    );
                                }
                            });
                        if sel != current {
                            match &sel {
                                Some(k) => {
                                    self.gui_state
                                        .custom_keymap
                                        .insert(button.to_string(), k.clone());
                                }
                                None => {
                                    self.gui_state.custom_keymap.remove(button);
                                }
                            }
                            self.gui_state.save();
                        }
                    });
                }
                ui.separator();
                if ui.button("Reset All to Preset").clicked() {
                    self.gui_state.custom_keymap.clear();
                    self.gui_state.save();
                }
            });
        if !open {
            self.gui_state.set_window_open("Input Mapping", false);
        }
    }

    fn render_execution_control_window(&mut self) {
        if self.gui_state.is_window_open("Execution Control") {
            let mut open = true;

            egui::Window::new("Execution Control")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    ui.horizontal(|ui| {
                        if self.gui_state.paused {
                            if ui.button("▶ Resume").clicked() {
                                self.gui_state.paused = false;
                                self.gui_state.save();
                            }
                        } else {
                            if ui.button("⏸ Pause").clicked() {
                                self.gui_state.paused = true;
                                self.gui_state.save();
                            }
                        }
                        if ui.button("⏭ Single Step").clicked() {
                            self.gui_state.single_step = true;
                            self.gui_state.paused = true;
                        }
                    });
                });
            if !open {
                self.gui_state.set_window_open("Execution Control", false);
            }
        }
    }

    fn render_m68k_status_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("M68k Status") {
            let mut open = true;

            egui::Window::new("M68k Status")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("PC: {:06X}", debug_info.m68k_pc),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("SR: {:04X}", debug_info.m68k_sr),
                    );
                    let sr = debug_info.m68k_sr;
                    ui.horizontal(|ui| {
                        Self::label_fmt(
                            &mut self.label_buffer,
                            ui,
                            format_args!(
                                "Flags: [ {} {} {} {} {} ]",
                                if sr & 0x10 != 0 { "X" } else { "x" },
                                if sr & 0x08 != 0 { "N" } else { "n" },
                                if sr & 0x04 != 0 { "Z" } else { "z" },
                                if sr & 0x02 != 0 { "V" } else { "v" },
                                if sr & 0x01 != 0 { "C" } else { "c" },
                            ),
                        );
                    });
                    ui.separator();
                    // We need to be careful with columns closure if we want to use self.label_buffer
                    // Since columns takes a closure that might outlive the current borrow,
                    // but egui's columns closure is called immediately.
                    // However, &mut self is already borrowed by the outer closure.
                    ui.columns(2, |columns| {
                        let mut d_buf = String::with_capacity(16);
                        let mut a_buf = String::with_capacity(16);
                        for i in 0..8 {
                            d_buf.clear();
                            a_buf.clear();
                            let _ = write!(&mut d_buf, "D{}: {:08X}", i, debug_info.m68k_d[i]);
                            let _ = write!(&mut a_buf, "A{}: {:08X}", i, debug_info.m68k_a[i]);
                            columns[0].label(&d_buf);
                            columns[1].label(&a_buf);
                        }
                    });
                    ui.separator();
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("USP: {:08X}", debug_info.m68k_usp),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("SSP: {:08X}", debug_info.m68k_ssp),
                    );
                });
            if !open {
                self.gui_state.set_window_open("M68k Status", false);
            }
        }
    }

    fn render_z80_status_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Z80 Status") {
            let mut open = true;

            egui::Window::new("Z80 Status")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("PC: {:04X}", debug_info.z80_pc),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("SP: {:04X}", debug_info.z80_sp),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("MEMPTR (WZ): {:04X}", debug_info.z80_memptr),
                    );
                    ui.separator();
                    let f = debug_info.z80_f;
                    ui.horizontal(|ui| {
                        Self::label_fmt(
                            &mut self.label_buffer,
                            ui,
                            format_args!(
                                "Flags: [ {} {} {} {} {} {} {} {} ]",
                                if f & 0x80 != 0 { "S" } else { "s" },
                                if f & 0x40 != 0 { "Z" } else { "z" },
                                if f & 0x20 != 0 { "Y" } else { "y" },
                                if f & 0x10 != 0 { "H" } else { "h" },
                                if f & 0x08 != 0 { "X" } else { "x" },
                                if f & 0x04 != 0 { "P" } else { "p" },
                                if f & 0x02 != 0 { "N" } else { "n" },
                                if f & 0x01 != 0 { "C" } else { "c" },
                            ),
                        );
                    });
                    ui.separator();
                    ui.columns(2, |columns| {
                        let mut buf = String::with_capacity(16);

                        buf.clear();
                        let _ = write!(&mut buf, "A:  {:02X}", debug_info.z80_a);
                        columns[0].label(&buf);

                        buf.clear();
                        let _ = write!(&mut buf, "F:  {:02X}", debug_info.z80_f);
                        columns[1].label(&buf);

                        buf.clear();
                        let _ = write!(
                            &mut buf,
                            "BC: {:02X}{:02X}",
                            debug_info.z80_b, debug_info.z80_c
                        );
                        columns[0].label(&buf);

                        buf.clear();
                        let _ = write!(
                            &mut buf,
                            "DE: {:02X}{:02X}",
                            debug_info.z80_d, debug_info.z80_e
                        );
                        columns[1].label(&buf);

                        buf.clear();
                        let _ = write!(
                            &mut buf,
                            "HL: {:02X}{:02X}",
                            debug_info.z80_h, debug_info.z80_l
                        );
                        columns[0].label(&buf);

                        buf.clear();
                        let _ = write!(&mut buf, "IX: {:04X}", debug_info.z80_ix);
                        columns[1].label(&buf);

                        buf.clear();
                        let _ = write!(&mut buf, "IY: {:04X}", debug_info.z80_iy);
                        columns[0].label(&buf);

                        buf.clear();
                        let _ = write!(&mut buf, "I:  {:02X}", debug_info.z80_i);
                        columns[1].label(&buf);

                        buf.clear();
                        let _ = write!(&mut buf, "R:  {:02X}", debug_info.z80_r);
                        columns[0].label(&buf);
                    });
                    ui.separator();
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("IM: {}", debug_info.z80_im),
                    );
                    Self::label_fmt(
                        &mut self.label_buffer,
                        ui,
                        format_args!("IFF1: {}", debug_info.z80_iff1),
                    );
                });
            if !open {
                self.gui_state.set_window_open("Z80 Status", false);
            }
        }
    }

    fn render_disassembly_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Disassembly") {
            let mut open = true;

            egui::Window::new("Disassembly")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("M68k Disassembly");
                        let mut gnu = self.gui_state.gnu_syntax;
                        if ui.checkbox(&mut gnu, "GNU syntax").changed() {
                            self.gui_state.gnu_syntax = gnu;
                            self.gui_state.save();
                        }
                    });
                    let gnu = self.gui_state.gnu_syntax;
                    egui::ScrollArea::vertical()
                        .id_source("m68k_disasm")
                        .show(ui, |ui| {
                            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                            for (addr, instr, words) in &debug_info.m68k_disasm {
                                let text = instr.format_with_words(*addr, words, gnu);
                                let is_current = *addr == debug_info.m68k_pc;
                                if is_current {
                                    Self::colored_label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        egui::Color32::YELLOW,
                                        format_args!("-> {:06X}: {}", addr, text),
                                    );
                                } else {
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!("   {:06X}: {}", addr, text),
                                    );
                                }
                            }
                        });
                    ui.separator();
                    ui.heading("Z80 Disassembly");
                    egui::ScrollArea::vertical()
                        .id_source("z80_disasm")
                        .show(ui, |ui| {
                            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                            for (addr, byte) in &debug_info.z80_disasm {
                                let is_current = *addr == debug_info.z80_pc;
                                if is_current {
                                    Self::colored_label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        egui::Color32::YELLOW,
                                        format_args!(
                                            "-> {:04X}: {}",
                                            addr, HEX_LOOKUP[*byte as usize]
                                        ),
                                    );
                                } else {
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!(
                                            "   {:04X}: {}",
                                            addr, HEX_LOOKUP[*byte as usize]
                                        ),
                                    );
                                }
                            }
                        });
                });
            if !open {
                self.gui_state.set_window_open("Disassembly", false);
            }
        }
    }

    fn render_palette_viewer_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Palette Viewer") {
            let mut open = true;

            egui::Window::new("Palette Viewer")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    for palette in 0..4 {
                        ui.horizontal(|ui| {
                            Self::label_fmt(
                                &mut self.label_buffer,
                                ui,
                                format_args!("Pal {}:", palette),
                            );
                            for i in 0..16 {
                                let idx = palette * 16 + i;
                                let color565 = debug_info.cram[idx];
                                let r = (((color565 >> 11) & 0x1F) << 3) as u8;
                                let g = (((color565 >> 5) & 0x3F) << 2) as u8;
                                let b = ((color565 & 0x1F) << 3) as u8;
                                let color = egui::Color32::from_rgb(r, g, b);

                                let (rect, response) = ui.allocate_at_least(
                                    egui::vec2(16.0, 16.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 0.0, color);
                                if response.hovered() {
                                    Self::on_hover_text_fmt(
                                        &mut self.label_buffer,
                                        &response,
                                        format_args!(
                                            "Index: {}\nRaw: {:04X}\nRGB565: {:04X}",
                                            idx, debug_info.cram_raw[idx], color565
                                        ),
                                    );
                                }
                            }
                        });
                    }
                });
            if !open {
                self.gui_state.set_window_open("Palette Viewer", false);
            }
        }
    }

    fn render_tile_viewer_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Tile Viewer") {
            let mut open = true;

            egui::Window::new("Tile Viewer")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    if let Some(vram) = &debug_info.vram {
                        // Render tiles to a buffer
                        let image = std::sync::Arc::make_mut(&mut self.tile_viewer_image);
                        for tile_idx in 0..2048 {
                            let tile_x = (tile_idx % 16) * 8;
                            let tile_y = (tile_idx / 16) * 8;

                            for y in 0..8 {
                                let row_addr = tile_idx * 32 + y * 4;
                                for x in 0..8 {
                                    let byte = vram[row_addr + (x / 2)];
                                    let color_idx =
                                        if x % 2 == 0 { byte >> 4 } else { byte & 0x0F };

                                    // Use first palette (0-15)
                                    let color565 = debug_info.cram[color_idx as usize];
                                    let r = (((color565 >> 11) & 0x1F) << 3) as u8;
                                    let g = (((color565 >> 5) & 0x3F) << 2) as u8;
                                    let b = ((color565 & 0x1F) << 3) as u8;

                                    let pixel_idx = (tile_y + y) * 128 + (tile_x + x);
                                    image.pixels[pixel_idx] = egui::Color32::from_rgb(r, g, b);
                                }
                            }
                        }
                    }

                    let texture = self.tile_texture.get_or_insert_with(|| {
                        // Use a 1x1 placeholder, never a 0x0 image: a zero
                        // dimension makes wgpu panic on texture creation.
                        ui.ctx().load_texture(
                            "tile_viewer",
                            egui::ColorImage::new([1, 1], egui::Color32::TRANSPARENT),
                            Default::default(),
                        )
                    });
                    texture.set(self.tile_viewer_image.clone(), Default::default());

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.image(&*texture);
                    });
                });
            if !open {
                self.gui_state.set_window_open("Tile Viewer", false);
            }
        }
    }

    fn render_sprite_viewer_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Sprite Viewer") {
            let mut open = true;

            egui::Window::new("Sprite Viewer")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    if let Some(vram) = &debug_info.vram {
                        let sat_base = ((debug_info.vdp_registers[5] as usize) & 0x7F) << 9;
                        let h40 = (debug_info.vdp_registers[12] & 0x81) == 0x81;
                        let max_sprites = if h40 { 80 } else { 64 };

                        let iter = crate::vdp::SpriteIterator {
                            vram: &vram[..],
                            next_idx: 0,
                            count: 0,
                            max_sprites,
                            sat_base,
                        };

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            egui::Grid::new("sprite_grid").striped(true).show(ui, |ui| {
                                ui.label("Idx");
                                ui.label("Pos");
                                ui.label("Size");
                                ui.label("Tile");
                                ui.label("Pal");
                                ui.label("Pri");
                                ui.label("Flip");
                                ui.label("Link");
                                ui.end_row();

                                for attr in iter {
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!("{}", attr.index),
                                    );
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!("{},{}", attr.h_pos, attr.v_pos),
                                    );
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!("{}x{}", attr.h_size, attr.v_size),
                                    );
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!("{:03X}", attr.base_tile),
                                    );
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!("{}", attr.palette),
                                    );
                                    ui.label(if attr.priority { "H" } else { "L" });
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!(
                                            "{}{}",
                                            if attr.h_flip { "H" } else { "-" },
                                            if attr.v_flip { "V" } else { "-" }
                                        ),
                                    );
                                    Self::label_fmt(
                                        &mut self.label_buffer,
                                        ui,
                                        format_args!("{}", attr.link),
                                    );
                                    ui.end_row();
                                }
                            });
                        });
                    }
                });
            if !open {
                self.gui_state.set_window_open("Sprite Viewer", false);
            }
        }
    }

    fn render_scroll_plane_viewer_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Scroll Plane Viewer") {
            let mut open = true;

            egui::Window::new("Scroll Plane Viewer")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    let size_bits = debug_info.vdp_registers[crate::vdp::REG_PLANE_SIZE];
                    let (plane_w, plane_h) = crate::vdp::Vdp::decode_plane_size(size_bits);

                    ui.label(format!("Plane Size: {}x{}", plane_w, plane_h));

                    ui.horizontal(|ui| {
                        if ui
                            .selectable_value(
                                &mut self.gui_state.scroll_plane_tab,
                                PlaneTab::PlaneA,
                                "Plane A",
                            )
                            .changed()
                        {
                            self.gui_state.save();
                        }
                        if ui
                            .selectable_value(
                                &mut self.gui_state.scroll_plane_tab,
                                PlaneTab::PlaneB,
                                "Plane B",
                            )
                            .changed()
                        {
                            self.gui_state.save();
                        }
                    });

                    let plane_a_base = ((debug_info.vdp_registers[2] as usize) & 0x38) << 10;
                    let plane_b_base = ((debug_info.vdp_registers[4] as usize) & 0x07) << 13;

                    let render_plane = |ui: &mut egui::Ui,
                                        base: usize,
                                        texture_opt: &mut Option<egui::TextureHandle>,
                                        image_arc: &mut std::sync::Arc<egui::ColorImage>,
                                        id: &str| {
                        if let Some(vram) = &debug_info.vram {
                            let image = std::sync::Arc::make_mut(image_arc);
                            let expected_size = [plane_w * 8, plane_h * 8];
                            if image.size != expected_size {
                                *image = egui::ColorImage::new(
                                    expected_size,
                                    egui::Color32::TRANSPARENT,
                                );
                            } else {
                                image.pixels.fill(egui::Color32::TRANSPARENT);
                            }

                            for ty in 0..plane_h {
                                for tx in 0..plane_w {
                                    let entry_addr = base + (ty * plane_w + tx) * 2;
                                    let entry = u16::from_be_bytes([
                                        vram[entry_addr],
                                        vram[entry_addr + 1],
                                    ]);
                                    let tile_idx = entry & 0x07FF;
                                    let palette = ((entry >> 13) & 0x03) as usize;
                                    let v_flip = (entry & 0x1000) != 0;
                                    let h_flip = (entry & 0x0800) != 0;

                                    for py in 0..8 {
                                        let row_addr = tile_idx as usize * 32
                                            + (if v_flip { 7 - py } else { py }) * 4;
                                        for px in 0..8 {
                                            let h_idx = if h_flip { 7 - px } else { px };
                                            let byte = vram[row_addr + h_idx / 2];
                                            let color_idx = if h_idx % 2 == 0 {
                                                byte >> 4
                                            } else {
                                                byte & 0x0F
                                            };

                                            let color565 =
                                                debug_info.cram[palette * 16 + color_idx as usize];
                                            let r = (((color565 >> 11) & 0x1F) << 3) as u8;
                                            let g = (((color565 >> 5) & 0x3F) << 2) as u8;
                                            let b = ((color565 & 0x1F) << 3) as u8;

                                            let pixel_idx =
                                                (ty * 8 + py) * plane_w * 8 + (tx * 8 + px);
                                            image.pixels[pixel_idx] =
                                                egui::Color32::from_rgb(r, g, b);
                                        }
                                    }
                                }
                            }
                        }
                        // A zero-sized image (VRAM not captured yet, or a 0x0
                        // plane) would create a 0-width GPU texture and panic
                        // wgpu ("Dimension X is zero"). Skip until real data.
                        if image_arc.size[0] == 0 || image_arc.size[1] == 0 {
                            ui.label(
                                "Plane data unavailable — resume the emulator with a ROM loaded.",
                            );
                            return;
                        }
                        let texture = texture_opt.get_or_insert_with(|| {
                            ui.ctx().load_texture(
                                id,
                                egui::ColorImage::new([1, 1], egui::Color32::TRANSPARENT),
                                Default::default(),
                            )
                        });
                        texture.set(image_arc.clone(), Default::default());
                        egui::ScrollArea::both().id_source(id).show(ui, |ui| {
                            ui.image(&*texture);
                        });
                    };

                    match self.gui_state.scroll_plane_tab {
                        PlaneTab::PlaneA => {
                            render_plane(
                                ui,
                                plane_a_base,
                                &mut self.plane_a_texture,
                                &mut self.plane_a_viewer_image,
                                "plane_a",
                            );
                        }
                        PlaneTab::PlaneB => {
                            render_plane(
                                ui,
                                plane_b_base,
                                &mut self.plane_b_texture,
                                &mut self.plane_b_viewer_image,
                                "plane_b",
                            );
                        }
                    }
                });
            if !open {
                self.gui_state.set_window_open("Scroll Plane Viewer", false);
            }
        }
    }

    fn render_vdp_memory_hex_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("VDP Memory Hex") {
            let mut open = true;

            egui::Window::new("VDP Memory Hex")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    if let Some(vram) = &debug_info.vram {
                        ui.collapsing("VRAM", |ui| {
                            egui::ScrollArea::vertical()
                                .id_source("vram_hex")
                                .show_rows(
                                    ui,
                                    ui.text_style_height(&egui::TextStyle::Monospace),
                                    0x10000 / 16,
                                    |ui, row_range| {
                                        egui::Grid::new("vram_grid").show(ui, |ui| {
                                            let mut l_buffer = String::with_capacity(64);
                                            for row in row_range {
                                                let addr = row * 16;
                                                l_buffer.clear();
                                                let _ = write!(&mut l_buffer, "{:04X}:", addr);
                                                ui.label(
                                                    egui::RichText::new(&l_buffer).monospace(),
                                                );

                                                l_buffer.clear();
                                                for i in 0..16 {
                                                    l_buffer.push_str(
                                                        HEX_LOOKUP[vram[addr + i] as usize],
                                                    );
                                                    l_buffer.push(' ');
                                                }
                                                ui.label(
                                                    egui::RichText::new(&l_buffer).monospace(),
                                                );
                                                ui.end_row();
                                            }
                                        });
                                    },
                                );
                        });
                    }
                    ui.collapsing("CRAM", |ui| {
                        egui::Grid::new("cram_grid").show(ui, |ui| {
                            let mut l_buffer = String::with_capacity(64);
                            for row in 0..4 {
                                let addr = row * 16;
                                l_buffer.clear();
                                let _ = write!(&mut l_buffer, "{:02X}:", addr);
                                ui.label(egui::RichText::new(&l_buffer).monospace());

                                l_buffer.clear();
                                for i in 0..16 {
                                    let val = if (addr + i) % 2 == 0 {
                                        debug_info.cram_raw[(addr + i) / 2] >> 8
                                    } else {
                                        debug_info.cram_raw[(addr + i) / 2] & 0xFF
                                    } as u8;
                                    l_buffer.push_str(HEX_LOOKUP[val as usize]);
                                    l_buffer.push(' ');
                                }
                                ui.label(egui::RichText::new(&l_buffer).monospace());
                                ui.end_row();
                            }
                        });
                    });
                    ui.collapsing("VSRAM", |ui| {
                        egui::Grid::new("vsram_grid").show(ui, |ui| {
                            let mut l_buffer = String::with_capacity(64);
                            for row in 0..5 {
                                let addr = row * 16;
                                l_buffer.clear();
                                let _ = write!(&mut l_buffer, "{:02X}:", addr);
                                ui.label(egui::RichText::new(&l_buffer).monospace());

                                l_buffer.clear();
                                for i in 0..16 {
                                    if addr + i < 80 {
                                        l_buffer.push_str(
                                            HEX_LOOKUP[debug_info.vsram[addr + i] as usize],
                                        );
                                        l_buffer.push(' ');
                                    } else {
                                        l_buffer.push_str("   ");
                                    }
                                }
                                ui.label(egui::RichText::new(&l_buffer).monospace());
                                ui.end_row();
                            }
                        });
                    });
                });
            if !open {
                self.gui_state.set_window_open("VDP Memory Hex", false);
            }
        }
    }

    fn render_memory_viewer_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Memory Viewer") {
            let mut open = true;

            egui::Window::new("Memory Viewer")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    if let Some(wram) = &debug_info.wram {
                        ui.collapsing("Work RAM (M68k)", |ui| {
                            egui::ScrollArea::vertical()
                                .id_source("wram_hex")
                                .show_rows(
                                    ui,
                                    ui.text_style_height(&egui::TextStyle::Monospace),
                                    0x10000 / 16,
                                    |ui, row_range| {
                                        egui::Grid::new("wram_grid").show(ui, |ui| {
                                            let mut l_buffer = String::with_capacity(64);
                                            for row in row_range {
                                                let addr = row * 16;
                                                l_buffer.clear();
                                                let _ = write!(&mut l_buffer, "{:04X}:", addr);
                                                ui.label(
                                                    egui::RichText::new(&l_buffer).monospace(),
                                                );

                                                l_buffer.clear();
                                                for i in 0..16 {
                                                    l_buffer.push_str(
                                                        HEX_LOOKUP[wram[addr + i] as usize],
                                                    );
                                                    l_buffer.push(' ');
                                                }
                                                ui.label(
                                                    egui::RichText::new(&l_buffer).monospace(),
                                                );
                                                ui.end_row();
                                            }
                                        });
                                    },
                                );
                        });
                    }
                    if let Some(z80_ram) = &debug_info.z80_ram {
                        ui.collapsing("Z80 RAM", |ui| {
                            egui::ScrollArea::vertical()
                                .id_source("z80_ram_hex")
                                .show_rows(
                                    ui,
                                    ui.text_style_height(&egui::TextStyle::Monospace),
                                    0x2000 / 16,
                                    |ui, row_range| {
                                        egui::Grid::new("z80_ram_grid").show(ui, |ui| {
                                            let mut l_buffer = String::with_capacity(64);
                                            for row in row_range {
                                                let addr = row * 16;
                                                l_buffer.clear();
                                                let _ = write!(&mut l_buffer, "{:04X}:", addr);
                                                ui.label(
                                                    egui::RichText::new(&l_buffer).monospace(),
                                                );

                                                l_buffer.clear();
                                                for i in 0..16 {
                                                    l_buffer.push_str(
                                                        HEX_LOOKUP[z80_ram[addr + i] as usize],
                                                    );
                                                    l_buffer.push(' ');
                                                }
                                                ui.label(
                                                    egui::RichText::new(&l_buffer).monospace(),
                                                );
                                                ui.end_row();
                                            }
                                        });
                                    },
                                );
                        });
                    }
                });
            if !open {
                self.gui_state.set_window_open("Memory Viewer", false);
            }
        }
    }

    fn render_sound_chip_visualizer_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Sound Chip Visualizer") {
            let mut open = true;

            egui::Window::new("Sound Chip Visualizer")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    ui.collapsing("SN76489 PSG", |ui| {
                        for i in 0..3 {
                            ui.horizontal(|ui| {
                                ui.label(format!("Tone {}:", i));
                                ui.label(format!("Freq: {:03X}", debug_info.psg_tone[i].frequency));
                                ui.label(format!("Vol: {:01X}", debug_info.psg_tone[i].volume));
                                let vol_norm = 1.0 - (debug_info.psg_tone[i].volume as f32 / 15.0);
                                ui.add(egui::ProgressBar::new(vol_norm).show_percentage());
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.label("Noise:");
                            ui.label(if debug_info.psg_noise.white_noise {
                                "White"
                            } else {
                                "Periodic"
                            });
                            ui.label(format!("Rate: {}", debug_info.psg_noise.shift_rate));
                            ui.label(format!("Vol: {:01X}", debug_info.psg_noise.volume));
                            let vol_norm = 1.0 - (debug_info.psg_noise.volume as f32 / 15.0);
                            ui.add(egui::ProgressBar::new(vol_norm).show_percentage());
                        });
                    });

                    ui.collapsing("YM2612 FM", |ui| {
                        for ch in 0..6 {
                            let bank = if ch < 3 { 0 } else { 1 };
                            let ch_offset = ch % 3;

                            ui.collapsing(format!("Channel {}", ch + 1), |ui| {
                                let fb_algo = debug_info.ym2612_regs[bank][0xB0 + ch_offset];
                                let feedback = (fb_algo >> 3) & 0x07;
                                let algo = fb_algo & 0x07;
                                ui.horizontal(|ui| {
                                    ui.label(format!("Algo: {}", algo));
                                    ui.label(format!("FB: {}", feedback));
                                    let pan = debug_info.ym2612_regs[bank][0xB4 + ch_offset];
                                    ui.label(format!(
                                        "Pan: {}{}",
                                        if pan & 0x80 != 0 { "L" } else { "-" },
                                        if pan & 0x40 != 0 { "R" } else { "-" }
                                    ));
                                });

                                egui::Grid::new(format!("ch_{}_ops", ch)).show(ui, |ui| {
                                    ui.label("Op");
                                    ui.label("MULT");
                                    ui.label("TL");
                                    ui.label("AR");
                                    ui.label("DR");
                                    ui.label("SR");
                                    ui.label("RR");
                                    ui.label("SL");
                                    ui.end_row();

                                    for op in 0..4 {
                                        let op_offset = ch_offset + (op * 4);
                                        let det_mul =
                                            debug_info.ym2612_regs[bank][0x30 + op_offset];
                                        let tl =
                                            debug_info.ym2612_regs[bank][0x40 + op_offset] & 0x7F;
                                        let rs_ar = debug_info.ym2612_regs[bank][0x50 + op_offset];
                                        let am_dr = debug_info.ym2612_regs[bank][0x60 + op_offset];
                                        let sr =
                                            debug_info.ym2612_regs[bank][0x70 + op_offset] & 0x1F;
                                        let sl_rr = debug_info.ym2612_regs[bank][0x80 + op_offset];

                                        ui.label(format!("{}", op + 1));
                                        ui.label(format!("{}", det_mul & 0x0F));
                                        ui.label(format!("{:02X}", tl));
                                        ui.label(format!("{:02X}", rs_ar & 0x1F));
                                        ui.label(format!("{:02X}", am_dr & 0x1F));
                                        ui.label(format!("{:02X}", sr));
                                        ui.label(format!("{:02X}", sl_rr & 0x0F));
                                        ui.label(format!("{:02X}", sl_rr >> 4));
                                        ui.end_row();
                                    }
                                });
                            });
                        }
                    });
                });
            if !open {
                self.gui_state
                    .set_window_open("Sound Chip Visualizer", false);
            }
        }
    }

    fn render_audio_channel_waveforms_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Audio Channel Waveforms") {
            let mut open = true;

            egui::Window::new("Audio Channel Waveforms")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    for ch in 0..10 {
                        let label = if ch < 6 {
                            format!("FM {}", ch + 1)
                        } else if ch < 9 {
                            format!("PSG Tone {}", ch - 6)
                        } else {
                            "PSG Noise".to_string()
                        };
                        ui.label(&label);

                        let (rect, _response) =
                            ui.allocate_at_least(egui::vec2(256.0, 48.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                        let points: [egui::Pos2; 128] = std::array::from_fn(|i| {
                            let val = debug_info.channel_waveforms[ch][i];
                            let x = rect.left() + (i as f32 * 2.0);
                            let y = rect.center().y - (val as f32 / 16384.0 * 20.0);
                            egui::pos2(x, y)
                        });

                        for i in 0..127 {
                            ui.painter().line_segment(
                                [points[i], points[i + 1]],
                                (1.0, egui::Color32::GREEN),
                            );
                        }
                    }
                });
            if !open {
                self.gui_state
                    .set_window_open("Audio Channel Waveforms", false);
            }
        }
    }

    fn render_controller_viewer_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("Controller Viewer") {
            let mut open = true;

            egui::Window::new("Controller Viewer")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    for (i, (state, c_type)) in [
                        (debug_info.port1_state, debug_info.port1_type),
                        (debug_info.port2_state, debug_info.port2_type),
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.group(|ui| {
                            ui.heading(format!("Port {}", i + 1));
                            ui.label(format!("Type: {:?}", c_type));
                            ui.label(format!("Buttons: {}", state.to_button_string()));

                            egui::Grid::new(format!("port_{}_grid", i)).show(ui, |ui| {
                                ui.label(if state.up { " [U] " } else { "  U  " });
                                ui.label(if state.down { " [D] " } else { "  D  " });
                                ui.label(if state.left { " [L] " } else { "  L  " });
                                ui.label(if state.right { " [R] " } else { "  R  " });
                                ui.end_row();
                                ui.label(if state.a { " [A] " } else { "  A  " });
                                ui.label(if state.b { " [B] " } else { "  B  " });
                                ui.label(if state.c { " [C] " } else { "  C  " });
                                ui.label(if state.start { " [S] " } else { "  S  " });
                                ui.end_row();
                                if matches!(c_type, crate::io::ControllerType::SixButton) {
                                    ui.label(if state.x { " [X] " } else { "  X  " });
                                    ui.label(if state.y { " [Y] " } else { "  Y  " });
                                    ui.label(if state.z { " [Z] " } else { "  Z  " });
                                    ui.label(if state.mode { " [M] " } else { "  M  " });
                                    ui.end_row();
                                }
                            });
                        });
                    }
                });
            if !open {
                self.gui_state.set_window_open("Controller Viewer", false);
            }
        }
    }

    fn render_expansion_status_window(&mut self) {
        if self.gui_state.is_window_open("Expansion Status") {
            let mut open = true;
            egui::Window::new("Expansion Status")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    ui.group(|ui| {
                        ui.heading("Sega CD");
                        ui.label("Status: NOT CONNECTED");
                        ui.add_enabled(false, egui::Button::new("Mount Disc..."));
                    });
                    ui.separator();
                    ui.group(|ui| {
                        ui.heading("Sega 32X");
                        ui.label("Status: NOT CONNECTED");
                        ui.add_enabled(false, egui::Button::new("Enable 32X"));
                    });
                });
            if !open {
                self.gui_state.set_window_open("Expansion Status", false);
            }
        }
    }

    fn render_state_browser_window(&mut self, debug_info: &DebugInfo) {
        if self.gui_state.is_window_open("State Browser") {
            let mut open = true;

            egui::Window::new("State Browser")
                .open(&mut open)
                .show(&self.egui_ctx, |ui| {
                    if let Some(path) = &debug_info.current_rom_path {
                        egui::Grid::new("state_browser_grid")
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Slot");
                                ui.label("Status");
                                ui.label("Actions");
                                ui.end_row();

                                for slot in 0..10 {
                                    ui.label(SLOT_NAMES[slot as usize]);
                                    let state_path = path.with_extension(SLOT_EXTS[slot as usize]);
                                    if state_path.exists() {
                                        let meta = state_path.metadata().ok();
                                        let time = meta
                                            .and_then(|m| m.modified().ok())
                                            .map(|t| {
                                                let duration = std::time::SystemTime::now()
                                                    .duration_since(t)
                                                    .unwrap_or_default();
                                                format!("{:.1}m ago", duration.as_secs_f32() / 60.0)
                                            })
                                            .unwrap_or_else(|| "Exists".to_string());
                                        ui.label(time);
                                        ui.horizontal(|ui| {
                                            if ui.button("Load").clicked() {
                                                self.gui_state.load_requested = Some(slot);
                                            }
                                            if ui.button("Overwrite").clicked() {
                                                self.gui_state.save_requested = Some(slot);
                                            }
                                            if ui.button("🗑").on_hover_text("Delete").clicked() {
                                                self.gui_state.delete_state_requested = Some(slot);
                                            }
                                        });
                                    } else {
                                        ui.label("Empty");
                                        if ui.button("Save").clicked() {
                                            self.gui_state.save_requested = Some(slot);
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    } else {
                        ui.label("No ROM loaded");
                    }
                });
            if !open {
                self.gui_state.set_window_open("State Browser", false);
            }
        }
    }
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let full_output = self.egui_ctx.end_frame();
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        // Update textures
        for (id, image_delta) in full_output.textures_delta.set {
            self.renderer
                .update_texture(device, queue, id, &image_delta);
        }
        // Prepare renderer
        self.renderer
            .update_buffers(device, queue, encoder, &paint_jobs, &self.screen_descriptor);
        // Render GUI
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: render_target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &attachments[..],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            self.renderer
                .render(&mut rpass, &paint_jobs, &self.screen_descriptor);
        }
        // Clean up textures
        for id in full_output.textures_delta.free {
            self.renderer.free_texture(&id);
        }
    }

    pub fn handle_exit(&mut self, emulator: &mut Emulator, record_path: &Option<String>) {
        if self.gui_state.auto_save_load {
            if let Some(path) = &emulator.current_rom_path {
                emulator.save_state_to_path(path.with_extension("auto"));
            }
        }
        self.gui_state.save();
        if let Some(path) = record_path {
            let script: InputScript = emulator.input.stop_recording();
            if let Err(e) = script.save(path) {
                eprintln!("Failed to save recorded script: {}", e);
            } else {
                println!("Recorded script saved to: {}", path);
            }
        }
    }
}

#[cfg(feature = "gui")]
fn collect_debug_info(
    emulator: &mut Emulator,
    gui_state: &GuiState,
    force_red: bool,
    pixels_frame: &mut [u8],
) -> DebugInfo {
    let mut bus = emulator.bus.borrow_mut();
    if force_red {
        bus.vdp.framebuffer.fill(0xF800); // Red in RGB565
    }
    let mut m68k_disasm = [(
        0u32,
        crate::cpu::instructions::Instruction::System(
            crate::cpu::instructions::SystemInstruction::Unimplemented { opcode: 0 },
        ),
        [0u16; 5],
    ); 10];
    let mut addr = emulator.cpu.pc;
    for item in &mut m68k_disasm {
        let opcode = bus.read_word(addr);
        let instr = crate::cpu::decode(opcode);
        let len = instr.length_words().min(5);
        let mut words = [0u16; 5];
        for (i, w) in words.iter_mut().enumerate().take(len as usize) {
            *w = bus.read_word(addr + (i as u32) * 2);
        }
        *item = (addr, instr, words);
        addr += instr.length_words() * 2;
    }

    let mut z80_disasm = [(0u16, 0u8); 10];
    let mut addr = emulator.z80.pc;
    for item in &mut z80_disasm {
        let byte = bus.read_byte(0xA00000 + addr as u32);
        *item = (addr, byte);
        addr += 1;
    }

    let mut cram_raw = [0u16; 64];
    for (i, value) in cram_raw.iter_mut().enumerate() {
        *value = u16::from_be_bytes([bus.vdp.cram[i * 2], bus.vdp.cram[i * 2 + 1]]);
    }

    let wram = if gui_state.is_window_open("Memory Viewer") {
        let mut wram = [0u8; 0x10000];
        wram.copy_from_slice(&bus.work_ram);
        Some(wram)
    } else {
        None
    };

    let z80_ram = if gui_state.is_window_open("Memory Viewer") {
        let mut z80_ram = [0u8; 0x2000];
        z80_ram.copy_from_slice(&bus.z80_ram);
        Some(z80_ram)
    } else {
        None
    };

    let vram = if gui_state.is_window_open("VDP Memory Hex")
        || gui_state.is_window_open("Tile Viewer")
        || gui_state.is_window_open("Sprite Viewer")
        || gui_state.is_window_open("Scroll Plane Viewer")
    {
        Some(bus.vdp.vram)
    } else {
        None
    };

    let info = DebugInfo {
        m68k_pc: emulator.cpu.pc,
        m68k_d: emulator.cpu.d,
        m68k_a: emulator.cpu.a,
        m68k_sr: emulator.cpu.sr,
        m68k_usp: emulator.cpu.usp,
        m68k_ssp: emulator.cpu.ssp,
        m68k_disasm,
        z80_pc: emulator.z80.pc,
        z80_a: emulator.z80.a,
        z80_f: emulator.z80.f,
        z80_b: emulator.z80.b,
        z80_c: emulator.z80.c,
        z80_d: emulator.z80.d,
        z80_e: emulator.z80.e,
        z80_h: emulator.z80.h,
        z80_l: emulator.z80.l,
        z80_ix: emulator.z80.ix,
        z80_iy: emulator.z80.iy,
        z80_sp: emulator.z80.sp,
        z80_i: emulator.z80.i,
        z80_r: emulator.z80.r,
        z80_memptr: emulator.z80.memptr,
        z80_iff1: emulator.z80.iff1,
        z80_im: emulator.z80.im,
        z80_disasm,
        frame_count: emulator.internal_frame_count,
        vdp_status: bus.vdp.read_status(),
        vdp_registers: bus.vdp.registers,
        display_enabled: bus.vdp.display_enabled(),
        bg_color_index: bus.vdp.registers[7],
        cram: bus.vdp.cram_cache,
        cram_raw,
        vram,
        vsram: bus.vdp.vsram,
        wram,
        z80_ram,
        ym2612_regs: bus.apu.fm.registers,
        psg_tone: bus.apu.psg.tones.clone(),
        psg_noise: bus.apu.psg.noise.clone(),
        channel_waveforms: bus.apu.channel_buffers,
        port1_state: bus.io.port1.state,
        port1_type: bus.io.port1.controller_type,
        port2_state: bus.io.port2.state,
        port2_type: bus.io.port2.controller_type,
        has_rom: !bus.rom.is_empty(),
        current_rom_path: emulator.current_rom_path.clone(),
    };
    frontend::rgb565_to_rgba8(&bus.vdp.framebuffer, pixels_frame);
    info
}

#[cfg(feature = "gui")]
fn handle_keyboard_input(
    key_event: &winit::event::KeyEvent,
    emulator: &mut Emulator,
    input: &mut crate::input::FrameInput,
    framework: &mut Framework,
    record_path: &Option<String>,
) -> bool {
    // If egui wants focus, don't process game input
    if framework.egui_ctx.wants_keyboard_input() {
        return false;
    }
    let pressed = key_event.state == ElementState::Pressed;
    // 1. Try physical key first
    let mut handled = false;
    if let PhysicalKey::Code(keycode) = key_event.physical_key {
        if keycode == KeyCode::Escape && pressed {
            println!("Escape pressed, exiting");
            framework.handle_exit(emulator, record_path);
            return true;
        }
        // Custom keymap (Input Mapping window) takes precedence over presets
        if let Some(button) =
            frontend::custom_button_for_key(keycode, &framework.gui_state.custom_keymap)
        {
            input.p1.set_button(button, pressed);
            handled = true;
        } else if let Some((button, _)) =
            frontend::keycode_to_button(keycode, emulator.input_mapping)
        {
            input.p1.set_button(button, pressed);
            handled = true;
        }
    }
    // 2. Fallback to logical key
    if !handled {
        use winit::keyboard::Key;
        if let Key::Named(named) = key_event.logical_key {
            let button = match named {
                winit::keyboard::NamedKey::ArrowUp => Some("up"),
                winit::keyboard::NamedKey::ArrowDown => Some("down"),
                winit::keyboard::NamedKey::ArrowLeft => Some("left"),
                winit::keyboard::NamedKey::ArrowRight => Some("right"),
                winit::keyboard::NamedKey::Enter => Some("start"),
                winit::keyboard::NamedKey::Space => Some("mode"),
                _ => None,
            };
            if let Some(btn) = button {
                input.p1.set_button(btn, pressed);
            }
        }
    }
    false
}

#[cfg(feature = "gui")]
pub fn run(mut emulator: Emulator, record_path: Option<String>) -> Result<(), String> {
    if emulator.input_mapping == InputMapping::Original {
        println!("Controls: Arrow keys=D-pad, Z=A, X=B, C=C, Enter=Start");
    } else {
        println!("Controls: WASD/Arrows=D-pad, J/Z=A, K/X=B, L/C=C, U=X, I=Y, O=Z, Enter=Start, Space=Mode");
    }
    println!("Press Escape to quit.");
    if let Some(path) = &record_path {
        println!("Recording inputs to: {}", path);
        emulator.input.start_recording();
    }
    #[cfg(target_os = "linux")]
    let event_loop = {
        use winit::platform::x11::EventLoopBuilderExtX11;
        let mut builder = winit::event_loop::EventLoopBuilder::new();
        builder.with_any_thread(true);
        builder.build()
    }
    .map_err(|e| e.to_string())?;

    #[cfg(not(target_os = "linux"))]
    let event_loop = winit::event_loop::EventLoop::new().map_err(|e| e.to_string())?;
    let size = winit::dpi::LogicalSize::new(
        frontend::GENESIS_WIDTH as f64 * 3.0,
        frontend::GENESIS_HEIGHT as f64 * 3.0,
    );
    let window = WindowBuilder::new()
        .with_title("Genteel - Sega Genesis Emulator")
        .with_inner_size(size)
        .with_min_inner_size(winit::dpi::LogicalSize::new(
            frontend::GENESIS_WIDTH as f64,
            frontend::GENESIS_HEIGHT as f64,
        ))
        .build(&event_loop)
        .map_err(|e| e.to_string())?;
    // Leak the window to get a &'static Window, simplifying lifetime management
    let window: &'static winit::window::Window = Box::leak(Box::new(window));
    let mut pixels = {
        let window_size = window.inner_size();
        let build_pixels = |backend| {
            let surface_texture =
                SurfaceTexture::new(window_size.width, window_size.height, window);
            pixels::PixelsBuilder::new(320, 240, surface_texture)
                .wgpu_backend(backend)
                .build()
        };

        match build_pixels(pixels::wgpu::Backends::all()) {
            Ok(p) => p,
            Err(_) => {
                log::warn!("wgpu Backends::all() failed; falling back to GL backend");
                build_pixels(pixels::wgpu::Backends::GL).map_err(|e| e.to_string())?
            }
        }
    };
    // Initialize egui framework
    let mut framework = Framework::new(
        &event_loop,
        window.inner_size().width,
        window.inner_size().height,
        window.scale_factor() as f32,
        &pixels,
        emulator.input_mapping,
    );
    // Audio setup
    let audio_buffer = audio::create_audio_buffer();
    let audio_output = match audio::AudioOutput::new(audio_buffer.clone()) {
        Ok(output) => Some(output),
        Err(e) => {
            eprintln!("Warning: Failed to initialize audio: {}", e);
            None
        }
    };
    if let Some(output) = &audio_output {
        emulator.bus.borrow_mut().sample_rate = output.sample_rate;
    }
    let _audio_output = audio_output;
    // Input and Timing state
    let mut input = crate::input::FrameInput::default();
    let mut frame_count: u64 = 0;
    let mut last_frame_inst = std::time::Instant::now();
    let mut fps_timer = std::time::Instant::now();
    let mut fps_count = 0;
    let mut pending_audio_upload = Vec::new();
    println!("Starting event loop...");
    event_loop
        .run(move |event, target| {
            let _keep_audio_alive = &_audio_output; // Ensure audio_output is moved into the closure

            match event {
                Event::WindowEvent { event, .. } => {
                    // Handle GUI events
                    framework.handle_event(window, &event);
                    match event {
                        WindowEvent::CloseRequested => {
                            println!("Using CloseRequested to exit");
                            framework.handle_exit(&mut emulator, &record_path);
                            target.exit();
                        }
                        WindowEvent::KeyboardInput {
                            event: key_event, ..
                        } => {
                            if handle_keyboard_input(
                                &key_event,
                                &mut emulator,
                                &mut input,
                                &mut framework,
                                &record_path,
                            ) {
                                target.exit();
                            }
                        }
                        WindowEvent::Resized(size) => {
                            if size.width > 0 && size.height > 0 {
                                pixels.resize_surface(size.width, size.height).ok();
                                framework.resize(size.width, size.height);
                            }
                        }
                        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            framework.scale_factor(scale_factor as f32);
                        }
                        WindowEvent::RedrawRequested => {
                            // Poll gamepads
                            #[cfg(feature = "gilrs")]
                            framework.poll_gamepads(&mut input.p1);

                            // Check for pick ROM request
                            if framework.gui_state.pick_rom_requested {
                                framework.pick_rom();
                                framework.gui_state.pick_rom_requested = false;
                            }

                            // Check for pending ROM load
                            let pending = {
                                match framework.pending_rom_path.lock() {
                                    Ok(mut lock) => lock.take(),
                                    Err(_) => {
                                        eprintln!("Failed to acquire pending_rom_path lock");
                                        None
                                    }
                                }
                            };
                            if let Some(path) = pending {
                                println!("Loading ROM: {:?}", path);
                                // Security: Whitelist the directory containing the ROM
                                if let Ok(canonical) = path.canonicalize() {
                                    if let Some(parent) = canonical.parent() {
                                        let _ = emulator.add_allowed_path(parent);
                                    }
                                }
                                if let Err(e) = emulator.load_rom(path.to_str().unwrap_or("")) {
                                    eprintln!("Failed to load ROM: {}", e);
                                } else {
                                    // Update recent ROMs
                                    let mut recent = framework.gui_state.recent_roms.clone();
                                    recent.retain(|p| p != &path);
                                    recent.insert(0, path.clone());
                                    recent.truncate(10);
                                    framework.gui_state.recent_roms = recent;
                                    framework.gui_state.save();

                                    // Auto-Load if enabled
                                    if framework.gui_state.auto_save_load {
                                        let auto_path = path.with_extension("auto");
                                        if auto_path.exists() {
                                            emulator.load_state_from_path(auto_path);
                                        }
                                    }
                                }
                            }
                            if framework.gui_state.reset_requested {
                                println!("Hard resetting emulator");
                                emulator.hard_reset();
                                framework.gui_state.reset_requested = false;
                            }
                            if framework.gui_state.close_requested {
                                println!("Closing ROM");
                                if framework.gui_state.auto_save_load {
                                    if let Some(path) = &emulator.current_rom_path {
                                        emulator.save_state_to_path(path.with_extension("auto"));
                                    }
                                }
                                emulator.close_rom();
                                framework.gui_state.save();
                                framework.gui_state.close_requested = false;
                            }
                            if let Some(slot) = framework.gui_state.save_requested {
                                emulator.save_state(slot);
                                framework.gui_state.save_requested = None;
                            }
                            if let Some(slot) = framework.gui_state.load_requested {
                                emulator.load_state(slot);
                                framework.gui_state.load_requested = None;
                            }
                            if let Some(slot) = framework.gui_state.delete_state_requested {
                                emulator.delete_state(slot);
                                framework.gui_state.delete_state_requested = None;
                            }

                            // Sync settings from GUI. single_step is NOT
                            // consumed here: frames only run in AboutToWait,
                            // and consuming the flag on an OS-triggered redraw
                            // would swallow the pending step.
                            emulator.input_mapping = framework.gui_state.input_mapping;
                            let force_red = framework.gui_state.force_red;
                            emulator.paused = framework.gui_state.paused;

                            // Poll GDB (can override GUI state)
                            emulator.poll_gdb();

                            // Sync emulator state back to GUI
                            framework.gui_state.paused = emulator.paused;
                            // Update FPS in title bar every second
                            if fps_timer.elapsed() >= std::time::Duration::from_secs(1) {
                                window.set_title(&format!(
                                    "Genteel - Sega Genesis Emulator | FPS: {}",
                                    fps_count
                                ));
                                fps_count = 0;
                                fps_timer = std::time::Instant::now();
                            }
                            // Debug: Print every 60 frames
                            if emulator.debug && frame_count % 60 == 1 {
                                emulator.log_debug(frame_count);
                            }

                            // Collect debug info and render
                            let debug_info = collect_debug_info(
                                &mut emulator,
                                &framework.gui_state,
                                force_red,
                                pixels.frame_mut(),
                            );

                            // Update egui
                            framework.prepare(window, &debug_info);
                            if let Err(e) = pixels.render_with(|encoder, render_target, context| {
                                // Render the board
                                context.scaling_renderer.render(encoder, render_target);
                                // Render GUI
                                framework.render(
                                    encoder,
                                    render_target,
                                    &context.device,
                                    &context.queue,
                                );
                                Ok(())
                            }) {
                                eprintln!("Render error: {}", e);
                                target.exit();
                            }
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => {
                    let now = std::time::Instant::now();
                    let frame_duration =
                        frame_duration_for_region(emulator.bus.borrow().vdp.is_pal);
                    let next_frame = last_frame_inst + frame_duration;
                    if now >= next_frame {
                        emulator.input_mapping = framework.gui_state.input_mapping;
                        emulator.paused = framework.gui_state.paused;
                        emulator.single_step = framework.gui_state.single_step;
                        framework.gui_state.single_step = false;
                        emulator.poll_gdb();
                        framework.gui_state.paused = emulator.paused;

                        let frames_to_run = frames_due(now, last_frame_inst, frame_duration, 4);

                        run_emulation_frames(
                            &mut emulator,
                            &input,
                            &mut pending_audio_upload,
                            frames_to_run,
                        );
                        flush_pending_audio_upload(&audio_buffer, &mut pending_audio_upload);
                        last_frame_inst += frame_duration.mul_f64(frames_to_run as f64);
                        // A stall longer than the catch-up budget (suspend,
                        // blocking window drag, sustained slowdown) must not
                        // be replayed at fast-forward speed: drop the backlog
                        // and resynchronize to wall-clock.
                        if now.duration_since(last_frame_inst) > frame_duration.mul_f64(4.0) {
                            last_frame_inst = now;
                        }
                        frame_count += frames_to_run as u64;
                        fps_count += frames_to_run as u64;
                        window.request_redraw();
                    } else {
                        flush_pending_audio_upload(&audio_buffer, &mut pending_audio_upload);
                    }
                    target.set_control_flow(winit::event_loop::ControlFlow::Poll);
                }
                _ => {}
            }
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_duration_for_region_matches_video_rate() {
        let ntsc = frame_duration_for_region(false);
        let pal = frame_duration_for_region(true);

        assert!(ntsc < pal);
        assert_eq!(ntsc, std::time::Duration::from_secs_f64(1.0 / 60.0));
        assert_eq!(pal, std::time::Duration::from_secs_f64(1.0 / 50.0));
    }

    #[test]
    fn test_trim_pending_audio_to_limit_drops_whole_stereo_frames() {
        let mut pending = vec![1, 2, 3, 4, 5, 6];
        trim_pending_audio_to_limit(&mut pending, 4);
        assert_eq!(pending, vec![3, 4, 5, 6]);
    }

    #[test]
    fn test_frames_due_is_bounded_and_zero_before_deadline() {
        let start = std::time::Instant::now();
        let frame = std::time::Duration::from_secs_f64(1.0 / 60.0);

        assert_eq!(frames_due(start, start, frame, 4), 0);
        assert_eq!(frames_due(start + frame, start, frame, 4), 1);
        assert_eq!(frames_due(start + frame.mul_f64(3.2), start, frame, 4), 3);
        assert_eq!(frames_due(start + frame.mul_f64(10.0), start, frame, 4), 4);
    }

    #[cfg(feature = "gilrs")]
    #[test]
    fn test_gilrs_initialization_failure() {
        // Set the environment variable to force initialization failure
        std::env::set_var("GENTEEL_TEST_FAIL_GILRS", "1");

        let result = init_gilrs();

        // Clean up the environment variable
        std::env::remove_var("GENTEEL_TEST_FAIL_GILRS");

        // Assert that initialization failed gracefully by returning None
        assert!(
            result.is_none(),
            "Expected Gilrs initialization to fail gracefully when GENTEEL_TEST_FAIL_GILRS is set"
        );
    }

    #[cfg(feature = "gilrs")]
    #[test]
    fn test_init_gilrs_with_builder_success() {
        let builder = || Ok::<i32, String>(42);
        let result = init_gilrs_with_builder(builder);
        assert_eq!(result, Some(42));
    }

    #[cfg(feature = "gilrs")]
    #[test]
    fn test_init_gilrs_with_builder_error() {
        let builder = || Err::<i32, String>("Initialization failed".to_string());
        let result = init_gilrs_with_builder(builder);
        assert_eq!(result, None);
    }

    #[cfg(feature = "gui")]
    #[test]
    fn test_get_gui_config_path_env_override() {
        let test_path = "custom_config_path.json";
        std::env::set_var("GENTEEL_GUI_CONFIG", test_path);
        let path = get_gui_config_path();
        std::env::remove_var("GENTEEL_GUI_CONFIG");
        assert_eq!(path, PathBuf::from(test_path));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn test_get_gui_config_path_default() {
        // Clear env var just in case
        std::env::remove_var("GENTEEL_GUI_CONFIG");
        let path = get_gui_config_path();

        // It should at least end with gui_config.json
        assert!(path.to_str().unwrap().ends_with("gui_config.json"));
    }
}
