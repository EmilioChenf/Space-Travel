//! Cámaras de vuelo: primera persona, tercera persona y orbital.
//! Todas se suavizan con delta time y las transiciones entre modos se
//! interpolan para evitar saltos bruscos.

use crate::math3d::{Vec3, damp_factor, smoothstep};
use crate::renderer::Camera;
use crate::ship::Ship;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    FirstPerson,
    ThirdPerson,
    Orbital,
}

impl CameraMode {
    pub fn next(self) -> Self {
        match self {
            CameraMode::ThirdPerson => CameraMode::FirstPerson,
            CameraMode::FirstPerson => CameraMode::Orbital,
            CameraMode::Orbital => CameraMode::ThirdPerson,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            CameraMode::FirstPerson => "Primera persona (cabina)",
            CameraMode::ThirdPerson => "Tercera persona",
            CameraMode::Orbital => "Orbital",
        }
    }
}

/// Entrada de cámara por cuadro.
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraInput {
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub zoom: f32,
}

pub struct CameraRig {
    pub mode: CameraMode,
    pub third_distance: f32,
    pub orbit_distance: f32,
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub fov: f32,
    smooth_position: Vec3,
    smooth_up: Vec3,
    from: Option<Camera>,
    transition: f32,
    last: Option<Camera>,
}

const TRANSITION_TIME: f32 = 0.45;

impl CameraRig {
    pub fn new(ship: &Ship) -> Self {
        let mut rig = Self {
            mode: CameraMode::ThirdPerson,
            third_distance: 7.5,
            orbit_distance: 12.0,
            orbit_yaw: 0.6,
            orbit_pitch: 0.35,
            fov: 62.0_f32.to_radians(),
            smooth_position: Vec3::ZERO,
            smooth_up: ship.up,
            from: None,
            transition: 1.0,
            last: None,
        };
        rig.snap(ship);
        rig
    }

    /// Coloca la cámara sin suavizado (al reiniciar o tras un warp).
    pub fn snap(&mut self, ship: &Ship) {
        self.smooth_position = self.third_person_target(ship);
        self.smooth_up = ship.up;
        self.from = None;
        self.transition = 1.0;
    }

    pub fn cycle(&mut self) {
        self.set_mode(self.mode.next());
    }

    pub fn set_mode(&mut self, mode: CameraMode) {
        if mode != self.mode {
            self.from = self.last;
            self.transition = 0.0;
            self.mode = mode;
        }
    }

    fn third_person_target(&self, ship: &Ship) -> Vec3 {
        ship.position - ship.forward * self.third_distance + ship.up * (self.third_distance * 0.3)
    }

    /// Posición de la cabina dentro del DeLorean en espacio mundo.
    pub fn cockpit_position(ship: &Ship) -> Vec3 {
        ship.position + ship.forward * 0.05 + ship.up * 0.42
    }

    pub fn update(&mut self, ship: &Ship, input: CameraInput, dt: f32) -> Camera {
        let dt = dt.min(0.1);
        self.smooth_up = self
            .smooth_up
            .lerp(ship.up, damp_factor(4.0, dt))
            .normalized();
        let desired = match self.mode {
            CameraMode::ThirdPerson => {
                self.third_distance = (self.third_distance - input.zoom * 0.8).clamp(4.0, 30.0);
                let target_pos = self.third_person_target(ship);
                self.smooth_position = self.smooth_position.lerp(target_pos, damp_factor(7.0, dt));
                // Evitar que la cámara se aleje demasiado con velocidad alta.
                let offset = self.smooth_position - ship.position;
                let max = self.third_distance * 1.8;
                if offset.length() > max {
                    self.smooth_position = ship.position + offset.normalized() * max;
                }
                Camera {
                    position: self.smooth_position,
                    target: ship.position + ship.forward * 4.0,
                    up: self.smooth_up,
                    fov_y: self.fov,
                    near: 0.08,
                    far: 3000.0,
                }
            }
            CameraMode::FirstPerson => {
                self.smooth_position = self.third_person_target(ship);
                let eye = Self::cockpit_position(ship);
                Camera {
                    position: eye,
                    target: eye + ship.forward * 10.0,
                    up: ship.up,
                    fov_y: self.fov * 1.08,
                    near: 0.05,
                    far: 3000.0,
                }
            }
            CameraMode::Orbital => {
                self.smooth_position = self.third_person_target(ship);
                if input.orbit_yaw.abs() < 1.0e-4 && input.orbit_pitch.abs() < 1.0e-4 {
                    self.orbit_yaw += 0.12 * dt; // rotación automática lenta
                }
                self.orbit_yaw += input.orbit_yaw;
                self.orbit_pitch = (self.orbit_pitch + input.orbit_pitch).clamp(-1.35, 1.35);
                self.orbit_distance = (self.orbit_distance - input.zoom * 1.2).clamp(4.0, 80.0);
                // Base local estable: arriba suavizado, adelante proyectado.
                let up = self.smooth_up;
                let forward = ship.forward.reject(up).normalized();
                let forward = if forward.length_squared() < 0.5 {
                    up.any_perpendicular()
                } else {
                    forward
                };
                let side = forward.cross(up);
                let horizontal = forward * self.orbit_yaw.cos() + side * self.orbit_yaw.sin();
                let dir = horizontal * self.orbit_pitch.cos() + up * self.orbit_pitch.sin();
                Camera {
                    position: ship.position + dir * self.orbit_distance,
                    target: ship.position,
                    up,
                    fov_y: self.fov,
                    near: 0.08,
                    far: 3000.0,
                }
            }
        };
        let camera = if let Some(from) = self.from {
            self.transition = (self.transition + dt / TRANSITION_TIME).min(1.0);
            let t = smoothstep(0.0, 1.0, self.transition);
            if self.transition >= 1.0 {
                self.from = None;
            }
            blend(&from, &desired, t)
        } else {
            desired
        };
        self.last = Some(camera);
        camera
    }

