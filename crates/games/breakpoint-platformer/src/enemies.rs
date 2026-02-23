use serde::{Deserialize, Serialize};

/// Enemy type variants in the Castlevania-style platformer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnemyType {
    /// Ground patrol, 1 HP, speed 2.0.
    Skeleton,
    /// Flying sine-wave patrol, 1 HP, speed 3.0.
    Bat,
    /// Armored ground patrol, 2 HP, speed 1.5.
    Knight,
    /// Floating shooter, 1 HP, fires projectiles every 3s.
    Medusa,
    /// Phases through walls, drifts toward nearest player. 1 HP, speed 1.5.
    Ghost,
    /// Perches on walls, swoops to attack. 2 HP, speed 4.0 during swoop.
    Gargoyle,
}

/// A single enemy instance in the game world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enemy {
    pub id: u16,
    pub enemy_type: EnemyType,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub hp: u8,
    pub patrol_min_x: f32,
    pub patrol_max_x: f32,
    pub alive: bool,
    pub respawn_timer: f32,
    pub facing_right: bool,
    pub anim_time: f32,
    pub shoot_timer: f32,
}

/// Enemy respawn delay in seconds.
pub const RESPAWN_DELAY: f32 = 5.0;

impl Enemy {
    /// Create a new enemy from a spawn definition.
    pub fn from_spawn(id: u16, spawn: &EnemySpawn) -> Self {
        let hp = match spawn.enemy_type {
            EnemyType::Knight | EnemyType::Gargoyle => 2,
            _ => 1,
        };
        Self {
            id,
            enemy_type: spawn.enemy_type,
            x: spawn.x,
            y: spawn.y,
            vx: 0.0,
            vy: 0.0,
            hp,
            patrol_min_x: spawn.patrol_min_x,
            patrol_max_x: spawn.patrol_max_x,
            alive: true,
            respawn_timer: 0.0,
            facing_right: true,
            anim_time: 0.0,
            shoot_timer: 0.0,
        }
    }

    /// Reset this enemy to its spawned state (used for respawning).
    pub fn respawn(&mut self) {
        self.hp = match self.enemy_type {
            EnemyType::Knight | EnemyType::Gargoyle => 2,
            _ => 1,
        };
        self.alive = true;
        self.respawn_timer = 0.0;
        self.x = (self.patrol_min_x + self.patrol_max_x) / 2.0;
        self.vx = 0.0;
        self.vy = 0.0;
        self.facing_right = true;
        self.anim_time = 0.0;
        self.shoot_timer = 0.0;
    }
}

/// A projectile fired by an enemy (e.g. Medusa head).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyProjectile {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub lifetime: f32,
}

/// Definition of where and what type of enemy should spawn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemySpawn {
    pub x: f32,
    pub y: f32,
    pub enemy_type: EnemyType,
    pub patrol_min_x: f32,
    pub patrol_max_x: f32,
}

/// Tick a skeleton enemy: ground patrol between bounds at speed 2.0.
fn tick_skeleton(e: &mut Enemy, dt: f32) {
    let speed = 2.0;
    if e.facing_right {
        e.vx = speed;
    } else {
        e.vx = -speed;
    }
    e.x += e.vx * dt;

    // Reverse at patrol bounds
    if e.x >= e.patrol_max_x {
        e.x = e.patrol_max_x;
        e.facing_right = false;
    } else if e.x <= e.patrol_min_x {
        e.x = e.patrol_min_x;
        e.facing_right = true;
    }
}

/// Tick a bat enemy: sine-wave flight with horizontal patrol at speed 3.0.
fn tick_bat(e: &mut Enemy, dt: f32, time: f32) {
    let speed = 3.0;
    let amplitude = 2.0;
    let freq = 1.5;

    if e.facing_right {
        e.vx = speed;
    } else {
        e.vx = -speed;
    }
    e.x += e.vx * dt;

    // Sine-wave vertical movement based on global time + enemy anim_time offset
    let base_y = (e.patrol_min_x + e.patrol_max_x) / 2.0; // use midpoint as a reference
    // Store initial y in a stable way: use the average of patrol bounds as baseline
    // The bat's y oscillates around its spawn y position
    e.vy = amplitude * freq * (freq * (time + e.anim_time)).cos();
    e.y += e.vy * dt;

    // Reverse at patrol bounds
    if e.x >= e.patrol_max_x {
        e.x = e.patrol_max_x;
        e.facing_right = false;
    } else if e.x <= e.patrol_min_x {
        e.x = e.patrol_min_x;
        e.facing_right = true;
    }

    let _ = base_y;
}

