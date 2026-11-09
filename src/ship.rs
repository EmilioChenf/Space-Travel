//! Nave espacial (DeLorean): física arcade basada en delta time,
//! orientación con base ortonormal (sin bloqueo de cardán), nivelación
//! automática respecto a la superficie curva y colisiones.

use crate::math3d::{Mat4, Vec3, damp_factor};

pub const WORLD_UP: Vec3 = Vec3::new(0.0, 1.0, 0.0);
/// Escala del modelo del DeLorean en el mundo (~2.3 unidades de largo).
pub const SHIP_SCALE: f32 = 0.45;
/// Radio de colisión de la nave.
pub const SHIP_RADIUS: f32 = 1.2;
pub const BASE_SPEED: f32 = 22.0;
pub const BOOST_SPEED: f32 = 60.0;

/// Entrada de vuelo normalizada (-1..1).
#[derive(Clone, Copy, Debug, Default)]
pub struct FlightInput {
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub throttle: f32,
    pub strafe: f32,
    pub lift: f32,
    pub boost: bool,
    pub brake: bool,
}

/// Contexto del entorno que limita la nave (cercanía a planetas).
#[derive(Clone, Copy, Debug)]
pub struct FlightContext {
    /// Factor de velocidad máxima (1 en espacio abierto, menor cerca).
    pub speed_factor: f32,
    /// Dirección "arriba" local (radial del planeta cercano) o mundo.
    pub local_up: Vec3,
    /// Intensidad de la nivelación automática (0..1).
    pub level_strength: f32,
}

impl Default for FlightContext {
    fn default() -> Self {
        Self {
            speed_factor: 1.0,
            local_up: WORLD_UP,
            level_strength: 0.3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Ship {
    pub position: Vec3,
    pub velocity: Vec3,
    /// Eje +X del modelo (hacia donde apunta el morro).
    pub forward: Vec3,
    /// Eje +Y del modelo.
    pub up: Vec3,
    pub hull: f32,
    /// Tiempo restante de invulnerabilidad tras un impacto.
    pub invulnerable: f32,
    /// Último empuje aplicado (para el tamaño de las llamas).
    pub thrust_visual: f32,
}

impl Default for Ship {
    fn default() -> Self {
        Self::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0))
    }
}

impl Ship {
    pub fn new(position: Vec3, forward: Vec3) -> Self {
        let forward = forward.normalized();
        let up = WORLD_UP.reject(forward).normalized();
        let up = if up.length_squared() < 0.5 {
            forward.any_perpendicular()
        } else {
            up
        };
        Self {
            position,
            velocity: Vec3::ZERO,
            forward,
            up,
            hull: 100.0,
            invulnerable: 0.0,
            thrust_visual: 0.0,
        }
    }

    /// Eje +Z del modelo (derecha de la nave).
    pub fn right(&self) -> Vec3 {
        self.forward.cross(self.up)
    }

    pub fn speed(&self) -> f32 {
        self.velocity.length()
    }

    /// Velocidad con signo a lo largo del morro.
    pub fn forward_speed(&self) -> f32 {
        self.velocity.dot(self.forward)
    }

    pub fn rotation(&self) -> Mat4 {
        Mat4::from_basis(self.forward, self.up, self.right(), Vec3::ZERO)
    }

    /// Matriz del modelo del DeLorean (centra el modelo y aplica escala).
    pub fn model_matrix(&self) -> Mat4 {
        Mat4::translation(self.position)
            * self.rotation()
            * Mat4::uniform_scale(SHIP_SCALE)
            * Mat4::translation(Vec3::new(0.0, -1.55, 0.0))
    }

    fn orthonormalize(&mut self) {
        self.forward = self.forward.normalized();
        self.up = self.up.reject(self.forward).normalized();
        if self.up.length_squared() < 0.5 {
            self.up = self.forward.any_perpendicular();
        }
    }

    pub fn face_towards(&mut self, direction: Vec3, up_hint: Vec3) {
        self.forward = direction.normalized();
        self.up = up_hint;
        self.orthonormalize();
    }

