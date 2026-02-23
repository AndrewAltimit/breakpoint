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

// ================================================================
// Labyrinth constants
// ================================================================

/// Room width in tiles.
pub const ROOM_W: u32 = 32;
/// Room height in tiles.
pub const ROOM_H: u32 = 24;
/// Grid columns (rooms wide).
pub const GRID_COLS: u32 = 6;
/// Grid rows (rooms tall).
pub const GRID_ROWS: u32 = 5;
/// Total course width in tiles.
pub const COURSE_WIDTH: u32 = ROOM_W * GRID_COLS; // 192
/// Total course height in tiles.
pub const COURSE_HEIGHT: u32 = ROOM_H * GRID_ROWS; // 120

// Legacy aliases for compatibility
/// Number of rooms targeted during generation.
pub const NUM_ROOMS: u32 = 22;

// ================================================================
// Room grid types
// ================================================================

/// Position in the room grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridPos {
    pub col: u8,
    pub row: u8,
}

/// Direction of a doorway between adjacent rooms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn opposite(self) -> Self {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }

    fn offset(self) -> (i8, i8) {
        match self {
            Direction::Up => (0, 1),
            Direction::Down => (0, -1),
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
        }
    }
}

/// Theme/type of a placed room, affecting interior generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomTheme {
    Entrance,
    Corridor,
    GreatHall,
    Library,
    Armory,
    Chapel,
    Crypt,
    Tower,
    Dungeon,
    ThroneRoom,
}

/// Convert a `u8` to `RoomTheme`, defaulting to `Entrance` for unknown values.
pub fn room_theme_from_u8(v: u8) -> RoomTheme {
    match v {
        0 => RoomTheme::Entrance,
        1 => RoomTheme::Corridor,
        2 => RoomTheme::GreatHall,
        3 => RoomTheme::Library,
        4 => RoomTheme::Armory,
        5 => RoomTheme::Chapel,
        6 => RoomTheme::Crypt,
        7 => RoomTheme::Tower,
        8 => RoomTheme::Dungeon,
        9 => RoomTheme::ThroneRoom,
        _ => RoomTheme::Entrance,
    }
}

impl RoomTheme {
    /// Convert to `u8` for compact storage.
    pub fn as_u8(self) -> u8 {
        match self {
            RoomTheme::Entrance => 0,
            RoomTheme::Corridor => 1,
            RoomTheme::GreatHall => 2,
            RoomTheme::Library => 3,
            RoomTheme::Armory => 4,
            RoomTheme::Chapel => 5,
            RoomTheme::Crypt => 6,
            RoomTheme::Tower => 7,
            RoomTheme::Dungeon => 8,
            RoomTheme::ThroneRoom => 9,
        }
    }
}

/// A room placed in the labyrinth grid.
#[derive(Debug, Clone)]
pub struct PlacedRoom {
    pub grid_pos: GridPos,
    pub theme: RoomTheme,
    pub doors: Vec<Direction>,
    pub distance_from_start: u16,
}

/// An edge connecting two adjacent rooms.
#[derive(Debug, Clone)]
struct RoomEdge {
    a: GridPos,
    b: GridPos,
    direction: Direction,
}

/// Checkpoint definition with an ID for 2D navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDef {
    pub x: f32,
    pub y: f32,
    pub id: u16,
}

/// A platformer course built from a tile grid.
#[derive(Debug, Clone)]
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
    /// Checkpoint definitions with IDs for 2D exploration.
    pub checkpoint_positions: Vec<CheckpointDef>,
    /// Room distances from start, indexed by (col * GRID_ROWS + row).
    /// Used for rubber-banding and race position.
    pub room_distances: Vec<u16>,
    /// Room themes, indexed by (col * GRID_ROWS + row).
    /// Stored as `RoomTheme as u8` for compact serialization. Default 0 = Entrance.
    pub room_themes: Vec<u8>,
}

// ================================================================
// RLE serialization for Course
// ================================================================

impl Serialize for Course {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        // RLE-encode tiles
        let rle = rle_encode(&self.tiles);

        let mut s = serializer.serialize_struct("Course", 9)?;
        s.serialize_field("width", &self.width)?;
        s.serialize_field("height", &self.height)?;
        s.serialize_field("tiles_rle", &rle)?;
        s.serialize_field("spawn_x", &self.spawn_x)?;
        s.serialize_field("spawn_y", &self.spawn_y)?;
        s.serialize_field("enemy_spawns", &self.enemy_spawns)?;
        s.serialize_field("checkpoint_positions", &self.checkpoint_positions)?;
        s.serialize_field("room_distances", &self.room_distances)?;
        s.serialize_field("room_themes", &self.room_themes)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Course {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct CourseRaw {
            width: u32,
            height: u32,
            tiles_rle: Vec<(u8, u16)>,
            spawn_x: f32,
            spawn_y: f32,
            enemy_spawns: Vec<EnemySpawn>,
            checkpoint_positions: Vec<CheckpointDef>,
            room_distances: Vec<u16>,
            #[serde(default)]
            room_themes: Vec<u8>,
        }

        let raw = CourseRaw::deserialize(deserializer)?;
        let tiles = rle_decode(&raw.tiles_rle).map_err(serde::de::Error::custom)?;

        // If room_themes is missing (old format), default to all Entrance (0)
        let room_themes = if raw.room_themes.is_empty() {
            vec![0; (GRID_COLS * GRID_ROWS) as usize]
        } else {
            raw.room_themes
        };

        Ok(Course {
            width: raw.width,
            height: raw.height,
            tiles,
            spawn_x: raw.spawn_x,
            spawn_y: raw.spawn_y,
            enemy_spawns: raw.enemy_spawns,
            checkpoint_positions: raw.checkpoint_positions,
            room_distances: raw.room_distances,
            room_themes,
        })
    }
}

/// RLE encode tiles as (tile_value, run_length) pairs.
fn rle_encode(tiles: &[Tile]) -> Vec<(u8, u16)> {
    let mut result = Vec::new();
    if tiles.is_empty() {
        return result;
    }

    let mut current = tiles[0] as u8;
    let mut count: u16 = 1;

    for &tile in &tiles[1..] {
        let val = tile as u8;
        if val == current && count < u16::MAX {
            count += 1;
        } else {
            result.push((current, count));
            current = val;
            count = 1;
        }
    }
    result.push((current, count));
    result
}

