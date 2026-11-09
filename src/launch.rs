//! Secuencia cinemática de despegue: cuenta regresiva, encendido, ascenso,
//! transición al espacio, apertura de la cofia, despliegue del DeLorean y
//! entrega del control. Todo depende del tiempo transcurrido (delta time).

use crate::math3d::{Mat4, Vec3, smoothstep};
use crate::procgen::align_y;
use crate::renderer::Camera;
use crate::solar::{PlanetId, planet_model, planet_position, planet_rotation};
use crate::worlds::{EARTH_RADIUS, LAUNCH_DIR, ROCKET_FAIRING_BASE, earth_height};

pub const T_IGNITION: f32 = 3.5;
pub const T_LIFTOFF: f32 = 5.0;
pub const T_FADE_OUT: f32 = 10.6;
pub const T_SPACE: f32 = 11.5;
pub const T_FAIRING: f32 = 13.0;
pub const T_EMERGE: f32 = 14.6;
pub const T_TURN: f32 = 16.6;
pub const T_HANDOVER: f32 = 18.2;
pub const T_END: f32 = 19.6;

/// Escala del cohete en tierra (unidades del modelo de la Tierra por unidad
/// del cohete) y en el espacio (unidades del mundo por unidad del cohete).
pub const GROUND_SCALE: f32 = 0.0625;
pub const SPACE_SCALE: f32 = 1.8;
const ROCKET_RADIUS: f32 = 0.62;
const ASCENT_ACCELERATION: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchPhase {
    Countdown,
    Ignition,
    Ascent,
    Transition,
    Separation,
    Deploy,
    Handover,
    Finished,
}

impl LaunchPhase {
    pub fn label(self) -> &'static str {
        match self {
            LaunchPhase::Countdown => "Cuenta regresiva",
            LaunchPhase::Ignition => "Encendido de motores",
            LaunchPhase::Ascent => "Despegue",
            LaunchPhase::Transition => "Saliendo de la atmosfera",
            LaunchPhase::Separation => "Apertura de la cofia",
            LaunchPhase::Deploy => "Despliegue del DeLorean",
            LaunchPhase::Handover => "Control transferido",
            LaunchPhase::Finished => "En vuelo",
        }
    }
}

/// Poses calculadas para un instante de la cinemática.
#[derive(Clone, Copy, Debug)]
pub struct LaunchFrame {
    pub phase: LaunchPhase,
    pub on_ground: bool,
    pub rocket: Mat4,
    pub fairing_left: Mat4,
    pub fairing_right: Mat4,
    pub pad: Mat4,
    /// Posición, morro y techo del DeLorean (si es visible).
    pub ship: Option<(Vec3, Vec3, Vec3)>,
    pub camera: Camera,
    /// Base de la tobera y dirección del escape (para partículas).
    pub nozzle: Vec3,
    pub exhaust_dir: Vec3,
    pub thrust: f32,
    pub fade: f32,
    pub flash: f32,
    pub countdown: Option<u32>,
}

pub struct LaunchSequence {
    pub time: f32,
    /// Desplazamiento del punto de encuentro respecto al centro de la Tierra.
    space_offset: Option<Vec3>,
    space_up: Vec3,
    tangent: Vec3,
}

impl Default for LaunchSequence {
    fn default() -> Self {
        Self::new()
    }
}

/// Radio (en unidades del modelo) del suelo bajo la plataforma.
fn pad_ground_radius() -> f32 {
    EARTH_RADIUS * (1.0 + earth_height(LAUNCH_DIR))
}

impl LaunchSequence {
    pub fn new() -> Self {
        Self {
            time: 0.0,
            space_offset: None,
            space_up: Vec3::new(0.0, 1.0, 0.0),
            tangent: Vec3::new(1.0, 0.0, 0.0),
        }
    }

    pub fn finished(&self) -> bool {
        self.time >= T_END
    }

