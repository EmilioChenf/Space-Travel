//! Recetas de los modelos generados proceduralmente.
//!
//! Cada función construye un modelo con geometría real (terreno desplazado,
//! volcanes, montañas de helado, cabañas, osos, cohete...) y materiales con
//! nombre. `generate_all` exporta todo a `recursos/modelos` como OBJ/MTL.
//! Los modelos existentes del DeLorean, Hielo y Oasis NO se tocan.

use crate::math3d::{Mat4, PI, Rng, Vec3};
use crate::model::Color3;
use crate::procgen::{
    ModelBuilder, Part, cube, cylinder, icosphere, lathe, lathe_arc, leaf, place_on_sphere, rock,
    roof, scatter_directions, sphere, torus, tube,
};
use crate::shaders::{fbm, ridged, value_noise};
use std::io;
use std::path::Path;

/// Radio base de los planetas generados (igual escala que Hielo y Oasis).
pub const PLANET_RADIUS: f32 = 5.0;
/// Dirección (espacio local de la Tierra) de la plataforma de lanzamiento.
pub const LAUNCH_DIR: Vec3 = Vec3::new(0.0, 0.5, 0.866_025_4);
/// Radio de la Tierra en unidades del modelo.
pub const EARTH_RADIUS: f32 = 5.5;

fn c(r: f32, g: f32, b: f32) -> Color3 {
    Color3::new(r, g, b)
}

fn angle_between(a: Vec3, b: Vec3) -> f32 {
    a.normalized().dot(b.normalized()).clamp(-1.0, 1.0).acos()
}

// ---------------------------------------------------------------------------
// Tierra
// ---------------------------------------------------------------------------

/// Altura relativa del terreno terrestre (0 = nivel del mar).
pub fn earth_height(d: Vec3) -> f32 {
    let continent = fbm(d * 1.5 + Vec3::new(4.0, 1.0, 2.0), 5);
    let launch = angle_between(d, LAUNCH_DIR);
    if launch < 0.2 {
        return 0.018;
    }
    if continent < 0.5 {
        return 0.0;
    }
    let base = 0.008 + (continent - 0.5) * 0.22;
    let mountains = ridged(d * 4.0 + Vec3::new(1.0, 7.0, 3.0)).powf(3.0)
        * 0.06
        * ((continent - 0.55) * 8.0).clamp(0.0, 1.0);
    let h = base + mountains;
    // Transición suave hacia la explanada de lanzamiento.
    if launch < 0.3 {
        let t = (launch - 0.2) / 0.1;
        return 0.018 * (1.0 - t) + h * t;
    }
    h
}

pub fn build_earth() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let ocean = b.material("Tierra_Oceano", c(0.03, 0.15, 0.42));
    let sand = b.material("Tierra_Costa_Arena", c(0.83, 0.76, 0.52));
    let grass = b.material("Tierra_Pradera", c(0.22, 0.55, 0.18));
    let forest = b.material("Tierra_Bosque", c(0.08, 0.36, 0.12));
    let desert = b.material("Tierra_Desierto_Arena", c(0.78, 0.6, 0.33));
    let mountain = b.material("Tierra_Montana_Roca", c(0.42, 0.36, 0.3));
    let snow = b.material("Tierra_Nieve", c(0.94, 0.96, 1.0));
    b.object("Planeta_Tierra");
    let mut part = icosphere(5);
    part.displace_radial(earth_height);
    b.add_classified(&part, Mat4::uniform_scale(EARTH_RADIUS), |p| {
        let d = p.normalized();
        let h = p.length() - 1.0;
        if d.y.abs() > 0.9 && h > -0.001 {
            return snow;
        }
        if h < 0.003 {
            ocean
        } else if h < 0.012 {
            sand
        } else if h > 0.1 {
            snow
        } else if h > 0.068 {
            mountain
        } else if value_noise(d * 6.0) > 0.62 && d.y.abs() < 0.4 {
            desert
        } else if fbm(d * 5.0, 2) > 0.52 {
            forest
        } else {
            grass
        }
    });
    b
}

pub fn build_clouds() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let cloud = b.material("Tierra_Nubes", c(0.96, 0.97, 1.0));
    b.object("Capa_Nubes");
    b.add(
        &icosphere(4),
        Mat4::uniform_scale(EARTH_RADIUS * 1.075),
        cloud,
    );
    b
}

/// Plataforma de lanzamiento (escala de cinemática; el cohete mide ~7 u).
pub fn build_launch_pad() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let concrete = b.material("Plataforma_Concreto", c(0.55, 0.55, 0.57));
    let steel = b.material("Plataforma_Acero_Rojo", c(0.72, 0.16, 0.1));
    let dark = b.material("Plataforma_Metal_Oscuro", c(0.15, 0.16, 0.18));
    let light = b.emissive("Plataforma_Luces", c(1.0, 0.85, 0.4), 3.0);
    b.object("Base");
    b.add(
        &cylinder(3.2, 3.0, 0.5, 24),
        Mat4::translation(Vec3::new(0.0, -0.3, 0.0)),
        concrete,
    );
    b.add(
        &cube(Vec3::new(1.4, 0.3, 4.5)),
        Mat4::translation(Vec3::new(0.0, 0.25, 2.6)),
        dark,
    );
    b.object("Torre");
    let tower_x = -2.0;
    for i in 0..4 {
        let (dx, dz) = [(-0.4, -0.4), (0.4, -0.4), (-0.4, 0.4), (0.4, 0.4)][i];
        b.add(
            &cube(Vec3::new(0.12, 8.5, 0.12)),
            Mat4::translation(Vec3::new(tower_x + dx, 4.45, dz)),
            steel,
        );
    }
    for level in 0..9 {
        let y = 0.6 + level as f32 * 0.95;
        b.add(
            &cube(Vec3::new(0.95, 0.08, 0.95)),
            Mat4::translation(Vec3::new(tower_x, y, 0.0)),
            steel,
        );
        // Diagonales de la celosía.
        let diag = Mat4::translation(Vec3::new(tower_x, y + 0.45, -0.42))
            * Mat4::rotation_z(if level % 2 == 0 { 0.78 } else { -0.78 });
        b.add(&cube(Vec3::new(1.2, 0.06, 0.06)), diag, steel);
        b.add(
            &sphere(6, 4),
            Mat4::translation(Vec3::new(tower_x + 0.5, y, 0.5)) * Mat4::uniform_scale(0.08),
            light,
        );
    }
    b.object("Brazo");
    b.add(
        &cube(Vec3::new(1.5, 0.2, 0.3)),
        Mat4::translation(Vec3::new(-1.1, 6.6, 0.0)),
        dark,
    );
    b.add(
        &cube(Vec3::new(1.5, 0.2, 0.3)),
        Mat4::translation(Vec3::new(-1.1, 3.4, 0.0)),
        dark,
    );
    b
}

