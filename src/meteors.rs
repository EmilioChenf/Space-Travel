//! Lluvia de meteoritos procedural con semilla configurable y cinturón de
//! asteroides. Comparte las mallas OBJ, el descarte del renderizador y las
//! colisiones esféricas con la nave.

use crate::assets::Assets;
use crate::math3d::{Mat4, PI, Rng, Vec3};
use crate::model::Color3;
use crate::particles::{Particle, ParticleSystem};
use crate::renderer::{DrawOptions, ShaderKind, SoftwareRenderer};

#[derive(Clone, Debug)]
pub struct Meteor {
    pub position: Vec3,
    pub velocity: Vec3,
    pub axis: Vec3,
    pub angle: f32,
    pub spin: f32,
    pub radius: f32,
    pub variant: usize,
    pub age: f32,
}

/// Evento producido por la lluvia de meteoritos en un cuadro.
#[derive(Clone, Copy, Debug, Default)]
pub struct MeteorEvents {
    pub ship_hits: u32,
    pub damage: f32,
}

pub struct MeteorShower {
    pub meteors: Vec<Meteor>,
    pub rng: Rng,
    pub seed: u32,
    pub max_meteors: usize,
    pub spawn_timer: f32,
    /// Ciclo de la lluvia: tiempo dentro del ciclo actual.
    pub cycle_time: f32,
    pub enabled: bool,
}

/// Duración del ciclo (calma + lluvia) en segundos.
pub const SHOWER_CYCLE: f32 = 70.0;
/// Segundos de lluvia intensa al final de cada ciclo.
pub const SHOWER_DURATION: f32 = 22.0;

impl MeteorShower {
    pub fn new(seed: u32) -> Self {
        Self {
            meteors: Vec::new(),
            rng: Rng::new(seed),
            seed,
            max_meteors: 28,
            spawn_timer: 0.0,
            cycle_time: 0.0,
            enabled: true,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.seed);
    }

    /// ¿Hay lluvia intensa activa ahora?
    pub fn storm_active(&self) -> bool {
        self.cycle_time > SHOWER_CYCLE - SHOWER_DURATION
    }

    pub fn seconds_to_storm(&self) -> f32 {
        (SHOWER_CYCLE - SHOWER_DURATION - self.cycle_time).max(0.0)
    }

    fn spawn(&mut self, around: Vec3, heading: Vec3) {
        // Aparecen delante de la nave a 90-160 unidades y cruzan cerca de ella.
        let mut dir = self.rng.unit_vector();
        if dir.dot(heading) < 0.0 {
            dir = dir - heading * (2.0 * dir.dot(heading));
        }
        let distance = self.rng.range(90.0, 160.0);
        let start = around + dir * distance;
        let aim = around + self.rng.unit_vector() * self.rng.range(4.0, 30.0);
        let speed = self.rng.range(14.0, 34.0);
        let variant = self.rng.index(3);
        let radius = self.rng.range(0.7, 2.2);
        self.meteors.push(Meteor {
            position: start,
            velocity: (aim - start).normalized() * speed,
            axis: self.rng.unit_vector(),
            angle: 0.0,
            spin: self.rng.range(-2.0, 2.0),
            radius,
            variant,
            age: 0.0,
        });
    }

    /// Actualiza trayectorias, genera nuevos meteoritos y detecta impactos.
    /// `obstacles` son esferas (centro, radio) de planetas y sol.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        dt: f32,
        ship_position: Vec3,
        ship_heading: Vec3,
        ship_radius: f32,
        obstacles: &[(Vec3, f32)],
        particles: &mut ParticleSystem,
        safe_zone: bool,
    ) -> MeteorEvents {
        let mut events = MeteorEvents::default();
        self.cycle_time = (self.cycle_time + dt) % SHOWER_CYCLE;
        if self.enabled && !safe_zone {
            let rate = if self.storm_active() { 3.2 } else { 0.35 };
            self.spawn_timer += dt * rate;
            while self.spawn_timer >= 1.0 {
                self.spawn_timer -= 1.0;
                if self.meteors.len() < self.max_meteors {
                    self.spawn(ship_position, ship_heading);
                }
            }
        }
        let mut i = 0;
        while i < self.meteors.len() {
            let m = &mut self.meteors[i];
            m.age += dt;
            m.position += m.velocity * dt;
            m.angle += m.spin * dt;
            // Estela de fuego.
            if particles.rng.next_f32() < 0.6 {
                let back = -m.velocity.normalized();
                let jitter = particles.rng.unit_vector() * 0.6;
                particles.spawn(Particle {
                    position: m.position + back * m.radius * 0.8,
                    velocity: back * 3.0 + jitter,
                    life: 0.9,
                    max_life: 0.9,
                    size: m.radius * 0.55,
                    size_end: m.radius * 0.1,
                    color: Color3::new(1.0, 0.55, 0.15),
                    color_end: Color3::new(0.25, 0.05, 0.02),
                    additive: true,
                    drag: 0.5,
                });
            }
            let mut destroyed = false;
            if (m.position - ship_position).length() < m.radius + ship_radius {
                events.ship_hits += 1;
                events.damage += 8.0 + m.radius * 6.0;
                destroyed = true;
            }
            for (center, radius) in obstacles {
                if (m.position - *center).length() < m.radius + radius {
                    destroyed = true;
                }
            }
            let far = (m.position - ship_position).length() > 260.0 || m.age > 25.0;
            if destroyed {
                let p = m.position;
                let size = m.radius;
                particles.burst(
                    p,
                    26,
                    12.0,
                    size * 0.5,
                    Color3::new(1.0, 0.8, 0.4),
                    Color3::new(0.4, 0.1, 0.02),
                    0.9,
                );
                self.meteors.swap_remove(i);
            } else if far {
                self.meteors.swap_remove(i);
            } else {
                i += 1;
            }
        }
        events
    }

    pub fn render(&self, renderer: &mut SoftwareRenderer, assets: &Assets) {
        for m in &self.meteors {
            let model = Mat4::translation(m.position)
                * Mat4::rotation_axis(m.axis, m.angle)
                * Mat4::uniform_scale(m.radius);
            let options = DrawOptions::new(ShaderKind::Meteor).param(m.velocity);
            renderer.draw_lod(&assets.meteors[m.variant], model, &options);
        }
    }

    /// Desplaza meteoritos cuando el marco de referencia se mueve.
    pub fn shift(&mut self, offset: Vec3) {
        for m in &mut self.meteors {
            m.position += offset;
        }
    }
}