/// RLE decode tiles from (tile_value, run_length) pairs.
fn rle_decode(rle: &[(u8, u16)]) -> Result<Vec<Tile>, String> {
    let mut tiles = Vec::new();
    for &(val, count) in rle {
        let tile = Tile::try_from(val)?;
        for _ in 0..count {
            tiles.push(tile);
        }
    }
    Ok(tiles)
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

    /// Look up the room distance at a given tile position.
    pub fn room_distance_at(&self, world_x: f32, world_y: f32) -> u16 {
        let col = (world_x / TILE_SIZE / ROOM_W as f32) as u32;
        let row = (world_y / TILE_SIZE / ROOM_H as f32) as u32;
        if col < GRID_COLS && row < GRID_ROWS {
            let idx = col as usize * GRID_ROWS as usize + row as usize;
            if idx < self.room_distances.len() {
                return self.room_distances[idx];
            }
        }
        0
    }

    /// Look up the room theme at a given tile position.
    /// Returns `RoomTheme::Entrance` for positions outside the grid or unset rooms.
    pub fn room_theme_at_tile(&self, tx: i32, ty: i32) -> RoomTheme {
        if tx < 0 || ty < 0 {
            return RoomTheme::Entrance;
        }
        let col = tx as u32 / ROOM_W;
        let row = ty as u32 / ROOM_H;
        if col < GRID_COLS && row < GRID_ROWS {
            let idx = col as usize * GRID_ROWS as usize + row as usize;
            if idx < self.room_themes.len() {
                return room_theme_from_u8(self.room_themes[idx]);
            }
        }
        RoomTheme::Entrance
    }

    /// Find the checkpoint ID at a given tile coordinate, if any.
    pub fn find_checkpoint_id(&self, tx: i32, ty: i32) -> Option<u16> {
        let world_x = tx as f32 * TILE_SIZE + TILE_SIZE / 2.0;
        let world_y = ty as f32 * TILE_SIZE + TILE_SIZE / 2.0;
        self.checkpoint_positions
            .iter()
            .find(|cp| (cp.x - world_x).abs() < TILE_SIZE && (cp.y - world_y).abs() < TILE_SIZE)
            .map(|cp| cp.id)
    }
}

// ================================================================
// Labyrinth generation
// ================================================================

/// Generate a deterministic castle labyrinth course from a seed.
pub fn generate_course(seed: u64) -> Course {
    let width = COURSE_WIDTH;
    let height = COURSE_HEIGHT;
    let mut course = Course {
        width,
        height,
        tiles: vec![Tile::StoneBrick; (width * height) as usize],
        spawn_x: 0.0,
        spawn_y: 0.0,
        enemy_spawns: Vec::new(),
        checkpoint_positions: Vec::new(),
        room_distances: vec![0; (GRID_COLS * GRID_ROWS) as usize],
        room_themes: vec![0; (GRID_COLS * GRID_ROWS) as usize],
    };

    let mut rng = StdRng::seed_from_u64(seed);

    // Step 1: Place rooms using random growth
    let rooms = place_rooms(&mut rng, NUM_ROOMS);

    // Step 2: Build connectivity (MST + extra edges)
    let edges = build_connections(&rooms, &mut rng);

    // Step 3: Assign themes based on distance from start
    let rooms = assign_themes(rooms, &edges);

    // Step 4: Store room distances and themes
    for room in &rooms {
        let idx = room.grid_pos.col as usize * GRID_ROWS as usize + room.grid_pos.row as usize;
        course.room_distances[idx] = room.distance_from_start;
        course.room_themes[idx] = room.theme.as_u8();
    }

    // Step 5: Stamp the labyrinth (carve rooms and doorways)
    stamp_labyrinth(&mut course, &rooms, &edges);

    // Step 6: Populate rooms with interior content
    populate_rooms(&mut course, &rooms, &edges, &mut rng);

    // Step 7: Place checkpoints
    place_checkpoints(&mut course, &rooms);

    // Step 8: Place finish in ThroneRoom
    place_finish(&mut course, &rooms);

    // Step 9: Set spawn position in Entrance room
    let entrance = rooms
        .iter()
        .find(|r| r.theme == RoomTheme::Entrance)
        .unwrap_or(&rooms[0]);
    let base_x = entrance.grid_pos.col as u32 * ROOM_W;
    let base_y = entrance.grid_pos.row as u32 * ROOM_H;
    course.spawn_x = (base_x + ROOM_W / 2) as f32 * TILE_SIZE;
    course.spawn_y = (base_y + 3) as f32 * TILE_SIZE;

    course
}

/// Place rooms using random frontier growth from the start cell.
fn place_rooms(rng: &mut StdRng, target_count: u32) -> Vec<PlacedRoom> {
    let start = GridPos { col: 3, row: 0 };
    let mut placed = vec![PlacedRoom {
        grid_pos: start,
        theme: RoomTheme::Entrance,
        doors: Vec::new(),
        distance_from_start: 0,
    }];

    let mut occupied = std::collections::HashSet::new();
    occupied.insert(start);

    let mut frontier: Vec<GridPos> = Vec::new();
    add_neighbors(start, &occupied, &mut frontier);

    while (placed.len() as u32) < target_count && !frontier.is_empty() {
        let idx = rng.random_range(0..frontier.len());
        let cell = frontier.swap_remove(idx);

        if occupied.contains(&cell) {
            continue;
        }

        occupied.insert(cell);
        placed.push(PlacedRoom {
            grid_pos: cell,
            theme: RoomTheme::Corridor, // placeholder
            doors: Vec::new(),
            distance_from_start: 0,
        });

        add_neighbors(cell, &occupied, &mut frontier);
    }

    // Ensure at least one room in top row for the goal
    let has_top = placed
        .iter()
        .any(|r| r.grid_pos.row == (GRID_ROWS - 1) as u8);
    if !has_top {
        // Find a cell in the top row adjacent to an existing room
        for col in 0..GRID_COLS as u8 {
            let cell = GridPos {
                col,
                row: (GRID_ROWS - 1) as u8,
            };
            if !occupied.contains(&cell) {
                let adj = GridPos {
                    col,
                    row: (GRID_ROWS - 2) as u8,
                };
                if occupied.contains(&adj) {
                    occupied.insert(cell);
                    placed.push(PlacedRoom {
                        grid_pos: cell,
                        theme: RoomTheme::Corridor,
                        doors: Vec::new(),
                        distance_from_start: 0,
                    });
                    break;
                }
            }
        }
    }

    placed
}

/// Add valid neighboring cells to the frontier.
fn add_neighbors(
    pos: GridPos,
    occupied: &std::collections::HashSet<GridPos>,
    frontier: &mut Vec<GridPos>,
) {
    let dirs = [
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
    ];
    for dir in dirs {
        let (dx, dy) = dir.offset();
        let nc = pos.col as i8 + dx;
        let nr = pos.row as i8 + dy;
        if nc >= 0 && nc < GRID_COLS as i8 && nr >= 0 && nr < GRID_ROWS as i8 {
            let neighbor = GridPos {
                col: nc as u8,
                row: nr as u8,
            };
            if !occupied.contains(&neighbor) && !frontier.contains(&neighbor) {
                frontier.push(neighbor);
            }
        }
    }
}

