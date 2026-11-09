//! Sistema de partículas simple con presupuesto fijo (humo, fuego,
//! chispas de impacto, nieve, arena, brasas, estelas de warp).

use crate::math3d::{Rng, Vec3};
use crate::model::Color3;
use crate::renderer::SoftwareRenderer;

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    pub size_end: f32,
    pub color: Color3,
    pub color_end: Color3,
    pub additive: bool,
    pub drag: f32,
}

pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pub budget: usize,
    pub rng: Rng,
}

impl ParticleSystem {
    pub fn new(budget: usize, seed: u32) -> Self {
        Self {
            particles: Vec::with_capacity(budget),
            budget,
            rng: Rng::new(seed),
        }
    }

    pub fn spawn(&mut self, particle: Particle) {
        if self.particles.len() < self.budget {
            self.particles.push(particle);
        }
    }

    /// Emite una ráfaga esférica (explosiones, impactos).
    #[allow(clippy::too_many_arguments)]
    pub fn burst(
        &mut self,
        center: Vec3,
        count: usize,
        speed: f32,
        size: f32,
        color: Color3,
        color_end: Color3,
        life: f32,
    ) {
        for _ in 0..count {
            let dir = self.rng.unit_vector();
            let v = dir * (speed * self.rng.range(0.3, 1.0));
            let l = life * self.rng.range(0.6, 1.0);
            self.spawn(Particle {
                position: center,
                velocity: v,
                life: l,
                max_life: l,
                size,
                size_end: size * 0.2,
                color,
                color_end,
                additive: true,
                drag: 1.5,
            });
        }
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.life -= dt;
            p.velocity *= (-p.drag * dt).exp();
            p.position += p.velocity * dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    /// Desplaza todas las partículas (cuando el marco de referencia se mueve).
    pub fn shift(&mut self, offset: Vec3) {
        for p in &mut self.particles {
            p.position += offset;
        }
    }

    pub fn render(&self, renderer: &mut SoftwareRenderer) {
        for p in &self.particles {
            let t = 1.0 - (p.life / p.max_life).clamp(0.0, 1.0);
            let size = p.size + (p.size_end - p.size) * t;
            let color = p.color * (1.0 - t) + p.color_end * t;
            let alpha = if p.additive {
                1.0 - t * t
            } else {
                (1.0 - t) * 0.8
            };
            renderer.draw_particle(p.position, size, color, alpha, p.additive);
        }
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_expire_and_respect_budget() {
        let mut system = ParticleSystem::new(10, 1);
        system.burst(
            Vec3::ZERO,
            50,
            5.0,
            0.2,
            Color3::new(1.0, 1.0, 1.0),
            Color3::new(0.0, 0.0, 0.0),
            1.0,
        );
        assert_eq!(system.particles.len(), 10);
        system.update(0.5);
        assert!(system.particles.iter().all(|p| p.position.length() > 0.0));
        system.update(1.0);
        assert!(system.particles.is_empty());
    }
}