/// Tick a knight enemy: armored ground patrol at speed 1.5.
fn tick_knight(e: &mut Enemy, dt: f32) {
    let speed = 1.5;
    if e.facing_right {
        e.vx = speed;
    } else {
        e.vx = -speed;
    }
    e.x += e.vx * dt;

    // Reverse at patrol bounds
    if e.x >= e.patrol_max_x {
        e.x = e.patrol_max_x;
        e.facing_right = false;
    } else if e.x <= e.patrol_min_x {
        e.x = e.patrol_min_x;
        e.facing_right = true;
    }
}

/// Tick a medusa enemy: float in place with small sine bob, shoot projectiles.
fn tick_medusa(e: &mut Enemy, dt: f32, projectiles: &mut Vec<EnemyProjectile>) {
    let bob_amplitude = 0.5;
    let bob_freq = 1.0;

    // Small vertical bob
    e.vy = bob_amplitude * bob_freq * (bob_freq * e.anim_time).cos();
    e.y += e.vy * dt;

    // Shooting logic: fire a projectile every 3.0 seconds
    e.shoot_timer += dt;
    if e.shoot_timer >= 3.0 {
        e.shoot_timer -= 3.0;
        // Shoot in the direction the medusa is facing
        let proj_speed = 4.0;
        let proj_vx = if e.facing_right {
            proj_speed
        } else {
            -proj_speed
        };
        projectiles.push(EnemyProjectile {
            x: e.x,
            y: e.y,
            vx: proj_vx,
            vy: 0.0,
            lifetime: 4.0,
        });
        // Alternate facing direction for variety
        e.facing_right = !e.facing_right;
    }
}

/// Tick a ghost enemy: drifts toward patrol center with phase-through movement.
/// Moves in a slow sine-wave pattern, ignoring walls.
fn tick_ghost(e: &mut Enemy, dt: f32) {
    let speed = 1.5;
    let center_x = (e.patrol_min_x + e.patrol_max_x) / 2.0;
    let drift_range = (e.patrol_max_x - e.patrol_min_x) / 2.0;

    // Slow sinusoidal drift around center
    let target_x = center_x + drift_range * (e.anim_time * 0.4).sin();
    let dx = target_x - e.x;
    e.vx = dx.clamp(-speed, speed);
    e.x += e.vx * dt;

    // Gentle vertical bob
    e.vy = 0.8 * (e.anim_time * 1.2).cos();
    e.y += e.vy * dt;

    e.facing_right = e.vx > 0.0;
}

/// Tick a gargoyle enemy: perches at patrol midpoint, swoops outward periodically.
fn tick_gargoyle(e: &mut Enemy, dt: f32) {
    let center_x = (e.patrol_min_x + e.patrol_max_x) / 2.0;
    let swoop_speed = 4.0;
    let swoop_cycle = 4.0; // seconds between swoops
    let swoop_duration = 1.0;

    let cycle_t = e.anim_time % swoop_cycle;
    if cycle_t < swoop_duration {
        // Swooping phase: fly outward then return
        let t = cycle_t / swoop_duration;
        let direction = if e.facing_right { 1.0 } else { -1.0 };
        // Triangle wave: go out for first half, return for second half
        let offset = if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 };
        let range = (e.patrol_max_x - e.patrol_min_x) / 2.0;
        e.x = center_x + direction * offset * range;
        e.vx = direction * swoop_speed;
    } else {
        // Perching phase: stay at center, slowly settle
        let drift = (e.x - center_x) * 0.95;
        e.x = center_x + drift * (1.0 - 2.0 * dt).max(0.0);
        e.vx = 0.0;
        // Toggle direction for next swoop near the end
        if cycle_t > swoop_cycle - 0.1 {
            e.facing_right = !e.facing_right;
        }
    }
}