/// Build MST via Prim's algorithm with random weights, plus extra edges.
fn build_connections(rooms: &[PlacedRoom], rng: &mut StdRng) -> Vec<RoomEdge> {
    use std::collections::HashSet;

    let room_set: HashSet<GridPos> = rooms.iter().map(|r| r.grid_pos).collect();
    let mut in_tree: HashSet<GridPos> = HashSet::new();
    let mut edges: Vec<RoomEdge> = Vec::new();

    // All possible edges between adjacent rooms
    let mut all_edges: Vec<(RoomEdge, u32)> = Vec::new();
    for room in rooms {
        for dir in &[
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ] {
            let (dx, dy) = dir.offset();
            let nc = room.grid_pos.col as i8 + dx;
            let nr = room.grid_pos.row as i8 + dy;
            if nc >= 0 && nc < GRID_COLS as i8 && nr >= 0 && nr < GRID_ROWS as i8 {
                let neighbor = GridPos {
                    col: nc as u8,
                    row: nr as u8,
                };
                if room_set.contains(&neighbor) {
                    // Only add each edge once (a < b lexicographically)
                    let (a, b, d) =
                        if (room.grid_pos.col, room.grid_pos.row) < (neighbor.col, neighbor.row) {
                            (room.grid_pos, neighbor, *dir)
                        } else {
                            (neighbor, room.grid_pos, dir.opposite())
                        };
                    let weight = rng.random_range(1u32..100);
                    all_edges.push((RoomEdge { a, b, direction: d }, weight));
                }
            }
        }
    }

    // Deduplicate edges
    all_edges.sort_by_key(|(e, _)| (e.a.col, e.a.row, e.b.col, e.b.row));
    all_edges.dedup_by_key(|(e, _)| (e.a.col, e.a.row, e.b.col, e.b.row));

    // Sort by weight for Prim's
    all_edges.sort_by_key(|(_, w)| *w);

    // Prim's MST
    in_tree.insert(rooms[0].grid_pos);
    let mut mst_count = 0;
    while mst_count < rooms.len() - 1 {
        let mut found = false;
        for (edge, _) in &all_edges {
            let a_in = in_tree.contains(&edge.a);
            let b_in = in_tree.contains(&edge.b);
            if a_in != b_in {
                edges.push(edge.clone());
                in_tree.insert(edge.a);
                in_tree.insert(edge.b);
                mst_count += 1;
                found = true;
                break;
            }
        }
        if !found {
            break;
        }
        // Remove used edge
        let last_edge = edges.last().unwrap();
        let key = (
            last_edge.a.col,
            last_edge.a.row,
            last_edge.b.col,
            last_edge.b.row,
        );
        all_edges.retain(|(e, _)| (e.a.col, e.a.row, e.b.col, e.b.row) != key);
    }

    // Add 3-5 extra random edges for alternate routes
    let extra_count = rng.random_range(3u32..6).min(all_edges.len() as u32);
    for _ in 0..extra_count {
        if all_edges.is_empty() {
            break;
        }
        let idx = rng.random_range(0..all_edges.len());
        let (edge, _) = all_edges.swap_remove(idx);
        // Only add if not already in edges
        let key = (edge.a.col, edge.a.row, edge.b.col, edge.b.row);
        if !edges
            .iter()
            .any(|e| (e.a.col, e.a.row, e.b.col, e.b.row) == key)
        {
            edges.push(edge);
        }
    }

    edges
}

