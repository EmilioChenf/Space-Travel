use crate::math3d::{PI, Vec2, Vec3};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub struct Color3 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3 {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }
}

impl std::ops::Add for Color3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.r + rhs.r, self.g + rhs.g, self.b + rhs.b)
    }
}
impl std::ops::Mul<f32> for Color3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.r * rhs, self.g * rhs, self.b * rhs)
    }
}
impl std::ops::Mul for Color3 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(self.r * rhs.r, self.g * rhs.g, self.b * rhs.b)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

/// Clasificación semántica del material. Se deduce del nombre del material
/// en el MTL (o de su emisión `Ke`) y permite que cada shader por software
/// trate de forma distinta el agua, la lava, el hielo, los dulces, etc.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MaterialKind {
    #[default]
    Standard,
    Water,
    Lava,
    Ice,
    Snow,
    Emissive,
    Candy,
    Chocolate,
    Glass,
    Foliage,
    Cloud,
    Sand,
    Metal,
    Rock,
}

impl MaterialKind {
    /// Deduce el tipo de material a partir de su nombre. El orden importa:
    /// "Hielo_Fogata" es emisivo aunque contenga "Hielo".
    pub fn from_name(name: &str) -> Self {
        let n = name.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
        if has(&[
            "fogata", "emisivo", "luminos", "neon", "faros", "luces", "brasa",
        ]) {
            Self::Emissive
        } else if has(&["lava", "magma", "incandescente"]) {
            Self::Lava
        } else if has(&["chocolate", "cacao"]) {
            Self::Chocolate
        } else if has(&["congelado", "cristal_hielo", "cristal_energia"]) {
            Self::Ice
        } else if has(&["oceano", "agua", "rio", "lago", "mar_"]) {
            Self::Water
        } else if has(&["nieve", "escarcha"]) {
            Self::Snow
        } else if has(&["hielo", "iglu", "glaciar", "placa_helada"]) {
            Self::Ice
        } else if has(&[
            "caramelo", "dulce", "piruleta", "gomita", "helado", "glaseado", "galleta", "bombon",
            "menta", "algodon", "fresa",
        ]) {
            Self::Candy
        } else if has(&["vidrio", "parabrisas", "ventana", "cristal"]) {
            Self::Glass
        } else if has(&[
            "hoja", "arbol", "pasto", "bosque", "pradera", "cesped", "pino", "cactus",
        ]) {
            Self::Foliage
        } else if has(&["nube"]) {
            Self::Cloud
        } else if has(&["arena", "duna", "desierto"]) {
            Self::Sand
        } else if has(&["acero", "cromado", "metal", "aluminio"]) {
            Self::Metal
        } else if has(&["roca", "piedra", "basalto", "ceniza", "montana", "crater"]) {
            Self::Rock
        } else {
            Self::Standard
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub vertices: [Vertex; 3],
    pub color: Color3,
    pub material: MaterialKind,
}

impl Triangle {
    pub fn new(vertices: [Vertex; 3], color: Color3) -> Self {
        Self {
            vertices,
            color,
            material: MaterialKind::Standard,
        }
    }
}

/// Malla como lista de triángulos independientes, con una esfera envolvente
/// en espacio local que usa el descarte por frustum y la selección de LOD.
#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub triangles: Vec<Triangle>,
    pub center: Vec3,
    pub radius: f32,
}

impl Mesh {
    pub fn from_triangles(triangles: Vec<Triangle>) -> Self {
        let mut mesh = Self {
            triangles,
            ..Self::default()
        };
        mesh.recompute_bounds();
        mesh
    }

    /// Calcula una esfera envolvente (centro de la caja AABB y radio máximo).
    pub fn recompute_bounds(&mut self) {
        if self.triangles.is_empty() {
            self.center = Vec3::ZERO;
            self.radius = 0.0;
            return;
        }
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for t in &self.triangles {
            for v in &t.vertices {
                min = Vec3::new(
                    min.x.min(v.position.x),
                    min.y.min(v.position.y),
                    min.z.min(v.position.z),
                );
                max = Vec3::new(
                    max.x.max(v.position.x),
                    max.y.max(v.position.y),
                    max.z.max(v.position.z),
                );
            }
        }
        let center = (min + max) * 0.5;
        let mut radius: f32 = 0.0;
        for t in &self.triangles {
            for v in &t.vertices {
                radius = radius.max((v.position - center).length());
            }
        }
        self.center = center;
        self.radius = radius;
    }