    pub fn phase(&self) -> LaunchPhase {
        let t = self.time;
        if t < T_IGNITION {
            LaunchPhase::Countdown
        } else if t < T_LIFTOFF {
            LaunchPhase::Ignition
        } else if t < T_FADE_OUT {
            LaunchPhase::Ascent
        } else if t < T_FAIRING {
            LaunchPhase::Transition
        } else if t < T_EMERGE {
            LaunchPhase::Separation
        } else if t < T_HANDOVER {
            LaunchPhase::Deploy
        } else if t < T_END {
            LaunchPhase::Handover
        } else {
            LaunchPhase::Finished
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
    }

    pub fn skip(&mut self) {
        self.time = T_END;
    }

    /// Fija el punto de encuentro en el espacio (una sola vez).
    fn ensure_space_anchor(&mut self, sim_time: f32) {
        if self.space_offset.is_none() {
            let up = planet_rotation(PlanetId::Tierra, sim_time)
                .transform_direction(LAUNCH_DIR)
                .normalized();
            self.space_up = up;
            let mut tangent = up.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
            if tangent.length_squared() < 0.5 {
                tangent = up.any_perpendicular();
            }
            self.tangent = tangent;
            self.space_offset = Some(up * (EARTH_RADIUS * 4.0 + 40.0));
        }
    }

    /// Altura del cohete sobre la plataforma en unidades del cohete.
    pub fn altitude(&self) -> f32 {
        let t = (self.time - T_LIFTOFF).max(0.0);
        0.5 * ASCENT_ACCELERATION * t * t
    }

    /// Calcula todas las poses de la cinemática para el instante actual.
    pub fn frame(&mut self, sim_time: f32) -> LaunchFrame {
        let t = self.time;
        let phase = self.phase();
        let countdown = (t < T_LIFTOFF).then(|| (T_LIFTOFF - t).ceil() as u32);
        if t < T_SPACE {
            self.ground_frame(sim_time, phase, countdown)
        } else {
            self.ensure_space_anchor(sim_time);
            self.space_frame(sim_time, phase)
        }
    }

    fn ground_frame(
        &self,
        sim_time: f32,
        phase: LaunchPhase,
        countdown: Option<u32>,
    ) -> LaunchFrame {
        let t = self.time;
        let earth = planet_model(PlanetId::Tierra, sim_time);
        let pad = earth
            * Mat4::translation(LAUNCH_DIR * (pad_ground_radius() - 0.005))
            * align_y(LAUNCH_DIR, 0.0)
            * Mat4::uniform_scale(GROUND_SCALE);
        let altitude = self.altitude();
        let shake = if t > T_IGNITION {
            (t * 53.0).sin() * 0.04 * (1.0 - (altitude / 60.0).min(1.0))
        } else {
            0.0
        };
        let rocket = pad * Mat4::translation(Vec3::new(shake, 0.55 + altitude, 0.0));
        let to_world = |p: Vec3| pad.transform_point(p);
        let rocket_center = Vec3::new(0.0, 0.55 + altitude + 3.5, 0.0);
        // Cámara en unidades de la plataforma.
        let (eye, target) = if t < T_LIFTOFF {
            let a = 0.9 + t * 0.16;
            (
                Vec3::new(a.cos() * 13.0, 2.2 + t * 0.2, a.sin() * 13.0),
                Vec3::new(0.0, 4.0, 0.0),
            )
        } else if t < 8.0 {
            (Vec3::new(10.0, 1.5, 9.0), rocket_center)
        } else {
            let k = smoothstep(8.0, 9.5, t);
            let ground = Vec3::new(10.0, 1.5, 9.0);
            let chase = rocket_center + Vec3::new(9.0, -7.0, 9.0);
            (ground.lerp(chase, k), rocket_center)
        };
        let pad_up = (to_world(Vec3::new(0.0, 1.0, 0.0)) - to_world(Vec3::ZERO)).normalized();
        let fade = smoothstep(T_FADE_OUT, T_SPACE, t);
        let thrust = if t < T_IGNITION {
            0.0
        } else {
            smoothstep(T_IGNITION, T_LIFTOFF, t) * 0.8 + 0.4
        };
        LaunchFrame {
            phase,
            on_ground: true,
            rocket,
            fairing_left: rocket,
            fairing_right: rocket,
            pad,
            ship: None,
            camera: Camera {
                position: to_world(eye),
                target: to_world(target),
                up: pad_up,
                fov_y: 55.0_f32.to_radians(),
                near: 0.02,
                far: 3000.0,
            },
            nozzle: rocket.transform_point(Vec3::new(0.0, -0.5, 0.0)),
            exhaust_dir: -pad_up,
            thrust,
            fade,
            flash: 0.0,
            countdown,
        }
    }

    fn space_frame(&self, sim_time: f32, phase: LaunchPhase) -> LaunchFrame {
        let t = self.time;
        let earth_center = planet_position(PlanetId::Tierra, sim_time);
        let up = self.space_up;
        let tangent = self.tangent;
        let side = tangent.cross(up);
        let anchor = earth_center + self.space_offset.unwrap_or(up * 60.0);
        // El cohete sigue subiendo despacio y, tras la separación, se aleja.
        let s = t - T_SPACE;
        let drift = (t - T_EMERGE).max(0.0);
        let rocket_base = anchor + up * (s * 3.0 - drift * drift * 0.8) - side * drift * 1.2;
        let tumble = drift * 0.25;
        let rocket_rot = Mat4::from_basis(tangent, up, side, Vec3::ZERO) * Mat4::rotation_x(tumble);
        let rocket = Mat4::translation(rocket_base) * rocket_rot * Mat4::uniform_scale(SPACE_SCALE);
        // Apertura de la cofia sobre bisagras en su base.
        let open = smoothstep(T_FAIRING, T_FAIRING + 1.4, t) * 1.9;
        let jettison = (t - T_FAIRING - 1.4).max(0.0);
        let hinge = |x: f32, angle: f32| {
            Mat4::translation(Vec3::new(
                x * (1.0 + jettison * 0.9),
                ROCKET_FAIRING_BASE + jettison * 0.4,
                0.0,
            )) * Mat4::rotation_z(angle)
                * Mat4::translation(Vec3::new(-x, -ROCKET_FAIRING_BASE, 0.0))
        };
        let fairing_left = rocket * hinge(-ROCKET_RADIUS, open + jettison * 0.6);
        let fairing_right = rocket * hinge(ROCKET_RADIUS, -(open + jettison * 0.6));

        // DeLorean: dentro de la cofia, sale con el morro hacia arriba y gira
        // hasta quedar horizontal.
        let inside = rocket_base + up * ((ROCKET_FAIRING_BASE + 0.9) * SPACE_SCALE);
        let emerge = smoothstep(T_EMERGE, T_TURN, t);
        let turn = smoothstep(T_TURN, T_HANDOVER, t);
        let cruise = (t - T_TURN).max(0.0);
        let ship_pos =
            inside + up * (emerge * 7.0 + turn * 2.0) + tangent * (cruise * cruise * 0.6);
        let forward = up.lerp(tangent, turn).normalized();
        let roof = (-tangent).lerp(up, turn).normalized();
        let ship = (t >= T_FAIRING).then_some((ship_pos, forward, roof));

        // Cámara cinematográfica en el espacio.
        let orbit = (t - T_SPACE) * 0.12;
        let cam_dir = (side * orbit.cos() + tangent * orbit.sin()).normalized();
        let focus = if t < T_EMERGE {
            rocket_base + up * 8.0
        } else {
            ship_pos
        };
        let eye = focus + cam_dir * 22.0 + up * 6.0 - tangent * 4.0;
        let flash = 1.0 - smoothstep(T_SPACE, T_SPACE + 1.0, t);
        let thrust = if t < T_FAIRING {
            0.9
        } else {
            (1.0 - smoothstep(T_FAIRING, T_FAIRING + 1.0, t)) * 0.9
        };
        LaunchFrame {
            phase,
            on_ground: false,
            rocket,
            fairing_left,
            fairing_right,
            pad: Mat4::identity(),
            ship,
            camera: Camera {
                position: eye,
                target: focus,
                up,
                fov_y: 55.0_f32.to_radians(),
                near: 0.05,
                far: 3000.0,
            },
            nozzle: rocket.transform_point(Vec3::new(0.0, -0.5, 0.0)),
            exhaust_dir: -up,
            thrust,
            fade: 0.0,
            flash,
            countdown: None,
        }
    }

    /// Pose final del DeLorean cuando termina (o se salta) la cinemática.
    pub fn final_ship_pose(&mut self, sim_time: f32) -> (Vec3, Vec3, Vec3) {
        let saved = self.time;
        self.time = T_END;
        self.ensure_space_anchor(sim_time);
        let frame = self.space_frame(sim_time, LaunchPhase::Finished);
        self.time = saved;
        frame.ship.expect("el DeLorean es visible al final")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_follow_the_timeline() {
        let mut seq = LaunchSequence::new();
        let mut phases = Vec::new();
        while !seq.finished() {
            let p = seq.phase();
            if phases.last() != Some(&p) {
                phases.push(p);
            }
            seq.update(0.1);
        }
        assert_eq!(phases.first(), Some(&LaunchPhase::Countdown));
        assert!(phases.contains(&LaunchPhase::Ascent));
        assert!(phases.contains(&LaunchPhase::Separation));
        assert!(phases.contains(&LaunchPhase::Deploy));
        assert_eq!(seq.phase(), LaunchPhase::Finished);
    }

    #[test]
    fn rocket_rises_only_after_liftoff() {
        let mut seq = LaunchSequence::new();
        seq.time = T_IGNITION + 0.5;
        assert_eq!(seq.altitude(), 0.0);
        let before = seq.frame(0.0).rocket.origin();
        seq.time = T_LIFTOFF + 3.0;
        let after = seq.frame(0.0).rocket.origin();
        let earth = planet_position(PlanetId::Tierra, 0.0);
        assert!((after - earth).length() > (before - earth).length() + 0.5);
    }

    #[test]
    fn countdown_counts_down() {
        let mut seq = LaunchSequence::new();
        seq.time = 0.2;
        assert_eq!(seq.frame(0.0).countdown, Some(5));
        seq.time = 4.2;
        assert_eq!(seq.frame(0.0).countdown, Some(1));
        seq.time = 6.0;
        assert_eq!(seq.frame(0.0).countdown, None);
    }

    #[test]
    fn fairing_opens_and_ship_emerges_moving_continuously() {
        let mut seq = LaunchSequence::new();
        seq.time = T_SPACE + 0.1;
        let closed = seq.frame(5.0);
        assert!(closed.ship.is_none());
        let mut previous: Option<Vec3> = None;
        let mut t = T_FAIRING;
        while t < T_END {
            seq.time = t;
            let f = seq.frame(5.0);
            let (p, forward, up) = f.ship.unwrap();
            assert!((forward.length() - 1.0).abs() < 1.0e-3 && (up.length() - 1.0).abs() < 1.0e-3);
            if let Some(prev) = previous {
                assert!(
                    (p - prev).length() < 2.5,
                    "el DeLorean no debe teletransportarse"
                );
            }
            previous = Some(p);
            t += 0.1;
        }
        seq.time = T_FAIRING + 2.0;
        let open = seq.frame(5.0);
        let tip_closed = closed
            .fairing_left
            .transform_point(Vec3::new(0.0, 7.0, 0.0));
        let tip_open = open.fairing_left.transform_point(Vec3::new(0.0, 7.0, 0.0));
        assert!(
            (tip_closed - tip_open).length() > 2.0,
            "la cofia debe abrirse"
        );
    }
}
