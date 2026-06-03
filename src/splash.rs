use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

use crate::theme::Theme;

// ── cyberpunk palette ────────────────────────────────────────────────────────

const COL_SKY_DEEP: Color = Color::Rgb(0x0a, 0x0e, 0x18);
const COL_SKY_MID: Color = Color::Rgb(0x10, 0x18, 0x28);
const COL_STAR: Color = Color::Rgb(0x40, 0x55, 0x70);
const COL_STAR_BRIGHT: Color = Color::Rgb(0x70, 0x90, 0xb0);

const COL_BUILDING_DARK: Color = Color::Rgb(0x0c, 0x10, 0x1a);
const COL_BUILDING_EDGE: Color = Color::Rgb(0x1a, 0x24, 0x34);
const COL_WINDOW_LIT: Color = Color::Rgb(0xd0, 0xb0, 0x50);
const COL_WINDOW_DARK: Color = Color::Rgb(0x14, 0x1a, 0x22);

const COL_NEON_CYAN: Color = Color::Rgb(0x00, 0xe0, 0xe0);
const COL_NEON_PINK: Color = Color::Rgb(0xe0, 0x30, 0x80);
const COL_NEON_AMBER: Color = Color::Rgb(0xf0, 0xa0, 0x20);
const COL_NEON_DIM: Color = Color::Rgb(0x20, 0x30, 0x30);

const COL_RAIN: Color = Color::Rgb(0x40, 0x60, 0x80);
const COL_RAIN_BRIGHT: Color = Color::Rgb(0x60, 0x90, 0xc0);
const COL_LIGHTNING: Color = Color::Rgb(0xe0, 0xf0, 0xff);

const COL_STREET: Color = Color::Rgb(0x14, 0x18, 0x22);
const COL_PUDDLE: Color = Color::Rgb(0x18, 0x30, 0x48);
const COL_STEAM: Color = Color::Rgb(0x30, 0x40, 0x50);
const COL_ANTENNA_RED: Color = Color::Rgb(0xc0, 0x20, 0x20);

const COL_CAR_BODY: Color = Color::Rgb(0x20, 0x28, 0x38);
const COL_CAR_WINDOW: Color = Color::Rgb(0x30, 0x50, 0x70);
const COL_TAILLIGHT_RED: Color = Color::Rgb(0xc0, 0x20, 0x10);
const COL_HEADLIGHT: Color = Color::Rgb(0xe0, 0xd0, 0x80);
const COL_BIKE_BODY: Color = Color::Rgb(0x50, 0x30, 0x60);

const COL_SAUCER_BODY: Color = Color::Rgb(0x60, 0x80, 0xa0);
const COL_SAUCER_DOME: Color = Color::Rgb(0x90, 0xd0, 0xff);
const COL_SAUCER_BEAM: Color = Color::Rgb(0x60, 0xff, 0x80);
const COL_BILLBOARD_FRAME: Color = Color::Rgb(0x30, 0x38, 0x50);
const COL_BILLBOARD_TEXT: Color = Color::Rgb(0x50, 0x70, 0x90);

// ── cell types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellKind {
    Sky,
    Skyline,
    Street,
    Puddle,
}

#[derive(Debug, Clone)]
struct GridCell {
    ch: char,
    fg: Color,
    kind: CellKind,
}

// ── entities ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Star {
    col: u16,
    row: u16,
    brightness: f32,
    twinkle_rate: u8,
    counter: u8,
}

#[derive(Debug, Clone, Copy)]
enum WindowState {
    Lit,
    Dark,
    Flickering { phase: u8 },
}

#[derive(Debug, Clone)]
struct Building {
    col_start: u16,
    width: u16,
    height: u16,
    windows: Vec<WindowState>,
    window_cols: u8,
    window_rows: u8,
    has_antenna: bool,
    antenna_blink: bool,
}

#[derive(Debug, Clone)]
enum NeonState {
    On,
    Off,
    Flickering { phase: u8, total: u8 },
}

#[derive(Debug, Clone)]
struct NeonSign {
    col: u16,
    row: u16,
    text: &'static str,
    color: Color,
    state: NeonState,
}

#[derive(Debug, Clone)]
struct RainDrop {
    col: u16,
    row: u16,
    speed: u8,
    ch: char,
}

#[derive(Debug, Clone)]
struct Lightning {
    active: bool,
    flash_ticks: u8,
    cooldown: u16,
    bolt_segments: Vec<(u16, u16)>,
}

#[derive(Debug, Clone)]
struct SteamParticle {
    col: u16,
    row: u16,
    ttl: u8,
    drift: i8,
}

#[derive(Debug, Clone)]
struct SteamVent {
    col: u16,
    particles: Vec<SteamParticle>,
    emit_rate: u8,
}

#[derive(Debug, Clone)]
struct Puddle {
    col_start: u16,
    width: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VehicleKind {
    Car,
    Motorbike,
}

#[derive(Debug, Clone)]
struct Vehicle {
    col: i32,
    row: u16,
    kind: VehicleKind,
    speed: i8,
}

#[derive(Debug, Clone)]
struct FlyingSaucer {
    col: i32,
    row: u16,
    speed: i8,
    beam_on: bool,
    beam_timer: u8,
}

#[derive(Debug, Clone)]
struct Billboard {
    col: u16,
    row: u16,
    lines: &'static [&'static str],
    color: Color,
}

#[derive(Debug, Clone)]
struct GlitchChar {
    index: usize,
    ch: char,
    ttl: u8,
}

// neon sign extended with per-char glitch
#[derive(Debug, Clone)]
struct NeonGlitch {
    sign_idx: usize,
    glitches: Vec<GlitchChar>,
}