    /// Simplificación por agrupamiento de vértices (vertex clustering).
    /// Cada vértice se mueve al promedio de su celda; los triángulos que
    /// colapsan se eliminan. Se usa para generar niveles de detalle (LOD).
    pub fn decimated(&self, cell_size: f32) -> Mesh {
        if cell_size <= 0.0 {
            return self.clone();
        }
        let key = |p: Vec3| {
            (
                (p.x / cell_size).floor() as i32,
                (p.y / cell_size).floor() as i32,
                (p.z / cell_size).floor() as i32,
            )
        };
        let mut sums: HashMap<(i32, i32, i32), (Vec3, Vec3, u32)> = HashMap::new();
        for t in &self.triangles {
            for v in &t.vertices {
                let entry = sums
                    .entry(key(v.position))
                    .or_insert((Vec3::ZERO, Vec3::ZERO, 0));
                entry.0 += v.position;
                entry.1 += v.normal;
                entry.2 += 1;
            }
        }
        let mut triangles = Vec::with_capacity(self.triangles.len() / 3);
        for t in &self.triangles {
            let keys = t.vertices.map(|v| key(v.position));
            if keys[0] == keys[1] || keys[1] == keys[2] || keys[0] == keys[2] {
                continue;
            }
            let mut vertices = t.vertices;
            for (vertex, k) in vertices.iter_mut().zip(keys) {
                let (p, n, count) = sums[&k];
                vertex.position = p / count as f32;
                let n = n.normalized();
                if n.length_squared() > 0.5 {
                    vertex.normal = n;
                }
            }
            triangles.push(Triangle { vertices, ..*t });
        }
        let mut mesh = Mesh::from_triangles(triangles);
        // Conservar la esfera original evita que el LOD cambie el culling.
        mesh.center = self.center;
        mesh.radius = self.radius;
        mesh
    }

    pub fn append(&mut self, other: &Mesh) {
        self.triangles.extend_from_slice(&other.triangles);
        self.recompute_bounds();
    }
}

/// Niveles de detalle de una misma malla. `levels[0]` es la malla completa.
/// Cada nivel indica a partir de qué radio proyectado (en píxeles) se usa.
#[derive(Debug, Default, Clone)]
pub struct LodMesh {
    pub levels: Vec<(Mesh, f32)>,
}

impl LodMesh {
    pub fn single(mesh: Mesh) -> Self {
        Self {
            levels: vec![(mesh, 0.0)],
        }
    }

    /// Crea niveles adicionales usando tamaños de celda relativos al radio.
    /// `steps` contiene pares (fracción del radio, radio mínimo en píxeles).
    pub fn with_levels(mesh: Mesh, steps: &[(f32, f32)]) -> Self {
        let radius = mesh.radius.max(1.0e-3);
        let mut levels = Vec::with_capacity(steps.len() + 1);
        let mut threshold_full = 0.0;
        if let Some(first) = steps.first() {
            threshold_full = first.1;
        }
        let decimated: Vec<(Mesh, f32)> = steps
            .iter()
            .enumerate()
            .map(|(i, (fraction, _))| {
                let next = steps.get(i + 1).map(|s| s.1).unwrap_or(0.0);
                (mesh.decimated(radius * fraction), next)
            })
            .collect();
        levels.push((mesh, threshold_full));
        levels.extend(decimated);
        Self { levels }
    }

    pub fn full(&self) -> &Mesh {
        &self.levels[0].0
    }