/// Cinturón de asteroides entre las órbitas de Dulces y Escandalosos.
pub struct AsteroidBelt {
    pub asteroids: Vec<(f32, f32, f32, f32, Vec3, usize, f32)>, // radio, ángulo, altura, tamaño, eje, variante, giro
}

pub const BELT_RADIUS: f32 = 365.0;

impl AsteroidBelt {
    pub fn new(seed: u32, count: usize) -> Self {
        let mut rng = Rng::new(seed);
        let asteroids = (0..count)
            .map(|_| {
                (
                    BELT_RADIUS + rng.range(-14.0, 14.0),
                    rng.range(0.0, 2.0 * PI),
                    rng.range(-6.0, 6.0),
                    rng.range(1.2, 4.0),
                    rng.unit_vector(),
                    rng.index(3),
                    rng.range(-0.6, 0.6),
                )
            })
            .collect();
        Self { asteroids }
    }

    fn position(entry: &(f32, f32, f32, f32, Vec3, usize, f32), time: f32) -> Vec3 {
        let (radius, angle, height, ..) = *entry;
        let a = angle + time * (2.2 / radius.powf(1.1));
        Vec3::new(a.cos() * radius, height, a.sin() * radius)
    }

    /// Esferas de colisión (posición, radio) en el instante `time`.
    pub fn colliders(&self, time: f32) -> impl Iterator<Item = (Vec3, f32)> + '_ {
        self.asteroids
            .iter()
            .map(move |e| (Self::position(e, time), e.3 * 0.9))
    }

    pub fn render(&self, renderer: &mut SoftwareRenderer, assets: &Assets, time: f32) {
        for e in &self.asteroids {
            let position = Self::position(e, time);
            let model = Mat4::translation(position)
                * Mat4::rotation_axis(e.4, time * e.6)
                * Mat4::uniform_scale(e.3);
            // Distancia de corte: los asteroides diminutos en pantalla no se dibujan.
            if renderer.projected_radius(position, e.3) < 0.8 {
                continue;
            }
            renderer.draw_lod(
                &assets.asteroids[e.5],
                model,
                &DrawOptions::new(ShaderKind::Rock),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showers_are_reproducible_with_the_same_seed() {
        let run = |seed| {
            let mut shower = MeteorShower::new(seed);
            shower.cycle_time = SHOWER_CYCLE - SHOWER_DURATION + 1.0;
            let mut particles = ParticleSystem::new(100, 1);
            for _ in 0..60 {
                shower.update(
                    0.1,
                    Vec3::ZERO,
                    Vec3::new(1.0, 0.0, 0.0),
                    1.0,
                    &[],
                    &mut particles,
                    false,
                );
            }
            shower
                .meteors
                .iter()
                .map(|m| m.position)
                .collect::<Vec<_>>()
        };
        let a = run(77);
        let b = run(77);
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((*x - *y).length() < 1.0e-4);
        }
        let c = run(78);
        assert!(a.len() != c.len() || a.iter().zip(&c).any(|(x, y)| (*x - *y).length() > 1.0));
    }

    #[test]
    fn meteor_hitting_the_ship_causes_damage_and_disappears() {
        let mut shower = MeteorShower::new(1);
        shower.enabled = false;
        shower.meteors.push(Meteor {
            position: Vec3::new(3.0, 0.0, 0.0),
            velocity: Vec3::new(-30.0, 0.0, 0.0),
            axis: Vec3::new(0.0, 1.0, 0.0),
            angle: 0.0,
            spin: 0.0,
            radius: 1.0,
            variant: 0,
            age: 0.0,
        });
        let mut particles = ParticleSystem::new(100, 1);
        let mut total = MeteorEvents::default();
        for _ in 0..10 {
            let e = shower.update(
                0.016,
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
                1.2,
                &[],
                &mut particles,
                false,
            );
            total.ship_hits += e.ship_hits;
            total.damage += e.damage;
        }
        assert_eq!(total.ship_hits, 1);
        assert!(total.damage > 0.0);
        assert!(shower.meteors.is_empty());
        assert!(
            !particles.particles.is_empty(),
            "debe haber efecto de impacto"
        );
    }

    #[test]
    fn meteor_count_is_bounded() {
        let mut shower = MeteorShower::new(5);
        shower.cycle_time = SHOWER_CYCLE - SHOWER_DURATION;
        let mut particles = ParticleSystem::new(10, 1);
        for _ in 0..400 {
            shower.update(
                0.1,
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
                0.1,
                &[],
                &mut particles,
                false,
            );
            assert!(shower.meteors.len() <= shower.max_meteors);
        }
    }
}
