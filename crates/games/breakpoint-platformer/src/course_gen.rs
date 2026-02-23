use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

use crate::enemies::EnemySpawn;
use crate::enemies::EnemyType;
use crate::physics::TILE_SIZE;

/// Water movement multiplier (0.5x speed in water).
pub const WATER_SPEED_FACTOR: f32 = 0.5;
/// Water jump velocity multiplier (0.7x jump in water).
pub const WATER_JUMP_FACTOR: f32 = 0.7;
/// Water buoyancy force (counters ~30% of gravity).
pub const WATER_BUOYANCY: f32 = 9.0;

/// Tile types for the Castlevania-style platformer course grid.
///
/// Serialized as a `u8` integer for compact wire representation
/// (9000-tile course would be ~63 KB as string enum names but only ~9 KB as u8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tile {
    Empty = 0,
    /// Solid stone brick wall/floor/ceiling.
    StoneBrick = 1,
    /// One-way platform (passable from below).
    Platform = 2,
    /// Spike hazard (deals 1 HP damage on contact).
    Spikes = 3,
    /// Race checkpoint.
    Checkpoint = 4,
    /// Race finish line.
    Finish = 5,
    /// Power-up spawn location.
    PowerUpSpawn = 6,
    /// Climbable ladder.
    Ladder = 7,
    /// Destructible wall (broken by whip attack).
    BreakableWall = 8,
    /// Decorative wall torch (no gameplay effect).
    DecoTorch = 9,
    /// Decorative stained glass (no gameplay effect).
    DecoStainedGlass = 10,
    /// Water tile (slows movement, adds buoyancy).
    Water = 11,
    /// Decorative cobweb (no gameplay effect).
    DecoCobweb = 12,
    /// Decorative hanging chain (no gameplay effect).
    DecoChain = 13,
}

impl From<Tile> for u8 {
    fn from(t: Tile) -> u8 {
        t as u8
    }
}

impl TryFrom<u8> for Tile {
    type Error = String;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Tile::Empty),
            1 => Ok(Tile::StoneBrick),
            2 => Ok(Tile::Platform),
            3 => Ok(Tile::Spikes),
            4 => Ok(Tile::Checkpoint),
            5 => Ok(Tile::Finish),
            6 => Ok(Tile::PowerUpSpawn),
            7 => Ok(Tile::Ladder),
            8 => Ok(Tile::BreakableWall),
            9 => Ok(Tile::DecoTorch),
            10 => Ok(Tile::DecoStainedGlass),
            11 => Ok(Tile::Water),
            12 => Ok(Tile::DecoCobweb),
            13 => Ok(Tile::DecoChain),
            _ => Err(format!("invalid tile value: {v}")),
        }
    }
}

impl Serialize for Tile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for Tile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(deserializer)?;
        Tile::try_from(v).map_err(serde::de::Error::custom)
    }
}

/// Room width in tiles.
pub const ROOM_WIDTH: u32 = 20;
/// Course height in tiles.
pub const COURSE_HEIGHT: u32 = 30;
/// Number of rooms in a generated course.
pub const NUM_ROOMS: u32 = 15;
/// Total course width in tiles.
pub const COURSE_WIDTH: u32 = ROOM_WIDTH * NUM_ROOMS; // 300

/// A platformer course built from a tile grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    /// Width in tiles.
    pub width: u32,
    /// Height in tiles.
    pub height: u32,
    /// Tile data stored row-major (y * width + x).
    pub tiles: Vec<Tile>,
    /// Spawn X position in world units.
    pub spawn_x: f32,
    /// Spawn Y position in world units.
    pub spawn_y: f32,
    /// Enemy spawn definitions for this course.
    pub enemy_spawns: Vec<EnemySpawn>,
    /// Checkpoint world positions (x, y).
    pub checkpoint_positions: Vec<(f32, f32)>,
}

impl Course {
    pub fn get_tile(&self, x: i32, y: i32) -> Tile {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return Tile::Empty;
        }
        self.tiles[y as usize * self.width as usize + x as usize]
    }

    pub fn set_tile(&mut self, x: u32, y: u32, tile: Tile) {
        if x < self.width && y < self.height {
            self.tiles[y as usize * self.width as usize + x as usize] = tile;
        }
    }
}