    /// Elige el nivel según el radio que ocupa el objeto en pantalla.
    pub fn select(&self, projected_radius_px: f32) -> (usize, &Mesh) {
        for (i, (mesh, min_px)) in self.levels.iter().enumerate() {
            if projected_radius_px >= *min_px {
                return (i, mesh);
            }
        }
        let last = self.levels.len() - 1;
        (last, &self.levels[last].0)
    }
}

#[derive(Clone, Copy, Default)]
struct FaceIndex {
    position: i32,
    uv: i32,
    normal: i32,
}

fn resolve_index(index: i32, count: usize) -> Option<usize> {
    let value = if index > 0 {
        index - 1
    } else if index < 0 {
        count as i32 + index
    } else {
        -1
    };
    (value >= 0 && value < count as i32).then_some(value as usize)
}

fn parse_face_index(token: &str) -> Result<FaceIndex, io::Error> {
    let mut parts = token.split('/');
    let parse = |value: Option<&str>| -> Result<i32, io::Error> {
        match value {
            Some("") | None => Ok(0),
            Some(text) => text
                .parse()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "índice OBJ inválido")),
        }
    };
    Ok(FaceIndex {
        position: parse(parts.next())?,
        uv: parse(parts.next())?,
        normal: parse(parts.next())?,
    })
}

/// Material leído del MTL: color difuso, emisión y tipo semántico.
#[derive(Clone, Copy, Debug)]
pub struct MaterialInfo {
    pub diffuse: Color3,
    pub emission: Color3,
    pub kind: MaterialKind,
}

