//! Rutas de recursos y carga centralizada de todos los modelos.
//!
//! Los modelos se cargan una sola vez con el lector OBJ/MTL propio
//! (`model.rs`) y se reutilizan para todas sus instancias. Cada malla
//! grande recibe niveles de detalle (LOD) por agrupamiento de vértices.

use crate::model::{
    LodMesh, Mesh, load_obj, load_obj_objects, make_thruster_flame, make_thruster_nozzle,
    make_uv_sphere,
};
use crate::surface::PlanetSurface;
use std::io;
use std::path::{Path, PathBuf};

/// Ruta del modelo de la nave relativa a la raíz del proyecto.
pub const DELOREAN_OBJ: &str = "recursos/modelos/nave_espacial/delorean/delorean.obj";
pub const OASIS_OBJ: &str = "recursos/modelos/planetas/oasis/oasis.obj";
pub const HIELO_OBJ: &str = "recursos/modelos/planetas/hielo/Hielo.obj";
pub const TIERRA_OBJ: &str = "recursos/modelos/planetas/tierra/Tierra.obj";
pub const NUBES_OBJ: &str = "recursos/modelos/planetas/tierra/Nubes.obj";
pub const PLATAFORMA_OBJ: &str = "recursos/modelos/planetas/tierra/Plataforma.obj";
pub const FUEGO_OBJ: &str = "recursos/modelos/planetas/fuego/Fuego.obj";
pub const DULCES_OBJ: &str = "recursos/modelos/planetas/dulces/Dulces.obj";
pub const ESCANDALOSOS_OBJ: &str = "recursos/modelos/planetas/escandalosos/Escandalosos.obj";
pub const SOL_OBJ: &str = "recursos/modelos/espacio/sol/Sol.obj";
pub const COHETE_OBJ: &str = "recursos/modelos/nave_espacial/cohete/cohete.obj";
pub const ARO_CONTROL_OBJ: &str = "recursos/modelos/espacio/marcadores/aro_control.obj";

/// Resuelve un recurso desde el directorio actual o desde la raíz conocida por
/// Cargo. Esto permite ejecutar el proyecto con `cargo run` y también desde la
/// mayoría de editores sin duplicar rutas por el código.
pub fn locate(relative_path: impl AsRef<Path>) -> PathBuf {
    let relative_path = relative_path.as_ref();
    let from_working_directory = PathBuf::from(relative_path);
    if from_working_directory.exists() {
        return from_working_directory;
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let next_to_exe = dir.join(relative_path);
        if next_to_exe.exists() {
            return next_to_exe;
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path)
}

fn load(path: &str) -> io::Result<Mesh> {
    load_obj(locate(path)).map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))
}

/// Accesorio que se distribuye proceduralmente sobre un planeta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Crystal,
    IceRock,
    Palm,
    Cactus,
    Ruin,
    Relic,
    VolcanicRock,
    Lollipop,
    CandyCane,
    Gumdrop,
    CandyRing,
    Pine,
    RoundTree,
    BearBrown,
    BearPanda,
    BearPolar,
}

impl PropKind {
    pub const ALL: [PropKind; 16] = [
        PropKind::Crystal,
        PropKind::IceRock,
        PropKind::Palm,
        PropKind::Cactus,
        PropKind::Ruin,
        PropKind::Relic,
        PropKind::VolcanicRock,
        PropKind::Lollipop,
        PropKind::CandyCane,
        PropKind::Gumdrop,
        PropKind::CandyRing,
        PropKind::Pine,
        PropKind::RoundTree,
        PropKind::BearBrown,
        PropKind::BearPanda,
        PropKind::BearPolar,
    ];