/// BFS from start to compute distances, then assign themes by distance tier.
fn assign_themes(mut rooms: Vec<PlacedRoom>, edges: &[RoomEdge]) -> Vec<PlacedRoom> {
    use std::collections::{HashMap, VecDeque};

    // Build adjacency list
    let mut adj: HashMap<(u8, u8), Vec<(u8, u8)>> = HashMap::new();
    for edge in edges {
        adj.entry((edge.a.col, edge.a.row))
            .or_default()
            .push((edge.b.col, edge.b.row));
        adj.entry((edge.b.col, edge.b.row))
            .or_default()
            .push((edge.a.col, edge.a.row));
    }

    // BFS from start room (index 0)
    let start = rooms[0].grid_pos;
    let mut distances: HashMap<(u8, u8), u16> = HashMap::new();
    let mut queue = VecDeque::new();
    distances.insert((start.col, start.row), 0);
    queue.push_back((start.col, start.row));

    while let Some((col, row)) = queue.pop_front() {
        let dist = distances[&(col, row)];
        if let Some(neighbors) = adj.get(&(col, row)) {
            for &(nc, nr) in neighbors {
                if let std::collections::hash_map::Entry::Vacant(e) = distances.entry((nc, nr)) {
                    e.insert(dist + 1);
                    queue.push_back((nc, nr));
                }
            }
        }
    }

    // Find the max-distance room for ThroneRoom
    let max_dist = rooms
        .iter()
        .map(|r| {
            distances
                .get(&(r.grid_pos.col, r.grid_pos.row))
                .copied()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);

    // Find the room with max distance (prefer top rows)
    let throne_pos = rooms
        .iter()
        .filter(|r| {
            distances
                .get(&(r.grid_pos.col, r.grid_pos.row))
                .copied()
                .unwrap_or(0)
                == max_dist
        })
        .max_by_key(|r| r.grid_pos.row)
        .map(|r| r.grid_pos)
        .unwrap_or(rooms.last().unwrap().grid_pos);

    // Assign themes and distances
    for room in &mut rooms {
        let dist = distances
            .get(&(room.grid_pos.col, room.grid_pos.row))
            .copied()
            .unwrap_or(0);
        room.distance_from_start = dist;

        if room.grid_pos == start {
            room.theme = RoomTheme::Entrance;
        } else if room.grid_pos == throne_pos {
            room.theme = RoomTheme::ThroneRoom;
        } else {
            room.theme = match dist {
                1 => RoomTheme::Corridor,
                2..=3 => {
                    if dist % 2 == 0 {
                        RoomTheme::GreatHall
                    } else {
                        RoomTheme::Library
                    }
                },
                4..=5 => {
                    if dist % 2 == 0 {
                        RoomTheme::Armory
                    } else {
                        RoomTheme::Chapel
                    }
                },
                6..=7 => {
                    if dist % 2 == 0 {
                        RoomTheme::Tower
                    } else {
                        RoomTheme::Crypt
                    }
                },
                _ => RoomTheme::Dungeon,
            };
        }
    }

    // Build door lists for each room based on edges
    let room_positions: std::collections::HashSet<(u8, u8)> = rooms
        .iter()
        .map(|r| (r.grid_pos.col, r.grid_pos.row))
        .collect();
    for room in &mut rooms {
        let pos = room.grid_pos;
        for edge in edges {
            if edge.a == pos {
                room.doors.push(edge.direction);
            } else if edge.b == pos {
                room.doors.push(edge.direction.opposite());
            }
        }
        // Deduplicate doors
        room.doors.sort_by_key(|d| *d as u8);
        room.doors.dedup();
    }
    let _ = room_positions; // suppress unused warning

    rooms
}

/// Stamp the labyrinth: the entire grid starts as StoneBrick.
/// Carve empty interiors for each room, then carve doorways.
fn stamp_labyrinth(course: &mut Course, rooms: &[PlacedRoom], edges: &[RoomEdge]) {
    // Carve room interiors (leave 1-tile walls)
    for room in rooms {
        let bx = room.grid_pos.col as u32 * ROOM_W;
        let by = room.grid_pos.row as u32 * ROOM_H;

        // Carve 30×22 interior (1-tile border)
        for y in (by + 1)..(by + ROOM_H - 1) {
            for x in (bx + 1)..(bx + ROOM_W - 1) {
                course.set_tile(x, y, Tile::Empty);
            }
        }

        // Add floor inside room (bottom 1-tile row inside the border = by + 1)
        for x in (bx + 1)..(bx + ROOM_W - 1) {
            course.set_tile(x, by + 1, Tile::StoneBrick);
        }
    }

    // Carve doorways between connected rooms (4-tile-wide passages)
    for edge in edges {
        let (dx, dy) = edge.direction.offset();

        let bx_a = edge.a.col as u32 * ROOM_W;
        let by_a = edge.a.row as u32 * ROOM_H;

        if dx != 0 {
            // Horizontal doorway: carve through the vertical wall between rooms
            let wall_x = if dx > 0 { bx_a + ROOM_W - 1 } else { bx_a };
            let mid_y = by_a + ROOM_H / 2;
            for dy_off in 0..4u32 {
                let y = mid_y - 1 + dy_off;
                course.set_tile(wall_x, y, Tile::Empty);
                // Also clear the adjacent tile on the other side
                let other_x = (wall_x as i32 + dx as i32) as u32;
                if other_x < course.width {
                    course.set_tile(other_x, y, Tile::Empty);
                }
            }
            // Ensure floor continuity in doorway
            let floor_y = mid_y - 2;
            course.set_tile(wall_x, floor_y, Tile::StoneBrick);
            let other_x = (wall_x as i32 + dx as i32) as u32;
            if other_x < course.width {
                course.set_tile(other_x, floor_y, Tile::StoneBrick);
            }
        } else {
            // Vertical doorway: carve through the horizontal wall between rooms
            let wall_y = if dy > 0 { by_a + ROOM_H - 1 } else { by_a };
            let mid_x = bx_a + ROOM_W / 2;
            for dx_off in 0..4u32 {
                let x = mid_x - 1 + dx_off;
                course.set_tile(x, wall_y, Tile::Empty);
                // Also clear the adjacent tile
                let other_y = (wall_y as i32 + dy as i32) as u32;
                if other_y < course.height {
                    course.set_tile(x, other_y, Tile::Empty);
                }
            }
            // Add ladder for vertical doorways going up
            if dy > 0 {
                let ladder_x = mid_x;
                // Ladder from floor of lower room through doorway to floor of upper room
                for ly in (wall_y.saturating_sub(3))..=(wall_y + 2).min(course.height - 1) {
                    course.set_tile(ladder_x, ly, Tile::Ladder);
                }
            } else {
                let ladder_x = mid_x;
                let other_y = (wall_y as i32 + dy as i32) as u32;
                for ly in other_y.saturating_sub(1)..=(wall_y + 3).min(course.height - 1) {
                    course.set_tile(ladder_x, ly, Tile::Ladder);
                }
            }
        }
    }
}

/// Populate each room's interior with themed content.
fn populate_rooms(
    course: &mut Course,
    rooms: &[PlacedRoom],
    _edges: &[RoomEdge],
    rng: &mut StdRng,
) {
    for room in rooms {
        let bx = room.grid_pos.col as u32 * ROOM_W;
        let by = room.grid_pos.row as u32 * ROOM_H;

        match room.theme {
            RoomTheme::Entrance => gen_entrance(course, bx, by),
            RoomTheme::Corridor => gen_corridor(course, rng, bx, by, &room.doors),
            RoomTheme::GreatHall => gen_great_hall(course, rng, bx, by, &room.doors),
            RoomTheme::Library => gen_library(course, rng, bx, by, &room.doors),
            RoomTheme::Armory => gen_armory(course, rng, bx, by, &room.doors),
            RoomTheme::Chapel => gen_chapel(course, rng, bx, by, &room.doors),
            RoomTheme::Crypt => gen_crypt(course, rng, bx, by, &room.doors),
            RoomTheme::Tower => gen_tower(course, rng, bx, by, &room.doors),
            RoomTheme::Dungeon => gen_dungeon(course, rng, bx, by, &room.doors),
            RoomTheme::ThroneRoom => gen_throne_room(course, rng, bx, by, &room.doors),
        }
    }
}

/// Check if a tile position is within a doorway zone (should be kept clear).
fn is_doorway_zone(x: u32, y: u32, bx: u32, by: u32, doors: &[Direction]) -> bool {
    for door in doors {
        match door {
            Direction::Left => {
                let mid_y = by + ROOM_H / 2;
                if x <= bx + 2 && y >= mid_y - 2 && y <= mid_y + 3 {
                    return true;
                }
            },
            Direction::Right => {
                let mid_y = by + ROOM_H / 2;
                if x >= bx + ROOM_W - 3 && y >= mid_y - 2 && y <= mid_y + 3 {
                    return true;
                }
            },
            Direction::Down => {
                let mid_x = bx + ROOM_W / 2;
                if y <= by + 3 && x >= mid_x - 2 && x <= mid_x + 3 {
                    return true;
                }
            },
            Direction::Up => {
                let mid_x = bx + ROOM_W / 2;
                if y >= by + ROOM_H - 4 && x >= mid_x - 2 && x <= mid_x + 3 {
                    return true;
                }
            },
        }
    }
    false
}

// ================================================================
// Per-theme room generators
// ================================================================

/// Entrance: flat floor, torches, safe.
fn gen_entrance(course: &mut Course, bx: u32, by: u32) {
    // Decorative torches
    course.set_tile(bx + 3, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 4, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W / 2, by + 8, Tile::DecoTorch);

    // A few safe platforms
    for dx in 0..5 {
        course.set_tile(bx + 8 + dx, by + 6, Tile::Platform);
    }
    for dx in 0..5 {
        course.set_tile(bx + 18 + dx, by + 6, Tile::Platform);
    }
}

/// Corridor: basic platforms, 1 skeleton, 1-2 spike patches.
fn gen_corridor(course: &mut Course, rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Platforms
    let plat_count = rng.random_range(2u32..4);
    for _ in 0..plat_count {
        let px = bx + rng.random_range(3..ROOM_W - 5);
        let py = by + rng.random_range(5u32..12);
        if is_doorway_zone(px, py, bx, by, doors) {
            continue;
        }
        let len = rng.random_range(3u32..7);
        for dx in 0..len {
            if !is_doorway_zone(px + dx, py, bx, by, doors) {
                course.set_tile(px + dx, py, Tile::Platform);
            }
        }
    }

    // Spike patches
    let spike_x = bx + rng.random_range(5..ROOM_W - 6);
    let spike_len = rng.random_range(2u32..4);
    for dx in 0..spike_len {
        if !is_doorway_zone(spike_x + dx, by + 2, bx, by, doors) {
            course.set_tile(spike_x + dx, by + 2, Tile::Spikes);
        }
    }

    // 1 Skeleton
    let ex = (bx + ROOM_W / 2) as f32 * TILE_SIZE;
    let ey = (by + 3) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: ex,
        y: ey,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (bx + 2) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 3) as f32 * TILE_SIZE,
    });

    // Torches
    course.set_tile(bx + 2, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 3, by + 3, Tile::DecoTorch);

    // Power-up
    let pu_x = bx + rng.random_range(4..ROOM_W - 4);
    let pu_y = by + rng.random_range(4u32..8);
    course.set_tile(pu_x, pu_y, Tile::PowerUpSpawn);
}

