//! Geometría procedural reutilizable y exportación a OBJ/MTL.
//!
//! Las piezas (`Part`) son mallas indexadas en espacio local. Un
//! `ModelBuilder` las coloca con matrices de transformación, les asigna
//! materiales con nombre y produce tanto archivos OBJ/MTL válidos (con
//! `v`, `vt`, `vn` y `f v/vt/vn`) como mallas listas para el renderizador.

use crate::math3d::{Mat4, PI, Rng, Vec2, Vec3, apply3};
use crate::model::{Color3, MaterialKind, Mesh, Triangle, Vertex};
use crate::shaders::{fbm, ridged};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io;
use std::path::Path;

/// Malla indexada local.
#[derive(Clone, Debug, Default)]
pub struct Part {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u32; 3]>,
    /// Material relativo por triángulo (índice dentro de la paleta de la
    /// llamada a `ModelBuilder::add_part_multi`). Vacío = un solo material.
    pub tags: Vec<u8>,
}

impl Part {
    pub fn add_vertex(&mut self, position: Vec3, normal: Vec3, uv: Vec2) -> u32 {
        self.vertices.push(Vertex {
            position,
            normal,
            uv,
        });
        (self.vertices.len() - 1) as u32
    }

    pub fn add_triangle(&mut self, a: u32, b: u32, c: u32) {
        self.triangles.push([a, b, c]);
    }

    /// Recalcula normales suaves promediando las normales de cara.
    pub fn recompute_normals(&mut self) {
        let mut sums = vec![Vec3::ZERO; self.vertices.len()];
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.vertices[i as usize].position);
            let n = (b - a).cross(c - a);
            for &i in t {
                sums[i as usize] += n;
            }
        }
        for (vertex, sum) in self.vertices.iter_mut().zip(sums) {
            let n = sum.normalized();
            if n.length_squared() > 0.5 {
                vertex.normal = n;
            }
        }
    }

    /// Une otra pieza (ya transformada) a esta.
    pub fn merge(&mut self, other: &Part, transform: Mat4) {
        let offset = self.vertices.len() as u32;
        let normal = transform.normal_matrix();
        for v in &other.vertices {
            self.vertices.push(Vertex {
                position: transform.transform_point(v.position),
                normal: apply3(&normal, v.normal).normalized(),
                uv: v.uv,
            });
        }
        for t in &other.triangles {
            self.triangles
                .push([t[0] + offset, t[1] + offset, t[2] + offset]);
        }
        if !other.tags.is_empty() || !self.tags.is_empty() {
            self.tags
                .resize(self.triangles.len() - other.triangles.len(), 0);
            if other.tags.is_empty() {
                self.tags
                    .extend(std::iter::repeat_n(0, other.triangles.len()));
            } else {
                self.tags.extend_from_slice(&other.tags);
            }
        }
    }

    /// Invierte los triángulos cuya normal geométrica apunta hacia `center`
    /// (válido para piezas convexas como techos o prismas).
    pub fn orient_outward(&mut self, center: Vec3) {
        for t in &mut self.triangles {
            let [a, b, c] = t.map(|i| self.vertices[i as usize].position);
            let n = (b - a).cross(c - a);
            if n.dot((a + b + c) / 3.0 - center) < 0.0 {
                t.swap(1, 2);
            }
        }
    }

    /// Desplaza cada vértice a lo largo de su dirección radial.
    pub fn displace_radial(&mut self, height: impl Fn(Vec3) -> f32) {
        for v in &mut self.vertices {
            let dir = v.position.normalized();
            v.position = dir * (1.0 + height(dir));
        }
        self.recompute_normals();
    }

    pub fn transformed(mut self, transform: Mat4) -> Part {
        let normal = transform.normal_matrix();
        for v in &mut self.vertices {
            v.position = transform.transform_point(v.position);
            v.normal = apply3(&normal, v.normal).normalized();
        }
        self
    }
}

