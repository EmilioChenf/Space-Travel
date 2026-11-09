//! Sistema solar: parámetros orbitales, transformaciones dependientes del
//! tiempo y distribución procedural de decoraciones sobre cada planeta.

use crate::assets::{Assets, PropKind};
use crate::math3d::{Mat4, PI, Rng, Vec3};
use crate::procgen::align_y;
use crate::renderer::ShaderKind;

/// Escala mundo de los planetas (radio del modelo ~5 → ~20 unidades).
pub const PLANET_SCALE: f32 = 4.0;
/// Radio del Sol en unidades del mundo.
pub const SUN_RADIUS: f32 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanetId {
    Tierra,
    Hielo,
    Oasis,
    Fuego,
    Dulces,
    Escandalosos,
}

impl PlanetId {
    pub const ALL: [PlanetId; 6] = [
        PlanetId::Tierra,
        PlanetId::Hielo,
        PlanetId::Oasis,
        PlanetId::Fuego,
        PlanetId::Dulces,
        PlanetId::Escandalosos,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Parámetros configurables de cada planeta.
#[derive(Clone, Copy, Debug)]
pub struct PlanetConfig {
    pub id: PlanetId,
    pub name: &'static str,
    pub orbit_radius: f32,
    /// Velocidad angular orbital (rad/s).
    pub orbit_speed: f32,
    /// Velocidad de rotación sobre su eje (rad/s).
    pub rotation_speed: f32,
    pub scale: f32,
    /// Ángulo inicial en la órbita (rad).
    pub phase: f32,
    /// Inclinación del plano orbital (rad).
    pub inclination: f32,
    /// Inclinación del eje de rotación (rad).
    pub axial_tilt: f32,
    pub shader: ShaderKind,
    pub description: &'static str,
}

pub const PLANETS: [PlanetConfig; 6] = [
    PlanetConfig {
        id: PlanetId::Tierra,
        name: "Tierra",
        orbit_radius: 190.0,
        orbit_speed: 0.012,
        rotation_speed: 0.05,
        scale: PLANET_SCALE,
        // Fase elegida para que la plataforma de lanzamiento esté de día.
        phase: 4.71,
        inclination: 0.0,
        axial_tilt: 0.41,
        shader: ShaderKind::Earth,
        description: "El comienzo: oceanos, continentes y la plataforma de lanzamiento",
    },
    PlanetConfig {
        id: PlanetId::Hielo,
        name: "Hielo",
        orbit_radius: 470.0,
        orbit_speed: 0.0055,
        rotation_speed: -0.04,
        scale: PLANET_SCALE,
        phase: 2.2,
        inclination: 0.05,
        axial_tilt: 0.16,
        shader: ShaderKind::Ice,
        description: "Planeta congelado con cristales de energia y ventiscas",
    },
    PlanetConfig {
        id: PlanetId::Oasis,
        name: "Oasis",
        orbit_radius: 260.0,
        orbit_speed: 0.009,
        rotation_speed: 0.055,
        scale: PLANET_SCALE,
        phase: 4.1,
        inclination: -0.03,
        axial_tilt: 0.12,
        shader: ShaderKind::Desert,
        description: "Desierto con dunas, palmeras, ruinas y un oasis",
    },
    PlanetConfig {
        id: PlanetId::Fuego,
        name: "Fuego",
        orbit_radius: 115.0,
        orbit_speed: 0.018,
        rotation_speed: 0.07,
        scale: PLANET_SCALE,
        phase: 1.2,
        inclination: 0.04,
        axial_tilt: 0.2,
        shader: ShaderKind::Lava,
        description: "Mundo volcanico con rios de lava y grietas incandescentes",
    },
    PlanetConfig {
        id: PlanetId::Dulces,
        name: "Dulces",
        orbit_radius: 330.0,
        orbit_speed: 0.0075,
        rotation_speed: 0.045,
        scale: PLANET_SCALE,
        phase: 5.4,
        inclination: 0.06,
        axial_tilt: 0.3,
        shader: ShaderKind::Candy,
        description: "Montanas de helado, piruletas, casas y lagos de chocolate",
    },
    PlanetConfig {
        id: PlanetId::Escandalosos,
        name: "Escandalosos",
        orbit_radius: 400.0,
        orbit_speed: 0.0065,
        rotation_speed: 0.04,
        scale: PLANET_SCALE,
        phase: 3.3,
        inclination: -0.05,
        axial_tilt: 0.22,
        shader: ShaderKind::Forest,
        description: "Bosques, rios, cabanas y tres osos amistosos",
    },
];

pub fn config(id: PlanetId) -> &'static PlanetConfig {
    &PLANETS[id.index()]
}

/// Posición orbital en el instante `time` (depende solo del tiempo).
pub fn planet_position(id: PlanetId, time: f32) -> Vec3 {
    let c = config(id);
    let angle = c.phase + time * c.orbit_speed;
    let flat = Vec3::new(
        angle.cos() * c.orbit_radius,
        0.0,
        angle.sin() * c.orbit_radius,
    );
    // Inclinación del plano orbital alrededor del eje X.
    Mat4::rotation_x(c.inclination).transform_point(flat)
}

/// Eje de rotación (inclinado) del planeta.
pub fn planet_axis(id: PlanetId) -> Vec3 {
    let c = config(id);
    Mat4::rotation_z(c.axial_tilt).transform_direction(Vec3::new(0.0, 1.0, 0.0))
}

/// Rotación propia del planeta en el instante `time`.
pub fn planet_rotation(id: PlanetId, time: f32) -> Mat4 {
    let c = config(id);
    Mat4::rotation_z(c.axial_tilt) * Mat4::rotation_y(time * c.rotation_speed)
}

/// Matriz modelo completa: traslación orbital * rotación * escala.
pub fn planet_model(id: PlanetId, time: f32) -> Mat4 {
    let c = config(id);
    Mat4::translation(planet_position(id, time))
        * planet_rotation(id, time)
        * Mat4::uniform_scale(c.scale)
}

/// Decoración colocada en coordenadas locales de un planeta (gira con él).
#[derive(Clone, Copy, Debug)]
pub struct PropInstance {
    pub kind: PropKind,
    pub local: Mat4,
    pub shader: ShaderKind,
}

/// Qué accesorios se distribuyen en cada planeta y cuántos.
fn prop_recipe(id: PlanetId) -> &'static [(PropKind, usize, f32, f32)] {
    // (tipo, cantidad, escala mínima, escala máxima) en unidades del modelo.
    match id {
        PlanetId::Tierra => &[],
        PlanetId::Hielo => &[
            (PropKind::Crystal, 26, 0.35, 0.7),
            (PropKind::IceRock, 22, 0.4, 0.8),
        ],
        PlanetId::Oasis => &[
            (PropKind::Palm, 18, 0.32, 0.5),
            (PropKind::Cactus, 18, 0.25, 0.42),
            (PropKind::Ruin, 6, 0.35, 0.5),
        ],
        PlanetId::Fuego => &[(PropKind::VolcanicRock, 40, 0.35, 0.9)],
        PlanetId::Dulces => &[
            (PropKind::Lollipop, 14, 0.35, 0.55),
            (PropKind::CandyCane, 14, 0.4, 0.6),
            (PropKind::Gumdrop, 26, 0.35, 0.6),
        ],
        PlanetId::Escandalosos => &[
            (PropKind::Pine, 40, 0.45, 0.8),
            (PropKind::RoundTree, 26, 0.45, 0.75),
        ],
    }
}