    fn step(&mut self, input: FlightInput, ctx: FlightContext, dt: f32) {
        // --- Rotación en ejes locales ---
        let right = self.right();
        let yaw = input.yaw * 1.5 * dt;
        let pitch = input.pitch * 1.25 * dt;
        let roll = input.roll * 1.9 * dt;
        // Guiñada: el morro gira hacia la derecha alrededor del eje arriba.
        self.forward = self.forward.rotate_around(self.up, -yaw);
        // Cabeceo: el morro sube alrededor del eje derecho.
        self.forward = self.forward.rotate_around(right, pitch);
        self.up = self.up.rotate_around(right, pitch);
        // Alabeo: el techo gira alrededor del morro.
        self.up = self.up.rotate_around(self.forward, roll);
        self.orthonormalize();

        // Nivelación automática: alinea el techo con la vertical local
        // (radial del planeta) cuando no hay alabeo manual.
        if input.roll.abs() < 0.01 && ctx.level_strength > 0.0 {
            let target = ctx.local_up.reject(self.forward).normalized();
            if target.length_squared() > 0.5 {
                let bank = -input.yaw * 0.35;
                let target = target.rotate_around(self.forward, bank);
                let factor = damp_factor(2.2 * ctx.level_strength, dt);
                self.up = self.up.lerp(target, factor);
                self.orthonormalize();
            }
        }

        // --- Traslación ---
        let max_speed = if input.boost { BOOST_SPEED } else { BASE_SPEED } * ctx.speed_factor;
        let acceleration = if input.boost { 55.0 } else { 26.0 } * ctx.speed_factor.max(0.4);
        // Sin empuje adicional hacia delante si ya se alcanzó el límite.
        let throttle = if input.throttle > 0.0 && self.velocity.dot(self.forward) >= max_speed {
            0.0
        } else {
            input.throttle
        };
        let thrust = self.forward * (throttle * acceleration)
            + self.right() * (input.strafe * 16.0)
            + self.up * (input.lift * 16.0);
        self.velocity += thrust * dt;
        self.thrust_visual += (input.throttle.max(0.0) + input.boost as u8 as f32 * 0.6
            - self.thrust_visual)
            * damp_factor(6.0, dt);

        if input.brake {
            self.velocity *= (-4.5 * dt).exp();
            if self.velocity.length() < 0.05 {
                self.velocity = Vec3::ZERO;
            }
        } else {
            // Desaceleración progresiva sin empuje.
            let coasting =
                input.throttle.abs() < 0.01 && input.strafe.abs() < 0.01 && input.lift.abs() < 0.01;
            let drag = if coasting { 0.55 } else { 0.12 };
            self.velocity *= (-drag * dt).exp();
            // Amortiguar el derrape lateral (sensación arcade).
            let along = self.forward * self.velocity.dot(self.forward);
            let lateral = self.velocity - along;
            let keep = if input.strafe.abs() > 0.01 || input.lift.abs() > 0.01 {
                1.0
            } else {
                (-1.8 * dt).exp()
            };
            self.velocity = along + lateral * keep;
        }
        // Límite de velocidad suave (cerca de planetas baja progresivamente).
        let speed = self.velocity.length();
        if speed > max_speed {
            let target = max_speed + (speed - max_speed) * (-4.0 * dt).exp();
            self.velocity *= target / speed;
        }
        // No retroceder más rápido que un tercio de la velocidad base.
        let backward = self.velocity.dot(self.forward);
        if backward < -BASE_SPEED * 0.35 {
            self.velocity += self.forward * (-BASE_SPEED * 0.35 - backward);
        }
        self.position += self.velocity * dt;
        self.invulnerable = (self.invulnerable - dt).max(0.0);
    }

    /// Simula con subpasos fijos para mantener la respuesta aun con cuadros lentos.
    pub fn simulate(&mut self, input: FlightInput, ctx: FlightContext, frame_dt: f32) {
        let frame_dt = frame_dt.clamp(0.0, 0.25);
        let steps = (frame_dt / (1.0 / 120.0)).ceil().clamp(1.0, 30.0) as usize;
        let dt = frame_dt / steps as f32;
        for _ in 0..steps {
            self.step(input, ctx, dt);
        }
    }

    /// Mantiene la nave fuera de una esfera de radio `radius`. Devuelve la
    /// velocidad de impacto (componente hacia dentro) si hubo contacto.
    pub fn keep_outside(&mut self, center: Vec3, radius: f32) -> Option<f32> {
        let offset = self.position - center;
        let distance = offset.length();
        if distance >= radius || distance < 1.0e-5 {
            return None;
        }
        let normal = offset / distance;
        self.position = center + normal * radius;
        let inward = self.velocity.dot(normal);
        if inward < 0.0 {
            self.velocity -= normal * inward;
            // Pequeño rebote para que se sienta el contacto.
            self.velocity += normal * (-inward * 0.15);
            return Some(-inward);
        }
        Some(0.0)
    }