// ---------------------------------------------------------------------------
// Primitivas
// ---------------------------------------------------------------------------

fn spherical_uv(dir: Vec3) -> Vec2 {
    let u = 0.5 + dir.z.atan2(dir.x) / (2.0 * PI);
    let v = 0.5 - dir.y.clamp(-1.0, 1.0).asin() / PI;
    Vec2::new(u, v)
}

/// Icosfera unitaria con subdivisiones (20 * 4^n triángulos).
pub fn icosphere(subdivisions: u32) -> Part {
    let t = (1.0 + 5.0_f32.sqrt()) * 0.5;
    let mut positions: Vec<Vec3> = [
        (-1.0, t, 0.0),
        (1.0, t, 0.0),
        (-1.0, -t, 0.0),
        (1.0, -t, 0.0),
        (0.0, -1.0, t),
        (0.0, 1.0, t),
        (0.0, -1.0, -t),
        (0.0, 1.0, -t),
        (t, 0.0, -1.0),
        (t, 0.0, 1.0),
        (-t, 0.0, -1.0),
        (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| Vec3::new(x, y, z).normalized())
    .collect();
    let mut faces: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..subdivisions {
        let mut cache: HashMap<(u32, u32), u32> = HashMap::new();
        let mut midpoint = |a: u32, b: u32, positions: &mut Vec<Vec3>| {
            let key = (a.min(b), a.max(b));
            *cache.entry(key).or_insert_with(|| {
                positions
                    .push(((positions[a as usize] + positions[b as usize]) * 0.5).normalized());
                (positions.len() - 1) as u32
            })
        };
        let mut next = Vec::with_capacity(faces.len() * 4);
        for [a, b, c] in faces {
            let ab = midpoint(a, b, &mut positions);
            let bc = midpoint(b, c, &mut positions);
            let ca = midpoint(c, a, &mut positions);
            next.push([a, ab, ca]);
            next.push([b, bc, ab]);
            next.push([c, ca, bc]);
            next.push([ab, bc, ca]);
        }
        faces = next;
    }
    let mut part = Part::default();
    for p in positions {
        part.add_vertex(p, p, spherical_uv(p));
    }
    // Las caras del icosaedro base ya están en sentido antihorario (CCW)
    // vistas desde fuera, que es la orientación frontal del rasterizador.
    part.triangles = faces;
    part
}

/// Superficie de revolución alrededor de +Y a partir de un perfil (radio, y).
pub fn lathe(profile: &[(f32, f32)], segments: usize) -> Part {
    lathe_arc(profile, segments, 0.0, 2.0 * PI)
}

/// Revolución parcial (para dividir la cofia del cohete en dos mitades).
pub fn lathe_arc(profile: &[(f32, f32)], segments: usize, start: f32, end: f32) -> Part {
    let segments = segments.max(3);
    let mut part = Part::default();
    let rows = profile.len();
    let closed = (end - start - 2.0 * PI).abs() < 1.0e-4;
    let columns = if closed { segments } else { segments + 1 };
    for s in 0..columns {
        let u = s as f32 / segments as f32;
        let angle = start + (end - start) * u;
        let (sin, cos) = angle.sin_cos();
        for (r, &(radius, y)) in profile.iter().enumerate() {
            let v = r as f32 / (rows - 1).max(1) as f32;
            part.add_vertex(
                Vec3::new(cos * radius, y, sin * radius),
                Vec3::new(cos, 0.0, sin),
                Vec2::new(u, v),
            );
        }
    }
    for s in 0..segments {
        let s_next = if closed { (s + 1) % segments } else { s + 1 };
        for r in 0..rows - 1 {
            let a = (s * rows + r) as u32;
            let b = (s_next * rows + r) as u32;
            let c = (s_next * rows + r + 1) as u32;
            let d = (s * rows + r + 1) as u32;
            // Se omiten triángulos degenerados en los polos (radio cero).
            if profile[r].0 > 1.0e-5 {
                part.add_triangle(a, c, b);
            }
            if profile[r + 1].0 > 1.0e-5 {
                part.add_triangle(a, d, c);
            }
        }
    }
    // Tapas planas cuando el perfil no termina en el eje.
    if closed {
        for &(row, flip) in &[(0usize, true), (rows - 1, false)] {
            let (radius, y) = profile[row];
            if radius <= 1.0e-4 {
                continue;
            }
            let normal = Vec3::new(0.0, if flip { -1.0 } else { 1.0 }, 0.0);
            let center = part.add_vertex(Vec3::new(0.0, y, 0.0), normal, Vec2::new(0.5, 0.5));
            let first = part.vertices.len() as u32;
            for s in 0..segments {
                let angle = start + (end - start) * s as f32 / segments as f32;
                part.add_vertex(
                    Vec3::new(angle.cos() * radius, y, angle.sin() * radius),
                    normal,
                    Vec2::new(0.5 + angle.cos() * 0.5, 0.5 + angle.sin() * 0.5),
                );
            }
            for s in 0..segments as u32 {
                let a = first + s;
                let b = first + (s + 1) % segments as u32;
                if flip {
                    part.add_triangle(center, a, b);
                } else {
                    part.add_triangle(center, b, a);
                }
            }
        }
    }
    part.recompute_normals_keep_caps();
    part
}

impl Part {
    /// Recalcula normales solo en los vértices compartidos por más de una
    /// cara lateral (las tapas ya tienen normales planas exactas).
    fn recompute_normals_keep_caps(&mut self) {
        let mut sums = vec![Vec3::ZERO; self.vertices.len()];
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.vertices[i as usize].position);
            let n = (b - a).cross(c - a);
            for &i in t {
                sums[i as usize] += n;
            }
        }
        for (vertex, sum) in self.vertices.iter_mut().zip(sums) {
            if vertex.normal.y.abs() > 0.999 && vertex.normal.x == 0.0 && vertex.normal.z == 0.0 {
                continue;
            }
            let n = sum.normalized();
            if n.length_squared() > 0.5 {
                vertex.normal = n;
            }
        }
    }
}