/// Room template types for procedural generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoomTemplate {
    Corridor,
    Staircase,
    TowerClimb,
    CathedralHall,
    CryptDepths,
    BridgeRun,
    BossArena,
    BranchSplit,
    BranchMerge,
}

/// Generate a deterministic course from a seed.
pub fn generate_course(seed: u64) -> Course {
    let width = COURSE_WIDTH;
    let height = COURSE_HEIGHT;
    let mut course = Course {
        width,
        height,
        tiles: vec![Tile::Empty; (width * height) as usize],
        spawn_x: 3.0 * TILE_SIZE,
        spawn_y: 4.0 * TILE_SIZE,
        enemy_spawns: Vec::new(),
        checkpoint_positions: Vec::new(),
    };

    let mut rng = StdRng::seed_from_u64(seed);

    // Build outer walls: floor (rows 0-1) and ceiling (rows 28-29)
    for x in 0..width {
        course.set_tile(x, 0, Tile::StoneBrick);
        course.set_tile(x, 1, Tile::StoneBrick);
        course.set_tile(x, height - 1, Tile::StoneBrick);
        course.set_tile(x, height - 2, Tile::StoneBrick);
    }

    // Build left wall for spawn room
    for y in 0..height {
        course.set_tile(0, y, Tile::StoneBrick);
    }

    // Generate spawn room (room 0): flat safe area
    generate_spawn_room(&mut course);

    // Assign templates to rooms 1-14
    let templates = assign_room_templates(&mut rng);

    // Generate each room
    for (room_idx, &template) in templates.iter().enumerate() {
        let actual_room = room_idx + 1; // templates[0] = room 1
        let base_x = actual_room as u32 * ROOM_WIDTH;
        generate_room(&mut course, &mut rng, base_x, actual_room as u32, template);
    }

    // Place checkpoints every 3 rooms (rooms 3, 6, 9, 12)
    for room_idx in (3..NUM_ROOMS).step_by(3) {
        let cx = room_idx * ROOM_WIDTH + ROOM_WIDTH / 2;
        // Find a good y for the checkpoint: first empty tile above the floor
        let cy = find_open_y(&course, cx, 2, 10);
        course.set_tile(cx, cy, Tile::Checkpoint);
        let world_x = cx as f32 * TILE_SIZE + TILE_SIZE / 2.0;
        let world_y = cy as f32 * TILE_SIZE + TILE_SIZE / 2.0;
        course.checkpoint_positions.push((world_x, world_y));
    }

    // Place finish line in last room
    let finish_base_x = (NUM_ROOMS - 1) * ROOM_WIDTH + ROOM_WIDTH - 4;
    let finish_y = find_open_y(&course, finish_base_x, 2, 10);
    course.set_tile(finish_base_x, finish_y, Tile::Finish);
    course.set_tile(finish_base_x + 1, finish_y, Tile::Finish);
    course.set_tile(finish_base_x + 2, finish_y, Tile::Finish);

    course
}

/// Find the lowest empty tile in a column between min_y and max_y (inclusive).
fn find_open_y(course: &Course, x: u32, min_y: u32, max_y: u32) -> u32 {
    for y in min_y..=max_y {
        if course.get_tile(x as i32, y as i32) == Tile::Empty {
            return y;
        }
    }
    min_y
}

/// Generate the spawn room (room 0): flat floor with some platforms.
fn generate_spawn_room(course: &mut Course) {
    // Floor is already placed (rows 0-1). Add some platforms for variety.
    for x in 3..8 {
        course.set_tile(x, 5, Tile::Platform);
    }
    // Decorative torch
    course.set_tile(2, 3, Tile::DecoTorch);
    course.set_tile(10, 3, Tile::DecoTorch);
}