    /// Indica si la nave debe dibujarse (en cabina se oculta).
    pub fn shows_ship(&self) -> bool {
        self.mode != CameraMode::FirstPerson || self.from.is_some()
    }
}

/// Interpola dos cámaras (posición, objetivo, arriba y FOV).
pub fn blend(a: &Camera, b: &Camera, t: f32) -> Camera {
    Camera {
        position: a.position.lerp(b.position, t),
        target: a.target.lerp(b.target, t),
        up: a.up.lerp(b.up, t).normalized(),
        fov_y: a.fov_y + (b.fov_y - a.fov_y) * t,
        near: a.near.min(b.near),
        far: a.far.max(b.far),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle_through_all_three() {
        let mut mode = CameraMode::ThirdPerson;
        let mut seen = vec![mode];
        for _ in 0..2 {
            mode = mode.next();
            seen.push(mode);
        }
        assert!(seen.contains(&CameraMode::FirstPerson));
        assert!(seen.contains(&CameraMode::Orbital));
        assert_eq!(mode.next(), CameraMode::ThirdPerson);
    }

    #[test]
    fn third_person_stays_behind_the_ship() {
        let ship = Ship::new(Vec3::new(10.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let mut rig = CameraRig::new(&ship);
        let camera = rig.update(&ship, CameraInput::default(), 0.016);
        assert!(camera.position.x < ship.position.x);
        assert!((camera.target - ship.position).dot(ship.forward) > 0.0);
    }

    #[test]
    fn switching_modes_blends_without_jumps() {
        let ship = Ship::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        let mut rig = CameraRig::new(&ship);
        let before = rig.update(&ship, CameraInput::default(), 0.016);
        rig.set_mode(CameraMode::FirstPerson);
        let first_frame = rig.update(&ship, CameraInput::default(), 0.016);
        let jump = (first_frame.position - before.position).length();
        let full = (CameraRig::cockpit_position(&ship) - before.position).length();
        assert!(
            jump < full * 0.2,
            "la transición debe ser gradual ({jump} de {full})"
        );
        for _ in 0..60 {
            rig.update(&ship, CameraInput::default(), 0.016);
        }
        let settled = rig.update(&ship, CameraInput::default(), 0.016);
        assert!((settled.position - CameraRig::cockpit_position(&ship)).length() < 1.0e-3);
        assert!(!rig.shows_ship());
    }

    #[test]
    fn orbital_camera_keeps_distance_and_zooms() {
        let ship = Ship::new(Vec3::new(5.0, 5.0, 5.0), Vec3::new(0.0, 0.0, 1.0));
        let mut rig = CameraRig::new(&ship);
        rig.mode = CameraMode::Orbital;
        let camera = rig.update(
            &ship,
            CameraInput {
                orbit_yaw: 0.3,
                ..CameraInput::default()
            },
            0.016,
        );
        assert!(((camera.position - ship.position).length() - rig.orbit_distance).abs() < 1.0e-3);
        let before = rig.orbit_distance;
        rig.update(
            &ship,
            CameraInput {
                zoom: 2.0,
                ..CameraInput::default()
            },
            0.016,
        );
        assert!(rig.orbit_distance < before);
    }
}