    pub fn path(self) -> &'static str {
        match self {
            PropKind::Crystal => "recursos/modelos/planetas/hielo/cristal.obj",
            PropKind::IceRock => "recursos/modelos/planetas/hielo/roca_helada.obj",
            PropKind::Palm => "recursos/modelos/planetas/oasis/palmera.obj",
            PropKind::Cactus => "recursos/modelos/planetas/oasis/cactus.obj",
            PropKind::Ruin => "recursos/modelos/planetas/oasis/ruina.obj",
            PropKind::Relic => "recursos/modelos/planetas/oasis/reliquia.obj",
            PropKind::VolcanicRock => "recursos/modelos/planetas/fuego/roca_volcanica.obj",
            PropKind::Lollipop => "recursos/modelos/planetas/dulces/piruleta.obj",
            PropKind::CandyCane => "recursos/modelos/planetas/dulces/baston.obj",
            PropKind::Gumdrop => "recursos/modelos/planetas/dulces/gomita.obj",
            PropKind::CandyRing => "recursos/modelos/planetas/dulces/aro_caramelo.obj",
            PropKind::Pine => "recursos/modelos/planetas/escandalosos/arbol_pino.obj",
            PropKind::RoundTree => "recursos/modelos/planetas/escandalosos/arbol_redondo.obj",
            PropKind::BearBrown => "recursos/modelos/planetas/escandalosos/oso_pardo.obj",
            PropKind::BearPanda => "recursos/modelos/planetas/escandalosos/oso_panda.obj",
            PropKind::BearPolar => "recursos/modelos/planetas/escandalosos/oso_polar.obj",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// Planetas con modelo y superficie consultable.
pub struct PlanetAsset {
    pub lod: LodMesh,
    pub surface: PlanetSurface,
}

/// Partes animables del cohete.
pub struct RocketAsset {
    pub body: Mesh,
    pub fairing_left: Mesh,
    pub fairing_right: Mesh,
}

pub struct Assets {
    pub ship: LodMesh,
    pub thruster_nozzle: Mesh,
    pub thruster_flame: Mesh,
    pub sun: Mesh,
    pub sphere: Mesh,
    /// Orden: Tierra, Hielo, Oasis, Fuego, Dulces, Escandalosos.
    pub planets: Vec<PlanetAsset>,
    pub clouds: LodMesh,
    pub launch_pad: Mesh,
    pub rocket: RocketAsset,
    pub meteors: Vec<LodMesh>,
    pub asteroids: Vec<LodMesh>,
    pub checkpoint: Mesh,
    pub props: Vec<LodMesh>,
}

/// Niveles de detalle para planetas (fracción del radio, píxeles mínimos).
const PLANET_LODS: [(f32, f32); 3] = [(0.025, 260.0), (0.06, 70.0), (0.14, 14.0)];
const PROP_LODS: [(f32, f32); 1] = [(0.12, 18.0)];

impl Assets {
    pub fn load() -> io::Result<Self> {
        let planet = |path: &str| -> io::Result<PlanetAsset> {
            let mesh = load(path)?;
            let surface = PlanetSurface::new(&mesh);
            Ok(PlanetAsset {
                lod: LodMesh::with_levels(mesh, &PLANET_LODS),
                surface,
            })
        };
        let planets = vec![
            planet(TIERRA_OBJ)?,
            planet(HIELO_OBJ)?,
            planet(OASIS_OBJ)?,
            planet(FUEGO_OBJ)?,
            planet(DULCES_OBJ)?,
            planet(ESCANDALOSOS_OBJ)?,
        ];
        let rocket_objects = load_obj_objects(locate(COHETE_OBJ))?;
        let mut body = Mesh::default();
        let mut fairing_left = Mesh::default();
        let mut fairing_right = Mesh::default();
        for (name, mesh) in rocket_objects {
            match name.as_str() {
                "Cofia_Izquierda" => fairing_left = mesh,
                "Cofia_Derecha" => fairing_right = mesh,
                _ => body.append(&mesh),
            }
        }
        let rock_lod = |path: String| -> io::Result<LodMesh> {
            Ok(LodMesh::with_levels(load(&path)?, &[(0.35, 10.0)]))
        };
        let ship = load(DELOREAN_OBJ)?;
        let props = PropKind::ALL
            .iter()
            .map(|kind| Ok(LodMesh::with_levels(load(kind.path())?, &PROP_LODS)))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            ship: LodMesh::with_levels(ship, &[(0.02, 70.0), (0.05, 22.0)]),
            thruster_nozzle: make_thruster_nozzle(16),
            thruster_flame: make_thruster_flame(16),
            sun: load(SOL_OBJ)?,
            sphere: make_uv_sphere(32, 20),
            planets,
            clouds: LodMesh::with_levels(load(NUBES_OBJ)?, &[(0.06, 60.0)]),
            launch_pad: load(PLATAFORMA_OBJ)?,
            rocket: RocketAsset {
                body,
                fairing_left,
                fairing_right,
            },
            meteors: (1..=3)
                .map(|i| {
                    rock_lod(format!(
                        "recursos/modelos/espacio/meteoritos/meteorito_{i}.obj"
                    ))
                })
                .collect::<io::Result<Vec<_>>>()?,
            asteroids: (1..=3)
                .map(|i| {
                    rock_lod(format!(
                        "recursos/modelos/espacio/asteroides/asteroide_{i}.obj"
                    ))
                })
                .collect::<io::Result<Vec<_>>>()?,
            checkpoint: load(ARO_CONTROL_OBJ)?,
            props,
        })
    }