/// Assign room templates for rooms 1-14, with branching at specific points.
fn assign_room_templates(rng: &mut StdRng) -> Vec<RoomTemplate> {
    let mut templates = Vec::with_capacity(14);

    // Room 1-3: intro rooms
    let intro_choices = [
        RoomTemplate::Corridor,
        RoomTemplate::Staircase,
        RoomTemplate::CathedralHall,
    ];
    for _ in 0..3 {
        templates.push(intro_choices[rng.random_range(0..intro_choices.len())]);
    }

    // Room 4: first branch split
    templates.push(RoomTemplate::BranchSplit);

    // Room 5-7: branch segment (varied rooms)
    let mid_choices = [
        RoomTemplate::TowerClimb,
        RoomTemplate::CryptDepths,
        RoomTemplate::BridgeRun,
        RoomTemplate::Corridor,
        RoomTemplate::CathedralHall,
    ];
    for _ in 0..3 {
        templates.push(mid_choices[rng.random_range(0..mid_choices.len())]);
    }

    // Room 8: first branch merge
    templates.push(RoomTemplate::BranchMerge);

    // Room 9-11: late segment
    let late_choices = [
        RoomTemplate::BossArena,
        RoomTemplate::TowerClimb,
        RoomTemplate::CryptDepths,
        RoomTemplate::Staircase,
    ];
    for _ in 0..3 {
        templates.push(late_choices[rng.random_range(0..late_choices.len())]);
    }

    // Room 12: optional second branch split
    templates.push(RoomTemplate::BranchSplit);

    // Room 13: final gauntlet
    templates.push(RoomTemplate::BossArena);

    // Room 14 (index 13): finish room
    templates.push(RoomTemplate::Corridor);

    templates
}

/// Generate a single room given its template.
fn generate_room(
    course: &mut Course,
    rng: &mut StdRng,
    base_x: u32,
    room_idx: u32,
    template: RoomTemplate,
) {
    match template {
        RoomTemplate::Corridor => gen_corridor(course, rng, base_x, room_idx),
        RoomTemplate::Staircase => gen_staircase(course, rng, base_x, room_idx),
        RoomTemplate::TowerClimb => gen_tower_climb(course, rng, base_x, room_idx),
        RoomTemplate::CathedralHall => gen_cathedral_hall(course, rng, base_x, room_idx),
        RoomTemplate::CryptDepths => gen_crypt_depths(course, rng, base_x, room_idx),
        RoomTemplate::BridgeRun => gen_bridge_run(course, rng, base_x, room_idx),
        RoomTemplate::BossArena => gen_boss_arena(course, rng, base_x, room_idx),
        RoomTemplate::BranchSplit => gen_branch_split(course, rng, base_x, room_idx),
        RoomTemplate::BranchMerge => gen_branch_merge(course, rng, base_x, room_idx),
    }
}

/// Corridor: flat floor with scattered platforms, enemies, and power-ups.
fn gen_corridor(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    // Floor is rows 0-1 (already placed). Add some raised sections.
    let plat_count = rng.random_range(2u32..5);
    for _ in 0..plat_count {
        let px = base_x + rng.random_range(2..ROOM_WIDTH - 2);
        let py = rng.random_range(4u32..8);
        let len = rng.random_range(2u32..5).min(ROOM_WIDTH - (px - base_x));
        for dx in 0..len {
            if px + dx < course.width {
                course.set_tile(px + dx, py, Tile::Platform);
            }
        }
    }

    // Spikes on floor in a few places
    let spike_x = base_x + rng.random_range(5..ROOM_WIDTH - 3);
    let spike_len = rng.random_range(2u32..4);
    for dx in 0..spike_len {
        if spike_x + dx < course.width {
            course.set_tile(spike_x + dx, 2, Tile::Spikes);
        }
    }

    // Enemy: skeleton or bat
    add_corridor_enemies(course, rng, base_x);

    // Power-up spawn
    let pu_x = base_x + rng.random_range(3..ROOM_WIDTH - 3);
    let pu_y = rng.random_range(3u32..7);
    course.set_tile(pu_x, pu_y, Tile::PowerUpSpawn);

    // Decorative torches
    course.set_tile(base_x + 1, 3, Tile::DecoTorch);
    course.set_tile(base_x + ROOM_WIDTH - 2, 3, Tile::DecoTorch);
}

