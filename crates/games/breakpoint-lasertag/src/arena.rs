use serde::{Deserialize, Serialize};

/// Wall type in the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallType {
    Solid,
    Reflective,
}

/// A wall segment defined by two endpoints on the XZ plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArenaWall {
    pub ax: f32,
    pub az: f32,
    pub bx: f32,
    pub bz: f32,
    pub wall_type: WallType,
}

/// A spawn point in the arena.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub x: f32,
    pub z: f32,
    pub angle: f32,
}

/// An arena definition for Laser Tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arena {
    pub name: String,
    pub width: f32,
    pub depth: f32,
    pub walls: Vec<ArenaWall>,
    pub spawn_points: Vec<SpawnPoint>,
    pub smoke_zones: Vec<(f32, f32, f32)>, // (x, z, radius)
}

/// Arena size preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArenaSize {
    Small,
    Default,
    Large,
}

/// Load an arena from a JSON file, returning `None` if the file is missing or invalid.
pub fn load_arena_from_file(path: &str) -> Option<Arena> {
    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<Arena>(&content) {
            Ok(arena) => Some(arena),
            Err(e) => {
                tracing::warn!("Failed to parse {path}: {e}");
                None
            },
        },
        Err(_) => None,
    }
}

/// Load an arena for the given size, preferring a JSON file from the arenas directory.
///
/// Checks env var `BREAKPOINT_ARENAS_DIR` (default `config/arenas`) for a file named
/// `{size}.json` (e.g. `small.json`, `default.json`, `large.json`).
/// Falls back to `generate_arena(size)` if the file is missing or unparseable.
pub fn load_arena(size: ArenaSize) -> Arena {
    let dir =
        std::env::var("BREAKPOINT_ARENAS_DIR").unwrap_or_else(|_| "config/arenas".to_string());
    let size_name = match size {
        ArenaSize::Small => "small",
        ArenaSize::Default => "default",
        ArenaSize::Large => "large",
    };
    let path = format!("{dir}/{size_name}.json");
    load_arena_from_file(&path).unwrap_or_else(|| generate_arena(size))
}