// ---------------------------------------------------------------------------
// Cohete y nave
// ---------------------------------------------------------------------------

pub const ROCKET_HEIGHT: f32 = 7.2;
pub const ROCKET_FAIRING_BASE: f32 = 4.6;

pub fn build_rocket() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let white = b.material("Cohete_Pintura_Blanca", c(0.92, 0.93, 0.95));
    let band = b.material("Cohete_Franja_Negra", c(0.08, 0.08, 0.1));
    let orange = b.material("Cohete_Pintura_Naranja", c(0.95, 0.42, 0.08));
    let nozzle = b.material("Cohete_Tobera_Metal", c(0.3, 0.3, 0.33));
    let window = b.emissive("Cohete_Ventanilla_Luminosa", c(0.4, 0.8, 1.0), 2.0);
    let r = 0.62;
    b.object("Cuerpo");
    b.add(
        &lathe(&[(r * 0.9, 0.0), (r, 0.25), (r, ROCKET_FAIRING_BASE)], 24),
        Mat4::identity(),
        white,
    );
    for &y in &[1.0, 2.6] {
        b.add(
            &cylinder(r * 1.01, r * 1.01, 0.18, 24),
            Mat4::translation(Vec3::new(0.0, y, 0.0)),
            band,
        );
    }
    b.add(
        &cylinder(r * 1.012, r * 1.012, 0.35, 24),
        Mat4::translation(Vec3::new(0.0, 3.8, 0.0)),
        orange,
    );
    b.add(
        &sphere(10, 6),
        Mat4::translation(Vec3::new(0.0, 3.3, r * 0.95)) * Mat4::scale(Vec3::new(0.16, 0.16, 0.06)),
        window,
    );
    b.object("Aletas");
    for i in 0..4 {
        let angle = i as f32 * PI * 0.5 + PI * 0.25;
        let fin = Mat4::rotation_y(-angle)
            * Mat4::translation(Vec3::new(r + 0.3, 0.55, 0.0))
            * Mat4::rotation_z(-0.12);
        b.add(&cube(Vec3::new(0.65, 1.2, 0.07)), fin, orange);
    }
    b.object("Tobera");
    b.add(
        &lathe(&[(0.28, 0.02), (0.5, -0.45), (0.42, -0.5)], 20),
        Mat4::identity(),
        nozzle,
    );
    // Cofia (ojiva) dividida en dos mitades para la secuencia de apertura.
    let ogive: Vec<(f32, f32)> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            (
                r * (1.0 - t * t).max(0.0).sqrt() * (1.0 - t * 0.15),
                ROCKET_FAIRING_BASE + t * 2.6,
            )
        })
        .collect();
    b.object("Cofia_Izquierda");
    b.add(
        &lathe_arc(&ogive, 12, PI * 0.5, PI * 1.5),
        Mat4::identity(),
        white,
    );
    b.object("Cofia_Derecha");
    b.add(
        &lathe_arc(&ogive, 12, -PI * 0.5, PI * 0.5),
        Mat4::identity(),
        white,
    );
    b
}

/// Marcador/aro de control con material emisivo.
pub fn build_checkpoint_ring() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let glow = b.emissive("Aro_Control_Luminoso", c(0.2, 1.0, 0.7), 2.5);
    let frame = b.material("Aro_Control_Metal", c(0.3, 0.32, 0.35));
    b.object("Aro_Control");
    b.add(&torus(1.0, 0.09, 40, 8), Mat4::identity(), glow);
    for i in 0..4 {
        let a = i as f32 * PI * 0.5;
        b.add(
            &cube(Vec3::new(0.18, 0.3, 0.18)),
            Mat4::translation(Vec3::new(a.cos(), 0.0, a.sin())),
            frame,
        );
    }
    b
}

// ---------------------------------------------------------------------------
// Sol, meteoritos y asteroides
// ---------------------------------------------------------------------------

pub fn build_sun() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let plasma = b.emissive("Sol_Plasma", c(1.0, 0.6, 0.1), 6.0);
    b.object("Sol");
    let mut part = icosphere(4);
    part.displace_radial(|d| (fbm(d * 3.0, 3) - 0.5) * 0.02);
    b.add(&part, Mat4::identity(), plasma);
    b
}

pub fn build_meteor(variant: u32) -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let stone = b.material("Meteorito_Roca", c(0.36, 0.3, 0.27));
    let hot = b.material("Meteorito_Roca_Quemada", c(0.2, 0.15, 0.13));
    b.object(&format!("Meteorito_{variant}"));
    let part = rock(2, 3.7 + variant as f32 * 5.3, 0.55);
    b.add_classified(&part, Mat4::identity(), |p| {
        if p.length() > 1.05 { stone } else { hot }
    });
    b
}

pub fn build_asteroid(variant: u32) -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let stone = b.material("Asteroide_Roca", c(0.45, 0.42, 0.4));
    let dark = b.material("Asteroide_Roca_Oscura", c(0.27, 0.25, 0.24));
    b.object(&format!("Asteroide_{variant}"));
    let part = rock(3, 11.0 + variant as f32 * 7.7, 0.4);
    let stretch = Mat4::scale(Vec3::new(1.0 + variant as f32 * 0.25, 0.8, 0.9));
    b.add_classified(&part, stretch, |p| {
        if fbm(p * 2.0, 2) > 0.5 { stone } else { dark }
    });
    b
}

// ---------------------------------------------------------------------------
// Planeta Fuego
// ---------------------------------------------------------------------------

fn volcano_directions() -> Vec<Vec3> {
    scatter_directions(&mut Rng::new(0xF1E6), 7, 0.65, |_| true)
}

pub fn fire_height(d: Vec3, volcanoes: &[Vec3]) -> f32 {
    let mut h = (fbm(d * 2.2 + Vec3::new(9.0, 2.0, 5.0), 4) - 0.45) * 0.09;
    h += ridged(d * 3.3).powf(4.0) * 0.05;
    for v in volcanoes {
        let a = angle_between(d, *v);
        if a < 0.32 {
            let t = 1.0 - a / 0.32;
            h += 0.17 * t.powf(1.6);
            if a < 0.07 {
                h -= (0.07 - a) * 2.4; // cráter
            }
        }
    }
    // Ríos de lava: canales donde el ruido cruza la mitad.
    let river = 1.0 - (fbm(d * 1.6 + Vec3::new(2.0, 8.0, 1.0), 3) * 2.0 - 1.0).abs();
    if river > 0.94 {
        h -= (river - 0.94) * 0.8;
    }
    h
}