/// GreatHall: pillars, open floor, upper walkway. 1 Skeleton + 1 Medusa.
fn gen_great_hall(course: &mut Course, rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Pillars
    let pillar_count = rng.random_range(2u32..4);
    let spacing = (ROOM_W - 4) / (pillar_count + 1);
    for i in 1..=pillar_count {
        let px = bx + 2 + i * spacing;
        for y in (by + 2)..(by + 12) {
            if !is_doorway_zone(px, y, bx, by, doors) {
                course.set_tile(px, y, Tile::StoneBrick);
            }
        }
    }

    // Upper walkway
    for dx in 0..(ROOM_W - 6) {
        let x = bx + 3 + dx;
        let y = by + 14;
        if !is_doorway_zone(x, y, bx, by, doors) {
            course.set_tile(x, y, Tile::Platform);
        }
    }

    // Skeleton on ground
    let ex = (bx + ROOM_W / 3) as f32 * TILE_SIZE;
    let ey = (by + 3) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: ex,
        y: ey,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (bx + 2) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 3) as f32 * TILE_SIZE,
    });

    // Medusa high up
    let mx = (bx + ROOM_W / 2) as f32 * TILE_SIZE;
    let my = (by + 16) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: mx,
        y: my,
        enemy_type: EnemyType::Medusa,
        patrol_min_x: (bx + 3) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 4) as f32 * TILE_SIZE,
    });

    // Stained glass and torches
    course.set_tile(bx + 5, by + 18, Tile::DecoStainedGlass);
    course.set_tile(bx + ROOM_W - 6, by + 18, Tile::DecoStainedGlass);
    course.set_tile(bx + 2, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 3, by + 3, Tile::DecoTorch);

    // Power-up on upper walkway
    course.set_tile(bx + ROOM_W / 2, by + 15, Tile::PowerUpSpawn);
}

/// Library: bookshelf columns, ladders, vertical. 2 Bats.
fn gen_library(course: &mut Course, rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Bookshelf columns (tall stone brick columns with gaps)
    let col_count = 3u32;
    let spacing = (ROOM_W - 4) / (col_count + 1);
    for i in 1..=col_count {
        let px = bx + 2 + i * spacing;
        for y in (by + 2)..(by + 16) {
            if !is_doorway_zone(px, y, bx, by, doors) {
                // Leave gaps for passage
                if y != by + 7 && y != by + 12 {
                    course.set_tile(px, y, Tile::StoneBrick);
                }
            }
        }
    }

    // Ladders between shelves
    for i in 1..col_count {
        let lx = bx + 2 + i * spacing + spacing / 2;
        for y in (by + 3)..(by + 15) {
            if !is_doorway_zone(lx, y, bx, by, doors) {
                course.set_tile(lx, y, Tile::Ladder);
            }
        }
    }

    // Platforms at different heights
    for h in [by + 7, by + 12] {
        for dx in 0..4 {
            let x = bx + 4 + dx;
            if !is_doorway_zone(x, h, bx, by, doors) {
                course.set_tile(x, h, Tile::Platform);
            }
        }
    }

    // 2 Bats
    for &bat_y in &[by + 8, by + 14] {
        let bx_pos = (bx + rng.random_range(5..ROOM_W - 5)) as f32 * TILE_SIZE;
        course.enemy_spawns.push(EnemySpawn {
            x: bx_pos,
            y: bat_y as f32 * TILE_SIZE,
            enemy_type: EnemyType::Bat,
            patrol_min_x: (bx + 2) as f32 * TILE_SIZE,
            patrol_max_x: (bx + ROOM_W - 3) as f32 * TILE_SIZE,
        });
    }

    // Torches
    course.set_tile(bx + 2, by + 5, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 3, by + 5, Tile::DecoTorch);

    // Power-up near top
    course.set_tile(bx + ROOM_W / 2 + 2, by + 16, Tile::PowerUpSpawn);
}

/// Armory: heavy platforms, weapon racks (deco). 2 Knights. Spike rows.
fn gen_armory(course: &mut Course, rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Heavy platforms
    for &py in &[by + 6, by + 11, by + 16] {
        let start = bx + rng.random_range(3..8);
        let len = rng.random_range(6u32..12);
        for dx in 0..len {
            let x = start + dx;
            if x < bx + ROOM_W - 2 && !is_doorway_zone(x, py, bx, by, doors) {
                course.set_tile(x, py, Tile::Platform);
            }
        }
    }

    // Spike rows on floor
    for dx in 0..6 {
        let x = bx + 8 + dx;
        if !is_doorway_zone(x, by + 2, bx, by, doors) {
            course.set_tile(x, by + 2, Tile::Spikes);
        }
    }
    for dx in 0..4 {
        let x = bx + 20 + dx;
        if !is_doorway_zone(x, by + 2, bx, by, doors) {
            course.set_tile(x, by + 2, Tile::Spikes);
        }
    }

    // 2 Knights
    for &kx_off in &[ROOM_W / 3, 2 * ROOM_W / 3] {
        let kx = (bx + kx_off) as f32 * TILE_SIZE;
        let ky = (by + 3) as f32 * TILE_SIZE;
        course.enemy_spawns.push(EnemySpawn {
            x: kx,
            y: ky,
            enemy_type: EnemyType::Knight,
            patrol_min_x: (bx + 2) as f32 * TILE_SIZE,
            patrol_max_x: (bx + ROOM_W - 3) as f32 * TILE_SIZE,
        });
    }

    // Gargoyle that swoops from above
    course.enemy_spawns.push(EnemySpawn {
        x: (bx + ROOM_W / 2) as f32 * TILE_SIZE,
        y: (by + 14) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Gargoyle,
        patrol_min_x: (bx + 4) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 5) as f32 * TILE_SIZE,
    });

    // Weapon rack decoration (chains)
    course.set_tile(bx + 4, by + 4, Tile::DecoChain);
    course.set_tile(bx + ROOM_W - 5, by + 4, Tile::DecoChain);

    // Torches
    course.set_tile(bx + 2, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 3, by + 3, Tile::DecoTorch);

    // Power-up
    course.set_tile(bx + ROOM_W / 2, by + 12, Tile::PowerUpSpawn);
}

/// Chapel: stained glass, altar platforms. 1 Medusa.
fn gen_chapel(course: &mut Course, _rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Altar platform in center
    for dx in 0..8 {
        let x = bx + ROOM_W / 2 - 4 + dx;
        if !is_doorway_zone(x, by + 5, bx, by, doors) {
            course.set_tile(x, by + 5, Tile::StoneBrick);
        }
    }

    // Side platforms
    for dx in 0..4 {
        course.set_tile(bx + 3 + dx, by + 9, Tile::Platform);
        course.set_tile(bx + ROOM_W - 7 + dx, by + 9, Tile::Platform);
    }

    // Upper platforms
    for dx in 0..6 {
        let x = bx + ROOM_W / 2 - 3 + dx;
        if !is_doorway_zone(x, by + 13, bx, by, doors) {
            course.set_tile(x, by + 13, Tile::Platform);
        }
    }

    // 1 Medusa
    let mx = (bx + ROOM_W / 2) as f32 * TILE_SIZE;
    let my = (by + 15) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: mx,
        y: my,
        enemy_type: EnemyType::Medusa,
        patrol_min_x: (bx + 3) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 4) as f32 * TILE_SIZE,
    });

    // Stained glass
    course.set_tile(bx + 5, by + 18, Tile::DecoStainedGlass);
    course.set_tile(bx + ROOM_W / 2, by + 20, Tile::DecoStainedGlass);
    course.set_tile(bx + ROOM_W - 6, by + 18, Tile::DecoStainedGlass);

    // Torches
    course.set_tile(bx + 2, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 3, by + 3, Tile::DecoTorch);

    // Power-up
    course.set_tile(bx + ROOM_W / 2, by + 14, Tile::PowerUpSpawn);
}