/// Tick all enemies. Dead enemies count down their respawn timer.
pub fn tick_enemies(
    enemies: &mut [Enemy],
    dt: f32,
    time: f32,
    projectiles: &mut Vec<EnemyProjectile>,
) {
    for e in enemies.iter_mut() {
        e.anim_time += dt;

        if !e.alive {
            e.respawn_timer -= dt;
            if e.respawn_timer <= 0.0 {
                e.respawn();
            }
            continue;
        }

        match e.enemy_type {
            EnemyType::Skeleton => tick_skeleton(e, dt),
            EnemyType::Bat => tick_bat(e, dt, time),
            EnemyType::Knight => tick_knight(e, dt),
            EnemyType::Medusa => tick_medusa(e, dt, projectiles),
            EnemyType::Ghost => tick_ghost(e, dt),
            EnemyType::Gargoyle => tick_gargoyle(e, dt),
        }
    }
}

/// Tick all projectiles: move them and remove expired ones.
pub fn tick_projectiles(projectiles: &mut Vec<EnemyProjectile>, dt: f32) {
    for proj in projectiles.iter_mut() {
        proj.x += proj.vx * dt;
        proj.y += proj.vy * dt;
        proj.lifetime -= dt;
    }
    projectiles.retain(|p| p.lifetime > 0.0);
}