pub fn build_fire_planet() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let lava = b.material("Fuego_Lava", c(1.0, 0.35, 0.03));
    let basalt = b.material("Fuego_Basalto", c(0.22, 0.17, 0.15));
    let dark = b.material("Fuego_Roca_Oscura", c(0.12, 0.09, 0.09));
    let ash = b.material("Fuego_Ceniza", c(0.36, 0.33, 0.31));
    let obsidian = b.material("Fuego_Obsidiana", c(0.06, 0.05, 0.08));
    let volcanoes = volcano_directions();
    b.object("Planeta_Fuego");
    let mut part = icosphere(5);
    part.displace_radial(|d| fire_height(d, &volcanoes));
    let vol = volcanoes.clone();
    b.add_classified(&part, Mat4::uniform_scale(PLANET_RADIUS), move |p| {
        let d = p.normalized();
        let h = p.length() - 1.0;
        let crater = vol.iter().any(|v| angle_between(d, *v) < 0.055);
        let river = 1.0 - (fbm(d * 1.6 + Vec3::new(2.0, 8.0, 1.0), 3) * 2.0 - 1.0).abs();
        if crater || river > 0.955 || h < -0.025 {
            lava
        } else if h > 0.12 {
            ash
        } else if fbm(d * 4.0, 2) > 0.55 {
            dark
        } else {
            basalt
        }
    });
    // Agujas de obsidiana alrededor de los volcanes.
    b.object("Agujas_Obsidiana");
    let mut rng = Rng::new(0x0B51);
    for v in &volcanoes {
        for _ in 0..5 {
            let offset = v
                .any_perpendicular()
                .rotate_around(*v, rng.range(0.0, 2.0 * PI));
            let d = (*v + offset * rng.range(0.3, 0.5)).normalized();
            let r = PLANET_RADIUS * (1.0 + fire_height(d, &volcanoes)) - 0.05;
            let spike = cylinder(0.18, 0.0, rng.range(0.5, 1.0), 5);
            b.add(
                &spike,
                place_on_sphere(d, r, rng.range(0.0, 6.0), 1.0)
                    * Mat4::rotation_x(rng.range(-0.3, 0.3)),
                obsidian,
            );
        }
    }
    b
}

pub fn build_volcanic_rock() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let rock_mat = b.material("Fuego_Roca_Volcanica", c(0.14, 0.11, 0.1));
    let crack = b.material("Fuego_Grieta_Lava", c(1.0, 0.4, 0.05));
    b.object("Roca_Volcanica");
    let part = rock(2, 21.0, 0.5);
    b.add_classified(
        &part,
        Mat4::scale(Vec3::new(0.6, 0.45, 0.55)) * Mat4::translation(Vec3::new(0.0, 0.6, 0.0)),
        |p| {
            if ridged(p * 2.5) > 0.93 {
                crack
            } else {
                rock_mat
            }
        },
    );
    b
}

// ---------------------------------------------------------------------------
// Planeta Dulces
// ---------------------------------------------------------------------------

fn candy_lakes() -> Vec<Vec3> {
    scatter_directions(&mut Rng::new(0xCA7E), 4, 1.0, |_| true)
}

const ROAD_NORMALS: [Vec3; 2] = [
    Vec3::new(0.0, 1.0, 0.0),
    Vec3::new(0.70710677, 0.0, 0.70710677),
];

pub fn candy_height(d: Vec3, lakes: &[Vec3]) -> f32 {
    let mut h = (fbm(d * 2.0 + Vec3::new(5.0, 3.0, 1.0), 3) - 0.5) * 0.06;
    for l in lakes {
        let a = angle_between(d, *l);
        if a < 0.3 {
            h -= 0.035 * (1.0 - a / 0.3).powf(0.6);
        }
    }
    for n in ROAD_NORMALS {
        let band = d.dot(n).abs();
        if band < 0.045 {
            h = h.max(0.004) + 0.006;
        }
    }
    h
}

fn is_road(d: Vec3) -> bool {
    ROAD_NORMALS.iter().any(|n| d.dot(*n).abs() < 0.04)
}

/// Piruleta: palito + disco con espiral de dos colores.
pub fn lollipop_part() -> Part {
    let mut part = Part::default();
    let stick = cylinder(0.05, 0.05, 1.6, 6);
    part.merge(&stick, Mat4::identity());
    let mut disc = Part::default();
    let rings = 6;
    let wedges = 24;
    let center = disc.add_vertex(
        Vec3::new(0.0, 0.0, 0.12),
        Vec3::new(0.0, 0.0, 1.0),
        crate::math3d::Vec2::new(0.5, 0.5),
    );
    let back_center = disc.add_vertex(
        Vec3::new(0.0, 0.0, -0.12),
        Vec3::new(0.0, 0.0, -1.0),
        crate::math3d::Vec2::new(0.5, 0.5),
    );
    let mut front = vec![vec![0u32; wedges]; rings];
    let mut back = vec![vec![0u32; wedges]; rings];
    for r in 0..rings {
        let radius = 0.6 * (r + 1) as f32 / rings as f32;
        let z = 0.12 * (1.0 - ((r + 1) as f32 / rings as f32).powi(2)) + 0.02;
        for w in 0..wedges {
            let a = w as f32 / wedges as f32 * 2.0 * PI;
            let p = Vec3::new(a.cos() * radius, a.sin() * radius, z);
            let uv = crate::math3d::Vec2::new(0.5 + a.cos() * 0.5, 0.5 + a.sin() * 0.5);
            front[r][w] = disc.add_vertex(
                p,
                Vec3::new(a.cos() * 0.2, a.sin() * 0.2, 1.0).normalized(),
                uv,
            );
            back[r][w] = disc.add_vertex(
                Vec3::new(p.x, p.y, -z),
                Vec3::new(a.cos() * 0.2, a.sin() * 0.2, -1.0).normalized(),
                uv,
            );
        }
    }
    for w in 0..wedges {
        let wn = (w + 1) % wedges;
        let tag = |r: usize| (((w + r * 2) / 3) % 2) as u8;
        disc.add_triangle(center, front[0][w], front[0][wn]);
        disc.tags.push(tag(0));
        disc.add_triangle(back_center, back[0][wn], back[0][w]);
        disc.tags.push(tag(0));
        for r in 0..rings - 1 {
            disc.add_triangle(front[r][w], front[r + 1][w], front[r + 1][wn]);
            disc.add_triangle(front[r][w], front[r + 1][wn], front[r][wn]);
            disc.add_triangle(back[r][w], back[r + 1][wn], back[r + 1][w]);
            disc.add_triangle(back[r][w], back[r][wn], back[r + 1][wn]);
            for _ in 0..4 {
                disc.tags.push(tag(r + 1));
            }
        }
        // borde
        let r = rings - 1;
        disc.add_triangle(front[r][w], back[r][w], back[r][wn]);
        disc.add_triangle(front[r][w], back[r][wn], front[r][wn]);
        disc.tags.push(1);
        disc.tags.push(1);
    }
    let stick_tris = part.triangles.len();
    part.tags = vec![2; stick_tris];
    part.merge(&disc, Mat4::translation(Vec3::new(0.0, 1.9, 0.0)));
    part
}

