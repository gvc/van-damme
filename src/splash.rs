use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme;

// ── palette ──────────────────────────────────────────────────────────────────

const COL_ROCK_A: Color = Color::Rgb(0x2a, 0x35, 0x45);
const COL_ROCK_B: Color = Color::Rgb(0x2e, 0x3b, 0x4e);
const COL_DIRT_A: Color = Color::Rgb(0x3a, 0x30, 0x20);
const COL_DIRT_B: Color = Color::Rgb(0x4a, 0x3d, 0x26);
const COL_TUNNEL: Color = Color::Rgb(0x1a, 0x22, 0x30);
const COL_WATER: Color = Color::Rgb(0x1a, 0x4a, 0x6e);
const COL_DWARF: Color = Color::Rgb(0xd4, 0xa8, 0x43);
const COL_PICK: Color = Color::Rgb(0xa0, 0x78, 0x38);
const COL_MUSHROOM: Color = Color::Rgb(0x6a, 0x3a, 0x8a);
const COL_DUST: Color = Color::Rgb(0xb4, 0x8c, 0x3c);
const COL_SKY: Color = Color::Rgb(0x14, 0x1c, 0x26);
const COL_STAR: Color = Color::Rgb(0x50, 0x60, 0x70);
const COL_GRASS: Color = Color::Rgb(0x2a, 0x50, 0x28);
const COL_TREE_GREEN: Color = Color::Rgb(0x28, 0x6a, 0x24);
const COL_TREE_DYING: Color = Color::Rgb(0x5a, 0x40, 0x18);
const COL_TRUNK: Color = Color::Rgb(0x4a, 0x30, 0x14);
const COL_ANIMAL: Color = Color::Rgb(0x8a, 0x70, 0x44);
const COL_BIRD: Color = Color::Rgb(0x70, 0x80, 0x90);
const COL_CLOUD: Color = Color::Rgb(0x38, 0x44, 0x54);

// ── terrain types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellKind {
    Sky,
    River,
    Surface,
    Rock,
    Dirt,
    Tunnel,
}

#[derive(Debug, Clone)]
struct GridCell {
    ch: char,
    fg: Color,
    kind: CellKind,
}

// ── world entities ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Star {
    col: u16,
    row: u16,
    visible: bool,
    counter: u8,
}

#[derive(Debug, Clone)]
struct RiverRow {
    row: u16,
    chars: Vec<char>,
    offset: usize,
}

#[derive(Debug, Clone)]
struct WaterDrip {
    col: u16,
    row: u16,
    ch: char,
    alive: bool,
}

#[derive(Debug, Clone)]
enum DwarfState {
    Walking,
    Digging {
        phase: u8,
        target_col: u16,
        target_row: u16,
    },
    Pausing {
        remaining: u8,
    },
}

#[derive(Debug, Clone)]
struct Dwarf {
    col: u16,
    row: u16,
    dir: i8,
    state: DwarfState,
    idle_ticks: u8,
    tunnel_idx: usize,
}

#[derive(Debug, Clone)]
struct DustParticle {
    col: u16,
    row: u16,
    ch: char,
    ttl: u8,
    ttl_max: u8,
    dy_counter: u8,
}

#[derive(Debug, Clone)]
struct Mushroom {
    col: u16,
    row: u16,
    pulse_phase: f32,
}

#[derive(Debug, Clone)]
struct Tunnel {
    row: u16,
    start_col: u16,
    end_col: u16,
}

// Animals roam the surface above-ground
#[derive(Debug, Clone)]
struct Animal {
    col: u16,
    dir: i8,
    ch: char,
    move_counter: u8,
    move_rate: u8, // ticks per step (higher = slower)
}

// Birds drift through the sky
#[derive(Debug, Clone)]
struct Bird {
    col: u16,
    row: u16,
    dir: i8,
    move_counter: u8,
}

// Clouds drift slowly in upper sky
#[derive(Debug, Clone)]
struct Cloud {
    col: i32, // signed so it can drift off-screen and wrap
    row: u16,
    width: u16,
    move_counter: u8,
    move_rate: u8,
}

// 5-minute loop at 80ms = 3750 ticks total per tree
// Growing: 3 stages × 333 ticks = ~1000; Mature: 1500; Dying: 750; Dead: 500
#[derive(Debug, Clone)]
enum TreePhase {
    Growing { stage: u8, ticks: u16 }, // stage 0/1/2 = sapling/young/full
    Mature { ticks: u16 },
    Dying { ticks: u16 },
    Dead { ttl: u16 },
}

#[derive(Debug, Clone)]
struct Tree {
    col: u16,       // centre column on surface_row
    phase: TreePhase,
}

// ── main state ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SplashState {
    width: u16,
    height: u16,
    sky_rows: u16,
    surface_row: u16,
    underground_start: u16,
    tunnels: Vec<Tunnel>,
    converted_cells: Vec<(u16, u16)>,
    rock_chars: Vec<char>,
    rock_is_dirt: Vec<bool>,
    stars: Vec<Star>,
    river_rows: Vec<RiverRow>,
    drips: Vec<WaterDrip>,
    dwarves: Vec<Dwarf>,
    mushrooms: Vec<Mushroom>,
    dust: Vec<DustParticle>,
    trees: Vec<Tree>,
    animals: Vec<Animal>,
    birds: Vec<Bird>,
    clouds: Vec<Cloud>,
    pub tick_count: u64,
}