/// Cilindro (o cono si `top_radius` = 0) de altura `height` sobre +Y.
pub fn cylinder(bottom_radius: f32, top_radius: f32, height: f32, segments: usize) -> Part {
    lathe(
        &[(bottom_radius, 0.0), (top_radius.max(0.0), height)],
        segments,
    )
}

/// Esfera UV unitaria; útil para cuerpos y cabezas ovaladas.
pub fn sphere(segments: usize, rings: usize) -> Part {
    let rings = rings.max(3);
    let profile: Vec<(f32, f32)> = (0..=rings)
        .map(|i| {
            let phi = PI * i as f32 / rings as f32;
            (phi.sin(), -phi.cos())
        })
        .collect();
    let mut part = lathe(&profile, segments);
    for v in &mut part.vertices {
        if v.position.length() > 1.0e-4 {
            v.normal = v.position.normalized();
        }
    }
    part
}

/// Caja centrada con normales planas por cara.
pub fn cube(size: Vec3) -> Part {
    let h = size * 0.5;
    let mut part = Part::default();
    let faces = [
        (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
        (Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
        (Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
        (Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 1.0, 0.0)),
    ];
    for (normal, up) in faces {
        let right = up.cross(normal);
        let corner = |a: f32, b: f32| (normal + right * a + up * b).mul_elem(h);
        let base = part.vertices.len() as u32;
        for (a, b, u, v) in [
            (-1.0, -1.0, 0.0, 1.0),
            (1.0, -1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0, 0.0),
            (-1.0, 1.0, 0.0, 0.0),
        ] {
            part.add_vertex(corner(a, b), normal, Vec2::new(u, v));
        }
        part.add_triangle(base, base + 1, base + 2);
        part.add_triangle(base, base + 2, base + 3);
    }
    part
}

/// Prisma triangular (techo a dos aguas) con base `width` x `depth`.
pub fn roof(width: f32, depth: f32, height: f32) -> Part {
    let w = width * 0.5;
    let d = depth * 0.5;
    let mut part = Part::default();
    let p = [
        Vec3::new(-w, 0.0, -d),
        Vec3::new(w, 0.0, -d),
        Vec3::new(0.0, height, -d),
        Vec3::new(-w, 0.0, d),
        Vec3::new(w, 0.0, d),
        Vec3::new(0.0, height, d),
    ];
    let mut quad = |a: usize, b: usize, c: usize, d2: usize| {
        let n = (p[b] - p[a]).cross(p[c] - p[a]).normalized();
        let base = part.vertices.len() as u32;
        for (i, &k) in [a, b, c, d2].iter().enumerate() {
            part.add_vertex(p[k], n, Vec2::new((i % 2) as f32, (i / 2) as f32));
        }
        part.add_triangle(base, base + 1, base + 2);
        part.add_triangle(base, base + 2, base + 3);
    };
    quad(1, 4, 5, 2);
    quad(3, 0, 2, 5);
    quad(0, 1, 4, 3);
    let mut tri = |a: usize, b: usize, c: usize| {
        let n = (p[b] - p[a]).cross(p[c] - p[a]).normalized();
        let base = part.vertices.len() as u32;
        for &k in &[a, b, c] {
            part.add_vertex(p[k], n, Vec2::new(0.5, 0.5));
        }
        part.add_triangle(base, base + 1, base + 2);
    };
    tri(0, 2, 1);
    tri(3, 4, 5);
    part.orient_outward(Vec3::new(0.0, height * 0.33, 0.0));
    part
}

/// Toro en el plano XZ (aros de control y de caramelo).
pub fn torus(major: f32, minor: f32, segments: usize, sides: usize) -> Part {
    let mut part = Part::default();
    for i in 0..segments {
        let u = i as f32 / segments as f32;
        let a = u * 2.0 * PI;
        let center = Vec3::new(a.cos() * major, 0.0, a.sin() * major);
        let outward = Vec3::new(a.cos(), 0.0, a.sin());
        for j in 0..sides {
            let v = j as f32 / sides as f32;
            let b = v * 2.0 * PI;
            let normal = outward * b.cos() + Vec3::new(0.0, b.sin(), 0.0);
            part.add_vertex(center + normal * minor, normal, Vec2::new(u, v));
        }
    }
    for i in 0..segments {
        for j in 0..sides {
            let a = (i * sides + j) as u32;
            let b = (((i + 1) % segments) * sides + j) as u32;
            let c = (((i + 1) % segments) * sides + (j + 1) % sides) as u32;
            let d = (i * sides + (j + 1) % sides) as u32;
            part.add_triangle(a, c, b);
            part.add_triangle(a, d, c);
        }
    }
    part
}

/// Tubo a lo largo de una trayectoria (bastones de caramelo, troncos).
/// `tags` alterna cada `stripe` segmentos (para rayas).
pub fn tube(path: &[Vec3], radius: impl Fn(f32) -> f32, sides: usize, stripe: usize) -> Part {
    let mut part = Part::default();
    let n = path.len();
    let mut previous_normal = (path[1] - path[0]).normalized().any_perpendicular();
    for (i, &p) in path.iter().enumerate() {
        let tangent = if i + 1 < n {
            (path[i + 1] - p).normalized()
        } else {
            (p - path[i - 1]).normalized()
        };
        let normal = previous_normal.reject(tangent).normalized();
        previous_normal = normal;
        let binormal = tangent.cross(normal);
        let t = i as f32 / (n - 1) as f32;
        let r = radius(t);
        for j in 0..sides {
            let a = j as f32 / sides as f32 * 2.0 * PI;
            let dir = normal * a.cos() + binormal * a.sin();
            part.add_vertex(p + dir * r, dir, Vec2::new(j as f32 / sides as f32, t));
        }
    }
    for i in 0..n - 1 {
        let tag = i.checked_div(stripe).map_or(0, |v| (v % 2) as u8);
        for j in 0..sides {
            let a = (i * sides + j) as u32;
            let b = ((i + 1) * sides + j) as u32;
            let c = ((i + 1) * sides + (j + 1) % sides) as u32;
            let d = (i * sides + (j + 1) % sides) as u32;
            part.add_triangle(a, c, b);
            part.add_triangle(a, d, c);
            part.tags.push(tag);
            part.tags.push(tag);
        }
    }
    // Tapa final redondeada simple (abanico).
    let last = path[n - 1];
    let tip = part.add_vertex(
        last + (last - path[n - 2]).normalized() * radius(1.0) * 0.6,
        (last - path[n - 2]).normalized(),
        Vec2::new(0.5, 1.0),
    );
    let tag = (n - 1).checked_div(stripe).map_or(0, |v| (v % 2) as u8);
    for j in 0..sides {
        let a = ((n - 1) * sides + j) as u32;
        let b = ((n - 1) * sides + (j + 1) % sides) as u32;
        part.add_triangle(a, b, tip);
        part.tags.push(tag);
    }
    part.recompute_normals();
    part
}

/// Hoja plana arqueada (palmeras): tira doble cara.
pub fn leaf(length: f32, width: f32, droop: f32, segments: usize) -> Part {
    let mut part = Part::default();
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let x = t * length;
        let y = -droop * t * t;
        let w = width * (1.0 - t).max(0.05) * (t * 3.0).min(1.0);
        let n = Vec3::new(2.0 * droop * t / length, 1.0, 0.0).normalized();
        part.add_vertex(Vec3::new(x, y, -w), n, Vec2::new(t, 0.0));
        part.add_vertex(Vec3::new(x, y + w * 0.25, 0.0), n, Vec2::new(t, 0.5));
        part.add_vertex(Vec3::new(x, y, w), n, Vec2::new(t, 1.0));
    }
    for i in 0..segments as u32 {
        let r = i * 3;
        let s = r + 3;
        for k in 0..2 {
            part.add_triangle(r + k, s + k + 1, s + k);
            part.add_triangle(r + k, r + k + 1, s + k + 1);
            // cara inferior
            part.add_triangle(r + k, s + k, s + k + 1);
            part.add_triangle(r + k, s + k + 1, r + k + 1);
        }
    }
    part
}

/// Roca irregular: icosfera desplazada por ruido.
pub fn rock(subdivisions: u32, seed: f32, roughness: f32) -> Part {
    let mut part = icosphere(subdivisions);
    let offset = Vec3::new(seed * 3.1, seed * 1.7, seed * 2.3);
    part.displace_radial(|d| {
        (fbm(d * 1.8 + offset, 4) - 0.5) * roughness
            + (ridged(d * 3.0 + offset) - 0.5) * roughness * 0.3
    });
    part
}

// ---------------------------------------------------------------------------
// Transformaciones de colocación
// ---------------------------------------------------------------------------

/// Matriz que alinea el eje +Y local con `up` (normal de la superficie).
pub fn align_y(up: Vec3, spin: f32) -> Mat4 {
    let y = up.normalized();
    let x0 = y.any_perpendicular();
    let x = x0.rotate_around(y, spin);
    let z = x.cross(y);
    Mat4::from_basis(x, y, z, Vec3::ZERO)
}

/// Transformación para colocar un objeto sobre una esfera en la dirección
/// `dir` a una distancia `radius` del centro, orientado según la normal.
pub fn place_on_sphere(dir: Vec3, radius: f32, spin: f32, scale: f32) -> Mat4 {
    Mat4::translation(dir.normalized() * radius) * align_y(dir, spin) * Mat4::uniform_scale(scale)
}

// ---------------------------------------------------------------------------
// Constructor de modelos con materiales y exportación OBJ/MTL
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct MaterialDef {
    pub name: String,
    pub diffuse: Color3,
    pub emission: Color3,
    pub shininess: f32,
}

struct ObjectData {
    name: String,
    faces: Vec<([u32; 3], usize)>,
}

#[derive(Default)]
pub struct ModelBuilder {
    pub materials: Vec<MaterialDef>,
    vertices: Vec<Vertex>,
    objects: Vec<ObjectData>,
}

impl ModelBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn find_or_add(&mut self, def: MaterialDef) -> usize {
        if let Some(i) = self.materials.iter().position(|m| m.name == def.name) {
            return i;
        }
        self.materials.push(def);
        self.materials.len() - 1
    }

    /// Registra (o reutiliza) un material difuso.
    pub fn material(&mut self, name: &str, diffuse: Color3) -> usize {
        self.find_or_add(MaterialDef {
            name: name.to_owned(),
            diffuse,
            emission: Color3::new(0.0, 0.0, 0.0),
            shininess: 32.0,
        })
    }

    /// Registra un material emisivo (Ke > 0).
    pub fn emissive(&mut self, name: &str, color: Color3, strength: f32) -> usize {
        self.find_or_add(MaterialDef {
            name: name.to_owned(),
            diffuse: color,
            emission: color * strength,
            shininess: 64.0,
        })
    }

    pub fn object(&mut self, name: &str) {
        self.objects.push(ObjectData {
            name: name.to_owned(),
            faces: Vec::new(),
        });
    }

    /// Añade una pieza transformada con un único material.
    pub fn add(&mut self, part: &Part, transform: Mat4, material: usize) {
        self.add_multi(part, transform, &[material]);
    }

    /// Añade una pieza cuyos triángulos eligen material por `tags`.
    pub fn add_multi(&mut self, part: &Part, transform: Mat4, palette: &[usize]) {
        if self.objects.is_empty() {
            self.object("Objeto");
        }
        let offset = self.vertices.len() as u32;
        let normal = transform.normal_matrix();
        let flip = {
            let m = transform.m;
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            det < 0.0
        };
        for v in &part.vertices {
            self.vertices.push(Vertex {
                position: transform.transform_point(v.position),
                normal: apply3(&normal, v.normal).normalized(),
                uv: v.uv,
            });
        }
        let object = self.objects.last_mut().expect("objeto activo");
        for (i, t) in part.triangles.iter().enumerate() {
            let tag = part.tags.get(i).copied().unwrap_or(0) as usize;
            let material = palette[tag.min(palette.len() - 1)];
            let tri = if flip {
                [t[0] + offset, t[2] + offset, t[1] + offset]
            } else {
                [t[0] + offset, t[1] + offset, t[2] + offset]
            };
            object.faces.push((tri, material));
        }
    }

    /// Añade una pieza eligiendo el material de cada cara con una función
    /// de la posición (local, antes de transformar) de su centro.
    pub fn add_classified(
        &mut self,
        part: &Part,
        transform: Mat4,
        classify: impl Fn(Vec3) -> usize,
    ) {
        let mut tagged = part.clone();
        let mut palette: Vec<usize> = Vec::new();
        tagged.tags = part
            .triangles
            .iter()
            .map(|t| {
                let c = (part.vertices[t[0] as usize].position
                    + part.vertices[t[1] as usize].position
                    + part.vertices[t[2] as usize].position)
                    / 3.0;
                let material = classify(c);
                match palette.iter().position(|&m| m == material) {
                    Some(i) => i as u8,
                    None => {
                        palette.push(material);
                        (palette.len() - 1) as u8
                    }
                }
            })
            .collect();
        if palette.is_empty() {
            return;
        }
        self.add_multi(&tagged, transform, &palette);
    }

    pub fn triangle_count(&self) -> usize {
        self.objects.iter().map(|o| o.faces.len()).sum()
    }

    /// Convierte el modelo en una malla para el renderizador.
    pub fn to_mesh(&self) -> Mesh {
        let mut triangles = Vec::with_capacity(self.triangle_count());
        for object in &self.objects {
            for (face, material) in &object.faces {
                let m = &self.materials[*material];
                let emissive = m.emission.r + m.emission.g + m.emission.b > 0.05;
                triangles.push(Triangle {
                    vertices: face.map(|i| self.vertices[i as usize]),
                    color: m.diffuse,
                    material: if emissive {
                        MaterialKind::Emissive
                    } else {
                        MaterialKind::from_name(&m.name)
                    },
                });
            }
        }
        Mesh::from_triangles(triangles)
    }

    /// Escribe `<nombre>.obj` y `<nombre>.mtl` en `directory`.
    pub fn write(&self, directory: &Path, name: &str, header: &str) -> io::Result<()> {
        std::fs::create_dir_all(directory)?;
        let mut mtl = String::new();
        let _ = writeln!(mtl, "# {header}");
        let _ = writeln!(
            mtl,
            "# Generado proceduralmente por Space Travel (src/procgen.rs, src/worlds.rs)"
        );
        for m in &self.materials {
            let _ = writeln!(mtl, "\nnewmtl {}", m.name);
            let _ = writeln!(mtl, "Ns {:.3}", m.shininess);
            let _ = writeln!(mtl, "Ka 1.000000 1.000000 1.000000");
            let _ = writeln!(
                mtl,
                "Kd {:.6} {:.6} {:.6}",
                m.diffuse.r, m.diffuse.g, m.diffuse.b
            );
            let _ = writeln!(mtl, "Ks 0.500000 0.500000 0.500000");
            let _ = writeln!(
                mtl,
                "Ke {:.6} {:.6} {:.6}",
                m.emission.r, m.emission.g, m.emission.b
            );
            let _ = writeln!(mtl, "Ni 1.450000\nd 1.000000\nillum 2");
        }
        std::fs::write(directory.join(format!("{name}.mtl")), mtl)?;

        let mut obj = String::with_capacity(self.vertices.len() * 90);
        let _ = writeln!(obj, "# {header}");
        let _ = writeln!(
            obj,
            "# Generado proceduralmente por Space Travel (src/procgen.rs, src/worlds.rs)"
        );
        let _ = writeln!(
            obj,
            "# Vertices: {}  Triangulos: {}",
            self.vertices.len(),
            self.triangle_count()
        );
        let _ = writeln!(obj, "mtllib {name}.mtl");
        for v in &self.vertices {
            let _ = writeln!(
                obj,
                "v {:.5} {:.5} {:.5}",
                v.position.x, v.position.y, v.position.z
            );
        }
        for v in &self.vertices {
            let _ = writeln!(obj, "vt {:.4} {:.4}", v.uv.x, 1.0 - v.uv.y);
        }
        for v in &self.vertices {
            let _ = writeln!(
                obj,
                "vn {:.4} {:.4} {:.4}",
                v.normal.x, v.normal.y, v.normal.z
            );
        }
        for object in &self.objects {
            if object.faces.is_empty() {
                continue;
            }
            let _ = writeln!(obj, "o {}", object.name);
            let mut current = usize::MAX;
            for (face, material) in &object.faces {
                if *material != current {
                    current = *material;
                    let _ = writeln!(obj, "usemtl {}", self.materials[current].name);
                }
                let [a, b, c] = face.map(|i| i + 1);
                let _ = writeln!(obj, "f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}");
            }
        }
        std::fs::write(directory.join(format!("{name}.obj")), obj)
    }
}