pub fn load_materials(path: &Path) -> HashMap<String, MaterialInfo> {
    let mut result: HashMap<String, MaterialInfo> = HashMap::new();
    let Ok(contents) = fs::read_to_string(path) else {
        return result;
    };
    let mut current = String::new();
    for line in contents.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("newmtl") => {
                current = words.next().unwrap_or_default().to_owned();
                result.insert(
                    current.clone(),
                    MaterialInfo {
                        diffuse: Color3::new(0.72, 0.76, 0.82),
                        emission: Color3::new(0.0, 0.0, 0.0),
                        kind: MaterialKind::from_name(&current),
                    },
                );
            }
            Some(key @ ("Kd" | "Ke")) if !current.is_empty() => {
                let values: Vec<f32> = words.filter_map(|word| word.parse().ok()).collect();
                if values.len() >= 3
                    && let Some(material) = result.get_mut(&current)
                {
                    let color = Color3::new(values[0], values[1], values[2]);
                    if key == "Kd" {
                        material.diffuse = color;
                    } else {
                        material.emission = color;
                        if color.r + color.g + color.b > 0.05 {
                            material.kind = MaterialKind::Emissive;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    result
}

fn make_vertex(
    index: FaceIndex,
    positions: &[Vec3],
    uvs: &[Vec2],
    normals: &[Vec3],
) -> Result<Vertex, io::Error> {
    let position = resolve_index(index.position, positions.len())
        .and_then(|i| positions.get(i).copied())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "cara OBJ sin posición válida")
        })?;
    let uv = resolve_index(index.uv, uvs.len())
        .and_then(|i| uvs.get(i).copied())
        .unwrap_or_default();
    let normal = resolve_index(index.normal, normals.len())
        .and_then(|i| normals.get(i).copied())
        .unwrap_or_default();
    Ok(Vertex {
        position,
        normal,
        uv,
    })
}

/// Carga un OBJ conservando cada objeto (`o`/`g`) como malla independiente.
/// Así se pueden animar partes por separado (cofia del cohete, osos, etc.).
pub fn load_obj_objects(path: impl AsRef<Path>) -> Result<Vec<(String, Mesh)>, io::Error> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;
    let directory = path.parent().unwrap_or(Path::new("."));
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut normals = Vec::new();
    let mut materials = HashMap::new();
    let mut current = MaterialInfo {
        diffuse: Color3::new(0.72, 0.76, 0.82),
        emission: Color3::new(0.0, 0.0, 0.0),
        kind: MaterialKind::Standard,
    };
    let mut objects: Vec<(String, Vec<Triangle>)> = vec![("default".to_owned(), Vec::new())];

    for line in contents.lines() {
        let mut words = line.split_whitespace();
        let Some(command) = words.next() else {
            continue;
        };
        match command {
            "v" => {
                let values: Vec<f32> = words.filter_map(|word| word.parse().ok()).collect();
                if values.len() >= 3 {
                    positions.push(Vec3::new(values[0], values[1], values[2]));
                }
            }
            "vt" => {
                let values: Vec<f32> = words.filter_map(|word| word.parse().ok()).collect();
                if values.len() >= 2 {
                    uvs.push(Vec2::new(values[0], values[1]));
                }
            }
            "vn" => {
                let values: Vec<f32> = words.filter_map(|word| word.parse().ok()).collect();
                if values.len() >= 3 {
                    normals.push(Vec3::new(values[0], values[1], values[2]));
                }
            }
            "o" | "g" => {
                let name = words.next().unwrap_or("objeto").to_owned();
                if objects.last().is_some_and(|(_, t)| t.is_empty()) {
                    objects.pop();
                }
                objects.push((name, Vec::new()));
            }
            "mtllib" => {
                if let Some(file) = words.next() {
                    materials = load_materials(&directory.join(file));
                }
            }
            "usemtl" => {
                if let Some(name) = words.next() {
                    current = materials.get(name).copied().unwrap_or(MaterialInfo {
                        diffuse: Color3::new(0.72, 0.76, 0.82),
                        emission: Color3::new(0.0, 0.0, 0.0),
                        kind: MaterialKind::from_name(name),
                    });
                }
            }
            "f" => {
                let face: Result<Vec<_>, _> = words.map(parse_face_index).collect();
                let face = face?;
                for i in 1..face.len().saturating_sub(1) {
                    let mut vertices = [
                        make_vertex(face[0], &positions, &uvs, &normals)?,
                        make_vertex(face[i], &positions, &uvs, &normals)?,
                        make_vertex(face[i + 1], &positions, &uvs, &normals)?,
                    ];
                    let face_normal = (vertices[1].position - vertices[0].position)
                        .cross(vertices[2].position - vertices[0].position)
                        .normalized();
                    for vertex in &mut vertices {
                        if vertex.normal.length_squared() < 1.0e-8 {
                            vertex.normal = face_normal;
                        }
                    }
                    let color = if current.kind == MaterialKind::Emissive
                        && current.emission.r + current.emission.g + current.emission.b > 0.05
                    {
                        let e = current.emission;
                        let m = e.r.max(e.g).max(e.b).max(1.0);
                        Color3::new(e.r / m, e.g / m, e.b / m)
                    } else {
                        current.diffuse
                    };
                    if let Some((_, triangles)) = objects.last_mut() {
                        triangles.push(Triangle {
                            vertices,
                            color,
                            material: current.kind,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let result: Vec<(String, Mesh)> = objects
        .into_iter()
        .filter(|(_, t)| !t.is_empty())
        .map(|(name, t)| (name, Mesh::from_triangles(t)))
        .collect();
    if result.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "el OBJ no contiene triángulos",
        ));
    }
    Ok(result)
}

/// Carga un OBJ completo como una sola malla.
pub fn load_obj(path: impl AsRef<Path>) -> Result<Mesh, io::Error> {
    let objects = load_obj_objects(path)?;
    let total = objects.iter().map(|(_, m)| m.triangles.len()).sum();
    let mut triangles = Vec::with_capacity(total);
    for (_, mesh) in objects {
        triangles.extend(mesh.triangles);
    }
    Ok(Mesh::from_triangles(triangles))
}

pub fn make_uv_sphere(longitudes: usize, latitudes: usize) -> Mesh {
    let longitudes = longitudes.max(3);
    let latitudes = latitudes.max(2);
    let sphere_vertex = |longitude: usize, latitude: usize| {
        let u = longitude as f32 / longitudes as f32;
        let v = latitude as f32 / latitudes as f32;
        let theta = u * 2.0 * PI;
        let phi = v * PI;
        let normal = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
        Vertex {
            position: normal,
            normal,
            uv: Vec2::new(u, v),
        }
    };
    let mut mesh = Mesh::default();
    for latitude in 0..latitudes {
        for longitude in 0..longitudes {
            let a = sphere_vertex(longitude, latitude);
            let b = sphere_vertex(longitude + 1, latitude);
            let c = sphere_vertex(longitude + 1, latitude + 1);
            let d = sphere_vertex(longitude, latitude + 1);
            let white = Color3::new(1.0, 1.0, 1.0);
            if latitude > 0 {
                mesh.triangles.push(Triangle::new([a, d, b], white));
            }
            if latitude + 1 < latitudes {
                mesh.triangles.push(Triangle::new([b, d, c], white));
            }
        }
    }
    mesh.recompute_bounds();
    mesh
}

/// Genera la carcasa metálica de un propulsor orientado hacia el eje -Y.
pub fn make_thruster_nozzle(segments: usize) -> Mesh {
    let segments = segments.max(3);
    let top_radius = 0.23;
    let bottom_radius = 0.17;
    let height = 0.24;
    let color = Color3::new(0.08, 0.12, 0.18);
    let mut mesh = Mesh::default();

    for index in 0..segments {
        let angle_a = index as f32 / segments as f32 * 2.0 * PI;
        let angle_b = (index + 1) as f32 / segments as f32 * 2.0 * PI;
        let normal_a = Vec3::new(
            angle_a.cos(),
            (top_radius - bottom_radius) / height,
            angle_a.sin(),
        )
        .normalized();
        let normal_b = Vec3::new(
            angle_b.cos(),
            (top_radius - bottom_radius) / height,
            angle_b.sin(),
        )
        .normalized();
        let top_a = Vertex {
            position: Vec3::new(angle_a.cos() * top_radius, 0.0, angle_a.sin() * top_radius),
            normal: normal_a,
            uv: Vec2::new(index as f32 / segments as f32, 0.0),
        };
        let top_b = Vertex {
            position: Vec3::new(angle_b.cos() * top_radius, 0.0, angle_b.sin() * top_radius),
            normal: normal_b,
            uv: Vec2::new((index + 1) as f32 / segments as f32, 0.0),
        };
        let bottom_a = Vertex {
            position: Vec3::new(
                angle_a.cos() * bottom_radius,
                -height,
                angle_a.sin() * bottom_radius,
            ),
            normal: normal_a,
            uv: Vec2::new(index as f32 / segments as f32, 1.0),
        };
        let bottom_b = Vertex {
            position: Vec3::new(
                angle_b.cos() * bottom_radius,
                -height,
                angle_b.sin() * bottom_radius,
            ),
            normal: normal_b,
            uv: Vec2::new((index + 1) as f32 / segments as f32, 1.0),
        };
        mesh.triangles
            .push(Triangle::new([top_a, bottom_a, top_b], color));
        mesh.triangles
            .push(Triangle::new([top_b, bottom_a, bottom_b], color));
    }
    mesh.recompute_bounds();
    mesh
}

/// Genera una llama cónica que sale desde la boca del propulsor hacia -Y.
pub fn make_thruster_flame(segments: usize) -> Mesh {
    let segments = segments.max(3);
    let radius = 0.15;
    let top_y = -0.20;
    let tip_y = -1.05;
    let color = Color3::new(0.08, 0.68, 1.0);
    let mut mesh = Mesh::default();

    for index in 0..segments {
        let angle_a = index as f32 / segments as f32 * 2.0 * PI;
        let angle_b = (index + 1) as f32 / segments as f32 * 2.0 * PI;
        let ring_a = Vertex {
            position: Vec3::new(angle_a.cos() * radius, top_y, angle_a.sin() * radius),
            normal: Vec3::new(angle_a.cos(), radius / (tip_y - top_y), angle_a.sin()).normalized(),
            uv: Vec2::new(index as f32 / segments as f32, 0.0),
        };
        let ring_b = Vertex {
            position: Vec3::new(angle_b.cos() * radius, top_y, angle_b.sin() * radius),
            normal: Vec3::new(angle_b.cos(), radius / (tip_y - top_y), angle_b.sin()).normalized(),
            uv: Vec2::new((index + 1) as f32 / segments as f32, 0.0),
        };
        let tip = Vertex {
            position: Vec3::new(0.0, tip_y, 0.0),
            normal: (ring_a.normal + ring_b.normal).normalized(),
            uv: Vec2::new((index as f32 + 0.5) / segments as f32, 1.0),
        };
        mesh.triangles
            .push(Triangle::new([ring_a, tip, ring_b], color));
    }
    mesh.recompute_bounds();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_has_expected_triangle_count() {
        let sphere = make_uv_sphere(8, 4);
        assert_eq!(sphere.triangles.len(), 8 * (4 - 1) * 2);
    }

    #[test]
    fn loads_the_delorean_obj() {
        let path = crate::assets::locate(crate::assets::DELOREAN_OBJ);
        let mesh = load_obj(path).expect("el DeLorean debe ser un OBJ válido");
        assert!(
            mesh.triangles.len() >= 90_000,
            "se esperaba el modelo detallado nuevo"
        );
    }

    #[test]
    fn loads_the_oasis_obj() {
        let path = crate::assets::locate(crate::assets::OASIS_OBJ);
        let mesh = load_obj(path).expect("Oasis debe ser un OBJ válido");
        assert!(
            mesh.triangles.len() >= 160_000,
            "se esperaba el modelo completo de Oasis"
        );
    }

    #[test]
    fn loads_the_ice_planet_obj() {
        let path = crate::assets::locate(crate::assets::HIELO_OBJ);
        let mesh = load_obj(path).expect("Hielo debe ser un OBJ válido");
        assert!(
            mesh.triangles.len() >= 9_000,
            "se esperaba el modelo completo del planeta de hielo"
        );
    }

    #[test]
    fn material_names_are_classified() {
        assert_eq!(
            MaterialKind::from_name("Hielo_Fogata_Naranja"),
            MaterialKind::Emissive
        );
        assert_eq!(
            MaterialKind::from_name("Hielo_Lago_Congelado"),
            MaterialKind::Ice
        );
        assert_eq!(MaterialKind::from_name("Hielo_Oceano"), MaterialKind::Water);
        assert_eq!(MaterialKind::from_name("Hielo_Nieve"), MaterialKind::Snow);
        assert_eq!(MaterialKind::from_name("Fuego_Lava"), MaterialKind::Lava);
        assert_eq!(
            MaterialKind::from_name("Dulces_Lago_Chocolate"),
            MaterialKind::Chocolate
        );
        assert_eq!(
            MaterialKind::from_name("Oasis_Arena_Continua"),
            MaterialKind::Sand
        );
    }

    #[test]
    fn sphere_bounds_are_unit() {
        let sphere = make_uv_sphere(12, 8);
        assert!(sphere.center.length() < 1.0e-4);
        assert!((sphere.radius - 1.0).abs() < 1.0e-3);
    }

    #[test]
    fn decimation_reduces_triangles_and_keeps_bounds() {
        let sphere = make_uv_sphere(64, 32);
        let low = sphere.decimated(0.25);
        assert!(low.triangles.len() < sphere.triangles.len() / 3);
        assert!(!low.triangles.is_empty());
        assert_eq!(low.radius, sphere.radius);
        let lod = LodMesh::with_levels(sphere, &[(0.1, 60.0), (0.3, 15.0)]);
        assert_eq!(lod.select(500.0).0, 0);
        assert_eq!(lod.select(30.0).0, 1);
        assert_eq!(lod.select(2.0).0, 2);
    }

    #[test]
    fn obj_objects_are_kept_separate() {
        let dir = std::env::temp_dir().join("space_travel_obj_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dos.obj");
        std::fs::write(
            &path,
            "o A\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no B\nv 0 0 1\nf 1 2 4\nf -1 -2 -3\n",
        )
        .unwrap();
        let objects = load_obj_objects(&path).unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].0, "A");
        assert_eq!(objects[1].1.triangles.len(), 2);
        assert!(load_obj(&path).unwrap().triangles.len() == 3);
    }

    #[test]
    fn creates_thruster_geometry() {
        assert_eq!(make_thruster_nozzle(16).triangles.len(), 32);
        assert_eq!(make_thruster_flame(16).triangles.len(), 16);
    }
}
