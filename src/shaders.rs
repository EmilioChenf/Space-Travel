//! Shaders de fragmentos ejecutados por software en la CPU.
//!
//! Cada `ShaderKind` implementa un algoritmo de color distinto (no solo
//! constantes diferentes): ruido procedural 3D, bandas de *cel shading*,
//! grietas animadas, cáusticas, destellos, luces nocturnas, descarte por
//! umbral, etc. Todos reciben el fragmento interpolado por el rasterizador.

use crate::math3d::{Vec2, Vec3, smoothstep};
use crate::model::{Color3, MaterialKind};
use crate::renderer::{DrawOptions, Uniforms};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderKind {
    /// Carrocería metálica del DeLorean con luces de propulsores.
    Metal,
    /// Iluminación Blinn-Phong genérica que respeta materiales.
    Lit,
    /// Hielo: escarcha procedural, fresnel, destellos titilantes.
    Ice,
    /// Sol: plasma animado con granulación y oscurecimiento de limbo.
    Sun,
    /// Llama azul de los propulsores (aditivo).
    Thruster,
    /// Color plano sin iluminación.
    Unlit,
    /// Tierra: océanos especulares, luces de ciudades nocturnas, atmósfera.
    Earth,
    /// Nubes: ruido fBm con descarte por umbral y deriva temporal.
    Cloud,
    /// Planeta de fuego: lava que fluye y grietas incandescentes animadas.
    Lava,
    /// Dulces: cel shading, brillo intenso y chispas de azúcar.
    Candy,
    /// Desierto: ondas de arena, granulado y cáusticas en el agua.
    Desert,
    /// Bosque caricaturesco: bandas de color, contorno y viento.
    Forest,
    /// Objetos coleccionables / aros: emisivos pulsantes con fresnel.
    Glow,
    /// Meteoritos con borde incandescente según su dirección de avance.
    Meteor,
    /// Asteroides con cráteres procedurales.
    Rock,
    /// Pintura del cohete con paneles.
    Rocket,
    /// Fuego del cohete (aditivo, gradiente por altura y parpadeo).
    Exhaust,
    /// Halo atmosférico aditivo en el borde de un planeta.
    Atmosphere,
}

/// Fragmento interpolado que llega al shader.
#[derive(Clone, Copy, Debug)]
pub struct Fragment {
    pub position: Vec3,
    pub normal: Vec3,
    /// Posición en espacio del modelo: los patrones giran con el planeta.
    pub local: Vec3,
    pub uv: Vec2,
    pub color: Color3,
    pub material: MaterialKind,
}

// ---------------------------------------------------------------------------
// Ruido procedural
// ---------------------------------------------------------------------------

#[inline]
fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00FF_FFFF) as f32 / 16_777_215.0
}

/// Ruido de valor 3D con interpolación suave (0..1).
pub fn value_noise(p: Vec3) -> f32 {
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let (tx, ty, tz) = (p.x - fx, p.y - fy, p.z - fz);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy, sz) = (s(tx), s(ty), s(tz));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c000 = hash3(ix, iy, iz);
    let c100 = hash3(ix + 1, iy, iz);
    let c010 = hash3(ix, iy + 1, iz);
    let c110 = hash3(ix + 1, iy + 1, iz);
    let c001 = hash3(ix, iy, iz + 1);
    let c101 = hash3(ix + 1, iy, iz + 1);
    let c011 = hash3(ix, iy + 1, iz + 1);
    let c111 = hash3(ix + 1, iy + 1, iz + 1);
    lerp(
        lerp(lerp(c000, c100, sx), lerp(c010, c110, sx), sy),
        lerp(lerp(c001, c101, sx), lerp(c011, c111, sx), sy),
        sz,
    )
}

/// Movimiento browniano fraccional: suma de octavas de ruido.
pub fn fbm(p: Vec3, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut norm = 0.0;
    for _ in 0..octaves {
        sum += value_noise(p * frequency) * amplitude;
        norm += amplitude;
        amplitude *= 0.5;
        frequency *= 2.03;
    }
    sum / norm
}