    /// Aplica daño respetando la invulnerabilidad temporal.
    pub fn damage(&mut self, amount: f32) -> bool {
        if self.invulnerable > 0.0 || amount <= 0.0 {
            return false;
        }
        self.hull = (self.hull - amount).max(0.0);
        self.invulnerable = 0.8;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throttle() -> FlightInput {
        FlightInput {
            throttle: 1.0,
            ..FlightInput::default()
        }
    }

    #[test]
    fn controls_keep_their_response_during_a_slow_frame() {
        let mut slow = Ship::default();
        slow.simulate(throttle(), FlightContext::default(), 0.2);
        let mut fast = Ship::default();
        for _ in 0..12 {
            fast.simulate(throttle(), FlightContext::default(), 0.2 / 12.0);
        }
        assert!(slow.speed() > 3.0);
        assert!(
            (slow.speed() - fast.speed()).abs() < 0.05,
            "independiente de los FPS"
        );
        assert!((slow.position - fast.position).length() < 0.05);
    }

    #[test]
    fn brake_stops_the_ship_without_reversing_it() {
        let mut ship = Ship::default();
        ship.velocity = ship.forward * 6.0;
        for _ in 0..30 {
            ship.simulate(
                FlightInput {
                    brake: true,
                    ..FlightInput::default()
                },
                FlightContext::default(),
                0.1,
            );
        }
        assert!(ship.speed() < 1.0e-3);
        assert!(ship.forward_speed() >= 0.0);
    }

    #[test]
    fn coasting_decelerates_progressively() {
        let mut ship = Ship::default();
        ship.velocity = ship.forward * 10.0;
        ship.simulate(FlightInput::default(), FlightContext::default(), 0.5);
        let after_half = ship.speed();
        ship.simulate(FlightInput::default(), FlightContext::default(), 0.5);
        assert!(after_half < 10.0 && after_half > 5.0);
        assert!(ship.speed() < after_half);
    }

    #[test]
    fn speed_limit_drops_near_planets() {
        let mut ship = Ship::default();
        let near = FlightContext {
            speed_factor: 0.25,
            ..FlightContext::default()
        };
        for _ in 0..60 {
            ship.simulate(throttle(), near, 0.1);
        }
        assert!(ship.speed() <= BASE_SPEED * 0.25 + 0.5);
    }

    #[test]
    fn yaw_pitch_and_roll_keep_an_orthonormal_basis() {
        let mut ship = Ship::default();
        let input = FlightInput {
            yaw: 1.0,
            pitch: 0.7,
            roll: -0.4,
            throttle: 1.0,
            ..FlightInput::default()
        };
        for _ in 0..200 {
            ship.simulate(input, FlightContext::default(), 0.05);
        }
        assert!((ship.forward.length() - 1.0).abs() < 1.0e-3);
        assert!((ship.up.length() - 1.0).abs() < 1.0e-3);
        assert!(ship.forward.dot(ship.up).abs() < 1.0e-3);
    }

    #[test]
    fn yaw_right_turns_toward_the_right_vector() {
        let mut ship = Ship::default();
        let right = ship.right();
        ship.simulate(
            FlightInput {
                yaw: 1.0,
                ..FlightInput::default()
            },
            FlightContext::default(),
            0.2,
        );
        assert!(ship.forward.dot(right) > 0.1);
    }

    #[test]
    fn auto_level_aligns_roof_with_local_up() {
        let mut ship = Ship::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        ship.up = ship.up.rotate_around(ship.forward, 1.0);
        let ctx = FlightContext {
            level_strength: 1.0,
            local_up: Vec3::new(0.0, 1.0, 0.0),
            ..FlightContext::default()
        };
        for _ in 0..40 {
            ship.simulate(FlightInput::default(), ctx, 0.1);
        }
        assert!(ship.up.dot(Vec3::new(0.0, 1.0, 0.0)) > 0.99);
    }

    #[test]
    fn collision_only_stops_motion_toward_a_planet() {
        let mut inward = Ship::new(Vec3::new(3.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        inward.velocity = Vec3::new(-4.0, 0.0, 0.0);
        let impact = inward.keep_outside(Vec3::ZERO, 4.0);
        assert!(impact.unwrap() > 3.9);
        assert!(inward.velocity.x >= 0.0);
        assert!((inward.position.length() - 4.0).abs() < 1.0e-4);

        let mut tangent = Ship::new(Vec3::new(3.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
        tangent.velocity = Vec3::new(0.0, 0.0, 4.0);
        tangent.keep_outside(Vec3::ZERO, 4.0);
        assert!((tangent.velocity.z - 4.0).abs() < 1.0e-4);
    }

    #[test]
    fn damage_respects_invulnerability() {
        let mut ship = Ship::default();
        assert!(ship.damage(20.0));
        assert!(!ship.damage(20.0));
        assert_eq!(ship.hull, 80.0);
    }
}