/// Distribuye `count` direcciones aleatorias reproducibles sobre la esfera
/// con una separación angular mínima (muestreo por rechazo).
pub fn scatter_directions(
    rng: &mut Rng,
    count: usize,
    min_angle: f32,
    accept: impl Fn(Vec3) -> bool,
) -> Vec<Vec3> {
    let min_dot = min_angle.cos();
    let mut result: Vec<Vec3> = Vec::with_capacity(count);
    let mut attempts = 0;
    while result.len() < count && attempts < count * 200 {
        attempts += 1;
        let d = rng.unit_vector();
        if !accept(d) {
            continue;
        }
        if result.iter().all(|o| o.dot(d) < min_dot) {
            result.push(d);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icosphere_has_expected_size_and_outward_faces() {
        let part = icosphere(2);
        assert_eq!(part.triangles.len(), 20 * 16);
        for t in &part.triangles {
            let [a, b, c] = t.map(|i| part.vertices[i as usize].position);
            let n = (b - a).cross(c - a);
            assert!(n.dot(a + b + c) > 0.0, "las caras deben mirar hacia fuera");
        }
    }

    #[test]
    fn lathe_and_primitives_face_outward() {
        for part in [
            cylinder(1.0, 0.5, 2.0, 12),
            sphere(12, 8),
            cube(Vec3::new(1.0, 2.0, 3.0)),
            roof(2.0, 3.0, 1.0),
        ] {
            let center = part.vertices.iter().fold(Vec3::ZERO, |a, v| a + v.position)
                / part.vertices.len() as f32;
            let mut outward = 0;
            for t in &part.triangles {
                let [a, b, c] = t.map(|i| part.vertices[i as usize].position);
                let n = (b - a).cross(c - a);
                if n.dot((a + b + c) / 3.0 - center) > 0.0 {
                    outward += 1;
                }
            }
            assert!(
                outward as f32 >= part.triangles.len() as f32 * 0.95,
                "{outward} de {}",
                part.triangles.len()
            );
        }
    }

    #[test]
    fn torus_faces_point_away_from_its_core_circle() {
        let part = torus(2.0, 0.4, 16, 8);
        for t in &part.triangles {
            let [a, b, c] = t.map(|i| part.vertices[i as usize].position);
            let centroid = (a + b + c) / 3.0;
            let core = Vec3::new(centroid.x, 0.0, centroid.z).normalized() * 2.0;
            assert!((b - a).cross(c - a).dot(centroid - core) > 0.0);
        }
    }

    #[test]
    fn builder_writes_a_loadable_obj() {
        let mut builder = ModelBuilder::new();
        let red = builder.material("Prueba_Roja", Color3::new(1.0, 0.0, 0.0));
        let lava = builder.emissive("Prueba_Lava", Color3::new(1.0, 0.4, 0.0), 3.0);
        builder.object("Esfera");
        builder.add(&icosphere(1), Mat4::identity(), red);
        builder.object("Caja");
        builder.add(
            &cube(Vec3::new(1.0, 1.0, 1.0)),
            Mat4::translation(Vec3::new(3.0, 0.0, 0.0)),
            lava,
        );
        let dir = std::env::temp_dir().join("space_travel_builder_test");
        builder.write(&dir, "prueba", "test").unwrap();
        let objects = crate::model::load_obj_objects(dir.join("prueba.obj")).unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].1.triangles.len(), 80);
        assert_eq!(objects[1].1.triangles[0].material, MaterialKind::Emissive);
        assert!((objects[0].1.triangles[0].color.r - 1.0).abs() < 1.0e-4);
    }

    #[test]
    fn scatter_is_reproducible_and_spaced() {
        let a = scatter_directions(&mut Rng::new(9), 20, 0.3, |_| true);
        let b = scatter_directions(&mut Rng::new(9), 20, 0.3, |_| true);
        assert_eq!(a.len(), 20);
        for (x, y) in a.iter().zip(&b) {
            assert!((*x - *y).length() < 1.0e-6);
        }
        for i in 0..a.len() {
            for j in i + 1..a.len() {
                assert!(a[i].dot(a[j]) < 0.3_f32.cos() + 1.0e-5);
            }
        }
    }
}