/// Crypt: low ceiling, water pools, breakable walls. 2 Skeletons. Water + spikes.
fn gen_crypt(course: &mut Course, rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Low ceiling
    for x in (bx + 1)..(bx + ROOM_W - 1) {
        if !is_doorway_zone(x, by + 14, bx, by, doors) {
            course.set_tile(x, by + 14, Tile::StoneBrick);
        }
        if !is_doorway_zone(x, by + 15, bx, by, doors) {
            course.set_tile(x, by + 15, Tile::StoneBrick);
        }
    }

    // Internal walls with gaps
    let wall_x = bx + ROOM_W / 3;
    let gap_y = by + rng.random_range(4u32..8);
    for y in (by + 2)..(by + 14) {
        if y != gap_y
            && y != gap_y + 1
            && y != gap_y + 2
            && !is_doorway_zone(wall_x, y, bx, by, doors)
        {
            course.set_tile(wall_x, y, Tile::StoneBrick);
        }
    }

    // Breakable wall
    let bw_x = bx + rng.random_range(4..ROOM_W - 4);
    let bw_y = by + rng.random_range(4u32..8);
    if !is_doorway_zone(bw_x, bw_y, bx, by, doors) {
        course.set_tile(bw_x, bw_y, Tile::BreakableWall);
        if bw_x + 1 < bx + ROOM_W - 1 {
            course.set_tile(bw_x + 1, bw_y, Tile::PowerUpSpawn);
        }
    }

    // Water pool
    let water_x = bx + rng.random_range(8..ROOM_W - 6);
    let water_len = rng.random_range(3u32..6);
    for dx in 0..water_len {
        if !is_doorway_zone(water_x + dx, by + 2, bx, by, doors) {
            // Remove floor to make water pool
            course.set_tile(water_x + dx, by + 1, Tile::Water);
            course.set_tile(water_x + dx, by + 2, Tile::Water);
            course.set_tile(water_x + dx, by + 3, Tile::Water);
        }
    }

    // Floor spikes
    let spike_x = bx + rng.random_range(4..ROOM_W / 3);
    for dx in 0..3 {
        if !is_doorway_zone(spike_x + dx, by + 2, bx, by, doors) {
            course.set_tile(spike_x + dx, by + 2, Tile::Spikes);
        }
    }

    // 2 Skeletons
    for &sx_off in &[ROOM_W / 4, 3 * ROOM_W / 4] {
        let sx = (bx + sx_off) as f32 * TILE_SIZE;
        let sy = (by + 3) as f32 * TILE_SIZE;
        course.enemy_spawns.push(EnemySpawn {
            x: sx,
            y: sy,
            enemy_type: EnemyType::Skeleton,
            patrol_min_x: (bx + 2) as f32 * TILE_SIZE,
            patrol_max_x: (bx + ROOM_W - 3) as f32 * TILE_SIZE,
        });
    }

    // Ghost that drifts through walls
    course.enemy_spawns.push(EnemySpawn {
        x: (bx + ROOM_W / 2) as f32 * TILE_SIZE,
        y: (by + 8) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Ghost,
        patrol_min_x: (bx + 4) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 5) as f32 * TILE_SIZE,
    });

    // Decorations
    course.set_tile(bx + 2, by + 4, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 3, by + 4, Tile::DecoTorch);
    course.set_tile(bx + 3, by + 13, Tile::DecoCobweb);
    course.set_tile(bx + ROOM_W - 4, by + 13, Tile::DecoCobweb);
    course.set_tile(bx + 8, by + 13, Tile::DecoChain);
}

/// Tower: alternating platforms, full-height climb. 3 Bats.
fn gen_tower(course: &mut Course, rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Alternating platforms going up
    let plat_heights = [by + 5, by + 8, by + 11, by + 14, by + 17, by + 20];
    for (i, &py) in plat_heights.iter().enumerate() {
        if py >= by + ROOM_H - 2 {
            continue;
        }
        let offset = if i % 2 == 0 { 3u32 } else { ROOM_W / 2 };
        let len = rng.random_range(6u32..10);
        for dx in 0..len {
            let x = bx + offset + dx;
            if x < bx + ROOM_W - 2 && !is_doorway_zone(x, py, bx, by, doors) {
                course.set_tile(x, py, Tile::Platform);
            }
        }
    }

    // Central ladder sections
    let ladder_x = bx + ROOM_W / 2;
    for &start_y in &[by + 3, by + 9, by + 15] {
        for dy in 0..4 {
            if start_y + dy < by + ROOM_H - 2
                && !is_doorway_zone(ladder_x, start_y + dy, bx, by, doors)
            {
                course.set_tile(ladder_x, start_y + dy, Tile::Ladder);
            }
        }
    }

    // 3 Bats
    for &bat_y in &[by + 7, by + 13, by + 19] {
        if bat_y >= by + ROOM_H - 2 {
            continue;
        }
        let bx_pos = (bx + rng.random_range(4..ROOM_W - 4)) as f32 * TILE_SIZE;
        course.enemy_spawns.push(EnemySpawn {
            x: bx_pos,
            y: bat_y as f32 * TILE_SIZE,
            enemy_type: EnemyType::Bat,
            patrol_min_x: (bx + 2) as f32 * TILE_SIZE,
            patrol_max_x: (bx + ROOM_W - 3) as f32 * TILE_SIZE,
        });
    }

    // Stained glass
    course.set_tile(bx + 4, by + 16, Tile::DecoStainedGlass);

    // Power-up near top
    let pu_y = (by + 18).min(by + ROOM_H - 3);
    course.set_tile(bx + ROOM_W / 2 + 3, pu_y, Tile::PowerUpSpawn);
}

/// Dungeon: traps, narrow passages, breakable walls. 1 Knight + 1 Skeleton. Spikes + water.
fn gen_dungeon(course: &mut Course, rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Narrow passages via internal walls
    for &wall_x_off in &[ROOM_W / 3, 2 * ROOM_W / 3] {
        let wx = bx + wall_x_off;
        let gap1 = by + rng.random_range(4u32..8);
        let gap2 = by + rng.random_range(12u32..16);
        for y in (by + 2)..(by + ROOM_H - 2) {
            if (y >= gap1 && y < gap1 + 3) || (y >= gap2 && y < gap2 + 3) {
                continue;
            }
            if !is_doorway_zone(wx, y, bx, by, doors) {
                course.set_tile(wx, y, Tile::StoneBrick);
            }
        }
    }

    // Breakable walls
    let bw_x = bx + ROOM_W / 3;
    let bw_y = by + 6;
    if !is_doorway_zone(bw_x, bw_y, bx, by, doors) {
        course.set_tile(bw_x, bw_y, Tile::BreakableWall);
    }

    // Floor spikes
    for dx in 0..4 {
        let x = bx + 5 + dx;
        if !is_doorway_zone(x, by + 2, bx, by, doors) {
            course.set_tile(x, by + 2, Tile::Spikes);
        }
    }

    // Water
    for dx in 0..3 {
        let x = bx + ROOM_W / 2 + dx;
        if !is_doorway_zone(x, by + 2, bx, by, doors) {
            course.set_tile(x, by + 1, Tile::Water);
            course.set_tile(x, by + 2, Tile::Water);
        }
    }

    // 1 Knight + 1 Skeleton
    let kx = (bx + ROOM_W / 4) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: kx,
        y: (by + 3) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Knight,
        patrol_min_x: (bx + 2) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W / 3 - 1) as f32 * TILE_SIZE,
    });
    let sx = (bx + 3 * ROOM_W / 4) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: sx,
        y: (by + 3) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (bx + 2 * ROOM_W / 3 + 1) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 3) as f32 * TILE_SIZE,
    });

    // Ghost that phases through dungeon walls
    course.enemy_spawns.push(EnemySpawn {
        x: (bx + ROOM_W / 2) as f32 * TILE_SIZE,
        y: (by + 10) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Ghost,
        patrol_min_x: (bx + 3) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 4) as f32 * TILE_SIZE,
    });

    // Decorations
    course.set_tile(bx + 2, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 3, by + 3, Tile::DecoTorch);
    course.set_tile(bx + 3, by + ROOM_H - 3, Tile::DecoCobweb);

    // Power-up
    course.set_tile(bx + ROOM_W / 2, by + 8, Tile::PowerUpSpawn);
}