// ── main state ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SplashState {
    width: u16,
    height: u16,
    sky_rows: u16,
    street_start: u16,
    buildings: Vec<Building>,
    neon_signs: Vec<NeonSign>,
    neon_glitches: Vec<NeonGlitch>,
    stars: Vec<Star>,
    rain: Vec<RainDrop>,
    lightning: Lightning,
    steam_vents: Vec<SteamVent>,
    puddles: Vec<Puddle>,
    vehicles: Vec<Vehicle>,
    saucers: Vec<FlyingSaucer>,
    billboards: Vec<Billboard>,
    pub tick_count: u64,
}

const NEON_TEXTS: &[&str] = &["BAR", "HOTEL", "NET", "24h", "///", "SYS"];
const NEON_COLORS: &[Color] = &[COL_NEON_CYAN, COL_NEON_PINK, COL_NEON_AMBER];
const RAIN_CHARS: &[char] = &['│', '╎', '|', '·'];
const GLITCH_CHARS: &[char] = &['░', '▒', '▓', '?', '■', '%', '#', '@', '!', '~'];

const BILLBOARD_ADS: &[&[&str]] = &[
    &["┌──────┐", "│NEURO │", "│ LINK │", "└──────┘"],
    &["┌────────┐", "│AUGM3NT │", "│ CORP   │", "└────────┘"],
    &["╔══════╗", "║B1O-SYS║", "║ v2.4  ║", "╚══════╝"],
    &["┌──────┐", "│ OMNI │", "│ NET  │", "└──────┘"],
    &["╔════╗", "║XENO║", "║TECH║", "╚════╝"],
    &["┌───────┐", "│SYNTH-X│", "│ MK-IV │", "└───────┘"],
    &["╔═════╗", "║VAULT║", "║  9  ║", "╚═════╝"],
];

