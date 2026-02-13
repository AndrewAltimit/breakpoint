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

/// Helper: create the 4 boundary walls for a course of given dimensions.
fn boundary_walls(w: f32, d: f32, h: f32) -> Vec<Wall> {
    vec![
        Wall {
            a: Vec3::new(0.0, 0.0, 0.0),
            b: Vec3::new(w, 0.0, 0.0),
            height: h,
        },
        Wall {
            a: Vec3::new(w, 0.0, 0.0),
            b: Vec3::new(w, 0.0, d),
            height: h,
        },
        Wall {
            a: Vec3::new(w, 0.0, d),
            b: Vec3::new(0.0, 0.0, d),
            height: h,
        },
        Wall {
            a: Vec3::new(0.0, 0.0, d),
            b: Vec3::new(0.0, 0.0, 0.0),
            height: h,
        },
    ]
}

/// Hole 2: Gentle Straight — straight shot, no obstacles.
fn gentle_straight() -> Course {
    let w = 12.0;
    let d = 24.0;
    Course {
        name: "Gentle Straight".to_string(),
        width: w,
        depth: d,
        par: 2,
        spawn_point: Vec3::new(w / 2.0, 0.0, 3.0),
        hole_position: Vec3::new(w / 2.0, 0.0, 21.0),
        walls: boundary_walls(w, d, 1.0),
        bumpers: vec![],
    }
}

/// Hole 3: The Bend — single wall forcing a bank shot.
fn the_bend() -> Course {
    let w = 16.0;
    let d = 26.0;
    let h = 1.0;
    let mut walls = boundary_walls(w, d, h);
    // Wall blocking direct path, forcing a deflection off the right wall
    walls.push(Wall {
        a: Vec3::new(4.0, 0.0, 13.0),
        b: Vec3::new(12.0, 0.0, 13.0),
        height: h,
    });
    Course {
        name: "The Bend".to_string(),
        width: w,
        depth: d,
        par: 3,
        spawn_point: Vec3::new(4.0, 0.0, 3.0),
        hole_position: Vec3::new(8.0, 0.0, 23.0),
        walls,
        bumpers: vec![Bumper {
            position: Vec3::new(13.0, 0.0, 19.0),
            radius: 1.0,
            bounce_speed: 8.0,
        }],
    }
}

/// Hole 4: Bumper Alley — narrow channel with bumpers.
fn bumper_alley() -> Course {
    let w = 14.0;
    let d = 28.0;
    let h = 1.0;
    let mut walls = boundary_walls(w, d, h);
    // Narrow channel walls
    walls.push(Wall {
        a: Vec3::new(3.0, 0.0, 8.0),
        b: Vec3::new(3.0, 0.0, 20.0),
        height: h,
    });
    walls.push(Wall {
        a: Vec3::new(11.0, 0.0, 8.0),
        b: Vec3::new(11.0, 0.0, 20.0),
        height: h,
    });
    Course {
        name: "Bumper Alley".to_string(),
        width: w,
        depth: d,
        par: 3,
        spawn_point: Vec3::new(7.0, 0.0, 3.0),
        hole_position: Vec3::new(7.0, 0.0, 25.0),
        walls,
        bumpers: vec![
            Bumper {
                position: Vec3::new(5.0, 0.0, 11.0),
                radius: 0.8,
                bounce_speed: 7.0,
            },
            Bumper {
                position: Vec3::new(9.0, 0.0, 14.0),
                radius: 0.8,
                bounce_speed: 7.0,
            },
            Bumper {
                position: Vec3::new(5.0, 0.0, 17.0),
                radius: 0.8,
                bounce_speed: 7.0,
            },
            Bumper {
                position: Vec3::new(9.0, 0.0, 20.0),
                radius: 0.8,
                bounce_speed: 7.0,
            },
        ],
    }
}