/// Bastón de caramelo con rayas alternas.
pub fn candy_cane_part() -> Part {
    let mut path = Vec::new();
    for i in 0..14 {
        path.push(Vec3::new(0.0, i as f32 * 0.14, 0.0));
    }
    for i in 1..=12 {
        let a = i as f32 / 12.0 * PI;
        path.push(Vec3::new(0.32 - a.cos() * 0.32, 1.82 + a.sin() * 0.32, 0.0));
    }
    tube(&path, |_| 0.08, 8, 2)
}

/// Gomita: gota redondeada.
pub fn gumdrop_part() -> Part {
    let profile: Vec<(f32, f32)> = (0..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            (
                (1.0 - t * t).max(0.0).sqrt() * 0.4 * (1.0 - t * 0.3),
                t * 0.55,
            )
        })
        .collect();
    lathe(&profile, 14)
}

pub fn build_candy_planet() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let frosting = b.material("Dulces_Glaseado_Rosa", c(0.98, 0.62, 0.78));
    let mint = b.material("Dulces_Glaseado_Menta", c(0.6, 0.95, 0.8));
    let road = b.material("Dulces_Camino_Galleta", c(0.8, 0.58, 0.32));
    let chocolate = b.material("Dulces_Lago_Chocolate", c(0.33, 0.17, 0.08));
    let waffle = b.material("Dulces_Cono_Galleta", c(0.86, 0.62, 0.3));
    let strawberry = b.material("Dulces_Helado_Fresa", c(1.0, 0.55, 0.65));
    let vanilla = b.material("Dulces_Helado_Vainilla", c(1.0, 0.95, 0.78));
    let pistachio = b.material("Dulces_Helado_Menta", c(0.55, 0.9, 0.65));
    let red = b.material("Dulces_Caramelo_Rojo", c(0.95, 0.1, 0.15));
    let white = b.material("Dulces_Caramelo_Blanco", c(0.98, 0.98, 0.98));
    let stick = b.material("Dulces_Palito_Galleta", c(0.95, 0.9, 0.8));
    let house = b.material("Dulces_Casa_Turron", c(0.45, 0.25, 0.12));
    let roof_mat = b.material("Dulces_Techo_Glaseado", c(1.0, 0.98, 0.95));
    let door = b.material("Dulces_Puerta_Bombon", c(0.95, 0.4, 0.6));
    let gum = [
        b.material("Dulces_Gomita_Verde", c(0.3, 0.9, 0.3)),
        b.material("Dulces_Gomita_Naranja", c(1.0, 0.55, 0.1)),
        b.material("Dulces_Gomita_Morada", c(0.65, 0.3, 0.9)),
    ];
    let lakes = candy_lakes();
    b.object("Planeta_Dulces");
    let mut part = icosphere(5);
    part.displace_radial(|d| candy_height(d, &lakes));
    let lk = lakes.clone();
    b.add_classified(&part, Mat4::uniform_scale(PLANET_RADIUS), move |p| {
        let d = p.normalized();
        if lk.iter().any(|l| angle_between(d, *l) < 0.2) {
            chocolate
        } else if is_road(d) {
            road
        } else if fbm(d * 2.5, 2) > 0.55 {
            mint
        } else {
            frosting
        }
    });
    let surface = |d: Vec3| PLANET_RADIUS * (1.0 + candy_height(d, &lakes)) - 0.03;
    let free = |d: Vec3| !is_road(d) && lakes.iter().all(|l| angle_between(d, *l) > 0.3);
    let mut rng = Rng::new(0xD01CE);
    let spots = scatter_directions(&mut rng, 34, 0.22, free);
    // Montañas de helado: cono de galleta con bolas apiladas.
    b.object("Montanas_Helado");
    for (i, d) in spots.iter().take(6).enumerate() {
        let base = place_on_sphere(*d, surface(*d), i as f32, 1.0);
        let flavors = [strawberry, vanilla, pistachio];
        b.add(&cylinder(1.1, 0.55, 1.6, 14), base, waffle);
        for k in 0..3 {
            let scoop = Mat4::translation(Vec3::new(0.0, 1.8 + k as f32 * 0.72, 0.0))
                * Mat4::uniform_scale(0.75 - k as f32 * 0.12);
            b.add(&sphere(14, 9), base * scoop, flavors[(i + k) % 3]);
        }
    }
    // Casas de chocolate/turrón con techo glaseado.
    b.object("Casas_Chocolate");
    for (i, d) in spots.iter().skip(6).take(5).enumerate() {
        let base = place_on_sphere(*d, surface(*d), i as f32 * 1.3, 1.0);
        b.add(
            &cube(Vec3::new(1.0, 0.8, 0.9)),
            base * Mat4::translation(Vec3::new(0.0, 0.4, 0.0)),
            house,
        );
        b.add(
            &roof(1.25, 1.05, 0.6),
            base * Mat4::translation(Vec3::new(0.0, 0.8, 0.0)),
            roof_mat,
        );
        b.add(
            &cube(Vec3::new(0.3, 0.45, 0.05)),
            base * Mat4::translation(Vec3::new(0.0, 0.22, 0.46)),
            door,
        );
    }
    // Piruletas gigantes y bastones.
    b.object("Piruletas");
    let lolly = lollipop_part();
    for (i, d) in spots.iter().skip(11).take(8).enumerate() {
        let colors = [
            [red, white, stick],
            [pistachio, white, stick],
            [gum[2], vanilla, stick],
        ][i % 3];
        b.add_multi(
            &lolly,
            place_on_sphere(*d, surface(*d), i as f32, 0.9),
            &colors,
        );
    }
    b.object("Bastones_Caramelo");
    let cane = candy_cane_part();
    for (i, d) in spots.iter().skip(19).take(7).enumerate() {
        b.add_multi(
            &cane,
            place_on_sphere(*d, surface(*d), i as f32 * 2.0, 1.0),
            &[red, white],
        );
    }
    // Árboles de caramelo: palito con gomitas.
    b.object("Arboles_Caramelo");
    let drop = gumdrop_part();
    for (i, d) in spots.iter().skip(26).enumerate() {
        let base = place_on_sphere(*d, surface(*d), i as f32, 1.0);
        b.add(&cylinder(0.08, 0.06, 1.1, 6), base, stick);
        for k in 0..4 {
            let a = k as f32 * PI * 0.5;
            let t = Mat4::translation(Vec3::new(
                a.cos() * 0.3,
                1.0 + (k % 2) as f32 * 0.25,
                a.sin() * 0.3,
            )) * Mat4::uniform_scale(0.8);
            b.add(&drop, base * t, gum[(i + k) % 3]);
        }
        b.add(
            &sphere(10, 6),
            base * Mat4::translation(Vec3::new(0.0, 1.45, 0.0)) * Mat4::uniform_scale(0.35),
            gum[i % 3],
        );
    }
    b
}

