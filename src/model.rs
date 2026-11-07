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

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub vertices: [Vertex; 3],
    pub color: Color3,
}

#[derive(Debug, Default)]
pub struct Mesh {
    pub triangles: Vec<Triangle>,
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

fn load_materials(path: &Path) -> HashMap<String, Color3> {
    let mut result = HashMap::new();
    let Ok(contents) = fs::read_to_string(path) else {
        return result;
    };
    let mut current = String::new();
    for line in contents.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("newmtl") => {
                current = words.next().unwrap_or_default().to_owned();
                result.insert(current.clone(), Color3::new(0.72, 0.76, 0.82));
            }
            Some("Kd") if !current.is_empty() => {
                let values: Vec<f32> = words.filter_map(|word| word.parse().ok()).collect();
                if values.len() >= 3 {
                    result.insert(
                        current.clone(),
                        Color3::new(values[0], values[1], values[2]),
                    );
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

pub fn load_obj(path: impl AsRef<Path>) -> Result<Mesh, io::Error> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;
    let directory = path.parent().unwrap_or(Path::new("."));
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut normals = Vec::new();
    let mut materials = HashMap::new();
    let mut current_color = Color3::new(0.72, 0.76, 0.82);
    let mut mesh = Mesh::default();

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
            "mtllib" => {
                if let Some(file) = words.next() {
                    materials = load_materials(&directory.join(file));
                }
            }
            "usemtl" => {
                if let Some(name) = words.next()
                    && let Some(color) = materials.get(name)
                {
                    current_color = *color;
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
                    mesh.triangles.push(Triangle {
                        vertices,
                        color: current_color,
                    });
                }
            }
            _ => {}
        }
    }

    if mesh.triangles.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "el OBJ no contiene triángulos",
        ));
    }
    Ok(mesh)
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
                mesh.triangles.push(Triangle {
                    vertices: [a, d, b],
                    color: white,
                });
            }
            if latitude + 1 < latitudes {
                mesh.triangles.push(Triangle {
                    vertices: [b, d, c],
                    color: white,
                });
            }
        }
    }
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
        mesh.triangles.push(Triangle {
            vertices: [top_a, bottom_a, top_b],
            color,
        });
        mesh.triangles.push(Triangle {
            vertices: [top_b, bottom_a, bottom_b],
            color,
        });
    }
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
        mesh.triangles.push(Triangle {
            vertices: [ring_a, tip, ring_b],
            color,
        });
    }
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
    fn creates_thruster_geometry() {
        assert_eq!(make_thruster_nozzle(16).triangles.len(), 32);
        assert_eq!(make_thruster_flame(16).triangles.len(), 16);
    }
}