/// Staircase: ascending platforms from left to right.
fn gen_staircase(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    let step_count = rng.random_range(4u32..7);
    let step_width = 3u32;

    for i in 0..step_count {
        let sx = base_x + i * step_width;
        let sy = 2 + i;
        for dx in 0..step_width {
            if sx + dx < course.width && sy < course.height {
                course.set_tile(sx + dx, sy, Tile::StoneBrick);
            }
        }
    }

    // Add spikes between some steps
    if step_count > 2 {
        let spike_step = rng.random_range(1..step_count - 1);
        let sx = base_x + spike_step * step_width;
        course.set_tile(sx, 2, Tile::Spikes);
    }

    // Knight enemy patrolling the staircase
    let enemy_x = (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE;
    let enemy_y = 3.0 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: enemy_x,
        y: enemy_y,
        enemy_type: EnemyType::Knight,
        patrol_min_x: base_x as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH) as f32 * TILE_SIZE,
    });

    // Torch decorations
    course.set_tile(base_x + 2, 4, Tile::DecoTorch);
}

/// Tower Climb: vertical climb with platforms, ladders, and bats.
fn gen_tower_climb(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    // Build side walls
    for y in 2..COURSE_HEIGHT - 2 {
        course.set_tile(base_x, y, Tile::StoneBrick);
        if base_x + ROOM_WIDTH - 1 < course.width {
            course.set_tile(base_x + ROOM_WIDTH - 1, y, Tile::StoneBrick);
        }
    }

    // Alternating platforms going up
    let plat_positions = [4u32, 8, 12, 16, 20, 24];
    for (i, &py) in plat_positions.iter().enumerate() {
        if py >= COURSE_HEIGHT - 2 {
            continue;
        }
        let offset = if i % 2 == 0 { 2u32 } else { 10u32 };
        let len = rng.random_range(5u32..9);
        for dx in 0..len {
            let px = base_x + offset + dx;
            if px < base_x + ROOM_WIDTH - 1 && px < course.width {
                course.set_tile(px, py, Tile::Platform);
            }
        }
    }

    // Central ladder sections
    let ladder_x = base_x + ROOM_WIDTH / 2;
    for y in [6u32, 10, 14, 18, 22] {
        for dy in 0..3 {
            if y + dy < COURSE_HEIGHT - 2 && ladder_x < course.width {
                course.set_tile(ladder_x, y + dy, Tile::Ladder);
            }
        }
    }

    // Bat enemies at various heights
    for &bat_y in &[6.0, 14.0, 22.0] {
        let bat_x = (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE;
        course.enemy_spawns.push(EnemySpawn {
            x: bat_x,
            y: bat_y * TILE_SIZE,
            enemy_type: EnemyType::Bat,
            patrol_min_x: (base_x + 2) as f32 * TILE_SIZE,
            patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
        });
    }

    // Power-up near the top
    let pu_y = rng.random_range(18u32..24).min(COURSE_HEIGHT - 3);
    course.set_tile(base_x + ROOM_WIDTH / 2 + 2, pu_y, Tile::PowerUpSpawn);

    // Decorative stained glass
    course.set_tile(base_x + 3, 15, Tile::DecoStainedGlass);
}

/// Cathedral Hall: large open room with high ceiling, pillars, and medusa.
fn gen_cathedral_hall(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    // Pillars
    let pillar_count = rng.random_range(2u32..4);
    let spacing = ROOM_WIDTH / (pillar_count + 1);
    for i in 1..=pillar_count {
        let px = base_x + i * spacing;
        for y in 2..10 {
            if px < course.width {
                course.set_tile(px, y, Tile::StoneBrick);
            }
        }
    }

    // Upper platforms between pillars
    for i in 0..pillar_count {
        let plat_x = base_x + i * spacing + spacing / 2;
        for dx in 0..3 {
            if plat_x + dx < course.width {
                course.set_tile(plat_x + dx, 8, Tile::Platform);
            }
        }
    }

    // Medusa floating high up
    let medusa_x = (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: medusa_x,
        y: 12.0 * TILE_SIZE,
        enemy_type: EnemyType::Medusa,
        patrol_min_x: (base_x + 2) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
    });

    // Skeleton on the ground
    add_corridor_enemies(course, rng, base_x);

    // Power-up on a high platform
    course.set_tile(base_x + ROOM_WIDTH / 2, 9, Tile::PowerUpSpawn);

    // Stained glass and torches
    course.set_tile(base_x + 5, 14, Tile::DecoStainedGlass);
    course.set_tile(base_x + 15, 14, Tile::DecoStainedGlass);
    course.set_tile(base_x + 2, 3, Tile::DecoTorch);
    course.set_tile(base_x + ROOM_WIDTH - 3, 3, Tile::DecoTorch);

    // Cobwebs in corners and chains from ceiling
    course.set_tile(base_x + 1, 14, Tile::DecoCobweb);
    course.set_tile(base_x + ROOM_WIDTH - 2, 14, Tile::DecoCobweb);
    course.set_tile(base_x + 8, 14, Tile::DecoChain);
    course.set_tile(base_x + 12, 14, Tile::DecoChain);
}