pub fn build_lollipop() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let red = b.material("Piruleta_Caramelo_Rojo", c(0.95, 0.12, 0.2));
    let white = b.material("Piruleta_Caramelo_Blanco", c(0.98, 0.98, 0.98));
    let stick = b.material("Piruleta_Palito_Galleta", c(0.95, 0.9, 0.8));
    b.object("Piruleta");
    b.add_multi(&lollipop_part(), Mat4::identity(), &[red, white, stick]);
    b
}

pub fn build_candy_cane() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let red = b.material("Baston_Caramelo_Rojo", c(0.95, 0.1, 0.15));
    let white = b.material("Baston_Caramelo_Blanco", c(0.98, 0.98, 0.98));
    b.object("Baston_Caramelo");
    b.add_multi(&candy_cane_part(), Mat4::identity(), &[red, white]);
    b
}

pub fn build_gumdrop() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let gum = b.material("Gomita_Azucarada", c(0.95, 0.3, 0.5));
    b.object("Gomita");
    b.add(&gumdrop_part(), Mat4::identity(), gum);
    b
}

/// Aro de caramelo para la carrera (toro con rayas).
pub fn build_candy_ring() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let red = b.material("Aro_Caramelo_Rojo", c(1.0, 0.15, 0.3));
    let white = b.material("Aro_Caramelo_Blanco", c(1.0, 1.0, 1.0));
    b.object("Aro_Caramelo");
    let mut ring = torus(1.0, 0.13, 48, 10);
    ring.tags = (0..ring.triangles.len())
        .map(|i| ((i / (10 * 2 * 3)) % 2) as u8)
        .collect();
    b.add_multi(&ring, Mat4::identity(), &[red, white]);
    b
}

// ---------------------------------------------------------------------------
// Planeta Escandalosos (bosque caricaturesco con osos originales)
// ---------------------------------------------------------------------------

pub fn bear_land_height(d: Vec3) -> f32 {
    let base = (fbm(d * 1.8 + Vec3::new(3.0, 9.0, 4.0), 4) - 0.5) * 0.08;
    let mountain_mask = ((fbm(d * 1.2 + Vec3::new(7.0, 1.0, 2.0), 3) - 0.56) * 6.0).clamp(0.0, 1.0);
    let mountains = ridged(d * 3.0).powf(2.0) * 0.16 * mountain_mask;
    let mut h = base + mountains;
    let river = 1.0 - (fbm(d * 1.4 + Vec3::new(1.0, 4.0, 6.0), 3) * 2.0 - 1.0).abs();
    if river > 0.95 && mountain_mask < 0.3 {
        h = h.min(0.0) - (river - 0.95) * 0.5;
    }
    h
}

fn is_river(d: Vec3) -> bool {
    let h = bear_land_height(d);
    h < -0.005
}

pub fn pine_part() -> (Part, Part) {
    let trunk = cylinder(0.12, 0.09, 0.5, 6);
    let mut leaves = Part::default();
    for (i, (r, y, hgt)) in [(0.75, 0.35, 0.9), (0.6, 0.85, 0.8), (0.42, 1.3, 0.7)]
        .iter()
        .enumerate()
    {
        let _ = i;
        leaves.merge(
            &cylinder(*r, 0.0, *hgt, 8),
            Mat4::translation(Vec3::new(0.0, *y, 0.0)),
        );
    }
    (trunk, leaves)
}

pub fn round_tree_part() -> (Part, Part) {
    let trunk = cylinder(0.12, 0.1, 0.8, 6);
    let mut crown = Part::default();
    crown.merge(
        &icosphere(1),
        Mat4::translation(Vec3::new(0.0, 1.15, 0.0)) * Mat4::uniform_scale(0.6),
    );
    crown.merge(
        &icosphere(1),
        Mat4::translation(Vec3::new(0.3, 0.95, 0.15)) * Mat4::uniform_scale(0.4),
    );
    crown.merge(
        &icosphere(1),
        Mat4::translation(Vec3::new(-0.3, 1.0, -0.1)) * Mat4::uniform_scale(0.42),
    );
    (trunk, crown)
}