impl SplashState {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            sky_rows: 0,
            street_start: 0,
            buildings: Vec::new(),
            neon_signs: Vec::new(),
            neon_glitches: Vec::new(),
            stars: Vec::new(),
            rain: Vec::new(),
            lightning: Lightning {
                active: false,
                flash_ticks: 0,
                cooldown: 100,
                bolt_segments: Vec::new(),
            },
            steam_vents: Vec::new(),
            puddles: Vec::new(),
            vehicles: Vec::new(),
            saucers: Vec::new(),
            billboards: Vec::new(),
            tick_count: 0,
        }
    }

    fn build_world(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;

        self.sky_rows = ((height as f32 * 0.25) as u16).max(2);
        let street_rows = ((height as f32 * 0.20) as u16).max(2);
        self.street_start = height.saturating_sub(street_rows);

        self.stars.clear();
        self.buildings.clear();
        self.neon_signs.clear();
        self.neon_glitches.clear();
        self.rain.clear();
        self.steam_vents.clear();
        self.puddles.clear();
        self.saucers.clear();
        self.billboards.clear();
        self.lightning.active = false;
        self.lightning.cooldown = fastrand::u16(100..300);
        self.lightning.bolt_segments.clear();

        // ── stars ─────────────────────────────────────────────────────────────
        for r in 0..self.sky_rows {
            for c in 0..width {
                if fastrand::u16(0..40) == 0 {
                    self.stars.push(Star {
                        col: c,
                        row: r,
                        brightness: fastrand::f32(),
                        twinkle_rate: fastrand::u8(8..20),
                        counter: fastrand::u8(0..15),
                    });
                }
            }
        }

        // ── buildings ─────────────────────────────────────────────────────────
        let skyline_height = self.street_start.saturating_sub(self.sky_rows);
        if skyline_height > 2 && width > 6 {
            let mut x: u16 = 0;
            while x < width.saturating_sub(3) {
                let bw = fastrand::u16(4..=12).min(width - x);
                let min_h = (skyline_height as f32 * 0.3) as u16;
                let max_h = (skyline_height as f32 * 0.92) as u16;
                let bh = fastrand::u16(min_h.max(2)..=max_h.max(3));

                let wcols = ((bw.saturating_sub(2)) / 3).min(4) as u8;
                let wrows = ((bh.saturating_sub(2)) / 3).min(8) as u8;
                let n_windows = (wcols as usize) * (wrows as usize);
                let windows: Vec<WindowState> = (0..n_windows)
                    .map(|_| {
                        let r = fastrand::u8(0..10);
                        if r < 6 {
                            WindowState::Lit
                        } else if r < 9 {
                            WindowState::Dark
                        } else {
                            WindowState::Flickering {
                                phase: fastrand::u8(0..4),
                            }
                        }
                    })
                    .collect();

                let has_antenna = bh >= max_h.saturating_sub(2) && fastrand::u8(0..3) == 0;

                self.buildings.push(Building {
                    col_start: x,
                    width: bw,
                    height: bh,
                    windows,
                    window_cols: wcols,
                    window_rows: wrows,
                    has_antenna,
                    antenna_blink: false,
                });

                let gap = fastrand::u16(0..=1);
                x += bw + gap;
            }
        }

        // ── neon signs ────────────────────────────────────────────────────────
        let n_signs = fastrand::u8(2..=4) as usize;
        let mut sign_attempts = 0usize;
        while self.neon_signs.len() < n_signs && sign_attempts < 40 {
            sign_attempts += 1;
            if self.buildings.is_empty() {
                break;
            }
            let b = &self.buildings[fastrand::usize(0..self.buildings.len())];
            if b.height < 4 || b.width < 5 {
                continue;
            }
            let text = NEON_TEXTS[fastrand::usize(0..NEON_TEXTS.len())];
            let text_w = text.len() as u16;
            if text_w + 2 > b.width {
                continue;
            }
            let col = b.col_start + fastrand::u16(1..=(b.width - text_w - 1));
            let building_top = self.street_start.saturating_sub(b.height);
            let row = building_top + fastrand::u16(1..b.height.saturating_sub(2).max(1) + 1);
            let color = NEON_COLORS[fastrand::usize(0..NEON_COLORS.len())];
            let state = if fastrand::u8(0..10) < 7 {
                NeonState::On
            } else {
                NeonState::Flickering {
                    phase: 0,
                    total: fastrand::u8(6..12),
                }
            };
            self.neon_signs.push(NeonSign {
                col,
                row,
                text,
                color,
                state,
            });
        }

        // ── rain ──────────────────────────────────────────────────────────────
        let n_rain = ((width as f32 * 0.15) as u16).max(3);
        for _ in 0..n_rain {
            self.rain.push(RainDrop {
                col: fastrand::u16(0..width),
                row: fastrand::u16(0..height),
                speed: if fastrand::u8(0..5) == 0 { 2 } else { 1 },
                ch: RAIN_CHARS[fastrand::usize(0..RAIN_CHARS.len())],
            });
        }

        // ── steam vents ───────────────────────────────────────────────────────
        let n_vents = fastrand::u8(1..=3) as usize;
        for _ in 0..n_vents {
            self.steam_vents.push(SteamVent {
                col: fastrand::u16(0..width),
                particles: Vec::new(),
                emit_rate: fastrand::u8(3..=6),
            });
        }

        // ── puddles ───────────────────────────────────────────────────────────
        let n_puddles = fastrand::u8(2..=4) as usize;
        for _ in 0..n_puddles {
            let pw = fastrand::u16(5..=15).min(width / 2);
            let pc = fastrand::u16(0..width.saturating_sub(pw));
            self.puddles.push(Puddle {
                col_start: pc,
                width: pw,
            });
        }

        // ── vehicles ─────────────────────────────────────────────────────────
        self.vehicles.clear();
        let street_rows_count = height.saturating_sub(self.street_start);
        if street_rows_count >= 2 {
            let n_vehicles = fastrand::u8(2..=5) as usize;
            for _ in 0..n_vehicles {
                let kind = if fastrand::u8(0..3) == 0 {
                    VehicleKind::Motorbike
                } else {
                    VehicleKind::Car
                };
                let row = self.street_start + fastrand::u16(1..street_rows_count.max(2));
                let speed = if fastrand::bool() { 1 } else { -1 };
                let col = fastrand::i32(0..width as i32);
                self.vehicles.push(Vehicle {
                    col,
                    row: row.min(height - 1),
                    kind,
                    speed,
                });
            }
        }

        // ── flying saucers ────────────────────────────────────────────────────
        // Saucers start off-screen and fly across once, then disappear.
        // Stagger their entry with an off-screen head start so they don't all appear at once.
        if self.sky_rows >= 3 && fastrand::u8(0..3) != 0 {
            let n_saucers = fastrand::u8(1..=2) as usize;
            for i in 0..n_saucers {
                let row = fastrand::u16(0..self.sky_rows.saturating_sub(1));
                let going_right = fastrand::bool();
                let saucer_w: i32 = 7;
                // stagger: first saucer enters immediately, next one starts further back
                let stagger = (i as i32) * (width as i32 / 2);
                let col = if going_right {
                    -saucer_w - stagger
                } else {
                    width as i32 + stagger
                };
                self.saucers.push(FlyingSaucer {
                    col,
                    row,
                    speed: if going_right { 1 } else { -1 },
                    beam_on: false,
                    beam_timer: fastrand::u8(60..120),
                });
            }
        }

        // ── billboards ────────────────────────────────────────────────────────
        let n_boards = fastrand::u8(2..=4) as usize;
        let mut board_attempts = 0usize;
        while self.billboards.len() < n_boards && board_attempts < 60 {
            board_attempts += 1;
            if self.buildings.is_empty() {
                break;
            }
            let b = &self.buildings[fastrand::usize(0..self.buildings.len())];
            let ad = BILLBOARD_ADS[fastrand::usize(0..BILLBOARD_ADS.len())];
            let ad_w = ad.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
            let ad_h = ad.len() as u16;
            if b.width < ad_w + 2 || b.height < ad_h + 2 {
                continue;
            }
            let building_top = self.street_start.saturating_sub(b.height);
            let col = b.col_start + (b.width.saturating_sub(ad_w)) / 2;
            let row = building_top + fastrand::u16(1..b.height.saturating_sub(ad_h).max(1) + 1);
            if row + ad_h >= self.street_start {
                continue;
            }
            let color = if fastrand::u8(0..3) == 0 {
                COL_NEON_PINK
            } else if fastrand::u8(0..2) == 0 {
                COL_NEON_AMBER
            } else {
                COL_BILLBOARD_TEXT
            };
            self.billboards.push(Billboard {
                col,
                row,
                lines: ad,
                color,
            });
        }
    }

    pub fn tick(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        self.tick_count += 1;

        // ── stars ─────────────────────────────────────────────────────────────
        for star in &mut self.stars {
            star.counter += 1;
            if star.counter >= star.twinkle_rate {
                star.counter = 0;
                star.brightness += 0.2;
                if star.brightness > 1.0 {
                    star.brightness -= 1.0;
                }
            }
        }

        // ── windows ───────────────────────────────────────────────────────────
        for building in &mut self.buildings {
            for window in &mut building.windows {
                match window {
                    WindowState::Lit => {
                        if fastrand::u16(0..300) == 0 {
                            *window = WindowState::Dark;
                        }
                    }
                    WindowState::Dark => {
                        if fastrand::u16(0..400) == 0 {
                            *window = WindowState::Lit;
                        }
                    }
                    WindowState::Flickering { phase } => {
                        *phase += 1;
                        if *phase > 6 {
                            *window = if fastrand::bool() {
                                WindowState::Lit
                            } else {
                                WindowState::Dark
                            };
                        }
                    }
                }
            }
            if building.has_antenna && self.tick_count.is_multiple_of(20) {
                building.antenna_blink = !building.antenna_blink;
            }
        }

        // ── neon signs ────────────────────────────────────────────────────────
        for sign in &mut self.neon_signs {
            match &mut sign.state {
                NeonState::On => {
                    if fastrand::u16(0..200) == 0 {
                        sign.state = NeonState::Flickering {
                            phase: 0,
                            total: fastrand::u8(6..12),
                        };
                    }
                }
                NeonState::Off => {
                    if fastrand::u16(0..150) == 0 {
                        sign.state = NeonState::On;
                    }
                }
                NeonState::Flickering { phase, total } => {
                    *phase += 1;
                    if *phase >= *total {
                        sign.state = if fastrand::u8(0..4) == 0 {
                            NeonState::Off
                        } else {
                            NeonState::On
                        };
                    }
                }
            }
        }

        // ── rain ──────────────────────────────────────────────────────────────
        for drop in &mut self.rain {
            drop.row += drop.speed as u16;
            if drop.row >= self.height {
                drop.row = 0;
                drop.col = fastrand::u16(0..self.width);
                drop.ch = RAIN_CHARS[fastrand::usize(0..RAIN_CHARS.len())];
            }
        }

        // ── lightning ─────────────────────────────────────────────────────────
        if self.lightning.active {
            self.lightning.flash_ticks = self.lightning.flash_ticks.saturating_sub(1);
            if self.lightning.flash_ticks == 0 {
                self.lightning.active = false;
                self.lightning.cooldown = fastrand::u16(200..600);
                self.lightning.bolt_segments.clear();
            }
        } else {
            self.lightning.cooldown = self.lightning.cooldown.saturating_sub(1);
            if self.lightning.cooldown == 0 && fastrand::u16(0..30) == 0 {
                self.trigger_lightning();
            }
        }

        // ── steam vents ───────────────────────────────────────────────────────
        for vent in &mut self.steam_vents {
            for p in &mut vent.particles {
                p.ttl = p.ttl.saturating_sub(1);
                p.row = p.row.saturating_sub(1);
                if fastrand::u8(0..3) == 0 {
                    let next = p.col as i32 + p.drift as i32;
                    if next >= 0 && next < self.width as i32 {
                        p.col = next as u16;
                    }
                }
            }
            vent.particles.retain(|p| p.ttl > 0);

            if fastrand::u8(0..vent.emit_rate) == 0 {
                let row = self.street_start;
                if row > 0 {
                    vent.particles.push(SteamParticle {
                        col: vent.col,
                        row: row.saturating_sub(1),
                        ttl: fastrand::u8(4..=8),
                        drift: if fastrand::bool() { 1 } else { -1 },
                    });
                }
            }
        }

        // ── vehicles ─────────────────────────────────────────────────────────
        for vehicle in &mut self.vehicles {
            vehicle.col += vehicle.speed as i32;
            let vw = match vehicle.kind {
                VehicleKind::Car => 5,
                VehicleKind::Motorbike => 3,
            };
            if vehicle.speed > 0 && vehicle.col > self.width as i32 + vw {
                vehicle.col = -(vw);
            } else if vehicle.speed < 0 && vehicle.col < -(vw) {
                vehicle.col = self.width as i32;
            }
        }

        // ── flying saucers ────────────────────────────────────────────────────
        let saucer_w: i32 = 7;
        for saucer in &mut self.saucers {
            saucer.col += saucer.speed as i32;
            saucer.beam_timer = saucer.beam_timer.saturating_sub(1);
            if saucer.beam_timer == 0 {
                saucer.beam_on = !saucer.beam_on;
                saucer.beam_timer = if saucer.beam_on {
                    fastrand::u8(5..15)
                } else {
                    fastrand::u8(30..80)
                };
            }
        }
        self.saucers.retain(|s| {
            if s.speed > 0 {
                s.col <= self.width as i32 + saucer_w
            } else {
                s.col >= -saucer_w
            }
        });

        // ── neon glitches ─────────────────────────────────────────────────────
        // decay existing glitches
        for ng in &mut self.neon_glitches {
            for g in &mut ng.glitches {
                g.ttl = g.ttl.saturating_sub(1);
            }
            ng.glitches.retain(|g| g.ttl > 0);
        }
        // spawn new glitches on flickering/on signs
        for (i, sign) in self.neon_signs.iter().enumerate() {
            if matches!(sign.state, NeonState::Off) {
                continue;
            }
            if fastrand::u16(0..80) != 0 {
                continue;
            }
            let text_len = sign.text.chars().count();
            if text_len == 0 {
                continue;
            }
            let char_idx = fastrand::usize(0..text_len);
            let glitch_ch = GLITCH_CHARS[fastrand::usize(0..GLITCH_CHARS.len())];
            let ng = self.neon_glitches.iter_mut().find(|ng| ng.sign_idx == i);
            let glitch = GlitchChar {
                index: char_idx,
                ch: glitch_ch,
                ttl: fastrand::u8(2..6),
            };
            if let Some(ng) = ng {
                ng.glitches.push(glitch);
            } else {
                self.neon_glitches.push(NeonGlitch {
                    sign_idx: i,
                    glitches: vec![glitch],
                });
            }
        }
    }

    fn trigger_lightning(&mut self) {
        self.lightning.active = true;
        self.lightning.flash_ticks = fastrand::u8(3..=5);
        self.lightning.bolt_segments.clear();

        let col = fastrand::u16(0..self.width);
        let mut c = col;
        for r in 0..self.sky_rows.min(self.street_start) {
            self.lightning.bolt_segments.push((c, r));
            let drift = fastrand::i8(-1..=1);
            let next = c as i32 + drift as i32;
            if next >= 0 && next < self.width as i32 {
                c = next as u16;
            }
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, t: &Theme) {
        if area.width < 4 || area.height < 3 {
            return;
        }

        if area.width != self.width || area.height != self.height {
            self.build_world(area.width, area.height);
        }

        let w = area.width as usize;
        let h = area.height as usize;

        // ── base grid ─────────────────────────────────────────────────────────
        let mut grid: Vec<GridCell> = (0..w * h)
            .map(|i| {
                let col = (i % w) as u16;
                let row = (i / w) as u16;
                self.base_cell(col, row)
            })
            .collect();

        // ── stars ─────────────────────────────────────────────────────────────
        for star in &self.stars {
            if star.row >= area.height || star.col >= area.width {
                continue;
            }
            let idx = star.row as usize * w + star.col as usize;
            if grid[idx].kind == CellKind::Sky {
                if star.brightness > 0.6 {
                    grid[idx].ch = '*';
                    grid[idx].fg = COL_STAR_BRIGHT;
                } else if star.brightness > 0.3 {
                    grid[idx].ch = '·';
                    grid[idx].fg = COL_STAR;
                } else {
                    grid[idx].ch = ' ';
                }
            }
        }

        // ── flying saucers ────────────────────────────────────────────────────
        // Two rows per saucer:
        //   row+0 (dome):  " ·(·)· "   chars: space ·(·)· space
        //   row+1 (body):  "·═╪═╪═·"   chars: · ═ ╪ ═ ╪ ═ ·
        // beam: vertical line below body if beam_on
        for saucer in &self.saucers {
            let dome_row = saucer.row;
            let body_row = saucer.row + 1;

            let dome_chars: &[(i32, char, Color)] = &[
                (1, '·', COL_SAUCER_BODY),
                (2, '(', COL_SAUCER_BODY),
                (3, '·', COL_SAUCER_DOME),
                (4, ')', COL_SAUCER_BODY),
                (5, '·', COL_SAUCER_BODY),
            ];
            let body_chars: &[(i32, char, Color)] = &[
                (0, '·', COL_SAUCER_BODY),
                (1, '═', COL_SAUCER_BODY),
                (2, '╪', COL_SAUCER_BODY),
                (3, '═', COL_SAUCER_BODY),
                (4, '╪', COL_SAUCER_BODY),
                (5, '═', COL_SAUCER_BODY),
                (6, '·', COL_SAUCER_BODY),
            ];

            if dome_row < area.height {
                for &(off, ch, color) in dome_chars {
                    let c = saucer.col + off;
                    if c >= 0 && (c as u16) < area.width {
                        let idx = dome_row as usize * w + c as usize;
                        if grid[idx].kind == CellKind::Sky {
                            grid[idx].ch = ch;
                            grid[idx].fg = color;
                        }
                    }
                }
            }
            if body_row < area.height {
                for &(off, ch, color) in body_chars {
                    let c = saucer.col + off;
                    if c >= 0 && (c as u16) < area.width {
                        let idx = body_row as usize * w + c as usize;
                        if grid[idx].kind == CellKind::Sky {
                            grid[idx].ch = ch;
                            grid[idx].fg = color;
                        }
                    }
                }
            }

            // tractor beam: 3 cells below body, centered
            if saucer.beam_on {
                let beam_col = saucer.col + 3;
                for beam_r in (body_row + 1)..=(body_row + 3) {
                    if beam_r >= area.height {
                        break;
                    }
                    if beam_col >= 0 && (beam_col as u16) < area.width {
                        let idx = beam_r as usize * w + beam_col as usize;
                        if grid[idx].kind == CellKind::Sky {
                            grid[idx].ch = '╎';
                            grid[idx].fg = COL_SAUCER_BEAM;
                        }
                    }
                }
            }
        }

        // ── building windows ──────────────────────────────────────────────────
        for building in &self.buildings {
            let building_top = self.street_start.saturating_sub(building.height);
            for wr in 0..building.window_rows {
                for wc in 0..building.window_cols {
                    let win_idx = wr as usize * building.window_cols as usize + wc as usize;
                    if win_idx >= building.windows.len() {
                        continue;
                    }

                    let cell_col = building.col_start + 1 + (wc as u16) * 3;
                    let cell_row = building_top + 2 + (wr as u16) * 3;

                    if cell_col >= area.width || cell_row >= area.height {
                        continue;
                    }
                    if cell_row >= self.street_start {
                        continue;
                    }

                    let idx = cell_row as usize * w + cell_col as usize;
                    match building.windows[win_idx] {
                        WindowState::Lit => {
                            grid[idx].ch = '▪';
                            grid[idx].fg = COL_WINDOW_LIT;
                        }
                        WindowState::Dark => {
                            grid[idx].ch = '▫';
                            grid[idx].fg = COL_WINDOW_DARK;
                        }
                        WindowState::Flickering { phase } => {
                            grid[idx].ch = '▪';
                            grid[idx].fg = if phase % 2 == 0 {
                                COL_WINDOW_LIT
                            } else {
                                COL_WINDOW_DARK
                            };
                        }
                    }
                }
            }

            // antenna
            if building.has_antenna {
                let antenna_col = building.col_start + building.width / 2;
                let antenna_row = self.street_start.saturating_sub(building.height + 1);
                if antenna_col < area.width && antenna_row < area.height {
                    let idx = antenna_row as usize * w + antenna_col as usize;
                    grid[idx].ch = '╻';
                    grid[idx].fg = if building.antenna_blink {
                        COL_ANTENNA_RED
                    } else {
                        COL_BUILDING_EDGE
                    };
                }
            }
        }

        // ── billboards ────────────────────────────────────────────────────────
        for board in &self.billboards {
            for (li, line) in board.lines.iter().enumerate() {
                let row = board.row + li as u16;
                if row >= area.height || row >= self.street_start {
                    continue;
                }
                for (ci, ch) in line.chars().enumerate() {
                    let col = board.col + ci as u16;
                    if col >= area.width {
                        break;
                    }
                    let idx = row as usize * w + col as usize;
                    if grid[idx].kind == CellKind::Skyline {
                        let fg = if ch == '│'
                            || ch == '─'
                            || ch == '┌'
                            || ch == '┐'
                            || ch == '└'
                            || ch == '┘'
                            || ch == '╔'
                            || ch == '╗'
                            || ch == '╚'
                            || ch == '╝'
                            || ch == '║'
                            || ch == '═'
                        {
                            COL_BILLBOARD_FRAME
                        } else {
                            board.color
                        };
                        grid[idx].ch = ch;
                        grid[idx].fg = fg;
                    }
                }
            }
        }

        // ── neon signs ────────────────────────────────────────────────────────
        for (sign_idx, sign) in self.neon_signs.iter().enumerate() {
            if sign.row >= area.height {
                continue;
            }
            let visible = match &sign.state {
                NeonState::On => true,
                NeonState::Off => false,
                NeonState::Flickering { phase, .. } => phase % 3 != 0,
            };
            let fg = if visible { sign.color } else { COL_NEON_DIM };
            let glitches = self.neon_glitches.iter().find(|ng| ng.sign_idx == sign_idx);
            for (i, ch) in sign.text.chars().enumerate() {
                let c = sign.col + i as u16;
                if c >= area.width {
                    break;
                }
                let idx = sign.row as usize * w + c as usize;
                let render_ch = glitches
                    .and_then(|ng| ng.glitches.iter().find(|g| g.index == i))
                    .map(|g| g.ch)
                    .unwrap_or(ch);
                grid[idx].ch = render_ch;
                grid[idx].fg = fg;
            }
        }

        // ── rain ──────────────────────────────────────────────────────────────
        for drop in &self.rain {
            if drop.row >= area.height || drop.col >= area.width {
                continue;
            }
            let idx = drop.row as usize * w + drop.col as usize;
            grid[idx].ch = drop.ch;
            grid[idx].fg = if drop.speed > 1 {
                COL_RAIN_BRIGHT
            } else {
                COL_RAIN
            };
        }

        // ── lightning ─────────────────────────────────────────────────────────
        if self.lightning.active {
            for &(bc, br) in &self.lightning.bolt_segments {
                if br < area.height && bc < area.width {
                    let idx = br as usize * w + bc as usize;
                    grid[idx].ch = '╋';
                    grid[idx].fg = COL_LIGHTNING;
                }
            }
            // flash effect: brighten sky cells
            let flash_intensity = self.lightning.flash_ticks as f32 / 5.0;
            for row in 0..self.sky_rows.min(area.height) {
                for col in 0..area.width {
                    let idx = row as usize * w + col as usize;
                    if grid[idx].kind == CellKind::Sky {
                        grid[idx].fg =
                            Self::lerp_color(grid[idx].fg, COL_LIGHTNING, flash_intensity * 0.3);
                    }
                }
            }
        }

        // ── puddles ───────────────────────────────────────────────────────────
        let puddle_row = self.street_start;
        if puddle_row < area.height {
            for puddle in &self.puddles {
                for c in puddle.col_start..(puddle.col_start + puddle.width).min(area.width) {
                    let idx = puddle_row as usize * w + c as usize;
                    grid[idx].ch = '▁';
                    grid[idx].fg = COL_PUDDLE;
                    grid[idx].kind = CellKind::Puddle;
                }
            }
        }

        // ── vehicles ──────────────────────────────────────────────────────────
        for vehicle in &self.vehicles {
            if vehicle.row >= area.height {
                continue;
            }
            let going_right = vehicle.speed > 0;
            match vehicle.kind {
                VehicleKind::Car => {
                    // shape: ▖█▓█▗  (5 wide) going right
                    // shape: ▗█▓█▖  (5 wide) going left
                    let parts: &[(i32, char, Color)] = if going_right {
                        &[
                            (0, '▖', COL_TAILLIGHT_RED),
                            (1, '█', COL_CAR_BODY),
                            (2, '▓', COL_CAR_WINDOW),
                            (3, '█', COL_CAR_BODY),
                            (4, '▗', COL_HEADLIGHT),
                        ]
                    } else {
                        &[
                            (0, '▗', COL_HEADLIGHT),
                            (1, '█', COL_CAR_BODY),
                            (2, '▓', COL_CAR_WINDOW),
                            (3, '█', COL_CAR_BODY),
                            (4, '▖', COL_TAILLIGHT_RED),
                        ]
                    };
                    for &(offset, ch, color) in parts {
                        let c = vehicle.col + offset;
                        if c >= 0 && (c as u16) < area.width {
                            let idx = vehicle.row as usize * w + c as usize;
                            grid[idx].ch = ch;
                            grid[idx].fg = color;
                        }
                    }
                }
                VehicleKind::Motorbike => {
                    // shape: ○▪● (3 wide)
                    let parts: &[(i32, char, Color)] = if going_right {
                        &[
                            (0, '○', COL_TAILLIGHT_RED),
                            (1, '▪', COL_BIKE_BODY),
                            (2, '●', COL_HEADLIGHT),
                        ]
                    } else {
                        &[
                            (0, '●', COL_HEADLIGHT),
                            (1, '▪', COL_BIKE_BODY),
                            (2, '○', COL_TAILLIGHT_RED),
                        ]
                    };
                    for &(offset, ch, color) in parts {
                        let c = vehicle.col + offset;
                        if c >= 0 && (c as u16) < area.width {
                            let idx = vehicle.row as usize * w + c as usize;
                            grid[idx].ch = ch;
                            grid[idx].fg = color;
                        }
                    }
                }
            }
        }

        // ── steam ─────────────────────────────────────────────────────────────
        for vent in &self.steam_vents {
            for p in &vent.particles {
                if p.row >= area.height || p.col >= area.width {
                    continue;
                }
                let idx = p.row as usize * w + p.col as usize;
                let frac = p.ttl as f32 / 8.0;
                grid[idx].ch = if frac > 0.5 { '░' } else { '·' };
                grid[idx].fg = Self::lerp_color(t.bg, COL_STEAM, frac);
            }
        }

        // ── write to buffer ───────────────────────────────────────────────────
        let buf = frame.buffer_mut();
        for row in 0..h {
            for col in 0..w {
                let cell = &grid[row * w + col];
                let pos = ratatui::layout::Position {
                    x: area.x + col as u16,
                    y: area.y + row as u16,
                };
                if let Some(bc) = buf.cell_mut(pos) {
                    bc.set_char(cell.ch);
                    bc.set_fg(cell.fg);
                    bc.set_bg(t.bg);
                }
            }
        }

        // title overlay on top
        self.render_title_overlay(frame, area, t);
    }

    fn base_cell(&self, col: u16, row: u16) -> GridCell {
        // sky zone
        if row < self.sky_rows {
            let t = row as f32 / self.sky_rows.max(1) as f32;
            let fg = Self::lerp_color(COL_SKY_DEEP, COL_SKY_MID, t);
            return GridCell {
                ch: ' ',
                fg,
                kind: CellKind::Sky,
            };
        }

        // street zone
        if row >= self.street_start {
            let street_chars = ['▁', '▂', '─', '▁'];
            return GridCell {
                ch: street_chars[col as usize % street_chars.len()],
                fg: COL_STREET,
                kind: CellKind::Street,
            };
        }

        // skyline zone: check if inside a building
        for building in &self.buildings {
            if col >= building.col_start
                && col < building.col_start + building.width
                && row >= self.street_start.saturating_sub(building.height)
            {
                let is_edge = col == building.col_start
                    || col == building.col_start + building.width - 1
                    || row == self.street_start.saturating_sub(building.height);
                let ch = if is_edge { '▓' } else { '░' };
                let fg = if is_edge {
                    COL_BUILDING_EDGE
                } else {
                    COL_BUILDING_DARK
                };
                return GridCell {
                    ch,
                    fg,
                    kind: CellKind::Skyline,
                };
            }
        }

        // open sky behind buildings (lower sky area)
        GridCell {
            ch: ' ',
            fg: COL_SKY_MID,
            kind: CellKind::Sky,
        }
    }

    fn render_title_overlay(&self, frame: &mut Frame, area: Rect, t: &Theme) {
        const VAN_ART: &[&str] = &[
            " ██╗   ██╗ █████╗ ███╗   ██╗",
            " ██║   ██║██╔══██╗████╗  ██║",
            " ██║   ██║███████║██╔██╗ ██║",
            " ╚██╗ ██╔╝██╔══██║██║╚██╗██║",
            "  ╚████╔╝ ██║  ██║██║ ╚████║",
            "   ╚═══╝  ╚═╝  ╚═╝╚═╝  ╚═══╝",
        ];
        const DAMME_ART: &[&str] = &[
            " ██████╗  █████╗ ███╗   ███╗███╗   ███╗███████╗",
            " ██╔══██╗██╔══██╗████╗ ████║████╗ ████║██╔════╝",
            " ██║  ██║███████║██╔████╔██║██╔████╔██║█████╗  ",
            " ██║  ██║██╔══██║██║╚██╔╝██║██║╚██╔╝██║██╔══╝  ",
            " ██████╔╝██║  ██║██║ ╚═╝ ██║██║ ╚═╝ ██║███████╗",
            " ╚═════╝ ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝     ╚═╝╚══════╝",
        ];
        const TAGLINE: &str = "tmux × claude session manager";

        let total_h = (VAN_ART.len() + 1 + DAMME_ART.len() + 2) as u16;
        let start_y = area.y + area.height.saturating_sub(total_h) / 2;
        let mut y = start_y;

        for line in VAN_ART {
            if y >= area.y + area.height {
                break;
            }
            frame.render_widget(
                Paragraph::new(Span::styled(
                    *line,
                    Style::default().fg(t.accent_bright),
                ))
                .alignment(Alignment::Center),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }

        y += 1;

        for line in DAMME_ART {
            if y >= area.y + area.height {
                break;
            }
            frame.render_widget(
                Paragraph::new(Span::styled(*line, Style::default().fg(t.accent)))
                    .alignment(Alignment::Center),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }

        y += 2;

        if y < area.y + area.height {
            frame.render_widget(
                Paragraph::new(Span::styled(TAGLINE, Style::default().fg(t.gray_dim)))
                    .alignment(Alignment::Center),
                Rect::new(area.x, y, area.width, 1),
            );
        }
    }

    fn lerp_color(a: Color, b: Color, t: f32) -> Color {
        let (ar, ag, ab) = rgb(a);
        let (br, bg, bb) = rgb(b);
        let r = (ar as f32 + (br as f32 - ar as f32) * t) as u8;
        let g = (ag as f32 + (bg as f32 - ag as f32) * t) as u8;
        let b_val = (ab as f32 + (bb as f32 - ab as f32) * t) as u8;
        Color::Rgb(r, g, b_val)
    }
}

fn rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn built(w: u16, h: u16) -> SplashState {
        let mut s = SplashState::new();
        s.build_world(w, h);
        s
    }

    #[test]
    fn test_zone_thresholds() {
        let s = built(120, 40);
        assert_eq!(s.sky_rows, 10);
        assert_eq!(s.street_start, 32);
    }

    #[test]
    fn test_buildings_within_bounds() {
        let s = built(120, 40);
        for b in &s.buildings {
            assert!(b.col_start + b.width <= s.width);
            let top = s.street_start.saturating_sub(b.height);
            assert!(top >= s.sky_rows || top < s.street_start);
        }
    }

    #[test]
    fn test_buildings_no_overlap() {
        let s = built(120, 40);
        for i in 0..s.buildings.len() {
            for j in (i + 1)..s.buildings.len() {
                let a = &s.buildings[i];
                let b = &s.buildings[j];
                let a_end = a.col_start + a.width;
                let b_end = b.col_start + b.width;
                assert!(a_end <= b.col_start || b_end <= a.col_start);
            }
        }
    }

    #[test]
    fn test_tick_increments() {
        let mut s = built(80, 24);
        s.tick();
        s.tick();
        s.tick();
        assert_eq!(s.tick_count, 3);
    }

    #[test]
    fn test_rain_wraps_at_bottom() {
        let mut s = built(80, 24);
        s.rain.clear();
        s.rain.push(RainDrop {
            col: 10,
            row: 23,
            speed: 1,
            ch: '│',
        });
        s.tick();
        assert_eq!(s.rain[0].row, 0);
    }

    #[test]
    fn test_lightning_cooldown() {
        let mut s = built(80, 24);
        s.lightning.cooldown = 100;
        s.lightning.active = false;
        s.tick();
        assert!(!s.lightning.active);
        assert_eq!(s.lightning.cooldown, 99);
    }

    #[test]
    fn test_neon_state_transitions() {
        let mut sign = NeonSign {
            col: 0,
            row: 0,
            text: "BAR",
            color: COL_NEON_CYAN,
            state: NeonState::Flickering {
                phase: 11,
                total: 12,
            },
        };
        // Simulate one tick of neon logic
        if let NeonState::Flickering { phase, total } = &mut sign.state {
            *phase += 1;
            if *phase >= *total {
                sign.state = NeonState::On;
            }
        }
        assert!(matches!(sign.state, NeonState::On));
    }

    #[test]
    fn test_window_flickering_resolves() {
        let mut w = WindowState::Flickering { phase: 6 };
        // Simulate tick
        if let WindowState::Flickering { phase } = &mut w {
            *phase += 1;
            if *phase > 6 {
                w = WindowState::Lit;
            }
        }
        assert!(matches!(w, WindowState::Lit));
    }

    #[test]
    fn test_star_brightness_wraps() {
        let mut star = Star {
            col: 0,
            row: 0,
            brightness: 0.9,
            twinkle_rate: 1,
            counter: 0,
        };
        star.counter += 1;
        if star.counter >= star.twinkle_rate {
            star.counter = 0;
            star.brightness += 0.2;
            if star.brightness > 1.0 {
                star.brightness -= 1.0;
            }
        }
        assert!(star.brightness < 0.2);
    }

    #[test]
    fn test_steam_particle_ttl_prunes() {
        let mut s = built(80, 24);
        s.steam_vents.clear();
        s.steam_vents.push(SteamVent {
            col: 10,
            particles: vec![SteamParticle {
                col: 10,
                row: 20,
                ttl: 1,
                drift: 1,
            }],
            emit_rate: 255, // won't emit new ones
        });
        s.tick();
        assert!(s.steam_vents[0].particles.is_empty());
    }

    #[test]
    fn test_draw_no_panic_small_area() {
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut s = SplashState::new();
        terminal
            .draw(|frame| {
                let area = frame.area();
                s.draw(frame, area, &crate::theme::SYNDICATE);
            })
            .unwrap();
    }

    #[test]
    fn test_draw_no_panic_tiny_area() {
        let backend = TestBackend::new(4, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut s = SplashState::new();
        terminal
            .draw(|frame| {
                let area = frame.area();
                s.draw(frame, area, &crate::theme::SYNDICATE);
            })
            .unwrap();
    }

    #[test]
    fn test_lerp_color_endpoints() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(255, 128, 64);
        assert_eq!(SplashState::lerp_color(a, b, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(SplashState::lerp_color(a, b, 1.0), Color::Rgb(255, 128, 64));
    }

    #[test]
    fn test_resize_rebuilds_world() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut s = SplashState::new();
        terminal
            .draw(|frame| {
                s.draw(frame, frame.area(), &crate::theme::SYNDICATE);
            })
            .unwrap();
        assert_eq!(s.width, 40);
        assert_eq!(s.height, 20);
        // "resize"
        let backend2 = TestBackend::new(60, 30);
        let mut terminal2 = Terminal::new(backend2).unwrap();
        terminal2
            .draw(|frame| {
                s.draw(frame, frame.area(), &crate::theme::SYNDICATE);
            })
            .unwrap();
        assert_eq!(s.width, 60);
        assert_eq!(s.height, 30);
    }

    #[test]
    fn test_vehicles_spawned_on_street() {
        let s = built(80, 24);
        assert!(!s.vehicles.is_empty());
        for v in &s.vehicles {
            assert!(v.row >= s.street_start);
            assert!(v.row < s.height);
        }
    }

    #[test]
    fn test_vehicle_wraps_right() {
        let mut s = built(80, 24);
        s.vehicles.clear();
        s.vehicles.push(Vehicle {
            col: 86,
            row: s.street_start + 1,
            kind: VehicleKind::Car,
            speed: 1,
        });
        s.tick();
        assert!(s.vehicles[0].col < 0);
    }

    #[test]
    fn test_vehicle_wraps_left() {
        let mut s = built(80, 24);
        s.vehicles.clear();
        s.vehicles.push(Vehicle {
            col: -6,
            row: s.street_start + 1,
            kind: VehicleKind::Motorbike,
            speed: -1,
        });
        s.tick();
        assert_eq!(s.vehicles[0].col, 80);
    }
}
