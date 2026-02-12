use serde::{Deserialize, Serialize};

/// A 3D point used for course geometry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
}

/// A wall segment on the course (two endpoints on the XZ plane + height).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Wall {
    pub a: Vec3,
    pub b: Vec3,
    pub height: f32,
}

/// A circular bumper that bounces balls away.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bumper {
    pub position: Vec3,
    pub radius: f32,
    pub bounce_speed: f32,
}

/// A mini-golf course definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub name: String,
    pub width: f32,
    pub depth: f32,
    pub par: u8,
    pub spawn_point: Vec3,
    pub hole_position: Vec3,
    pub walls: Vec<Wall>,
    pub bumpers: Vec<Bumper>,
}

/// Create the default mini-golf course.
///
/// Layout: 20x30 rectangular course on the XZ plane.
/// Spawn at bottom-center, hole near top. L-shaped obstacle in the middle,
/// two bumpers to add variety.
pub fn default_course() -> Course {
    let w = 20.0_f32;
    let d = 30.0_f32;
    let wall_h = 1.0;

    // Boundary walls (counter-clockwise)
    let boundary = vec![
        // Bottom
        Wall {
            a: Vec3::new(0.0, 0.0, 0.0),
            b: Vec3::new(w, 0.0, 0.0),
            height: wall_h,
        },
        // Right
        Wall {
            a: Vec3::new(w, 0.0, 0.0),
            b: Vec3::new(w, 0.0, d),
            height: wall_h,
        },
        // Top
        Wall {
            a: Vec3::new(w, 0.0, d),
            b: Vec3::new(0.0, 0.0, d),
            height: wall_h,
        },
        // Left
        Wall {
            a: Vec3::new(0.0, 0.0, d),
            b: Vec3::new(0.0, 0.0, 0.0),
            height: wall_h,
        },
    ];

    // L-shaped obstacle in the middle of the course
    let obstacle = vec![
        // Horizontal part of L (runs left-to-right at z=15)
        Wall {
            a: Vec3::new(5.0, 0.0, 15.0),
            b: Vec3::new(14.0, 0.0, 15.0),
            height: wall_h,
        },
        // Vertical part of L (runs bottom-to-top at x=14)
        Wall {
            a: Vec3::new(14.0, 0.0, 15.0),
            b: Vec3::new(14.0, 0.0, 22.0),
            height: wall_h,
        },
    ];

    let mut walls = boundary;
    walls.extend(obstacle);

    let bumpers = vec![
        Bumper {
            position: Vec3::new(7.0, 0.0, 10.0),
            radius: 1.0,
            bounce_speed: 8.0,
        },
        Bumper {
            position: Vec3::new(16.0, 0.0, 20.0),
            radius: 1.2,
            bounce_speed: 9.0,
        },
    ];

    Course {
        name: "Starter Course".to_string(),
        width: w,
        depth: d,
        par: 3,
        spawn_point: Vec3::new(w / 2.0, 0.0, 3.0),
        hole_position: Vec3::new(w / 2.0, 0.0, 27.0),
        walls,
        bumpers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_course_has_valid_geometry() {
        let course = default_course();
        assert_eq!(course.par, 3);
        assert!(
            course.walls.len() >= 4,
            "Should have at least boundary walls"
        );
        assert_eq!(course.bumpers.len(), 2);
        // Spawn and hole are inside the course
        assert!(course.spawn_point.x > 0.0 && course.spawn_point.x < course.width);
        assert!(course.spawn_point.z > 0.0 && course.spawn_point.z < course.depth);
        assert!(course.hole_position.x > 0.0 && course.hole_position.x < course.width);
        assert!(course.hole_position.z > 0.0 && course.hole_position.z < course.depth);
    }
}