pub fn build_bear_planet() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let grass = b.material("Escandalosos_Pasto", c(0.36, 0.7, 0.28));
    let meadow = b.material("Escandalosos_Pradera_Clara", c(0.55, 0.8, 0.35));
    let rock_mat = b.material("Escandalosos_Montana_Roca", c(0.5, 0.45, 0.42));
    let snow = b.material("Escandalosos_Nieve", c(0.95, 0.96, 1.0));
    let water = b.material("Escandalosos_Rio_Agua", c(0.2, 0.55, 0.9));
    let bark = b.material("Escandalosos_Tronco", c(0.42, 0.26, 0.14));
    let pine = b.material("Escandalosos_Pino_Hoja", c(0.1, 0.45, 0.25));
    let leafy = b.material("Escandalosos_Arbol_Hoja", c(0.3, 0.62, 0.2));
    let logs = b.material("Escandalosos_Cabana_Madera", c(0.55, 0.33, 0.17));
    let roof_mat = b.material("Escandalosos_Techo_Rojo", c(0.78, 0.2, 0.15));
    let stone = b.material("Escandalosos_Piedra", c(0.6, 0.6, 0.62));
    let fire = b.emissive("Escandalosos_Fogata_Luminosa", c(1.0, 0.55, 0.1), 3.0);
    b.object("Planeta_Escandalosos");
    let mut part = icosphere(5);
    part.displace_radial(bear_land_height);
    b.add_classified(&part, Mat4::uniform_scale(PLANET_RADIUS), |p| {
        let d = p.normalized();
        let h = p.length() - 1.0;
        if h < -0.004 {
            water
        } else if h > 0.1 {
            snow
        } else if h > 0.055 {
            rock_mat
        } else if fbm(d * 3.0, 2) > 0.55 {
            meadow
        } else {
            grass
        }
    });
    let surface = |d: Vec3| PLANET_RADIUS * (1.0 + bear_land_height(d)) - 0.03;
    let flat = |d: Vec3| {
        let h = bear_land_height(d);
        (-0.0..0.045).contains(&h) && !is_river(d)
    };
    let mut rng = Rng::new(0xBEA2);
    // Bosques en grupos.
    b.object("Bosques");
    let (pine_trunk, pine_leaves) = pine_part();
    let (round_trunk, round_crown) = round_tree_part();
    let groves = scatter_directions(&mut rng, 9, 0.5, flat);
    for g in &groves {
        for _ in 0..8 {
            let offset = g
                .any_perpendicular()
                .rotate_around(*g, rng.range(0.0, 2.0 * PI));
            let d = (*g + offset * rng.range(0.02, 0.22)).normalized();
            if !flat(d) {
                continue;
            }
            let scale = rng.range(0.7, 1.2);
            let t = place_on_sphere(d, surface(d), rng.range(0.0, 6.0), scale);
            if rng.next_f32() < 0.6 {
                b.add(&pine_trunk, t, bark);
                b.add(&pine_leaves, t, pine);
            } else {
                b.add(&round_trunk, t, bark);
                b.add(&round_crown, t, leafy);
            }
        }
    }
    // Cabañas.
    b.object("Cabanas");
    let cabins = scatter_directions(&mut rng, 3, 0.8, flat);
    for (i, d) in cabins.iter().enumerate() {
        let base = place_on_sphere(*d, surface(*d), i as f32 * 2.1, 1.0);
        for k in 0..5 {
            let y = 0.1 + k as f32 * 0.17;
            let log = Mat4::translation(Vec3::new(0.0, y, 0.0)) * Mat4::rotation_z(PI * 0.5);
            b.add(
                &cylinder(0.09, 0.09, 1.3, 6),
                base * log * Mat4::translation(Vec3::new(0.0, -0.65, 0.45)),
                logs,
            );
            b.add(
                &cylinder(0.09, 0.09, 1.3, 6),
                base * log * Mat4::translation(Vec3::new(0.0, -0.65, -0.45)),
                logs,
            );
        }
        b.add(
            &cube(Vec3::new(1.2, 0.85, 0.9)),
            base * Mat4::translation(Vec3::new(0.0, 0.45, 0.0)),
            logs,
        );
        b.add(
            &roof(1.5, 1.2, 0.65),
            base * Mat4::translation(Vec3::new(0.0, 0.88, 0.0)),
            roof_mat,
        );
        // Fogata con piedras.
        let camp = base * Mat4::translation(Vec3::new(1.3, 0.0, 0.4));
        for k in 0..6 {
            let a = k as f32 / 6.0 * 2.0 * PI;
            b.add(
                &icosphere(0),
                camp * Mat4::translation(Vec3::new(a.cos() * 0.3, 0.05, a.sin() * 0.3))
                    * Mat4::uniform_scale(0.09),
                stone,
            );
        }
        b.add(&cylinder(0.18, 0.0, 0.4, 6), camp, fire);
    }
    // Rocas decorativas.
    b.object("Rocas");
    for d in scatter_directions(&mut rng, 14, 0.3, flat) {
        let r = rock(1, d.x * 10.0, 0.5);
        b.add(
            &r,
            place_on_sphere(d, surface(d), 0.0, rng.range(0.15, 0.3)),
            stone,
        );
    }
    b
}

pub fn build_pine() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let bark = b.material("Pino_Tronco", c(0.42, 0.26, 0.14));
    let leaves = b.material("Pino_Hoja", c(0.12, 0.45, 0.25));
    let (trunk, crown) = pine_part();
    b.object("Arbol_Pino");
    b.add(&trunk, Mat4::identity(), bark);
    b.add(&crown, Mat4::identity(), leaves);
    b
}

pub fn build_round_tree() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let bark = b.material("Arbol_Tronco", c(0.45, 0.28, 0.15));
    let leaves = b.material("Arbol_Hoja_Redonda", c(0.32, 0.65, 0.22));
    let (trunk, crown) = round_tree_part();
    b.object("Arbol_Redondo");
    b.add(&trunk, Mat4::identity(), bark);
    b.add(&crown, Mat4::identity(), leaves);
    b
}

#[derive(Clone, Copy)]
pub enum BearStyle {
    Brown,
    Panda,
    Polar,
}

/// Oso estilizado original hecho de primitivas (altura ~1.6, mira a +Z).
pub fn build_bear(style: BearStyle) -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let (fur, accent, muzzle_color, name) = match style {
        BearStyle::Brown => (
            c(0.5, 0.31, 0.17),
            c(0.5, 0.31, 0.17),
            c(0.85, 0.7, 0.5),
            "Oso_Pardo",
        ),
        BearStyle::Panda => (
            c(0.96, 0.96, 0.95),
            c(0.07, 0.07, 0.08),
            c(0.98, 0.98, 0.97),
            "Oso_Panda",
        ),
        BearStyle::Polar => (
            c(0.97, 0.97, 0.94),
            c(0.97, 0.97, 0.94),
            c(0.99, 0.98, 0.95),
            "Oso_Polar",
        ),
    };
    let fur_m = b.material(&format!("{name}_Pelaje"), fur);
    let accent_m = b.material(&format!("{name}_Pelaje_Detalle"), accent);
    let muzzle_m = b.material(&format!("{name}_Hocico"), muzzle_color);
    let black = b.material(&format!("{name}_Ojos_Nariz"), c(0.03, 0.03, 0.04));
    let belly = b.material(&format!("{name}_Panza"), fur * 1.05);
    b.object(name);
    let s = |x: f32, y: f32, z: f32| Mat4::scale(Vec3::new(x, y, z));
    let t = |x: f32, y: f32, z: f32| Mat4::translation(Vec3::new(x, y, z));
    let ball = sphere(14, 10);
    // Cuerpo y panza.
    b.add(&ball, t(0.0, 0.62, 0.0) * s(0.48, 0.58, 0.42), fur_m);
    b.add(&ball, t(0.0, 0.6, 0.2) * s(0.32, 0.4, 0.25), belly);
    // Cabeza, orejas, hocico, nariz y ojos.
    b.add(&ball, t(0.0, 1.32, 0.05) * s(0.36, 0.32, 0.33), fur_m);
    for side in [-1.0, 1.0] {
        b.add(
            &ball,
            t(side * 0.26, 1.6, 0.0) * s(0.11, 0.11, 0.07),
            accent_m,
        );
        // Ojos (con parche para el panda).
        if matches!(style, BearStyle::Panda) {
            b.add(
                &ball,
                t(side * 0.13, 1.37, 0.3) * s(0.09, 0.07, 0.04),
                accent_m,
            );
        }
        b.add(
            &ball,
            t(side * 0.12, 1.38, 0.34) * s(0.035, 0.04, 0.02),
            black,
        );
        // Brazos y piernas.
        b.add(
            &ball,
            t(side * 0.46, 0.78, 0.08) * Mat4::rotation_z(side * 0.5) * s(0.13, 0.28, 0.13),
            accent_m,
        );
        b.add(
            &ball,
            t(side * 0.22, 0.14, 0.05) * s(0.15, 0.15, 0.18),
            accent_m,
        );
    }
    b.add(&ball, t(0.0, 1.25, 0.3) * s(0.15, 0.1, 0.1), muzzle_m);
    b.add(&ball, t(0.0, 1.29, 0.39) * s(0.06, 0.04, 0.03), black);
    b
}