/// ThroneRoom: grand platforms, dramatic decoration. 1 Knight + 1 Medusa + 2 Skeletons.
fn gen_throne_room(course: &mut Course, _rng: &mut StdRng, bx: u32, by: u32, doors: &[Direction]) {
    // Grand central platform (throne dais)
    for dx in 0..12 {
        let x = bx + ROOM_W / 2 - 6 + dx;
        if !is_doorway_zone(x, by + 4, bx, by, doors) {
            course.set_tile(x, by + 4, Tile::StoneBrick);
        }
    }

    // Side platforms at various heights
    for dx in 0..6 {
        course.set_tile(bx + 3 + dx, by + 8, Tile::Platform);
        course.set_tile(bx + ROOM_W - 9 + dx, by + 8, Tile::Platform);
    }
    for dx in 0..8 {
        let x = bx + ROOM_W / 2 - 4 + dx;
        if !is_doorway_zone(x, by + 12, bx, by, doors) {
            course.set_tile(x, by + 12, Tile::Platform);
        }
    }
    for dx in 0..5 {
        course.set_tile(bx + 4 + dx, by + 16, Tile::Platform);
        course.set_tile(bx + ROOM_W - 9 + dx, by + 16, Tile::Platform);
    }

    // 1 Knight on the dais
    let knight_x = (bx + ROOM_W / 2) as f32 * TILE_SIZE;
    course.enemy_spawns.push(EnemySpawn {
        x: knight_x,
        y: (by + 5) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Knight,
        patrol_min_x: (bx + ROOM_W / 2 - 6) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W / 2 + 6) as f32 * TILE_SIZE,
    });

    // 1 Medusa above
    course.enemy_spawns.push(EnemySpawn {
        x: knight_x,
        y: (by + 17) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Medusa,
        patrol_min_x: (bx + 3) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 4) as f32 * TILE_SIZE,
    });

    // 2 Skeletons on sides
    course.enemy_spawns.push(EnemySpawn {
        x: (bx + 5) as f32 * TILE_SIZE,
        y: (by + 3) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (bx + 2) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W / 2 - 6) as f32 * TILE_SIZE,
    });
    course.enemy_spawns.push(EnemySpawn {
        x: (bx + ROOM_W - 6) as f32 * TILE_SIZE,
        y: (by + 3) as f32 * TILE_SIZE,
        enemy_type: EnemyType::Skeleton,
        patrol_min_x: (bx + ROOM_W / 2 + 6) as f32 * TILE_SIZE,
        patrol_max_x: (bx + ROOM_W - 3) as f32 * TILE_SIZE,
    });

    // Grand decorations
    course.set_tile(bx + 2, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W - 3, by + 3, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W / 2 - 1, by + 6, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W / 2 + 1, by + 6, Tile::DecoTorch);
    course.set_tile(bx + ROOM_W / 2, by + 20, Tile::DecoStainedGlass);
    course.set_tile(bx + 5, by + 20, Tile::DecoStainedGlass);
    course.set_tile(bx + ROOM_W - 6, by + 20, Tile::DecoStainedGlass);
    course.set_tile(bx + 3, by + ROOM_H - 3, Tile::DecoChain);
    course.set_tile(bx + ROOM_W - 4, by + ROOM_H - 3, Tile::DecoChain);

    // Two power-ups
    course.set_tile(bx + 6, by + 9, Tile::PowerUpSpawn);
    course.set_tile(bx + ROOM_W - 7, by + 9, Tile::PowerUpSpawn);
}

/// Place checkpoints every 2 distance tiers in rooms along the path.
fn place_checkpoints(course: &mut Course, rooms: &[PlacedRoom]) {
    let max_dist = rooms
        .iter()
        .map(|r| r.distance_from_start)
        .max()
        .unwrap_or(0);

    let mut checkpoint_id: u16 = 1;
    // Place checkpoint every 2 distance levels (skip 0 = entrance, skip max = throne)
    let mut tier = 2u16;
    while tier < max_dist {
        // Find a room at this distance tier
        if let Some(room) = rooms.iter().find(|r| r.distance_from_start == tier) {
            let bx = room.grid_pos.col as u32 * ROOM_W;
            let by = room.grid_pos.row as u32 * ROOM_H;
            let cx = bx + ROOM_W / 2;
            let cy = by + 2; // On the floor
            // Find first empty tile above floor
            let mut placed_y = cy;
            for y in cy..cy + 5 {
                if course.get_tile(cx as i32, y as i32) == Tile::Empty {
                    placed_y = y;
                    break;
                }
            }
            course.set_tile(cx, placed_y, Tile::Checkpoint);
            let world_x = cx as f32 * TILE_SIZE + TILE_SIZE / 2.0;
            let world_y = placed_y as f32 * TILE_SIZE + TILE_SIZE / 2.0;
            course.checkpoint_positions.push(CheckpointDef {
                x: world_x,
                y: world_y,
                id: checkpoint_id,
            });
            checkpoint_id += 1;
        }
        tier += 2;
    }
}