    pub fn prop(&self, kind: PropKind) -> &LodMesh {
        &self.props[kind.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_models() {
        for path in [
            DELOREAN_OBJ,
            OASIS_OBJ,
            HIELO_OBJ,
            TIERRA_OBJ,
            NUBES_OBJ,
            PLATAFORMA_OBJ,
            FUEGO_OBJ,
            DULCES_OBJ,
            ESCANDALOSOS_OBJ,
            SOL_OBJ,
            COHETE_OBJ,
            ARO_CONTROL_OBJ,
        ] {
            assert!(locate(path).is_file(), "falta {path}");
            let mtl = locate(path).with_extension("mtl");
            assert!(mtl.is_file(), "falta el MTL de {path}");
        }
        for kind in PropKind::ALL {
            assert!(locate(kind.path()).is_file(), "falta {}", kind.path());
        }
    }

    #[test]
    fn every_obj_references_existing_materials() {
        let root = locate("recursos/modelos");
        let mut stack = vec![root];
        let mut checked = 0;
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("obj") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let mtllib = text
                    .lines()
                    .find_map(|l| l.strip_prefix("mtllib "))
                    .expect("todo OBJ debe declarar mtllib");
                let materials =
                    crate::model::load_materials(&path.parent().unwrap().join(mtllib.trim()));
                assert!(!materials.is_empty(), "{path:?}: MTL vacío o inexistente");
                for name in text.lines().filter_map(|l| l.strip_prefix("usemtl ")) {
                    assert!(
                        materials.contains_key(name.trim()),
                        "{path:?}: material {name} no definido"
                    );
                }
                let faces = text.lines().filter(|l| l.starts_with("f ")).count();
                assert!(faces > 0, "{path:?} no tiene caras");
                checked += 1;
            }
        }
        assert!(checked >= 30, "solo se revisaron {checked} OBJ");
    }

    #[test]
    fn all_assets_load_with_lods_and_surfaces() {
        let assets = Assets::load().expect("los recursos deben cargar");
        assert_eq!(assets.planets.len(), 6);
        for planet in &assets.planets {
            assert!(planet.lod.levels.len() >= 3);
            let full = planet.lod.full().triangles.len();
            let low = planet.lod.levels.last().unwrap().0.triangles.len();
            assert!(
                low * 4 < full,
                "el LOD más bajo debe reducir triángulos ({low} vs {full})"
            );
            assert!(planet.surface.base_radius > 4.0 && planet.surface.base_radius < 6.5);
        }
        assert!(!assets.rocket.fairing_left.triangles.is_empty());
        assert!(!assets.rocket.fairing_right.triangles.is_empty());
        assert_eq!(assets.meteors.len(), 3);
        assert_eq!(assets.props.len(), PropKind::ALL.len());
    }
}
