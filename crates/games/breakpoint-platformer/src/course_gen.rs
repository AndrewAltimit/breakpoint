use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::physics::TILE_SIZE;

/// Tile types for the platformer course grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tile {
    Empty,
    Solid,
    Platform,
    Hazard,
    Checkpoint,
    Finish,
    PowerUpSpawn,
}

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
}

impl Course {
    pub fn get_tile(&self, x: i32, y: i32) -> Tile {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return Tile::Empty;
        }
        self.tiles[y as usize * self.width as usize + x as usize]
    }

    fn set_tile(&mut self, x: u32, y: u32, tile: Tile) {
        if x < self.width && y < self.height {
            self.tiles[y as usize * self.width as usize + x as usize] = tile;
        }
    }
}

/// Chunk width in tiles (each procedural section is this wide).
const CHUNK_WIDTH: u32 = 10;
/// Course height in tiles.
pub const COURSE_HEIGHT: usize = 20;
/// Number of chunks in a generated course.
const NUM_CHUNKS: u32 = 10;
/// Course width in tiles (total chunks * chunk width).
pub const COURSE_WIDTH: usize = 100; // CHUNK_WIDTH * NUM_CHUNKS

/// Generate a deterministic course from a seed.
pub fn generate_course(seed: u64) -> Course {
    let width = CHUNK_WIDTH * NUM_CHUNKS;
    let height = COURSE_HEIGHT as u32;
    let mut course = Course {
        width,
        height,
        tiles: vec![Tile::Empty; (width * height) as usize],
        spawn_x: 2.0 * TILE_SIZE,
        spawn_y: 3.0 * TILE_SIZE,
    };

    let mut rng = StdRng::seed_from_u64(seed);

    // Ground floor (solid bottom 2 rows)
    for x in 0..width {
        course.set_tile(x, 0, Tile::Solid);
        course.set_tile(x, 1, Tile::Solid);
    }

    // Spawn area (first chunk is flat with some platforms)
    for y in 2..4 {
        course.set_tile(0, y, Tile::Solid);
    }

    // Generate each chunk
    for chunk_idx in 1..NUM_CHUNKS {
        let base_x = chunk_idx * CHUNK_WIDTH;
        generate_chunk(&mut course, &mut rng, base_x, chunk_idx);
    }

    // Place checkpoints every 3 chunks
    for chunk_idx in (3..NUM_CHUNKS).step_by(3) {
        let cx = chunk_idx * CHUNK_WIDTH + CHUNK_WIDTH / 2;
        course.set_tile(cx, 2, Tile::Checkpoint);
    }

    // Place finish line in last chunk
    let finish_x = width - 3;
    course.set_tile(finish_x, 2, Tile::Finish);
    course.set_tile(finish_x + 1, 2, Tile::Finish);

    course
}

fn generate_chunk(course: &mut Course, rng: &mut StdRng, base_x: u32, _chunk_idx: u32) {
    let pattern = rng.random_range(0u8..5);

    match pattern {
        0 => {
            // Flat section with a pit
            let pit_start = base_x + rng.random_range(3..7);
            let pit_width = rng.random_range(2..4);
            for x in pit_start..pit_start + pit_width {
                if x < course.width {
                    course.set_tile(x, 0, Tile::Empty);
                    course.set_tile(x, 1, Tile::Empty);
                }
            }
            // Hazard at bottom of pit
            for x in pit_start..pit_start + pit_width {
                if x < course.width {
                    // Hazard is below, effectively falling = respawn
                }
            }
        },
        1 => {
            // Raised platforms
            let plat_y = rng.random_range(4u32..8);
            let plat_start = base_x + rng.random_range(1..4);
            let plat_len = rng.random_range(3..6);
            for x in plat_start..plat_start + plat_len {
                if x < course.width {
                    course.set_tile(x, plat_y, Tile::Platform);
                }
            }
            // Power-up on one platform
            if plat_start + 1 < course.width {
                course.set_tile(plat_start + 1, plat_y + 1, Tile::PowerUpSpawn);
            }
        },
        2 => {
            // Staircase going up
            for i in 0..5u32 {
                let x = base_x + i * 2;
                let y = 2 + i;
                if x < course.width && y < course.height {
                    course.set_tile(x, y, Tile::Solid);
                    if x + 1 < course.width {
                        course.set_tile(x + 1, y, Tile::Solid);
                    }
                }
            }
        },
        3 => {
            // Wall with gap
            let wall_x = base_x + CHUNK_WIDTH / 2;
            let gap_y = rng.random_range(3u32..6);
            for y in 2..10 {
                if y != gap_y && y != gap_y + 1 && wall_x < course.width {
                    course.set_tile(wall_x, y, Tile::Solid);
                }
            }
        },
        _ => {
            // Hazard section
            let hz_start = base_x + rng.random_range(2..5);
            let hz_len = rng.random_range(2..4);
            for x in hz_start..hz_start + hz_len {
                if x < course.width {
                    course.set_tile(x, 2, Tile::Hazard);
                }
            }
            // Platform above hazard for safe passage
            for x in hz_start..hz_start + hz_len + 1 {
                if x < course.width {
                    course.set_tile(x, 5, Tile::Platform);
                }
            }
        },
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
        // First row should be mostly solid
        let solid_count = (0..course.width)
            .filter(|&x| course.get_tile(x as i32, 0) == Tile::Solid)
            .count();
        assert!(
            solid_count > course.width as usize / 2,
            "Ground should be mostly solid"
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
}