/// Generate an arena based on size preset.
pub fn generate_arena(size: ArenaSize) -> Arena {
    let (width, depth) = match size {
        ArenaSize::Small => (30.0, 30.0),
        ArenaSize::Default => (50.0, 50.0),
        ArenaSize::Large => (70.0, 70.0),
    };

    // Boundary walls (solid)
    let mut walls = vec![
        ArenaWall {
            ax: 0.0,
            az: 0.0,
            bx: width,
            bz: 0.0,
            wall_type: WallType::Solid,
        },
        ArenaWall {
            ax: width,
            az: 0.0,
            bx: width,
            bz: depth,
            wall_type: WallType::Solid,
        },
        ArenaWall {
            ax: width,
            az: depth,
            bx: 0.0,
            bz: depth,
            wall_type: WallType::Solid,
        },
        ArenaWall {
            ax: 0.0,
            az: depth,
            bx: 0.0,
            bz: 0.0,
            wall_type: WallType::Solid,
        },
    ];

    // Interior obstacles
    let cx = width / 2.0;
    let cz = depth / 2.0;

    // Cross-shaped center obstacle (reflective)
    walls.push(ArenaWall {
        ax: cx - 3.0,
        az: cz,
        bx: cx + 3.0,
        bz: cz,
        wall_type: WallType::Reflective,
    });
    walls.push(ArenaWall {
        ax: cx,
        az: cz - 3.0,
        bx: cx,
        bz: cz + 3.0,
        wall_type: WallType::Reflective,
    });

    // Corner barriers (solid)
    let offset = width * 0.25;
    walls.push(ArenaWall {
        ax: offset,
        az: offset - 2.0,
        bx: offset,
        bz: offset + 2.0,
        wall_type: WallType::Solid,
    });
    walls.push(ArenaWall {
        ax: width - offset,
        az: offset - 2.0,
        bx: width - offset,
        bz: offset + 2.0,
        wall_type: WallType::Solid,
    });
    walls.push(ArenaWall {
        ax: offset,
        az: depth - offset - 2.0,
        bx: offset,
        bz: depth - offset + 2.0,
        wall_type: WallType::Solid,
    });
    walls.push(ArenaWall {
        ax: width - offset,
        az: depth - offset - 2.0,
        bx: width - offset,
        bz: depth - offset + 2.0,
        wall_type: WallType::Solid,
    });

    // Spawn points around the perimeter
    let inset = 3.0;
    let spawn_points = vec![
        SpawnPoint {
            x: inset,
            z: inset,
            angle: 0.78,
        },
        SpawnPoint {
            x: width - inset,
            z: inset,
            angle: 2.36,
        },
        SpawnPoint {
            x: width - inset,
            z: depth - inset,
            angle: 3.93,
        },
        SpawnPoint {
            x: inset,
            z: depth - inset,
            angle: 5.50,
        },
        SpawnPoint {
            x: cx,
            z: inset,
            angle: 1.57,
        },
        SpawnPoint {
            x: cx,
            z: depth - inset,
            angle: 4.71,
        },
        SpawnPoint {
            x: inset,
            z: cz,
            angle: 0.0,
        },
        SpawnPoint {
            x: width - inset,
            z: cz,
            angle: std::f32::consts::PI,
        },
    ];

    // Smoke zones
    let smoke_zones = vec![(cx - 8.0, cz - 8.0, 3.0), (cx + 8.0, cz + 8.0, 3.0)];

    Arena {
        name: match size {
            ArenaSize::Small => "Small Arena".to_string(),
            ArenaSize::Default => "Default Arena".to_string(),
            ArenaSize::Large => "Large Arena".to_string(),
        },
        width,
        depth,
        walls,
        spawn_points,
        smoke_zones,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_has_valid_geometry() {
        for size in [ArenaSize::Small, ArenaSize::Default, ArenaSize::Large] {
            let arena = generate_arena(size);
            assert!(arena.walls.len() >= 4, "Need at least boundary walls");
            assert!(!arena.spawn_points.is_empty(), "Need spawn points");

            // All spawn points inside bounds
            for sp in &arena.spawn_points {
                assert!(sp.x > 0.0 && sp.x < arena.width);
                assert!(sp.z > 0.0 && sp.z < arena.depth);
            }
        }
    }

    #[test]
    fn load_from_missing_file_returns_none() {
        assert!(load_arena_from_file("/nonexistent/path/arena.json").is_none());
    }

    #[test]
    fn json_roundtrip_preserves_arena() {
        for size in [ArenaSize::Small, ArenaSize::Default, ArenaSize::Large] {
            let arena = generate_arena(size);
            let json = serde_json::to_string(&arena).unwrap();
            let loaded: Arena = serde_json::from_str(&json).unwrap();
            assert_eq!(arena.name, loaded.name);
            assert_eq!(arena.walls.len(), loaded.walls.len());
            assert_eq!(arena.spawn_points.len(), loaded.spawn_points.len());
            assert_eq!(arena.smoke_zones.len(), loaded.smoke_zones.len());
            assert!((arena.width - loaded.width).abs() < f32::EPSILON);
            assert!((arena.depth - loaded.depth).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn load_arena_falls_back_to_generated() {
        // Point at a nonexistent directory so no JSON files are found.
        unsafe {
            std::env::set_var("BREAKPOINT_ARENAS_DIR", "/nonexistent/arenas/dir");
        }
        let arena = load_arena(ArenaSize::Default);
        let generated = generate_arena(ArenaSize::Default);
        assert_eq!(arena.name, generated.name);
        assert_eq!(arena.walls.len(), generated.walls.len());
        // Restore to avoid affecting other tests
        unsafe {
            std::env::remove_var("BREAKPOINT_ARENAS_DIR");
        }
    }
}