/// Kill an enemy: set alive=false and start respawn timer.
pub fn kill_enemy(enemy: &mut Enemy) {
    enemy.alive = false;
    enemy.respawn_timer = RESPAWN_DELAY;
    enemy.vx = 0.0;
    enemy.vy = 0.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skeleton_spawn() -> EnemySpawn {
        EnemySpawn {
            x: 5.0,
            y: 2.0,
            enemy_type: EnemyType::Skeleton,
            patrol_min_x: 3.0,
            patrol_max_x: 7.0,
        }
    }

    fn make_bat_spawn() -> EnemySpawn {
        EnemySpawn {
            x: 10.0,
            y: 5.0,
            enemy_type: EnemyType::Bat,
            patrol_min_x: 8.0,
            patrol_max_x: 12.0,
        }
    }

    fn make_knight_spawn() -> EnemySpawn {
        EnemySpawn {
            x: 15.0,
            y: 2.0,
            enemy_type: EnemyType::Knight,
            patrol_min_x: 13.0,
            patrol_max_x: 17.0,
        }
    }

    fn make_medusa_spawn() -> EnemySpawn {
        EnemySpawn {
            x: 20.0,
            y: 6.0,
            enemy_type: EnemyType::Medusa,
            patrol_min_x: 18.0,
            patrol_max_x: 22.0,
        }
    }

    #[test]
    fn skeleton_patrols_between_bounds() {
        let spawn = make_skeleton_spawn();
        let mut enemy = Enemy::from_spawn(0, &spawn);
        enemy.x = spawn.patrol_min_x;
        enemy.facing_right = true;

        let mut projectiles = Vec::new();
        // Tick forward for a while
        for _ in 0..100 {
            tick_enemies(
                std::slice::from_mut(&mut enemy),
                0.05,
                0.0,
                &mut projectiles,
            );
        }

        // Should have bounced and be within bounds
        assert!(
            enemy.x >= spawn.patrol_min_x && enemy.x <= spawn.patrol_max_x,
            "Skeleton should stay within patrol bounds: x={}",
            enemy.x,
        );
    }

    #[test]
    fn bat_has_vertical_movement() {
        let spawn = make_bat_spawn();
        let mut enemy = Enemy::from_spawn(1, &spawn);
        let initial_y = enemy.y;

        let mut projectiles = Vec::new();
        // Tick and check that y changes (sine wave)
        let mut y_changed = false;
        for i in 0..60 {
            let time = i as f32 * 0.05;
            tick_enemies(
                std::slice::from_mut(&mut enemy),
                0.05,
                time,
                &mut projectiles,
            );
            if (enemy.y - initial_y).abs() > 0.01 {
                y_changed = true;
            }
        }

        assert!(y_changed, "Bat should have vertical sine-wave movement");
    }

    #[test]
    fn knight_has_2_hp() {
        let spawn = make_knight_spawn();
        let enemy = Enemy::from_spawn(2, &spawn);
        assert_eq!(enemy.hp, 2, "Knight should have 2 HP");
    }

    #[test]
    fn medusa_fires_projectiles() {
        let spawn = make_medusa_spawn();
        let mut enemy = Enemy::from_spawn(3, &spawn);
        let mut projectiles = Vec::new();

        // Tick for 3+ seconds to trigger a shot
        for i in 0..70 {
            let time = i as f32 * 0.05;
            tick_enemies(
                std::slice::from_mut(&mut enemy),
                0.05,
                time,
                &mut projectiles,
            );
        }

        assert!(
            !projectiles.is_empty(),
            "Medusa should have fired at least one projectile"
        );
    }

    #[test]
    fn projectile_tick_removes_expired() {
        let mut projectiles = vec![
            EnemyProjectile {
                x: 0.0,
                y: 0.0,
                vx: 1.0,
                vy: 0.0,
                lifetime: 0.5,
            },
            EnemyProjectile {
                x: 0.0,
                y: 0.0,
                vx: -1.0,
                vy: 0.0,
                lifetime: 3.0,
            },
        ];

        tick_projectiles(&mut projectiles, 1.0);

        assert_eq!(projectiles.len(), 1, "Expired projectile should be removed");
        assert!(
            projectiles[0].lifetime > 0.0,
            "Remaining projectile should still be alive"
        );
    }

    #[test]
    fn projectile_moves() {
        let mut projectiles = vec![EnemyProjectile {
            x: 5.0,
            y: 3.0,
            vx: 2.0,
            vy: 1.0,
            lifetime: 5.0,
        }];

        tick_projectiles(&mut projectiles, 0.5);

        assert!((projectiles[0].x - 6.0).abs() < 0.001);
        assert!((projectiles[0].y - 3.5).abs() < 0.001);
    }

    #[test]
    fn dead_enemy_respawns_after_delay() {
        let spawn = make_skeleton_spawn();
        let mut enemy = Enemy::from_spawn(0, &spawn);
        kill_enemy(&mut enemy);
        assert!(!enemy.alive, "Enemy should be dead after kill");
        assert!(
            (enemy.respawn_timer - RESPAWN_DELAY).abs() < 0.01,
            "Respawn timer should be set to {}",
            RESPAWN_DELAY,
        );

        let mut projectiles = Vec::new();
        // Tick past the respawn delay
        for _ in 0..120 {
            tick_enemies(
                std::slice::from_mut(&mut enemy),
                0.05,
                0.0,
                &mut projectiles,
            );
        }

        assert!(enemy.alive, "Enemy should respawn after delay");
        assert_eq!(enemy.hp, 1, "Skeleton should have 1 HP after respawning");
    }

    #[test]
    fn knight_respawns_with_2_hp() {
        let spawn = make_knight_spawn();
        let mut enemy = Enemy::from_spawn(2, &spawn);
        kill_enemy(&mut enemy);

        let mut projectiles = Vec::new();
        for _ in 0..120 {
            tick_enemies(
                std::slice::from_mut(&mut enemy),
                0.05,
                0.0,
                &mut projectiles,
            );
        }

        assert!(enemy.alive, "Knight should respawn after delay");
        assert_eq!(enemy.hp, 2, "Knight should respawn with 2 HP");
    }

    #[test]
    fn enemy_from_spawn_initializes_correctly() {
        let spawn = make_skeleton_spawn();
        let enemy = Enemy::from_spawn(42, &spawn);
        assert_eq!(enemy.id, 42);
        assert_eq!(enemy.enemy_type, EnemyType::Skeleton);
        assert!((enemy.x - spawn.x).abs() < 0.001);
        assert!((enemy.y - spawn.y).abs() < 0.001);
        assert!(enemy.alive);
        assert_eq!(enemy.hp, 1);
    }
}