// ---------------------------------------------------------------------------
// Decoraciones de Hielo y Oasis (modelos nuevos, sin modificar los originales)
// ---------------------------------------------------------------------------

pub fn crystal_part() -> Part {
    let mut part = Part::default();
    let shard = lathe(&[(0.0, 0.0), (0.16, 0.08), (0.16, 0.8), (0.0, 1.1)], 6);
    for (i, (tilt, spin, scale)) in [
        (0.0, 0.0, 1.0),
        (0.45, 0.3, 0.7),
        (0.5, 2.4, 0.6),
        (0.4, 4.3, 0.75),
        (0.7, 1.4, 0.45),
    ]
    .iter()
    .enumerate()
    {
        let _ = i;
        let t = Mat4::rotation_y(*spin) * Mat4::rotation_z(*tilt) * Mat4::uniform_scale(*scale);
        part.merge(&shard, t);
    }
    part
}

pub fn build_crystal() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let crystal = b.material("Cristal_Energia_Hielo", c(0.45, 0.85, 1.0));
    b.object("Cristal_Energia");
    b.add(&crystal_part(), Mat4::identity(), crystal);
    b
}

pub fn build_ice_rock() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let stone = b.material("Roca_Helada_Piedra", c(0.4, 0.45, 0.52));
    let snow = b.material("Roca_Helada_Nieve", c(0.92, 0.96, 1.0));
    b.object("Roca_Helada");
    let r = rock(2, 31.0, 0.45);
    b.add_classified(
        &r,
        Mat4::scale(Vec3::new(0.6, 0.4, 0.5)) * Mat4::translation(Vec3::new(0.0, 0.5, 0.0)),
        |p| {
            if p.y > 0.25 { snow } else { stone }
        },
    );
    b
}

pub fn build_palm() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let trunk = b.material("Palmera_Tronco", c(0.5, 0.35, 0.2));
    let leaves = b.material("Palmera_Hoja_Verde", c(0.18, 0.6, 0.2));
    let coconut = b.material("Palmera_Coco", c(0.35, 0.22, 0.1));
    b.object("Palmera");
    let path: Vec<Vec3> = (0..12)
        .map(|i| {
            let t = i as f32 / 11.0;
            Vec3::new(t * t * 0.45, t * 2.2, 0.0)
        })
        .collect();
    b.add(
        &tube(&path, |t| 0.12 - t * 0.04, 7, 0),
        Mat4::identity(),
        trunk,
    );
    let top = Vec3::new(0.45, 2.2, 0.0);
    for i in 0..7 {
        let a = i as f32 / 7.0 * 2.0 * PI;
        b.add(
            &leaf(1.3, 0.22, 0.55, 6),
            Mat4::translation(top) * Mat4::rotation_y(a) * Mat4::rotation_z(0.25),
            leaves,
        );
    }
    for i in 0..3 {
        let a = i as f32 / 3.0 * 2.0 * PI;
        b.add(
            &sphere(8, 6),
            Mat4::translation(top + Vec3::new(a.cos() * 0.13, -0.12, a.sin() * 0.13))
                * Mat4::uniform_scale(0.1),
            coconut,
        );
    }
    b
}

pub fn build_cactus() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let green = b.material("Cactus_Verde", c(0.25, 0.55, 0.25));
    let flower = b.material("Cactus_Flor_Rosa", c(1.0, 0.4, 0.65));
    b.object("Cactus");
    let profile: Vec<(f32, f32)> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            let r = if t < 0.85 {
                0.2
            } else {
                0.2 * (1.0 - ((t - 0.85) / 0.15).powi(2)).max(0.0).sqrt()
            };
            (r, t * 1.8)
        })
        .collect();
    let body = lathe(&profile, 10);
    b.add(&body, Mat4::identity(), green);
    let arm = lathe(&profile, 8);
    b.add(
        &arm,
        Mat4::translation(Vec3::new(0.28, 0.6, 0.0)) * Mat4::uniform_scale(0.45),
        green,
    );
    b.add(
        &cylinder(0.08, 0.08, 0.3, 6),
        Mat4::translation(Vec3::new(0.0, 0.7, 0.0)) * Mat4::rotation_z(-PI * 0.5),
        green,
    );
    b.add(
        &arm,
        Mat4::translation(Vec3::new(-0.26, 0.9, 0.0)) * Mat4::uniform_scale(0.38),
        green,
    );
    b.add(
        &cylinder(0.07, 0.07, 0.28, 6),
        Mat4::translation(Vec3::new(0.0, 1.0, 0.0)) * Mat4::rotation_z(PI * 0.5),
        green,
    );
    b.add(
        &sphere(8, 5),
        Mat4::translation(Vec3::new(0.0, 1.82, 0.0)) * Mat4::uniform_scale(0.09),
        flower,
    );
    b
}

pub fn build_ruin() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let stone = b.material("Ruina_Piedra_Arenisca", c(0.82, 0.7, 0.5));
    let dark = b.material("Ruina_Piedra_Oscura", c(0.55, 0.45, 0.33));
    b.object("Ruina_Antigua");
    b.add(
        &cube(Vec3::new(2.4, 0.2, 1.4)),
        Mat4::translation(Vec3::new(0.0, 0.1, 0.0)),
        dark,
    );
    let heights = [1.6, 1.1, 1.6, 0.6];
    for (i, h) in heights.iter().enumerate() {
        let x = -0.9 + i as f32 * 0.6;
        b.add(
            &cylinder(0.14, 0.12, *h, 10),
            Mat4::translation(Vec3::new(x, 0.2, 0.0)),
            stone,
        );
    }
    b.add(
        &cube(Vec3::new(1.5, 0.18, 0.4)),
        Mat4::translation(Vec3::new(-0.6, 1.88, 0.0)),
        stone,
    );
    b.add(
        &cube(Vec3::new(0.4, 0.3, 0.4)),
        Mat4::translation(Vec3::new(0.9, 0.35, 0.6)) * Mat4::rotation_y(0.4),
        dark,
    );
    b
}