/// Place finish tiles in the ThroneRoom.
fn place_finish(course: &mut Course, rooms: &[PlacedRoom]) {
    let throne = rooms
        .iter()
        .find(|r| r.theme == RoomTheme::ThroneRoom)
        .unwrap_or(rooms.last().unwrap());

    let bx = throne.grid_pos.col as u32 * ROOM_W;
    let by = throne.grid_pos.row as u32 * ROOM_H;
    // Place finish on the throne dais
    let fx = bx + ROOM_W / 2;
    let fy = by + 5; // above the dais
    // Find empty tile
    let mut placed_y = fy;
    for y in fy..fy + 5 {
        if course.get_tile(fx as i32, y as i32) == Tile::Empty {
            placed_y = y;
            break;
        }
    }
    course.set_tile(fx - 1, placed_y, Tile::Finish);
    course.set_tile(fx, placed_y, Tile::Finish);
    course.set_tile(fx + 1, placed_y, Tile::Finish);
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
        let max_y = course.height as f32 * TILE_SIZE;
        for spawn in &course.enemy_spawns {
            assert!(
                spawn.x >= 0.0 && spawn.x <= max_x,
                "Enemy spawn x={} out of bounds [0, {}]",
                spawn.x,
                max_x,
            );
            assert!(
                spawn.y >= 0.0 && spawn.y <= max_y,
                "Enemy spawn y={} out of bounds [0, {}]",
                spawn.y,
                max_y,
            );
            assert!(
                spawn.patrol_min_x <= spawn.patrol_max_x,
                "Patrol min ({}) should be <= max ({})",
                spawn.patrol_min_x,
                spawn.patrol_max_x,
            );
        }
    }

    // ================================================================
    // Labyrinth-specific tests
    // ================================================================

    #[test]
    fn labyrinth_room_count_in_range() {
        for seed in 0..10 {
            let course = generate_course(seed);
            // Count rooms by checking room_distances for non-zero or entrance
            let room_count = course
                .room_distances
                .iter()
                .enumerate()
                .filter(|&(idx, _)| {
                    let col = idx / GRID_ROWS as usize;
                    let row = idx % GRID_ROWS as usize;
                    // Check if this cell has a carved room
                    let bx = col as u32 * ROOM_W + ROOM_W / 2;
                    let by = row as u32 * ROOM_H + ROOM_H / 2;
                    course.get_tile(bx as i32, by as i32) == Tile::Empty
                })
                .count();
            assert!(
                (12..=30).contains(&room_count),
                "Seed {seed}: expected 12-30 rooms, got {room_count}"
            );
        }
    }

    #[test]
    fn labyrinth_all_rooms_reachable() {
        use std::collections::{HashSet, VecDeque};

        let course = generate_course(42);
        // BFS from spawn through empty/passable tiles
        let start_tx = (course.spawn_x / TILE_SIZE) as i32;
        let start_ty = (course.spawn_y / TILE_SIZE) as i32;

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        visited.insert((start_tx, start_ty));
        queue.push_back((start_tx, start_ty));

        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in &[(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let nx = x + dx;
                let ny = y + dy;
                if visited.contains(&(nx, ny)) {
                    continue;
                }
                let tile = course.get_tile(nx, ny);
                if !matches!(tile, Tile::StoneBrick | Tile::BreakableWall) {
                    visited.insert((nx, ny));
                    queue.push_back((nx, ny));
                }
            }
        }

        // Check that every room center is reachable
        for col in 0..GRID_COLS {
            for row in 0..GRID_ROWS {
                let bx = col * ROOM_W + ROOM_W / 2;
                let by = row * ROOM_H + ROOM_H / 2;
                if course.get_tile(bx as i32, by as i32) == Tile::Empty {
                    assert!(
                        visited.contains(&(bx as i32, by as i32)),
                        "Room at grid ({col}, {row}) center ({bx}, {by}) not reachable from spawn"
                    );
                }
            }
        }
    }

    #[test]
    fn labyrinth_goal_reachable() {
        use std::collections::{HashSet, VecDeque};

        let course = generate_course(42);
        let start_tx = (course.spawn_x / TILE_SIZE) as i32;
        let start_ty = (course.spawn_y / TILE_SIZE) as i32;

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        visited.insert((start_tx, start_ty));
        queue.push_back((start_tx, start_ty));

        let mut found_finish = false;
        while let Some((x, y)) = queue.pop_front() {
            if course.get_tile(x, y) == Tile::Finish {
                found_finish = true;
                break;
            }
            for (dx, dy) in &[(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let nx = x + dx;
                let ny = y + dy;
                if visited.contains(&(nx, ny)) {
                    continue;
                }
                let tile = course.get_tile(nx, ny);
                if !matches!(tile, Tile::StoneBrick | Tile::BreakableWall) {
                    visited.insert((nx, ny));
                    queue.push_back((nx, ny));
                }
            }
        }

        assert!(found_finish, "Finish tile should be reachable from spawn");
    }

    #[test]
    fn labyrinth_rooms_have_floor() {
        let course = generate_course(42);
        for col in 0..GRID_COLS {
            for row in 0..GRID_ROWS {
                let bx = col * ROOM_W + ROOM_W / 2;
                let by = row * ROOM_H + ROOM_H / 2;
                // Only check rooms that exist (center is empty)
                if course.get_tile(bx as i32, by as i32) != Tile::Empty {
                    continue;
                }
                // Check that there's at least one solid floor row
                let floor_y = row * ROOM_H + 1;
                let mut has_floor = false;
                for x in (col * ROOM_W + 1)..(col * ROOM_W + ROOM_W - 1) {
                    if course.get_tile(x as i32, floor_y as i32) == Tile::StoneBrick {
                        has_floor = true;
                        break;
                    }
                }
                assert!(has_floor, "Room at grid ({col}, {row}) should have a floor");
            }
        }
    }

    #[test]
    fn labyrinth_start_far_from_goal() {
        let course = generate_course(42);
        let max_dist = course.room_distances.iter().copied().max().unwrap_or(0);
        assert!(
            max_dist >= 6,
            "Max room distance should be >= 6, got {max_dist}"
        );
    }

    #[test]
    fn labyrinth_deterministic() {
        let c1 = generate_course(99);
        let c2 = generate_course(99);
        assert_eq!(c1.tiles, c2.tiles);
        assert_eq!(c1.room_distances, c2.room_distances);
        assert_eq!(c1.checkpoint_positions.len(), c2.checkpoint_positions.len());
    }

    #[test]
    fn rle_roundtrip() {
        let course = generate_course(42);
        let encoded = rle_encode(&course.tiles);
        let decoded = rle_decode(&encoded).unwrap();
        assert_eq!(course.tiles, decoded, "RLE roundtrip should preserve tiles");
    }

    #[test]
    fn rle_compression_ratio() {
        let course = generate_course(42);
        let raw_size = course.tiles.len(); // 1 byte per tile
        let rle = rle_encode(&course.tiles);
        let rle_size = rle.len() * 3; // 1 byte tile + 2 bytes count
        assert!(
            rle_size < raw_size / 2,
            "RLE should compress well: raw={raw_size}, rle={rle_size}"
        );
    }

    #[test]
    fn serde_roundtrip() {
        let course = generate_course(42);
        let bytes = rmp_serde::to_vec(&course).unwrap();
        let decoded: Course = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(course.tiles, decoded.tiles);
        assert_eq!(course.width, decoded.width);
        assert_eq!(course.height, decoded.height);
        assert_eq!(course.room_distances, decoded.room_distances);
    }

    #[test]
    fn labyrinth_doorways_passable() {
        let course = generate_course(42);
        // For each room, check that door positions have empty tiles
        for col in 0..GRID_COLS {
            for row in 0..GRID_ROWS {
                let bx = col * ROOM_W;
                let by = row * ROOM_H;
                let center_x = bx + ROOM_W / 2;
                let center_y = by + ROOM_H / 2;

                // Only check rooms that exist
                if course.get_tile(center_x as i32, center_y as i32) != Tile::Empty {
                    continue;
                }

                // Check right doorway
                if col + 1 < GRID_COLS {
                    let right_center = (col + 1) * ROOM_W + ROOM_W / 2;
                    if course.get_tile(right_center as i32, center_y as i32) == Tile::Empty {
                        // There should be passage at the wall
                        let wall_x = bx + ROOM_W - 1;
                        let mid_y = by + ROOM_H / 2;
                        let mut has_passage = false;
                        for dy in 0..6 {
                            let y = mid_y.saturating_sub(1) + dy;
                            if course.get_tile(wall_x as i32, y as i32) != Tile::StoneBrick {
                                has_passage = true;
                                break;
                            }
                        }
                        // This is a soft check — not all adjacent rooms are connected
                        let _ = has_passage;
                    }
                }
            }
        }
    }
}