/// Ruido "ridged": crestas finas donde el ruido cruza 0.5.
pub fn ridged(p: Vec3) -> f32 {
    1.0 - (value_noise(p) * 2.0 - 1.0).abs()
}

/// Valor aleatorio estable por celda (para chispas y luces).
pub fn cell_hash(p: Vec3) -> f32 {
    hash3(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
}

fn mix(a: Color3, b: Color3, t: f32) -> Color3 {
    a * (1.0 - t) + b * t
}

// ---------------------------------------------------------------------------
// Iluminación común
// ---------------------------------------------------------------------------

struct Lighting {
    n_dot_l: f32,
    to_sun: Vec3,
    to_camera: Vec3,
    rim: f32,
}

impl Lighting {
    fn new(f: &Fragment, u: &Uniforms) -> Self {
        let to_sun = (u.sun_position - f.position).normalized();
        let to_camera = (u.camera_position - f.position).normalized();
        Self {
            n_dot_l: f.normal.dot(to_sun),
            to_sun,
            to_camera,
            rim: 1.0 - f.normal.dot(to_camera).max(0.0),
        }
    }
    fn diffuse(&self) -> f32 {
        self.n_dot_l.max(0.0)
    }
    fn specular(&self, normal: Vec3, shininess: f32) -> f32 {
        let half = (self.to_sun + self.to_camera).normalized();
        normal.dot(half).max(0.0).powf(shininess)
    }
}

fn point_lights(f: &Fragment, u: &Uniforms) -> Color3 {
    let mut result = Color3::new(0.0, 0.0, 0.0);
    for light in &u.lights {
        let to_light = light.position - f.position;
        let distance = to_light.length();
        if distance >= light.radius || distance <= 1.0e-5 {
            continue;
        }
        let facing = f.normal.dot(to_light / distance).max(0.0);
        let attenuation = (1.0 - distance / light.radius).powi(2);
        result = result + light.color * (attenuation * (0.25 + facing * 1.4));
    }
    result
}

/// ¿El shader descarta fragmentos (prueba alfa)? Esos triángulos deben
/// evaluarse ya en el visibility buffer para no tapar lo que hay detrás.
pub fn uses_alpha_test(shader: ShaderKind) -> bool {
    matches!(shader, ShaderKind::Cloud)
}

/// Cobertura de un fragmento con prueba alfa (barata, sin iluminación).
pub fn covers(f: &Fragment, u: &Uniforms, shader: ShaderKind) -> bool {
    match shader {
        ShaderKind::Cloud => cloud_density(f, u) >= CLOUD_THRESHOLD,
        _ => true,
    }
}

const CLOUD_THRESHOLD: f32 = 0.6;

fn cloud_density(f: &Fragment, u: &Uniforms) -> f32 {
    let drift = Vec3::new(u.time * 0.03, 0.0, u.time * 0.017);
    fbm(f.local.normalized() * 2.4 + drift, 4)
}

/// Punto de entrada: devuelve el color final o `None` para descartar.
pub fn shade(f: &Fragment, u: &Uniforms, options: &DrawOptions) -> Option<Color3> {
    let color = match options.shader {
        ShaderKind::Unlit => f.color,
        ShaderKind::Sun => sun(f, u),
        ShaderKind::Thruster => thruster(f, u),
        ShaderKind::Exhaust => exhaust(f, u, options),
        ShaderKind::Atmosphere => atmosphere(f, u, options),
        ShaderKind::Cloud => return cloud(f, u).map(|c| c * options.tint),
        ShaderKind::Metal => metal(f, u),
        ShaderKind::Lit => lit(f, u),
        ShaderKind::Ice => ice(f, u),
        ShaderKind::Earth => earth(f, u),
        ShaderKind::Lava => lava(f, u),
        ShaderKind::Candy => candy(f, u),
        ShaderKind::Desert => desert(f, u),
        ShaderKind::Forest => forest(f, u),
        ShaderKind::Glow => return Some(glow(f, u, options)),
        ShaderKind::Meteor => meteor(f, u, options),
        ShaderKind::Rock => rock(f, u),
        ShaderKind::Rocket => rocket(f, u),
    };
    Some(color * options.tint)
}

fn emissive(f: &Fragment, u: &Uniforms, flicker: f32) -> Color3 {
    let pulse = 1.0 + flicker * (u.time * 11.0 + f.local.x * 7.0 + f.local.z * 5.0).sin();
    f.color * (1.15 * pulse) + Color3::new(0.25, 0.18, 0.1) * flicker
}

fn sun(f: &Fragment, u: &Uniforms) -> Color3 {
    let t = u.time;
    let n = f.local.normalized();
    let plasma = fbm(n * 3.2 + Vec3::new(t * 0.12, -t * 0.07, t * 0.05), 4);
    let granules = value_noise(n * 18.0 + Vec3::new(0.0, t * 0.6, 0.0));
    let waves = 0.5 + 0.5 * (n.y * 12.0 + n.x * 6.0 + t * 2.0 + plasma * 6.0).sin();
    let to_camera = (u.camera_position - f.position).normalized();
    let limb = f.normal.dot(to_camera).max(0.0);
    let heat = (plasma * 0.7 + waves * 0.2 + granules * 0.25).clamp(0.0, 1.2);
    let core = mix(
        Color3::new(1.0, 0.32, 0.03),
        Color3::new(1.0, 0.86, 0.38),
        heat,
    );
    // Limbo: el borde es más oscuro y rojizo, el centro más brillante.
    core * (0.65 + 0.55 * limb.powf(0.6)) + Color3::new(0.4, 0.1, 0.0) * (1.0 - limb).powi(3)
}

fn thruster(f: &Fragment, u: &Uniforms) -> Color3 {
    let pulse = 0.82 + 0.18 * (u.time * 18.0 + f.position.y * 10.0).sin();
    let hot_core = (1.0 - f.uv.y).clamp(0.0, 1.0);
    f.color * (1.15 * pulse) + Color3::new(0.72, 0.92, 1.0) * (0.38 + hot_core * 0.62)
}

fn exhaust(f: &Fragment, u: &Uniforms, options: &DrawOptions) -> Color3 {
    let along = f.uv.y.clamp(0.0, 1.0);
    let flicker = value_noise(Vec3::new(
        f.local.x * 3.0,
        f.local.y * 2.0 - u.time * 14.0,
        f.local.z * 3.0,
    ));
    let heat = (1.0 - along) * (0.6 + 0.6 * flicker);
    let color = if heat > 0.7 {
        mix(
            Color3::new(1.0, 0.85, 0.3),
            Color3::new(1.0, 1.0, 0.9),
            (heat - 0.7) / 0.3,
        )
    } else if heat > 0.35 {
        mix(
            Color3::new(1.0, 0.35, 0.05),
            Color3::new(1.0, 0.85, 0.3),
            (heat - 0.35) / 0.35,
        )
    } else {
        mix(
            Color3::new(0.25, 0.04, 0.02),
            Color3::new(1.0, 0.35, 0.05),
            heat / 0.35,
        )
    };
    color * (heat * 1.3 * options.scalar.max(0.2))
}

fn atmosphere(f: &Fragment, u: &Uniforms, options: &DrawOptions) -> Color3 {
    let light = Lighting::new(f, u);
    let facing = f.normal.dot(light.to_camera).abs();
    let rim = (1.0 - facing).powf(2.5);
    let day = 0.25 + 0.75 * smoothstep(-0.3, 0.4, light.n_dot_l);
    options.tint * (rim * day * 0.8)
}

fn cloud(f: &Fragment, u: &Uniforms) -> Option<Color3> {
    let density = cloud_density(f, u);
    if density < CLOUD_THRESHOLD {
        return None;
    }
    let light = Lighting::new(f, u);
    let thickness = smoothstep(0.6, 0.78, density);
    let shade = 0.32 + 0.75 * light.diffuse();
    Some(Color3::new(0.95, 0.97, 1.0) * shade * (0.75 + 0.25 * thickness))
}

fn metal(f: &Fragment, u: &Uniforms) -> Color3 {
    if f.material == MaterialKind::Emissive {
        return emissive(f, u, 0.05);
    }
    let light = Lighting::new(f, u);
    let shininess = if f.material == MaterialKind::Glass {
        90.0
    } else {
        48.0
    };
    let specular = light.specular(f.normal, shininess);
    let rim = light.rim.powf(2.2);
    let brushing = 0.92 + 0.08 * (f.position.x * 75.0 + f.position.z * 11.0).sin();
    let front_fill = f.normal.dot(light.to_camera).max(0.0);
    let base = f.color;
    base * (0.40 + 0.50 * light.diffuse() + 0.10 * front_fill) * brushing
        + base * rim * 0.11
        + Color3::new(0.10, 0.24, 0.42) * rim * 0.18
        + Color3::new(1.0, 0.94, 0.78) * specular * 0.72
        + point_lights(f, u)
}

fn lit(f: &Fragment, u: &Uniforms) -> Color3 {
    if f.material == MaterialKind::Emissive {
        return emissive(f, u, 0.12);
    }
    let light = Lighting::new(f, u);
    let (shininess, strength) = match f.material {
        MaterialKind::Water | MaterialKind::Glass => (60.0, 0.6),
        MaterialKind::Metal => (40.0, 0.5),
        _ => (10.0, 0.18),
    };
    let specular = light.specular(f.normal, shininess);
    let rim = light.rim.powf(2.2);
    f.color * (0.42 + 0.55 * light.diffuse())
        + f.color * rim * 0.12
        + Color3::new(0.32, 0.66, 0.78) * rim * 0.08
        + Color3::new(1.0, 0.92, 0.76) * specular * strength
        + point_lights(f, u) * 0.6
}

fn ice(f: &Fragment, u: &Uniforms) -> Color3 {
    if f.material == MaterialKind::Emissive {
        return emissive(f, u, 0.18);
    }
    let light = Lighting::new(f, u);
    let p = f.local;
    let frost = fbm(p * 2.3, 3);
    let mut base = mix(
        f.color,
        f.color * 1.18 + Color3::new(0.02, 0.07, 0.10),
        frost,
    );
    if matches!(f.material, MaterialKind::Water | MaterialKind::Ice) {
        // Grietas en el hielo: líneas finas donde el ruido cruza 0.5.
        let crack = ridged(p * 3.5).powf(18.0);
        base = base * (1.0 - 0.35 * crack) + Color3::new(0.6, 0.9, 1.0) * crack * 0.25;
    }
    let fresnel = light.rim.powf(3.0);
    let specular = light.specular(f.normal, 36.0);
    // Destellos: celdas aleatorias que titilan con el tiempo.
    let cell = cell_hash(p * 26.0);
    let sparkle = if cell > 0.975 {
        ((u.time * 3.0 + cell * 60.0).sin() * 0.5 + 0.5).powf(6.0) * light.diffuse().max(0.25)
    } else {
        0.0
    };
    // Dispersión subsuperficial falsa: el lado oscuro conserva tono cian.
    let subsurface = (1.0 - light.diffuse()) * 0.18;
    base * (0.46 + 0.46 * light.diffuse())
        + Color3::new(0.28, 0.72, 0.92) * (fresnel * 0.35 + subsurface)
        + Color3::new(0.85, 0.97, 1.0) * (specular * 0.45 + sparkle * 1.6)
        + point_lights(f, u) * 0.7
}

fn earth(f: &Fragment, u: &Uniforms) -> Color3 {
    if f.material == MaterialKind::Emissive {
        return emissive(f, u, 0.1);
    }
    let light = Lighting::new(f, u);
    let day = smoothstep(-0.15, 0.25, light.n_dot_l);
    let rim = light.rim;
    let mut color;
    if f.material == MaterialKind::Water {
        let waves = value_noise(f.local * 9.0 + Vec3::new(u.time * 0.3, 0.0, u.time * 0.2));
        let deep = Color3::new(0.03, 0.17, 0.5);
        let shallow = Color3::new(0.08, 0.42, 0.75);
        let base = mix(deep, shallow, (waves * 0.6 + 0.2).clamp(0.0, 1.0));
        let n = (f.normal + (f.local * 6.0).normalized() * ((waves - 0.5) * 0.08)).normalized();
        let glint = light.specular(n, 70.0);
        color = base * (0.18 + 0.82 * light.diffuse()) + Color3::new(1.0, 0.95, 0.8) * glint * 0.9;
    } else {
        color = f.color * (0.12 + 0.88 * light.diffuse());
        // Luces de ciudades en el lado nocturno.
        if !matches!(f.material, MaterialKind::Snow | MaterialKind::Rock) {
            let cell = cell_hash(f.local * 34.0);
            if cell > 0.9 {
                let city = (1.0 - day) * (cell - 0.9) * 10.0;
                color = color + Color3::new(1.0, 0.72, 0.32) * city * 0.9;
            }
        }
    }
    color + Color3::new(0.25, 0.55, 1.0) * rim.powf(2.5) * (0.15 + 0.6 * day)
}

fn lava(f: &Fragment, u: &Uniforms) -> Color3 {
    let t = u.time;
    let p = f.local;
    match f.material {
        MaterialKind::Lava => {
            // Lava que fluye: fBm desplazado en el tiempo y pulsos térmicos.
            let flow = fbm(p * 1.7 + Vec3::new(t * 0.22, -t * 0.15, t * 0.09), 4);
            let swirl = value_noise(p * 4.0 - Vec3::new(t * 0.4, t * 0.1, 0.0));
            let heat = (flow * 0.75 + swirl * 0.35).clamp(0.0, 1.0);
            let pulse = 0.85 + 0.15 * (t * 2.5 + flow * 9.0).sin();
            let color = mix(
                Color3::new(0.45, 0.02, 0.0),
                Color3::new(1.0, 0.42, 0.02),
                heat,
            );
            color * (1.2 * pulse) + Color3::new(1.0, 0.9, 0.35) * heat.powf(5.0) * 1.2
        }
        MaterialKind::Emissive => emissive(f, u, 0.25),
        _ => {
            let light = Lighting::new(f, u);
            // Grietas incandescentes que laten.
            let crack = ridged(p * 2.6 + Vec3::new(0.0, t * 0.05, 0.0)).powf(14.0);
            let glow_pulse = 0.65 + 0.35 * (t * 3.0 + p.x * 2.0 + p.z * 1.3).sin();
            let ash = fbm(p * 5.0, 2);
            let rock = f.color * (0.75 + 0.5 * ash);
            // Rebote rojizo de la lava desde abajo (hemisferio).
            let up = p.normalized();
            let bounce = (1.0 - f.normal.dot(up).max(0.0)) * 0.12;
            rock * (0.16 + 0.75 * light.diffuse())
                + Color3::new(0.9, 0.18, 0.02) * bounce
                + Color3::new(1.0, 0.36, 0.04) * crack * glow_pulse * 1.4
                + Color3::new(1.0, 0.7, 0.4) * light.specular(f.normal, 20.0) * 0.12
                + point_lights(f, u)
        }
    }
}

fn candy(f: &Fragment, u: &Uniforms) -> Color3 {
    if f.material == MaterialKind::Emissive {
        return emissive(f, u, 0.15);
    }
    let light = Lighting::new(f, u);
    let mut normal = f.normal;
    if f.material == MaterialKind::Chocolate {
        // Ondas en el chocolate líquido: perturbación animada de la normal.
        let w = Vec3::new(
            (f.local.x * 9.0 + u.time * 2.2).sin(),
            (f.local.y * 8.0 - u.time * 1.7).sin(),
            (f.local.z * 10.0 + u.time * 1.9).sin(),
        );
        normal = (normal + w * 0.12).normalized();
    }
    let d = normal.dot(light.to_sun).max(0.0);
    // Cel shading en tres bandas.
    let band = if d > 0.55 {
        1.0
    } else if d > 0.2 {
        0.78
    } else {
        0.55
    };
    let half = (light.to_sun + light.to_camera).normalized();
    let spec_raw = normal.dot(half).max(0.0).powf(70.0);
    let spec = if spec_raw > 0.45 { 1.0 } else { 0.0 };
    let mut base = f.color;
    if f.material == MaterialKind::Candy {
        // Chispas de azúcar de colores en el glaseado.
        let cell = cell_hash(f.local * 30.0);
        if cell > 0.93 {
            let palette = [
                Color3::new(1.0, 0.25, 0.5),
                Color3::new(0.3, 0.8, 1.0),
                Color3::new(1.0, 0.95, 0.3),
                Color3::new(0.5, 1.0, 0.45),
            ];
            base = palette[((cell - 0.93) * 57.0) as usize % 4];
        }
    }
    let rim = light.rim.powf(3.0);
    base * band
        + Color3::new(1.0, 0.7, 0.9) * rim * 0.35
        + Color3::new(1.0, 1.0, 1.0) * spec * 0.55
        + point_lights(f, u) * 0.5
}

fn desert(f: &Fragment, u: &Uniforms) -> Color3 {
    if f.material == MaterialKind::Emissive {
        return emissive(f, u, 0.1);
    }
    let light = Lighting::new(f, u);
    let p = f.local;
    match f.material {
        MaterialKind::Water => {
            // Cáusticas animadas.
            let c = ((p.x * 7.0 + u.time * 1.3).sin()
                + (p.z * 8.0 - u.time * 1.1).sin()
                + ((p.x + p.y) * 6.0 + u.time * 0.9).sin())
            .abs()
                / 3.0;
            let caustic = (1.0 - c).powf(6.0);
            f.color * (0.5 + 0.5 * light.diffuse())
                + Color3::new(0.7, 1.0, 0.95) * caustic * 0.45
                + Color3::new(1.0, 1.0, 0.95) * light.specular(f.normal, 80.0) * 0.8
        }
        MaterialKind::Sand | MaterialKind::Standard | MaterialKind::Rock => {
            // Ondas de arena moduladas por ruido + granulado fino.
            let warp = fbm(p * 1.3, 3);
            let ripples = 0.5 + 0.5 * ((p.x * 0.8 + p.z * 0.6) * 18.0 + warp * 9.0).sin();
            let grain = cell_hash(p * 90.0) * 0.08;
            let tone = fbm(p * 0.6 + Vec3::new(3.0, 1.0, 7.0), 2);
            let sand = mix(f.color, f.color * Color3::new(1.12, 0.92, 0.75), tone);
            let shade = 0.38 + 0.6 * light.diffuse();
            sand * (shade * (0.9 + 0.12 * ripples) + grain)
                + Color3::new(1.0, 0.75, 0.45) * light.rim.powf(3.0) * 0.18
                + point_lights(f, u) * 0.5
        }
        _ => {
            let sway = 0.94 + 0.06 * (u.time * 2.0 + p.y * 4.0 + p.x * 3.0).sin();
            f.color * (0.4 + 0.58 * light.diffuse()) * sway
                + Color3::new(1.0, 0.9, 0.7) * light.specular(f.normal, 16.0) * 0.15
        }
    }
}

fn forest(f: &Fragment, u: &Uniforms) -> Color3 {
    if f.material == MaterialKind::Emissive {
        return emissive(f, u, 0.2);
    }
    let light = Lighting::new(f, u);
    let p = f.local;
    let d = light.diffuse();
    let band = if d > 0.5 {
        1.0
    } else if d > 0.15 {
        0.72
    } else {
        0.48
    };
    // Ambiente hemisférico: cielo azul arriba, tierra cálida abajo.
    let up = if p.length() > 0.5 {
        p.normalized()
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let sky = f.normal.dot(up) * 0.5 + 0.5;
    let ambient = mix(
        Color3::new(0.35, 0.22, 0.12),
        Color3::new(0.45, 0.6, 0.85),
        sky,
    ) * 0.22;
    let mut base = f.color;
    match f.material {
        MaterialKind::Foliage => {
            let variation = fbm(p * 1.8, 2);
            let wind = 0.93 + 0.07 * (u.time * 2.6 + p.x * 3.0 + p.y * 2.0).sin();
            base = mix(base, base * Color3::new(1.15, 1.1, 0.7), variation) * wind;
        }
        MaterialKind::Water => {
            let flow = (p.x * 6.0 + p.y * 4.0 - u.time * 3.0).sin() * 0.5 + 0.5;
            base = mix(base, Color3::new(0.6, 0.85, 1.0), flow.powf(8.0) * 0.6);
        }
        _ => {}
    }
    let facing = f.normal.dot(light.to_camera);
    // Contorno de tinta en las siluetas (estilo caricatura).
    let outline = if facing < 0.22 { 0.35 } else { 1.0 };
    (base * band + base * ambient) * outline
        + Color3::new(1.0, 1.0, 0.95)
            * if light.specular(f.normal, 40.0) > 0.6 {
                0.25
            } else {
                0.0
            }
        + point_lights(f, u) * 0.6
}

fn glow(f: &Fragment, u: &Uniforms, options: &DrawOptions) -> Color3 {
    let light = Lighting::new(f, u);
    let pulse = 0.75 + 0.25 * (u.time * 4.0 + options.scalar).sin();
    let fresnel = light.rim.powf(2.0);
    let facets = 0.8 + 0.2 * f.normal.dot(light.to_sun).max(0.0);
    let base = mix(f.color, options.tint, 0.65);
    base * (0.55 + 0.45 * facets) * pulse
        + options.tint * fresnel * 0.9
        + Color3::new(1.0, 1.0, 1.0) * light.specular(f.normal, 50.0) * 0.6
}

fn meteor(f: &Fragment, u: &Uniforms, options: &DrawOptions) -> Color3 {
    let light = Lighting::new(f, u);
    let p = f.local;
    let craters = fbm(p * 3.0, 3);
    let base = f.color * (0.6 + 0.6 * craters);
    // `param` es la dirección de avance en espacio mundo.
    let direction = options.param.normalized();
    let leading = f.normal.dot(direction).max(0.0).powf(2.0);
    let flicker = 0.8 + 0.2 * value_noise(p * 6.0 + Vec3::new(u.time * 5.0, 0.0, 0.0));
    base * (0.18 + 0.8 * light.diffuse()) + Color3::new(1.0, 0.45, 0.08) * leading * flicker * 1.3
}

fn rock(f: &Fragment, u: &Uniforms) -> Color3 {
    let light = Lighting::new(f, u);
    let p = f.local;
    let crater = (1.0 - value_noise(p * 2.2)).powf(3.0);
    let speck = cell_hash(p * 40.0) * 0.1;
    f.color * ((0.15 + 0.8 * light.diffuse()) * (1.0 - crater * 0.45) + speck)
        + Color3::new(0.8, 0.85, 1.0) * light.specular(f.normal, 12.0) * 0.08
}

fn rocket(f: &Fragment, u: &Uniforms) -> Color3 {
    if f.material == MaterialKind::Emissive {
        return emissive(f, u, 0.1);
    }
    let light = Lighting::new(f, u);
    let panel = (f.local.y * 1.4).fract();
    let seam = if !(0.03..=0.97).contains(&panel) {
        0.7
    } else {
        1.0
    };
    let specular = light.specular(f.normal, 60.0);
    f.color * (0.45 + 0.6 * light.diffuse()) * seam
        + Color3::new(1.0, 0.97, 0.9) * specular * 0.6
        + Color3::new(0.3, 0.45, 0.7) * light.rim.powf(3.0) * 0.2
        + point_lights(f, u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniforms() -> Uniforms {
        Uniforms {
            time: 1.0,
            camera_position: Vec3::new(0.0, 0.0, 10.0),
            sun_position: Vec3::new(0.0, 0.0, 100.0),
            lights: Vec::new(),
        }
    }

    fn fragment(material: MaterialKind) -> Fragment {
        Fragment {
            position: Vec3::new(0.0, 0.0, 5.0),
            normal: Vec3::new(0.0, 0.0, 1.0),
            local: Vec3::new(0.3, 0.7, 5.0),
            uv: Vec2::new(0.5, 0.5),
            color: Color3::new(0.5, 0.5, 0.5),
            material,
        }
    }

    #[test]
    fn noise_is_bounded_and_continuous() {
        for i in 0..200 {
            let p = Vec3::new(i as f32 * 0.37, i as f32 * 0.11, -(i as f32) * 0.23);
            let n = value_noise(p);
            assert!((0.0..=1.0).contains(&n));
            let near = value_noise(p + Vec3::new(1.0e-3, 0.0, 0.0));
            assert!((n - near).abs() < 0.02);
        }
    }

    #[test]
    fn shaders_produce_distinct_results_for_the_same_fragment() {
        let u = uniforms();
        let f = fragment(MaterialKind::Standard);
        let kinds = [
            ShaderKind::Lit,
            ShaderKind::Ice,
            ShaderKind::Lava,
            ShaderKind::Candy,
            ShaderKind::Desert,
            ShaderKind::Forest,
            ShaderKind::Earth,
            ShaderKind::Rock,
        ];
        let colors: Vec<Color3> = kinds
            .iter()
            .map(|k| shade(&f, &u, &DrawOptions::new(*k)).unwrap())
            .collect();
        for i in 0..colors.len() {
            for j in i + 1..colors.len() {
                let d = (colors[i].r - colors[j].r).abs()
                    + (colors[i].g - colors[j].g).abs()
                    + (colors[i].b - colors[j].b).abs();
                assert!(
                    d > 0.005,
                    "{:?} y {:?} producen el mismo color",
                    kinds[i],
                    kinds[j]
                );
            }
        }
    }

    #[test]
    fn lava_changes_over_time() {
        let mut u = uniforms();
        let f = fragment(MaterialKind::Lava);
        let a = shade(&f, &u, &DrawOptions::new(ShaderKind::Lava)).unwrap();
        u.time = 7.3;
        let b = shade(&f, &u, &DrawOptions::new(ShaderKind::Lava)).unwrap();
        assert!((a.r - b.r).abs() + (a.g - b.g).abs() > 0.01);
        assert!(a.r > a.b, "la lava debe ser cálida");
    }

    #[test]
    fn clouds_discard_thin_regions() {
        let u = uniforms();
        let mut discarded = 0;
        for i in 0..400 {
            let mut f = fragment(MaterialKind::Cloud);
            let a = i as f32 * 0.21;
            f.local = Vec3::new(a.cos() * 5.0, (a * 0.7).sin() * 5.0, a.sin() * 5.0);
            if shade(&f, &u, &DrawOptions::new(ShaderKind::Cloud)).is_none() {
                discarded += 1;
            }
        }
        assert!(discarded > 50 && discarded < 400);
    }
}