/// Crypt Depths: dark, narrow passages with spikes and breakable walls.
fn gen_crypt_depths(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    // Low ceiling
    for x in base_x..base_x + ROOM_WIDTH {
        if x < course.width {
            course.set_tile(x, 12, Tile::StoneBrick);
            course.set_tile(x, 13, Tile::StoneBrick);
        }
    }

    // Internal walls with gaps
    let wall_count = rng.random_range(2u32..4);
    let spacing = ROOM_WIDTH / (wall_count + 1);
    for i in 1..=wall_count {
        let wx = base_x + i * spacing;
        let gap_y = rng.random_range(3u32..8);
        for y in 2..12 {
            if y != gap_y && y != gap_y + 1 && y != gap_y + 2 && wx < course.width {
                course.set_tile(wx, y, Tile::StoneBrick);
            }
        }
    }

    // Breakable walls hiding secrets
    let bw_x = base_x + rng.random_range(3..ROOM_WIDTH - 3);
    let bw_y = rng.random_range(3u32..8);
    course.set_tile(bw_x, bw_y, Tile::BreakableWall);
    // Power-up behind breakable wall
    if bw_x + 1 < course.width {
        course.set_tile(bw_x + 1, bw_y, Tile::PowerUpSpawn);
    }

    // Floor spikes
    let spike_x = base_x + rng.random_range(4..ROOM_WIDTH - 4);
    for dx in 0..3 {
        if spike_x + dx < course.width {
            course.set_tile(spike_x + dx, 2, Tile::Spikes);
        }
    }

    // Skeleton enemies
    let skel_x = (base_x + ROOM_WIDTH / 3) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: skel_x,
        y: 3.0 * TILE_SIZE,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (base_x + 2) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE,
    });

    let skel2_x = (base_x + 2 * ROOM_WIDTH / 3) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: skel2_x,
        y: 3.0 * TILE_SIZE,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
    });

    // Flooded floor section (water pool)
    let water_x = base_x + rng.random_range(8..ROOM_WIDTH - 5);
    let water_len = rng.random_range(3u32..6);
    for dx in 0..water_len {
        if water_x + dx < course.width {
            course.set_tile(water_x + dx, 2, Tile::Water);
            course.set_tile(water_x + dx, 3, Tile::Water);
        }
    }

    // Torch decorations
    course.set_tile(base_x + 2, 4, Tile::DecoTorch);
    course.set_tile(base_x + ROOM_WIDTH - 3, 4, Tile::DecoTorch);

    // Cobwebs and chains
    course.set_tile(base_x + 1, 11, Tile::DecoCobweb);
    course.set_tile(base_x + ROOM_WIDTH - 2, 11, Tile::DecoCobweb);
    course.set_tile(base_x + 6, 11, Tile::DecoChain);
    course.set_tile(base_x + 14, 11, Tile::DecoChain);
}

