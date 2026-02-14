use bevy::prelude::*;

use crate::game::GameEntity;

/// Marker for sink celebration particles.
#[derive(Component)]
pub struct SinkParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

/// Spawn a burst of particles at the given position with the given color.
pub fn spawn_sink_particles(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    position: Vec3,
    color: Color,
) {
    let particle_mesh = meshes.add(Sphere::new(0.08));
    let count = 12 + (fastrand::u32(0..5)) as usize;

    for _ in 0..count {
        let angle = fastrand::f32() * std::f32::consts::TAU;
        let speed = 2.0 + fastrand::f32() * 4.0;
        let up_speed = 3.0 + fastrand::f32() * 3.0;
        let lifetime = 0.4 + fastrand::f32() * 0.3;

        let velocity = Vec3::new(angle.cos() * speed, up_speed, angle.sin() * speed);

        let particle_mat = materials.add(StandardMaterial {
            base_color: color,
            emissive: color.to_linear() * 2.0,
            unlit: true,
            ..default()
        });

        commands.spawn((
            GameEntity,
            SinkParticle {
                velocity,
                lifetime,
                max_lifetime: lifetime,
            },
            Mesh3d(particle_mesh.clone()),
            MeshMaterial3d(particle_mat),
            Transform::from_translation(position),
        ));
    }
}

/// Update particles: apply gravity, move, shrink, despawn when expired.
pub fn sink_particle_update_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut SinkParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform) in &mut query {
        particle.velocity.y -= 15.0 * dt;
        transform.translation += particle.velocity * dt;

        particle.lifetime -= dt;
        if particle.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let progress = particle.lifetime / particle.max_lifetime;
        transform.scale = Vec3::splat(progress.max(0.01));
    }
}