pub fn build_relic() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let gold = b.material("Reliquia_Oro_Metal", c(1.0, 0.78, 0.25));
    let stone = b.material("Reliquia_Pedestal_Piedra", c(0.6, 0.5, 0.4));
    let gem = b.emissive("Reliquia_Gema_Luminosa", c(0.2, 1.0, 0.8), 2.5);
    b.object("Reliquia_Desierto");
    b.add(&cylinder(0.35, 0.3, 0.2, 8), Mat4::identity(), stone);
    b.add(
        &lathe(
            &[
                (0.18, 0.2),
                (0.24, 0.4),
                (0.14, 0.75),
                (0.2, 0.9),
                (0.0, 0.95),
            ],
            12,
        ),
        Mat4::identity(),
        gold,
    );
    b.add(
        &sphere(10, 7),
        Mat4::translation(Vec3::new(0.0, 1.08, 0.0)) * Mat4::uniform_scale(0.16),
        gold,
    );
    b.add(
        &icosphere(0),
        Mat4::translation(Vec3::new(0.0, 1.08, 0.15)) * Mat4::uniform_scale(0.07),
        gem,
    );
    b
}

// ---------------------------------------------------------------------------
// Exportación
// ---------------------------------------------------------------------------

/// Entrada del catálogo: (carpeta relativa a recursos/modelos, nombre, constructor).
pub type CatalogEntry = (&'static str, &'static str, fn() -> ModelBuilder);

/// Lista de modelos generados.
pub fn catalog() -> Vec<CatalogEntry> {
    vec![
        (
            "planetas/tierra",
            "Tierra",
            build_earth as fn() -> ModelBuilder,
        ),
        ("planetas/tierra", "Nubes", build_clouds),
        ("planetas/tierra", "Plataforma", build_launch_pad),
        ("planetas/fuego", "Fuego", build_fire_planet),
        ("planetas/fuego", "roca_volcanica", build_volcanic_rock),
        ("planetas/dulces", "Dulces", build_candy_planet),
        ("planetas/dulces", "piruleta", build_lollipop),
        ("planetas/dulces", "baston", build_candy_cane),
        ("planetas/dulces", "gomita", build_gumdrop),
        ("planetas/dulces", "aro_caramelo", build_candy_ring),
        ("planetas/escandalosos", "Escandalosos", build_bear_planet),
        ("planetas/escandalosos", "arbol_pino", build_pine),
        ("planetas/escandalosos", "arbol_redondo", build_round_tree),
        ("planetas/escandalosos", "oso_pardo", || {
            build_bear(BearStyle::Brown)
        }),
        ("planetas/escandalosos", "oso_panda", || {
            build_bear(BearStyle::Panda)
        }),
        ("planetas/escandalosos", "oso_polar", || {
            build_bear(BearStyle::Polar)
        }),
        ("planetas/hielo", "cristal", build_crystal),
        ("planetas/hielo", "roca_helada", build_ice_rock),
        ("planetas/oasis", "palmera", build_palm),
        ("planetas/oasis", "cactus", build_cactus),
        ("planetas/oasis", "ruina", build_ruin),
        ("planetas/oasis", "reliquia", build_relic),
        ("espacio/sol", "Sol", build_sun),
        ("espacio/meteoritos", "meteorito_1", || build_meteor(1)),
        ("espacio/meteoritos", "meteorito_2", || build_meteor(2)),
        ("espacio/meteoritos", "meteorito_3", || build_meteor(3)),
        ("espacio/asteroides", "asteroide_1", || build_asteroid(1)),
        ("espacio/asteroides", "asteroide_2", || build_asteroid(2)),
        ("espacio/asteroides", "asteroide_3", || build_asteroid(3)),
        ("espacio/marcadores", "aro_control", build_checkpoint_ring),
        ("nave_espacial/cohete", "cohete", build_rocket),
    ]
}

/// Genera todos los modelos dentro de `models_root` (recursos/modelos).
pub fn generate_all(models_root: &Path) -> io::Result<Vec<(String, usize)>> {
    let mut report = Vec::new();
    for (folder, name, build) in catalog() {
        let builder = build();
        builder.write(
            &models_root.join(folder),
            name,
            &format!("Space Travel - modelo {name}"),
        )?;
        report.push((format!("{folder}/{name}.obj"), builder.triangle_count()));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_generated_model_has_real_geometry() {
        for (folder, name, build) in catalog() {
            let builder = build();
            let mesh = builder.to_mesh();
            assert!(mesh.triangles.len() >= 20, "{folder}/{name} casi vacío");
            for t in &mesh.triangles {
                for v in &t.vertices {
                    assert!(
                        v.position.x.is_finite()
                            && v.position.y.is_finite()
                            && v.position.z.is_finite()
                    );
                    assert!(
                        (v.normal.length() - 1.0).abs() < 0.02,
                        "{name}: normal no unitaria"
                    );
                }
            }
        }
    }

    #[test]
    fn planets_have_varied_terrain_and_materials() {
        for build in [
            build_fire_planet,
            build_candy_planet,
            build_bear_planet,
            build_earth,
        ] {
            let builder = build();
            assert!(builder.materials.len() >= 5);
            let mesh = builder.to_mesh();
            let mut min_r = f32::MAX;
            let mut max_r: f32 = 0.0;
            for t in &mesh.triangles {
                for v in &t.vertices {
                    min_r = min_r.min(v.position.length());
                    max_r = max_r.max(v.position.length());
                }
            }
            assert!(max_r - min_r > 0.3, "el terreno debe tener relieve real");
            assert!(mesh.triangles.len() > 20_000);
        }
    }

    #[test]
    fn launch_site_is_flat_land() {
        let h = earth_height(LAUNCH_DIR);
        assert!(h > 0.01);
        let nearby = (LAUNCH_DIR + Vec3::new(0.05, 0.0, 0.0)).normalized();
        assert!((earth_height(nearby) - h).abs() < 1.0e-4);
    }

    #[test]
    fn rocket_has_separable_fairing_halves() {
        let builder = build_rocket();
        let dir = std::env::temp_dir().join("space_travel_rocket_test");
        builder.write(&dir, "cohete", "test").unwrap();
        let objects = crate::model::load_obj_objects(dir.join("cohete.obj")).unwrap();
        let names: Vec<&str> = objects.iter().map(|(n, _)| n.as_str()).collect();
        for expected in [
            "Cuerpo",
            "Cofia_Izquierda",
            "Cofia_Derecha",
            "Tobera",
            "Aletas",
        ] {
            assert!(names.contains(&expected), "falta {expected}");
        }
    }
}
