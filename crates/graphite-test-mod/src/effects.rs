use graphite_api::commands::CommandQueue;
use std::f64::consts::{PI, TAU};

pub struct ParticleEffects;

impl ParticleEffects {
    pub fn ring(
        cmd: &mut CommandQueue,
        cx: f64,
        cy: f64,
        cz: f64,
        radius: f64,
        points: u32,
        particle: u32,
    ) {
        for i in 0..points {
            let angle = i as f64 * TAU / points as f64;
            let x = cx + angle.cos() * radius;
            let z = cz + angle.sin() * radius;
            let _ = cmd.spawn_particle(particle, x, cy, z, 1.0);
        }
    }

    pub fn helix(
        cmd: &mut CommandQueue,
        cx: f64,
        cy: f64,
        cz: f64,
        radius: f64,
        height: f64,
        points: u32,
        particle: u32,
    ) {
        for i in 0..points {
            let t = i as f64 / points as f64;
            let angle = t * TAU * 2.0;
            let y = cy + t * height;

            let x1 = cx + angle.cos() * radius;
            let z1 = cz + angle.sin() * radius;
            let _ = cmd.spawn_particle(particle, x1, y, z1, 1.0);

            let x2 = cx + (angle + PI).cos() * radius;
            let z2 = cz + (angle + PI).sin() * radius;
            let _ = cmd.spawn_particle(particle, x2, y, z2, 1.0);
        }
    }
}