/// Bridge Run: platforming over a pit with falling hazards.
fn gen_bridge_run(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    // Remove floor in the middle to create a pit
    let pit_start = base_x + 3;
    let pit_end = base_x + ROOM_WIDTH - 3;
    for x in pit_start..pit_end {
        if x < course.width {
            course.set_tile(x, 0, Tile::Empty);
            course.set_tile(x, 1, Tile::Empty);
            // Alternate water and spikes at the pit bottom
            if (x - pit_start) % 3 < 2 {
                course.set_tile(x, 0, Tile::Water);
                course.set_tile(x, 1, Tile::Water);
            } else {
                course.set_tile(x, 0, Tile::Spikes);
            }
        }
    }

    // Bridge platforms across the pit
    let bridge_y = rng.random_range(5u32..8);
    let gap_positions: Vec<u32> = (0..3)
        .map(|_| rng.random_range(pit_start + 1..pit_end - 1))
        .collect();

    for x in pit_start..pit_end {
        if x < course.width && !gap_positions.contains(&x) {
            course.set_tile(x, bridge_y, Tile::Platform);
        }
    }

    // Higher platforms for alternative route
    for _ in 0..2 {
        let hx = base_x + rng.random_range(4..ROOM_WIDTH - 4);
        let hy = bridge_y + rng.random_range(4u32..8);
        let len = rng.random_range(3u32..6);
        for dx in 0..len {
            if hx + dx < course.width && hy < COURSE_HEIGHT - 2 {
                course.set_tile(hx + dx, hy, Tile::Platform);
            }
        }
    }

    // Bats flying over the pit
    let bat_x = (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: bat_x,
        y: (bridge_y + 3) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Bat,
        patrol_min_x: (base_x + 3) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 3) as f32 * TILE_SIZE,
    });

    // Power-up over the pit
    course.set_tile(base_x + ROOM_WIDTH / 2, bridge_y + 2, Tile::PowerUpSpawn);
}

/// Boss Arena: large open room with multiple enemies and power-ups.
fn gen_boss_arena(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    // Side walls
    for y in 2..20 {
        course.set_tile(base_x, y, Tile::StoneBrick);
        if base_x + ROOM_WIDTH - 1 < course.width {
            course.set_tile(base_x + ROOM_WIDTH - 1, y, Tile::StoneBrick);
        }
    }

    // Raised platforms for combat
    for i in 0..3 {
        let py = 5 + i * 5;
        let px = base_x + rng.random_range(3..8);
        let len = rng.random_range(4u32..8);
        for dx in 0..len {
            if px + dx < base_x + ROOM_WIDTH - 1 && py < COURSE_HEIGHT - 2 {
                course.set_tile(px + dx, py, Tile::Platform);
            }
        }
    }

    // Knight enemy (tough)
    let knight_x = (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: knight_x,
        y: 3.0 * TILE_SIZE,
        enemy_type: EnemyType::Knight,
        patrol_min_x: (base_x + 2) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
    });

    // Medusa high up
    course.enemy_spawns.push(EnemySpawn {
        x: knight_x,
        y: 14.0 * TILE_SIZE,
        enemy_type: EnemyType::Medusa,
        patrol_min_x: (base_x + 3) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 3) as f32 * TILE_SIZE,
    });

    // Skeleton adds
    add_corridor_enemies(course, rng, base_x);

    // Two power-ups
    course.set_tile(base_x + 5, 6, Tile::PowerUpSpawn);
    course.set_tile(base_x + 15, 11, Tile::PowerUpSpawn);

    // Decorations
    course.set_tile(base_x + 2, 3, Tile::DecoTorch);
    course.set_tile(base_x + ROOM_WIDTH - 3, 3, Tile::DecoTorch);
    course.set_tile(base_x + ROOM_WIDTH / 2, 18, Tile::DecoStainedGlass);
}

/// Branch Split: room splits into upper and lower paths.
fn gen_branch_split(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    // Horizontal divider creating upper and lower paths
    let divider_y = 14u32;
    for x in (base_x + 8)..base_x + ROOM_WIDTH {
        if x < course.width {
            course.set_tile(x, divider_y, Tile::StoneBrick);
        }
    }

    // Lower path: flat floor with some obstacles
    let spike_x = base_x + rng.random_range(5..10);
    for dx in 0..2 {
        if spike_x + dx < course.width {
            course.set_tile(spike_x + dx, 2, Tile::Spikes);
        }
    }

    // Upper path: platforms leading up
    for i in 0..4 {
        let px = base_x + 2 + i * 4;
        let py = 10 + i;
        if px < course.width && py < divider_y {
            course.set_tile(px, py, Tile::Platform);
            course.set_tile(px + 1, py, Tile::Platform);
        }
    }

    // Ladder to reach upper path
    for y in 4..divider_y {
        course.set_tile(base_x + 5, y, Tile::Ladder);
    }

    // Upper path entry has platforms above the divider
    for dx in 0..6 {
        if base_x + 8 + dx < course.width {
            course.set_tile(base_x + 8 + dx, divider_y + 3, Tile::Platform);
        }
    }

    // Enemies on both paths
    let lower_enemy_x = (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: lower_enemy_x,
        y: 3.0 * TILE_SIZE,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (base_x + 2) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
    });

    let upper_enemy_x = (base_x + 12) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: upper_enemy_x,
        y: (divider_y + 4) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Bat,
        patrol_min_x: (base_x + 8) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
    });

    // Power-up on upper path (reward for taking the harder route)
    course.set_tile(base_x + 14, divider_y + 4, Tile::PowerUpSpawn);

    // Torch at the split
    course.set_tile(base_x + 3, 3, Tile::DecoTorch);
}