/// Hole 5: Dogleg — 90-degree L-shaped path.
fn dogleg() -> Course {
    let w = 22.0;
    let d = 22.0;
    let h = 1.0;
    let mut walls = boundary_walls(w, d, h);
    // Inner corner walls creating an L-shaped path
    walls.push(Wall {
        a: Vec3::new(10.0, 0.0, 0.0),
        b: Vec3::new(10.0, 0.0, 12.0),
        height: h,
    });
    walls.push(Wall {
        a: Vec3::new(10.0, 0.0, 12.0),
        b: Vec3::new(22.0, 0.0, 12.0),
        height: h,
    });
    Course {
        name: "Dogleg".to_string(),
        width: w,
        depth: d,
        par: 3,
        spawn_point: Vec3::new(5.0, 0.0, 3.0),
        hole_position: Vec3::new(16.0, 0.0, 18.0),
        walls,
        bumpers: vec![Bumper {
            position: Vec3::new(5.0, 0.0, 18.0),
            radius: 1.2,
            bounce_speed: 8.0,
        }],
    }
}

/// Hole 6: The Funnel — converging walls near hole.
fn the_funnel() -> Course {
    let w = 18.0;
    let d = 26.0;
    let h = 1.0;
    let mut walls = boundary_walls(w, d, h);
    // Left converging wall
    walls.push(Wall {
        a: Vec3::new(2.0, 0.0, 16.0),
        b: Vec3::new(7.0, 0.0, 23.0),
        height: h,
    });
    // Right converging wall
    walls.push(Wall {
        a: Vec3::new(16.0, 0.0, 16.0),
        b: Vec3::new(11.0, 0.0, 23.0),
        height: h,
    });
    Course {
        name: "The Funnel".to_string(),
        width: w,
        depth: d,
        par: 3,
        spawn_point: Vec3::new(9.0, 0.0, 3.0),
        hole_position: Vec3::new(9.0, 0.0, 24.0),
        walls,
        bumpers: vec![
            Bumper {
                position: Vec3::new(5.0, 0.0, 10.0),
                radius: 1.0,
                bounce_speed: 8.0,
            },
            Bumper {
                position: Vec3::new(13.0, 0.0, 10.0),
                radius: 1.0,
                bounce_speed: 8.0,
            },
        ],
    }
}

/// Hole 7: Pinball — few walls, many bumpers.
fn pinball() -> Course {
    let w = 20.0;
    let d = 30.0;
    let h = 1.0;
    let mut walls = boundary_walls(w, d, h);
    // A couple small walls to add structure
    walls.push(Wall {
        a: Vec3::new(3.0, 0.0, 10.0),
        b: Vec3::new(8.0, 0.0, 10.0),
        height: h,
    });
    walls.push(Wall {
        a: Vec3::new(12.0, 0.0, 20.0),
        b: Vec3::new(17.0, 0.0, 20.0),
        height: h,
    });
    Course {
        name: "Pinball".to_string(),
        width: w,
        depth: d,
        par: 4,
        spawn_point: Vec3::new(10.0, 0.0, 3.0),
        hole_position: Vec3::new(10.0, 0.0, 27.0),
        walls,
        bumpers: vec![
            Bumper {
                position: Vec3::new(6.0, 0.0, 7.0),
                radius: 1.0,
                bounce_speed: 9.0,
            },
            Bumper {
                position: Vec3::new(14.0, 0.0, 7.0),
                radius: 1.0,
                bounce_speed: 9.0,
            },
            Bumper {
                position: Vec3::new(10.0, 0.0, 13.0),
                radius: 1.2,
                bounce_speed: 10.0,
            },
            Bumper {
                position: Vec3::new(5.0, 0.0, 18.0),
                radius: 0.9,
                bounce_speed: 8.0,
            },
            Bumper {
                position: Vec3::new(15.0, 0.0, 24.0),
                radius: 1.0,
                bounce_speed: 9.0,
            },
            Bumper {
                position: Vec3::new(10.0, 0.0, 22.0),
                radius: 0.8,
                bounce_speed: 8.0,
            },
        ],
    }
}

/// Hole 8: Zigzag — alternating walls creating an S-path.
fn zigzag() -> Course {
    let w = 16.0;
    let d = 32.0;
    let h = 1.0;
    let mut walls = boundary_walls(w, d, h);
    // Alternating walls from left and right
    walls.push(Wall {
        a: Vec3::new(0.0, 0.0, 8.0),
        b: Vec3::new(11.0, 0.0, 8.0),
        height: h,
    });
    walls.push(Wall {
        a: Vec3::new(5.0, 0.0, 16.0),
        b: Vec3::new(16.0, 0.0, 16.0),
        height: h,
    });
    walls.push(Wall {
        a: Vec3::new(0.0, 0.0, 24.0),
        b: Vec3::new(11.0, 0.0, 24.0),
        height: h,
    });
    Course {
        name: "Zigzag".to_string(),
        width: w,
        depth: d,
        par: 4,
        spawn_point: Vec3::new(13.0, 0.0, 3.0),
        hole_position: Vec3::new(13.0, 0.0, 29.0),
        walls,
        bumpers: vec![
            Bumper {
                position: Vec3::new(13.0, 0.0, 12.0),
                radius: 1.0,
                bounce_speed: 8.0,
            },
            Bumper {
                position: Vec3::new(3.0, 0.0, 20.0),
                radius: 1.0,
                bounce_speed: 8.0,
            },
        ],
    }
}