/// Distribución procedural reproducible (semilla) sobre la superficie curva.
/// Cada objeto se orienta según la normal radial y se coloca a la altura
/// real del terreno obtenida lanzando rayos contra la malla del planeta.
pub fn scatter_props(id: PlanetId, assets: &Assets, seed: u32) -> Vec<PropInstance> {
    let surface = &assets.planets[id.index()].surface;
    let mut rng = Rng::new(seed ^ (id.index() as u32).wrapping_mul(0x9E37_79B9));
    let shader = config(id).shader;
    let mut result: Vec<PropInstance> = Vec::new();
    let mut used: Vec<Vec3> = Vec::new();
    for &(kind, count, min_scale, max_scale) in prop_recipe(id) {
        let mut placed = 0;
        let mut attempts = 0;
        while placed < count && attempts < count * 60 {
            attempts += 1;
            let dir = rng.unit_vector();
            if used.iter().any(|u| u.dot(dir) > 0.9975) {
                continue;
            }
            if !surface.is_clear(dir, 0.05) {
                continue; // ya hay una decoración del modelo original
            }
            let ground = surface.ground_at(dir, 0.05);
            // Evitar agua: el agua suele ser la capa más baja del modelo.
            if ground < surface.base_radius * 0.985 {
                continue;
            }
            let scale = rng.range(min_scale, max_scale);
            let local = Mat4::translation(dir * (ground - 0.02))
                * align_y(dir, rng.range(0.0, 2.0 * PI))
                * Mat4::uniform_scale(scale);
            result.push(PropInstance {
                kind,
                local,
                shader,
            });
            used.push(dir);
            placed += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbits_depend_only_on_time_and_keep_their_radius() {
        for c in PLANETS {
            let a = planet_position(c.id, 10.0);
            let b = planet_position(c.id, 10.0);
            assert!((a - b).length() < 1.0e-6);
            assert!((a.length() - c.orbit_radius).abs() < 0.01);
            let later = planet_position(c.id, 20.0);
            let expected = c.orbit_speed * 10.0 * c.orbit_radius; // longitud de arco
            assert!(((later - a).length() - expected).abs() < expected * 0.01 + 0.01);
        }
    }

    #[test]
    fn rotation_advances_with_time_and_preserves_scale() {
        let m0 = planet_model(PlanetId::Hielo, 0.0);
        let m1 = planet_model(PlanetId::Hielo, 30.0);
        assert!((m0.max_scale() - PLANET_SCALE).abs() < 1.0e-4);
        let p = Vec3::new(5.0, 0.0, 0.0);
        let a = m0.transform_point(p) - m0.origin();
        let b = m1.transform_point(p) - m1.origin();
        let angle = a.normalized().dot(b.normalized()).clamp(-1.0, 1.0).acos();
        assert!((angle - (0.04_f32 * 30.0)).abs() < 0.05);
    }

    #[test]
    fn planets_are_separated_enough_to_navigate() {
        let mut radii: Vec<f32> = PLANETS.iter().map(|p| p.orbit_radius).collect();
        radii.sort_by(|a, b| a.total_cmp(b));
        assert!(radii[0] > SUN_RADIUS * 2.0);
        for pair in radii.windows(2) {
            assert!(pair[1] - pair[0] > 50.0);
        }
    }
}