/// Branch Merge: two paths converge back into one.
fn gen_branch_merge(course: &mut Course, rng: &mut StdRng, base_x: u32, _room_idx: u32) {
    // Divider for first half, opening up in second half
    let divider_y = 14u32;
    let merge_x = base_x + ROOM_WIDTH / 2;
    for x in base_x..merge_x {
        if x < course.width {
            course.set_tile(x, divider_y, Tile::StoneBrick);
        }
    }

    // Platforms descending from upper path
    for i in 0..4 {
        let px = merge_x + i * 3;
        let py = divider_y - 1 - i * 2;
        if px < course.width && py > 2 && py < COURSE_HEIGHT {
            course.set_tile(px, py, Tile::Platform);
            if px + 1 < course.width {
                course.set_tile(px + 1, py, Tile::Platform);
            }
        }
    }

    // Lower path: some platforms for variety
    let plat_x = base_x + rng.random_range(2..6);
    for dx in 0..4 {
        if plat_x + dx < course.width {
            course.set_tile(plat_x + dx, 5, Tile::Platform);
        }
    }

    // Knight enemy guarding the merge point
    let knight_x = merge_x as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: knight_x,
        y: 3.0 * TILE_SIZE,
        enemy_type: EnemyType::Knight,
        patrol_min_x: (base_x + 2) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
    });

    // Power-up at merge point
    course.set_tile(merge_x + 2, 3, Tile::PowerUpSpawn);

    // Torches
    course.set_tile(base_x + 2, 3, Tile::DecoTorch);
    course.set_tile(base_x + ROOM_WIDTH - 3, 3, Tile::DecoTorch);
}