impl SplashState {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            sky_rows: 0,
            surface_row: 0,
            underground_start: 0,
            tunnels: Vec::new(),
            converted_cells: Vec::new(),
            rock_chars: Vec::new(),
            rock_is_dirt: Vec::new(),
            stars: Vec::new(),
            river_rows: Vec::new(),
            drips: Vec::new(),
            dwarves: Vec::new(),
            mushrooms: Vec::new(),
            dust: Vec::new(),
            trees: Vec::new(),
            animals: Vec::new(),
            birds: Vec::new(),
            clouds: Vec::new(),
            tick_count: 0,
        }
    }

    fn build_world(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.converted_cells.clear();
        self.drips.clear();
        self.dust.clear();

        self.sky_rows = ((height as f32 * 0.30) as u16).max(2);
        self.surface_row = self.sky_rows;
        self.underground_start = self.sky_rows + 1;

        let ug_start = self.underground_start;
        let ug_end = height.saturating_sub(1);

        // ── rock/dirt per-cell chars ──────────────────────────────────────────
        let total = (width as usize) * (height as usize);
        let rock_set = ['#', '▓', '▒', '░', '╬', '·'];
        let dirt_set = ['▓', '▒', '#', '·', '·', '·'];
        self.rock_chars = (0..total)
            .map(|_| {
                let is_dirt = fastrand::u8(0..10) < 3;
                let set = if is_dirt { &dirt_set } else { &rock_set };
                set[fastrand::usize(0..set.len())]
            })
            .collect();
        self.rock_is_dirt = (0..total).map(|_| fastrand::u8(0..10) < 3).collect();

        // ── tunnels ───────────────────────────────────────────────────────────
        self.tunnels.clear();
        let tunnel_count = fastrand::u8(2..=3) as usize;
        if ug_start < ug_end {
            let available: Vec<u16> = (ug_start..ug_end).collect();
            let mut used_rows: Vec<u16> = Vec::new();
            let mut attempts = 0usize;
            while self.tunnels.len() < tunnel_count && attempts < 30 {
                attempts += 1;
                let row = available[fastrand::usize(0..available.len())];
                if used_rows.iter().any(|&r| r.abs_diff(row) < 2) {
                    continue;
                }
                let min_len = (width as f32 * 0.3) as u16;
                let max_len = (width as f32 * 0.7) as u16;
                let len = min_len + fastrand::u16(0..=(max_len - min_len).max(1));
                let max_start = width.saturating_sub(len);
                let start_col = if max_start > 0 { fastrand::u16(0..max_start) } else { 0 };
                let end_col = (start_col + len).min(width.saturating_sub(1));
                used_rows.push(row);
                self.tunnels.push(Tunnel { row, start_col, end_col });
            }
        }
        // sort by row for connector logic
        self.tunnels.sort_by_key(|t| t.row);

        // clear surface entities rebuilt below
        self.animals.clear();
        self.birds.clear();
        self.clouds.clear();

        // ── stars ─────────────────────────────────────────────────────────────
        self.stars.clear();
        for r in 0..self.sky_rows {
            for c in 0..width {
                if fastrand::u16(0..50) == 0 {
                    self.stars.push(Star {
                        col: c,
                        row: r,
                        visible: true,
                        counter: fastrand::u8(10..25),
                    });
                }
            }
        }

        // ── river rows ────────────────────────────────────────────────────────
        self.river_rows.clear();
        let river_chars = ['≈', '~', '≋'];
        let n_rivers = if self.sky_rows > 3 { fastrand::u8(1..=2) } else { 1 };
        let mut used_river_rows: Vec<u16> = Vec::new();
        for _ in 0..n_rivers {
            if self.sky_rows < 2 {
                break;
            }
            let mut attempts = 0;
            loop {
                attempts += 1;
                if attempts > 20 {
                    break;
                }
                let r = fastrand::u16(1..self.sky_rows.saturating_sub(1).max(1) + 1);
                if used_river_rows.contains(&r) {
                    continue;
                }
                used_river_rows.push(r);
                let chars: Vec<char> = (0..width)
                    .map(|_| river_chars[fastrand::usize(0..river_chars.len())])
                    .collect();
                self.river_rows.push(RiverRow { row: r, chars, offset: 0 });
                break;
            }
        }

        // ── dwarves ───────────────────────────────────────────────────────────
        self.dwarves.clear();
        for (idx, tunnel) in self.tunnels.iter().enumerate() {
            if tunnel.end_col <= tunnel.start_col {
                continue;
            }
            let len = tunnel.end_col - tunnel.start_col;
            let offsets = [len / 3, (len * 2) / 3];
            for offset in offsets {
                let col = tunnel.start_col + offset;
                let dir: i8 = if fastrand::bool() { 1 } else { -1 };
                self.dwarves.push(Dwarf {
                    col,
                    row: tunnel.row,
                    dir,
                    state: DwarfState::Walking,
                    idle_ticks: fastrand::u8(0..15),
                    tunnel_idx: idx,
                });
            }
        }

        // ── mushrooms ─────────────────────────────────────────────────────────
        self.mushrooms.clear();
        let n_mushrooms = 3.min(self.tunnels.len() * 2);
        let mut attempts = 0;
        while self.mushrooms.len() < n_mushrooms && attempts < 50 {
            attempts += 1;
            if self.tunnels.is_empty() {
                break;
            }
            let t = &self.tunnels[fastrand::usize(0..self.tunnels.len())];
            if t.end_col <= t.start_col {
                continue;
            }
            let col = t.start_col + fastrand::u16(0..=(t.end_col - t.start_col));
            let row = t.row;
            let occupied = self.mushrooms.iter().any(|m| m.col == col && m.row == row)
                || self.dwarves.iter().any(|d| d.col == col && d.row == row);
            if !occupied {
                self.mushrooms.push(Mushroom {
                    col,
                    row,
                    pulse_phase: fastrand::f32() * std::f32::consts::TAU,
                });
            }
        }

        // ── trees ─────────────────────────────────────────────────────────────
        self.trees.clear();
        if self.sky_rows >= 2 && width >= 10 {
            let n_trees = fastrand::u16(3..=6) as usize;
            let min_spacing: u16 = (width / (n_trees as u16 + 1)).max(4);
            let mut attempts = 0usize;
            while self.trees.len() < n_trees && attempts < 60 {
                attempts += 1;
                let col = fastrand::u16(1..width.saturating_sub(1));
                let too_close = self.trees.iter().any(|t| t.col.abs_diff(col) < min_spacing);
                if too_close {
                    continue;
                }
                // stagger initial phase so trees don't all cycle together
                let phase_offset = fastrand::u16(0..3750);
                let phase = Self::tree_phase_from_offset(phase_offset);
                self.trees.push(Tree { col, phase });
            }
        }

        // ── animals ───────────────────────────────────────────────────────────
        // DF fauna chars: d=dog/deer, c=cat, b=boar, h=horse, r=rabbit
        let animal_chars = ['d', 'c', 'b', 'h', 'r', 'f'];
        let n_animals = fastrand::u16(2..=4) as usize;
        for _ in 0..n_animals {
            let col = fastrand::u16(0..width);
            let dir: i8 = if fastrand::bool() { 1 } else { -1 };
            let ch = animal_chars[fastrand::usize(0..animal_chars.len())];
            let move_rate = fastrand::u8(2..=5);
            self.animals.push(Animal { col, dir, ch, move_counter: 0, move_rate });
        }

        // ── birds ─────────────────────────────────────────────────────────────
        if self.sky_rows >= 3 {
            let n_birds = fastrand::u16(2..=5) as usize;
            for _ in 0..n_birds {
                let col = fastrand::u16(0..width);
                let row = fastrand::u16(0..self.sky_rows.saturating_sub(1).max(1));
                let dir: i8 = if fastrand::bool() { 1 } else { -1 };
                self.birds.push(Bird { col, row, dir, move_counter: fastrand::u8(0..4) });
            }
        }

        // ── clouds ────────────────────────────────────────────────────────────
        if self.sky_rows >= 2 {
            let n_clouds = fastrand::u16(2..=4) as usize;
            for _ in 0..n_clouds {
                let cloud_width = fastrand::u16(4..=10);
                let col = fastrand::i32(0..width as i32);
                let row = fastrand::u16(0..self.sky_rows.saturating_sub(1).max(1));
                let move_rate = fastrand::u8(6..=15);
                self.clouds.push(Cloud {
                    col,
                    row,
                    width: cloud_width,
                    move_counter: 0,
                    move_rate,
                });
            }
        }
    }

    fn tree_phase_from_offset(offset: u16) -> TreePhase {
        match offset {
            0..=999 => {
                let stage = (offset / 333).min(2) as u8;
                let ticks = offset % 333;
                TreePhase::Growing { stage, ticks }
            }
            1000..=2499 => TreePhase::Mature { ticks: offset - 1000 },
            2500..=3249 => TreePhase::Dying { ticks: offset - 2500 },
            _ => TreePhase::Dead { ttl: offset - 3250 },
        }
    }

    pub fn tick(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        self.tick_count += 1;

        // ── river scroll ──────────────────────────────────────────────────────
        let river_chars = ['≈', '~', '≋'];
        for rr in &mut self.river_rows {
            rr.offset = (rr.offset + 1) % rr.chars.len().max(1);
            // occasionally mutate a char
            if fastrand::u8(0..8) == 0 {
                let idx = fastrand::usize(0..rr.chars.len().max(1));
                rr.chars[idx] = river_chars[fastrand::usize(0..river_chars.len())];
            }
        }

        // ── stars ─────────────────────────────────────────────────────────────
        for star in &mut self.stars {
            if star.counter == 0 {
                if fastrand::u8(0..10) == 0 {
                    star.visible = !star.visible;
                }
                star.counter = fastrand::u8(12..28);
            } else {
                star.counter -= 1;
            }
        }

        // ── dust ──────────────────────────────────────────────────────────────
        for p in &mut self.dust {
            if p.ttl == 0 {
                continue;
            }
            p.ttl -= 1;
            p.dy_counter += 1;
            if p.dy_counter >= 2 {
                p.dy_counter = 0;
                p.row = p.row.saturating_sub(1);
            }
        }
        self.dust.retain(|p| p.ttl > 0);

        // ── drips ─────────────────────────────────────────────────────────────
        // Collect solid positions separately to avoid borrow conflict.
        let solid_check: Vec<(u16, u16, bool)> = self
            .drips
            .iter()
            .map(|d| {
                if !d.alive {
                    return (d.col, d.row, false);
                }
                let next_row = d.row + 1;
                if next_row >= self.height {
                    return (d.col, next_row, false);
                }
                let solid = self.is_solid(d.col, next_row);
                (d.col, next_row, !solid)
            })
            .collect();
        for (drip, (_, next_row, keep)) in self.drips.iter_mut().zip(solid_check.iter()) {
            if !drip.alive {
                continue;
            }
            if !keep {
                drip.alive = false;
            } else {
                drip.row = *next_row;
            }
        }
        self.drips.retain(|d| d.alive);

        // spawn new drips from river rows
        let drip_chars = ['│', '╎', '·'];
        let river_rows_snapshot: Vec<(u16, usize)> =
            self.river_rows.iter().map(|rr| (rr.row, rr.chars.len())).collect();
        for (row, len) in river_rows_snapshot {
            if row + 1 >= self.height {
                continue;
            }
            for c in 0..self.width {
                if fastrand::u16(0..120) == 0 && (c as usize) < len {
                    self.drips.push(WaterDrip {
                        col: c,
                        row: row + 1,
                        ch: drip_chars[fastrand::usize(0..drip_chars.len())],
                        alive: true,
                    });
                }
            }
        }

        // ── dwarves ───────────────────────────────────────────────────────────
        for idx in 0..self.dwarves.len() {
            self.step_dwarf(idx);
        }

        // ── mushroom pulse ────────────────────────────────────────────────────
        for m in &mut self.mushrooms {
            m.pulse_phase += 0.15;
            if m.pulse_phase > std::f32::consts::TAU {
                m.pulse_phase -= std::f32::consts::TAU;
            }
        }

        // ── animals ───────────────────────────────────────────────────────────
        for animal in &mut self.animals {
            animal.move_counter += 1;
            if animal.move_counter >= animal.move_rate {
                animal.move_counter = 0;
                // occasionally change direction
                if fastrand::u8(0..20) == 0 {
                    animal.dir = -animal.dir;
                }
                let next = animal.col as i32 + animal.dir as i32;
                if next < 0 || next >= self.width as i32 {
                    animal.dir = -animal.dir;
                } else {
                    animal.col = next as u16;
                }
            }
        }

        // ── birds ─────────────────────────────────────────────────────────────
        for bird in &mut self.birds {
            bird.move_counter += 1;
            if bird.move_counter >= 3 {
                bird.move_counter = 0;
                // birds wrap around screen edges
                let next = bird.col as i32 + bird.dir as i32;
                if next < 0 {
                    bird.col = self.width.saturating_sub(1);
                } else if next >= self.width as i32 {
                    bird.col = 0;
                } else {
                    bird.col = next as u16;
                }
                // small vertical drift
                if fastrand::u8(0..30) == 0 && self.sky_rows > 1 {
                    let new_row = bird.row as i32 + if fastrand::bool() { 1 } else { -1 };
                    if new_row >= 0 && new_row < self.sky_rows as i32 - 1 {
                        bird.row = new_row as u16;
                    }
                }
            }
        }

        // ── clouds ────────────────────────────────────────────────────────────
        for cloud in &mut self.clouds {
            cloud.move_counter += 1;
            if cloud.move_counter >= cloud.move_rate {
                cloud.move_counter = 0;
                cloud.col -= 1; // clouds always drift left (DF wind)
                // wrap when fully off left edge
                if cloud.col + (cloud.width as i32) < 0 {
                    cloud.col = self.width as i32;
                }
            }
        }

        // ── trees ─────────────────────────────────────────────────────────────
        for tree in &mut self.trees {
            tree.phase = match tree.phase {
                TreePhase::Growing { stage, ticks } => {
                    if ticks + 1 >= 333 {
                        if stage < 2 {
                            TreePhase::Growing { stage: stage + 1, ticks: 0 }
                        } else {
                            TreePhase::Mature { ticks: 0 }
                        }
                    } else {
                        TreePhase::Growing { stage, ticks: ticks + 1 }
                    }
                }
                TreePhase::Mature { ticks } => {
                    if ticks + 1 >= 1500 {
                        TreePhase::Dying { ticks: 0 }
                    } else {
                        TreePhase::Mature { ticks: ticks + 1 }
                    }
                }
                TreePhase::Dying { ticks } => {
                    if ticks + 1 >= 750 {
                        TreePhase::Dead { ttl: 0 }
                    } else {
                        TreePhase::Dying { ticks: ticks + 1 }
                    }
                }
                TreePhase::Dead { ttl } => {
                    if ttl + 1 >= 500 {
                        TreePhase::Growing { stage: 0, ticks: 0 }
                    } else {
                        TreePhase::Dead { ttl: ttl + 1 }
                    }
                }
            };
        }
    }

    fn step_dwarf(&mut self, idx: usize) {
        let tunnel_idx = self.dwarves[idx].tunnel_idx;
        let (t_start, t_end, t_row) = if tunnel_idx < self.tunnels.len() {
            let t = &self.tunnels[tunnel_idx];
            (t.start_col, t.end_col, t.row)
        } else {
            return;
        };

        let state = self.dwarves[idx].state.clone();
        match state {
            DwarfState::Walking => {
                let d = &mut self.dwarves[idx];
                d.idle_ticks += 1;

                // decide to dig (~every 60–80 ticks at 80ms ≈ 5–6s)
                if d.idle_ticks >= 60 && fastrand::u8(0..5) == 0 {
                    d.idle_ticks = 0;
                    // try to find an adjacent rock cell
                    let candidates: Vec<(u16, u16)> = [
                        (d.col.wrapping_add_signed(d.dir as i16), d.row),
                        (d.col, d.row.saturating_sub(1)),
                        (d.col, d.row + 1),
                    ]
                    .iter()
                    .filter(|&&(c, r)| {
                        c < self.width
                            && r < self.height
                            && r >= self.underground_start
                            && self.is_solid(c, r)
                    })
                    .copied()
                    .collect();

                    if let Some(&(tc, tr)) = candidates.first() {
                        self.dwarves[idx].state = DwarfState::Digging {
                            phase: 0,
                            target_col: tc,
                            target_row: tr,
                        };
                        return;
                    }
                }

                // walk
                let d = &mut self.dwarves[idx];
                let next_col = d.col as i32 + d.dir as i32;
                if next_col < t_start as i32 || next_col > t_end as i32 {
                    d.dir = -d.dir;
                } else {
                    d.col = next_col as u16;
                }
            }

            DwarfState::Digging { phase, target_col, target_row } => {
                if phase < 3 {
                    self.dwarves[idx].state = DwarfState::Digging {
                        phase: phase + 1,
                        target_col,
                        target_row,
                    };
                } else {
                    // convert target to tunnel
                    self.converted_cells.push((target_col, target_row));
                    // also extend the tunnel record if adjacent
                    if target_row == t_row
                        && let Some(t) = self.tunnels.get_mut(tunnel_idx)
                    {
                        if target_col == t.start_col.saturating_sub(1) {
                            t.start_col = t.start_col.saturating_sub(1);
                        } else if target_col == t.end_col + 1 && t.end_col + 1 < self.width {
                            t.end_col += 1;
                        }
                    }
                    self.spawn_dust(target_col, target_row);
                    let pause = fastrand::u8(3..=8);
                    self.dwarves[idx].state = DwarfState::Pausing { remaining: pause };
                }
            }

            DwarfState::Pausing { remaining } => {
                if remaining == 0 {
                    self.dwarves[idx].state = DwarfState::Walking;
                } else {
                    self.dwarves[idx].state = DwarfState::Pausing {
                        remaining: remaining - 1,
                    };
                }
            }
        }
    }

    fn spawn_dust(&mut self, col: u16, row: u16) {
        let dust_chars = ['*', '+', '·'];
        let count = fastrand::u8(3..=5);
        for _ in 0..count {
            let dc = col.saturating_add_signed(fastrand::i16(-1..=1) as i16);
            let dr = row.saturating_add_signed(fastrand::i16(-1..=0) as i16);
            let ttl = fastrand::u8(3..=6);
            self.dust.push(DustParticle {
                col: dc.min(self.width.saturating_sub(1)),
                row: dr,
                ch: dust_chars[fastrand::usize(0..dust_chars.len())],
                ttl,
                ttl_max: ttl,
                dy_counter: 0,
            });
        }
    }

    /// Returns true if the cell is solid rock/dirt (not sky, not tunnel, not converted).
    fn is_solid(&self, col: u16, row: u16) -> bool {
        if row < self.underground_start || col >= self.width || row >= self.height {
            return false;
        }
        if self.converted_cells.contains(&(col, row)) {
            return false;
        }
        for t in &self.tunnels {
            if row == t.row && col >= t.start_col && col <= t.end_col {
                return false;
            }
        }
        true
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 4 || area.height < 3 {
            return;
        }

        if area.width != self.width || area.height != self.height {
            self.build_world(area.width, area.height);
        }

        // build grid
        let w = area.width as usize;
        let h = area.height as usize;
        let mut grid: Vec<GridCell> = (0..w * h)
            .map(|i| {
                let col = (i % w) as u16;
                let row = (i / w) as u16;
                self.base_cell(col, row)
            })
            .collect();

        // overlay river
        for rr in &self.river_rows {
            if rr.row >= area.height {
                continue;
            }
            for c in 0..area.width as usize {
                let char_idx = (c + rr.offset) % rr.chars.len().max(1);
                grid[rr.row as usize * w + c] = GridCell {
                    ch: rr.chars[char_idx],
                    fg: COL_WATER,
                    kind: CellKind::River,
                };
            }
        }

        // overlay stars
        for star in &self.stars {
            if star.row >= area.height || star.col >= area.width {
                continue;
            }
            let idx = star.row as usize * w + star.col as usize;
            if grid[idx].kind == CellKind::Sky {
                grid[idx].ch = if star.visible { '·' } else { ' ' };
                grid[idx].fg = COL_STAR;
            }
        }

        // overlay tunnels
        for t in &self.tunnels {
            if t.row >= area.height {
                continue;
            }
            for c in t.start_col..=t.end_col.min(area.width.saturating_sub(1)) {
                grid[t.row as usize * w + c as usize] = GridCell {
                    ch: ' ',
                    fg: COL_TUNNEL,
                    kind: CellKind::Tunnel,
                };
            }
        }

        // overlay converted cells
        for &(col, row) in &self.converted_cells {
            if row < area.height && col < area.width {
                grid[row as usize * w + col as usize] = GridCell {
                    ch: ' ',
                    fg: COL_TUNNEL,
                    kind: CellKind::Tunnel,
                };
            }
        }

        // mushrooms
        for m in &self.mushrooms {
            if m.row >= area.height || m.col >= area.width {
                continue;
            }
            let t = (m.pulse_phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let fg = Self::lerp_color(COL_TUNNEL, COL_MUSHROOM, t);
            grid[m.row as usize * w + m.col as usize] = GridCell {
                ch: '♠',
                fg,
                kind: CellKind::Tunnel,
            };
        }

        // dust
        for p in &self.dust {
            if p.row >= area.height || p.col >= area.width {
                continue;
            }
            let t = p.ttl as f32 / p.ttl_max as f32;
            let fg = Self::lerp_color(COL_TUNNEL, COL_DUST, t);
            let idx = p.row as usize * w + p.col as usize;
            grid[idx].ch = p.ch;
            grid[idx].fg = fg;
        }

        // drips
        for drip in &self.drips {
            if !drip.alive || drip.row >= area.height || drip.col >= area.width {
                continue;
            }
            let idx = drip.row as usize * w + drip.col as usize;
            grid[idx].ch = drip.ch;
            grid[idx].fg = COL_WATER;
        }

        // dwarves
        for d in &self.dwarves {
            if d.row >= area.height || d.col >= area.width {
                continue;
            }
            let dig_phase_chars = ['/', '|', '\\', '-'];
            match d.state {
                DwarfState::Digging { phase, target_col, target_row } => {
                    // dwarf char
                    let idx = d.row as usize * w + d.col as usize;
                    grid[idx].ch = '@';
                    grid[idx].fg = COL_DWARF;
                    // pick at target
                    if target_row < area.height && target_col < area.width {
                        let tidx = target_row as usize * w + target_col as usize;
                        grid[tidx].ch = dig_phase_chars[phase as usize % 4];
                        grid[tidx].fg = COL_PICK;
                    }
                }
                _ => {
                    let idx = d.row as usize * w + d.col as usize;
                    grid[idx].ch = '@';
                    grid[idx].fg = COL_DWARF;
                }
            }
        }

        // clouds (behind everything else in sky)
        for cloud in &self.clouds {
            if cloud.row >= area.height {
                continue;
            }
            // two-row cloud: ░▒░░▒ on top, ▒▓▒▓▒ on bottom
            let cloud_top = ['░', '▒', '░', '▒', '░'];
            let cloud_bot = ['▒', '░', '▒', '░', '▒'];
            for i in 0..cloud.width {
                let cx = cloud.col + i as i32;
                if cx < 0 || cx >= area.width as i32 {
                    continue;
                }
                let cx = cx as u16;
                // top row of cloud
                if cloud.row < area.height {
                    let idx = cloud.row as usize * w + cx as usize;
                    if grid[idx].kind == CellKind::Sky {
                        grid[idx].ch = cloud_top[i as usize % cloud_top.len()];
                        grid[idx].fg = COL_CLOUD;
                    }
                }
                // bottom row of cloud (one below)
                let bot_row = cloud.row + 1;
                if bot_row < area.height && bot_row < self.sky_rows {
                    let idx = bot_row as usize * w + cx as usize;
                    if grid[idx].kind == CellKind::Sky {
                        grid[idx].ch = cloud_bot[i as usize % cloud_bot.len()];
                        grid[idx].fg = COL_CLOUD;
                    }
                }
            }
        }

        // trees
        for tree in &self.trees {
            self.draw_tree(&mut grid, w, area.height, tree);
        }

        // animals on surface
        let surface = self.surface_row;
        for animal in &self.animals {
            if surface == 0 || animal.col >= area.width {
                continue;
            }
            let row = surface.saturating_sub(1);
            if row >= area.height {
                continue;
            }
            let idx = row as usize * w + animal.col as usize;
            // only place on sky/surface-adjacent cells (don't overwrite tree trunks)
            if grid[idx].kind == CellKind::Sky || grid[idx].kind == CellKind::Surface {
                grid[idx].ch = animal.ch;
                grid[idx].fg = COL_ANIMAL;
            }
        }

        // birds in sky
        for bird in &self.birds {
            if bird.row >= area.height || bird.col >= area.width {
                continue;
            }
            let idx = bird.row as usize * w + bird.col as usize;
            if grid[idx].kind == CellKind::Sky {
                // direction: left='\' right='/'  DF uses v for perched, ' for flying
                grid[idx].ch = if bird.dir > 0 { '\'' } else { '`' };
                grid[idx].fg = COL_BIRD;
            }
        }

        // write to terminal buffer
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
                    bc.set_bg(theme::BG);
                }
            }
        }

        // title overlay on top
        self.render_title_overlay(frame, area);
    }

    fn base_cell(&self, col: u16, row: u16) -> GridCell {
        if row < self.sky_rows {
            return GridCell { ch: ' ', fg: COL_SKY, kind: CellKind::Sky };
        }
        if row == self.surface_row {
            let grass_chars = [':', ';', ','];
            return GridCell {
                ch: grass_chars[col as usize % grass_chars.len()],
                fg: COL_GRASS,
                kind: CellKind::Surface,
            };
        }
        let idx = row as usize * self.width as usize + col as usize;
        let ch = self.rock_chars.get(idx).copied().unwrap_or('#');
        let is_dirt = self.rock_is_dirt.get(idx).copied().unwrap_or(false);
        let fg = if is_dirt {
            if col.is_multiple_of(2) { COL_DIRT_A } else { COL_DIRT_B }
        } else if (row + col).is_multiple_of(2) {
            COL_ROCK_A
        } else {
            COL_ROCK_B
        };
        GridCell {
            ch,
            fg,
            kind: if is_dirt { CellKind::Dirt } else { CellKind::Rock },
        }
    }

    fn render_title_overlay(&self, frame: &mut Frame, area: Rect) {
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
                Paragraph::new(Span::styled(*line, Style::default().fg(theme::ORANGE_BRIGHT)))
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
                Paragraph::new(Span::styled(*line, Style::default().fg(theme::ORANGE)))
                    .alignment(Alignment::Center),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }

        y += 2;

        if y < area.y + area.height {
            frame.render_widget(
                Paragraph::new(Span::styled(TAGLINE, Style::default().fg(theme::GRAY_DIM)))
                    .alignment(Alignment::Center),
                Rect::new(area.x, y, area.width, 1),
            );
        }
    }

    fn draw_tree(&self, grid: &mut Vec<GridCell>, w: usize, height: u16, tree: &Tree) {
        let surface = self.surface_row;
        if surface == 0 || tree.col >= self.width {
            return;
        }

        let (stage, canopy_color, show_trunk) = match tree.phase {
            TreePhase::Dead { .. } => return,
            TreePhase::Growing { stage, .. } => (stage, COL_TREE_GREEN, stage >= 1),
            TreePhase::Mature { .. } => (2u8, COL_TREE_GREEN, true),
            TreePhase::Dying { ticks } => {
                let t = ticks as f32 / 750.0;
                let col = Self::lerp_color(COL_TREE_GREEN, COL_TREE_DYING, t);
                (2u8, col, true)
            }
        };

        // stage 0: 1-cell canopy, no trunk
        // stage 1: 1-cell trunk + 1-cell canopy
        // stage 2: 2-cell trunk + 3-wide canopy top
        let col = tree.col;

        let set = |grid: &mut Vec<GridCell>, c: u16, r: u16, ch: char, fg: Color| {
            if c < self.width && r < height {
                let idx = r as usize * w + c as usize;
                grid[idx].ch = ch;
                grid[idx].fg = fg;
            }
        };

        match stage {
            0 => {
                // sapling: single ♣ just above surface
                if surface >= 1 {
                    set(grid, col, surface - 1, '♣', canopy_color);
                }
            }
            1 => {
                // young: trunk + canopy
                if surface >= 2 {
                    set(grid, col, surface - 1, '│', COL_TRUNK);
                    set(grid, col, surface - 2, '♣', canopy_color);
                } else if surface >= 1 {
                    set(grid, col, surface - 1, '♣', canopy_color);
                }
            }
            _ => {
                // full: 2-cell trunk, 5-wide bottom canopy, 3-wide top canopy
                if surface >= 1 && show_trunk {
                    set(grid, col, surface - 1, '│', COL_TRUNK);
                }
                if surface >= 2 && show_trunk {
                    set(grid, col, surface - 2, '│', COL_TRUNK);
                }
                // wide bottom canopy row
                if surface >= 3 {
                    for dc in -2i16..=2 {
                        let c = col.wrapping_add_signed(dc);
                        if c < self.width {
                            set(grid, c, surface - 3, '♣', canopy_color);
                        }
                    }
                }
                // narrow top canopy row
                if surface >= 4 {
                    for dc in -1i16..=1 {
                        let c = col.wrapping_add_signed(dc);
                        if c < self.width {
                            set(grid, c, surface - 4, '♣', canopy_color);
                        }
                    }
                }
                // peak
                if surface >= 5 {
                    set(grid, col, surface - 5, '▲', canopy_color);
                }
                if surface < 3 {
                    set(grid, col, surface.saturating_sub(1), '♣', canopy_color);
                }
            }
        }
    }

    fn lerp_color(a: Color, b: Color, t: f32) -> Color {
        let (ar, ag, ab) = rgb(a);
        let (br, bg, bb) = rgb(b);
        let r = (ar as f32 + (br as f32 - ar as f32) * t) as u8;
        let g = (ag as f32 + (bg as f32 - ag as f32) * t) as u8;
        let b = (ab as f32 + (bb as f32 - ab as f32) * t) as u8;
        Color::Rgb(r, g, b)
    }
}