/// Hole 9: Fortress — complex wall maze with multiple passages.
fn fortress() -> Course {
    let w = 24.0;
    let d = 34.0;
    let h = 1.0;
    let mut walls = boundary_walls(w, d, h);
    // Outer barrier with gaps
    walls.push(Wall {
        a: Vec3::new(4.0, 0.0, 10.0),
        b: Vec3::new(10.0, 0.0, 10.0),
        height: h,
    });
    walls.push(Wall {
        a: Vec3::new(14.0, 0.0, 10.0),
        b: Vec3::new(20.0, 0.0, 10.0),
        height: h,
    });
    // Middle barrier
    walls.push(Wall {
        a: Vec3::new(8.0, 0.0, 18.0),
        b: Vec3::new(8.0, 0.0, 26.0),
        height: h,
    });
    walls.push(Wall {
        a: Vec3::new(16.0, 0.0, 18.0),
        b: Vec3::new(16.0, 0.0, 26.0),
        height: h,
    });
    // Inner barrier near hole
    walls.push(Wall {
        a: Vec3::new(10.0, 0.0, 26.0),
        b: Vec3::new(14.0, 0.0, 26.0),
        height: h,
    });
    Course {
        name: "Fortress".to_string(),
        width: w,
        depth: d,
        par: 4,
        spawn_point: Vec3::new(12.0, 0.0, 3.0),
        hole_position: Vec3::new(12.0, 0.0, 30.0),
        walls,
        bumpers: vec![
            Bumper {
                position: Vec3::new(12.0, 0.0, 14.0),
                radius: 1.2,
                bounce_speed: 9.0,
            },
            Bumper {
                position: Vec3::new(6.0, 0.0, 22.0),
                radius: 1.0,
                bounce_speed: 8.0,
            },
            Bumper {
                position: Vec3::new(18.0, 0.0, 22.0),
                radius: 1.0,
                bounce_speed: 8.0,
            },
        ],
    }
}

/// Returns all 9 courses in play order (index 0 = hole 1, etc.).
pub fn all_courses() -> Vec<Course> {
    vec![
        default_course(),
        gentle_straight(),
        the_bend(),
        bumper_alley(),
        dogleg(),
        the_funnel(),
        pinball(),
        zigzag(),
        fortress(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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

    #[test]
    fn all_courses_returns_nine() {
        let courses = all_courses();
        assert_eq!(courses.len(), 9);
    }

    #[test]
    fn all_courses_have_valid_geometry() {
        for (i, course) in all_courses().iter().enumerate() {
            assert!(
                course.walls.len() >= 4,
                "Hole {} ({}) should have at least 4 boundary walls, has {}",
                i + 1,
                course.name,
                course.walls.len()
            );
            assert!(
                course.spawn_point.x > 0.0 && course.spawn_point.x < course.width,
                "Hole {} ({}) spawn X out of bounds",
                i + 1,
                course.name
            );
            assert!(
                course.spawn_point.z > 0.0 && course.spawn_point.z < course.depth,
                "Hole {} ({}) spawn Z out of bounds",
                i + 1,
                course.name
            );
            assert!(
                course.hole_position.x > 0.0 && course.hole_position.x < course.width,
                "Hole {} ({}) hole X out of bounds",
                i + 1,
                course.name
            );
            assert!(
                course.hole_position.z > 0.0 && course.hole_position.z < course.depth,
                "Hole {} ({}) hole Z out of bounds",
                i + 1,
                course.name
            );
        }
    }

    #[test]
    fn all_courses_have_unique_names() {
        let courses = all_courses();
        let names: HashSet<&str> = courses.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names.len(),
            courses.len(),
            "All courses should have unique names"
        );
    }
}