/// Helper: add standard corridor enemies (skeleton and optional bat).
fn add_corridor_enemies(course: &mut Course, rng: &mut StdRng, base_x: u32) {
    // Ground skeleton
    let skel_x = (base_x + rng.random_range(4..ROOM_WIDTH - 4)) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: skel_x,
        y: 3.0 * TILE_SIZE,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (base_x + 2) as f32 * TILE_SIZE,
        patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
    });

    // 50% chance of a bat
    if rng.random_range(0u32..2) == 0 {
        let bat_x = (base_x + ROOM_WIDTH / 2) as f32 * TILE_SIZE;
        course.enemy_spawns.push(EnemySpawn {
            x: bat_x,
            y: 8.0 * TILE_SIZE,
            enemy_type: EnemyType::Bat,
            patrol_min_x: (base_x + 2) as f32 * TILE_SIZE,
            patrol_max_x: (base_x + ROOM_WIDTH - 2) as f32 * TILE_SIZE,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_generation() {
        let c1 = generate_course(42);
        let c2 = generate_course(42);
        assert_eq!(c1.tiles, c2.tiles, "Same seed must produce same course");
        assert_eq!(
            c1.enemy_spawns.len(),
            c2.enemy_spawns.len(),
            "Same seed must produce same enemy spawns"
        );
    }

    #[test]
    fn different_seeds_different_courses() {
        let c1 = generate_course(42);
        let c2 = generate_course(123);
        assert_ne!(
            c1.tiles, c2.tiles,
            "Different seeds should produce different courses"
        );
    }

    #[test]
    fn has_finish_tile() {
        let course = generate_course(42);
        let has_finish = course.tiles.contains(&Tile::Finish);
        assert!(has_finish, "Course must have at least one Finish tile");
    }

    #[test]
    fn has_solid_ground() {
        let course = generate_course(42);
        // Check that most of row 0 has StoneBrick or Spikes (bridge rooms remove floor)
        let ground_count = (0..course.width)
            .filter(|&x| {
                let tile = course.get_tile(x as i32, 0);
                matches!(tile, Tile::StoneBrick | Tile::Spikes)
            })
            .count();
        assert!(
            ground_count > course.width as usize / 3,
            "Ground should have significant solid/spikes coverage: {}/{}",
            ground_count,
            course.width,
        );
    }

    #[test]
    fn spawn_inside_bounds() {
        let course = generate_course(42);
        let max_x = course.width as f32 * TILE_SIZE;
        let max_y = course.height as f32 * TILE_SIZE;
        assert!(course.spawn_x > 0.0 && course.spawn_x < max_x);
        assert!(course.spawn_y > 0.0 && course.spawn_y < max_y);
    }

    #[test]
    fn course_dimensions_correct() {
        let course = generate_course(42);
        assert_eq!(course.width, COURSE_WIDTH);
        assert_eq!(course.height, COURSE_HEIGHT);
        assert_eq!(course.tiles.len(), (COURSE_WIDTH * COURSE_HEIGHT) as usize);
    }

    #[test]
    fn has_enemy_spawns() {
        let course = generate_course(42);
        assert!(
            !course.enemy_spawns.is_empty(),
            "Course should have enemy spawns"
        );
    }

    #[test]
    fn has_checkpoint_positions() {
        let course = generate_course(42);
        assert!(
            !course.checkpoint_positions.is_empty(),
            "Course should have checkpoint positions"
        );
    }

    #[test]
    fn checkpoints_every_3_rooms() {
        let course = generate_course(42);
        // Should have checkpoints at rooms 3, 6, 9, 12 = 4 checkpoints
        assert_eq!(
            course.checkpoint_positions.len(),
            4,
            "Should have 4 checkpoints (rooms 3, 6, 9, 12)"
        );
    }

    #[test]
    fn has_checkpoint_tiles() {
        let course = generate_course(42);
        let has_checkpoint = course.tiles.contains(&Tile::Checkpoint);
        assert!(
            has_checkpoint,
            "Course must have at least one Checkpoint tile"
        );
    }

    #[test]
    fn has_powerup_spawns() {
        let course = generate_course(42);
        let pu_count = course
            .tiles
            .iter()
            .filter(|&&t| t == Tile::PowerUpSpawn)
            .count();
        assert!(
            pu_count >= 5,
            "Course should have at least 5 power-up spawn tiles, got {}",
            pu_count,
        );
    }

    #[test]
    fn has_ladder_tiles() {
        let course = generate_course(42);
        let ladder_count = course.tiles.iter().filter(|&&t| t == Tile::Ladder).count();
        assert!(
            ladder_count > 0,
            "Course should have at least one Ladder tile"
        );
    }

    #[test]
    fn has_breakable_walls() {
        // Need to try a few seeds since not all rooms have breakable walls
        let mut found = false;
        for seed in 0..20 {
            let course = generate_course(seed);
            if course.tiles.contains(&Tile::BreakableWall) {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "At least one seed should produce a course with BreakableWall tiles"
        );
    }

    #[test]
    fn has_decorative_tiles() {
        let course = generate_course(42);
        let has_torch = course.tiles.contains(&Tile::DecoTorch);
        assert!(has_torch, "Course should have decorative torch tiles");
    }

    #[test]
    fn course_always_has_finish_for_many_seeds() {
        for seed in 0..20 {
            let course = generate_course(seed);
            let has_finish = course.tiles.contains(&Tile::Finish);
            assert!(
                has_finish,
                "Course with seed {seed} should have at least one Finish tile"
            );
        }
    }

    #[test]
    fn enemy_spawns_within_course_bounds() {
        let course = generate_course(42);
        let max_x = course.width as f32 * TILE_SIZE;
        for spawn in &course.enemy_spawns {
            assert!(
                spawn.x >= 0.0 && spawn.x <= max_x,
                "Enemy spawn x={} out of bounds [0, {}]",
                spawn.x,
                max_x,
            );
            assert!(
                spawn.patrol_min_x <= spawn.patrol_max_x,
                "Patrol min ({}) should be <= max ({})",
                spawn.patrol_min_x,
                spawn.patrol_max_x,
            );
        }
    }
}