fn rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn built(w: u16, h: u16) -> SplashState {
        let mut s = SplashState::new();
        s.build_world(w, h);
        s
    }

    #[test]
    fn test_zone_thresholds() {
        let s = built(120, 40);
        assert_eq!(s.sky_rows, 12);
        assert_eq!(s.surface_row, 12);
        assert_eq!(s.underground_start, 13);
    }

    #[test]
    fn test_tunnel_count() {
        let s = built(120, 40);
        assert!(s.tunnels.len() >= 2 && s.tunnels.len() <= 3);
    }

    #[test]
    fn test_tunnel_rows_in_underground_zone() {
        let s = built(120, 40);
        for t in &s.tunnels {
            assert!(t.row >= s.underground_start && t.row < s.height);
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
    fn test_river_scroll_advances_offset() {
        let mut s = built(80, 24);
        let initial_offsets: Vec<usize> = s.river_rows.iter().map(|r| r.offset).collect();
        s.tick();
        for (i, rr) in s.river_rows.iter().enumerate() {
            let expected = (initial_offsets[i] + 1) % rr.chars.len().max(1);
            assert_eq!(rr.offset, expected);
        }
    }

    #[test]
    fn test_drip_despawns_on_rock() {
        let mut s = built(80, 30);
        // find a solid cell
        let solid_row = s.underground_start + 2;
        let solid_col = {
            let mut col = None;
            for c in 0..s.width {
                if s.is_solid(c, solid_row) {
                    col = Some(c);
                    break;
                }
            }
            col
        };
        if let Some(col) = solid_col {
            s.drips.push(WaterDrip {
                col,
                row: solid_row - 1,
                ch: '│',
                alive: true,
            });
            s.tick(); // drip moves to solid_row → despawns
            assert!(s.drips.is_empty() || s.drips.iter().all(|d| !d.alive || d.col != col));
        }
    }

    #[test]
    fn test_dust_ttl_decrements_and_prunes() {
        let mut s = built(80, 24);
        s.dust.push(DustParticle {
            col: 10,
            row: 15,
            ch: '*',
            ttl: 2,
            ttl_max: 2,
            dy_counter: 0,
        });
        s.tick();
        assert_eq!(s.dust.len(), 1);
        assert_eq!(s.dust[0].ttl, 1);
        s.tick();
        assert!(s.dust.is_empty());
    }

    #[test]
    fn test_dwarf_pausing_transitions_to_walking() {
        let mut s = built(80, 24);
        if !s.dwarves.is_empty() {
            s.dwarves[0].state = DwarfState::Pausing { remaining: 1 };
            s.step_dwarf(0);
            assert!(matches!(s.dwarves[0].state, DwarfState::Pausing { remaining: 0 }));
            s.step_dwarf(0);
            assert!(matches!(s.dwarves[0].state, DwarfState::Walking));
        }
    }

    #[test]
    fn test_dwarf_bounces_at_tunnel_end() {
        let mut s = built(80, 24);
        if !s.dwarves.is_empty() && !s.tunnels.is_empty() {
            let tunnel_end = s.tunnels[s.dwarves[0].tunnel_idx].end_col;
            s.dwarves[0].col = tunnel_end;
            s.dwarves[0].dir = 1;
            s.dwarves[0].state = DwarfState::Walking;
            s.dwarves[0].idle_ticks = 0;
            s.step_dwarf(0);
            assert_eq!(s.dwarves[0].dir, -1);
        }
    }

    #[test]
    fn test_lerp_color_endpoints() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(255, 128, 64);
        assert_eq!(SplashState::lerp_color(a, b, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(SplashState::lerp_color(a, b, 1.0), Color::Rgb(255, 128, 64));
    }

    #[test]
    fn test_mushroom_pulse_advances() {
        let mut s = built(80, 24);
        if !s.mushrooms.is_empty() {
            let before = s.mushrooms[0].pulse_phase;
            s.tick();
            let after = s.mushrooms[0].pulse_phase;
            let delta = (after - before).abs();
            assert!((delta - 0.15).abs() < 0.01 || delta < 0.01); // allow wrap
        }
    }

    #[test]
    fn test_draw_no_panic_small_area() {
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut s = SplashState::new();
        terminal
            .draw(|frame| {
                let area = frame.area();
                s.draw(frame, area);
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
                s.draw(frame, area);
            })
            .unwrap();
    }
}
