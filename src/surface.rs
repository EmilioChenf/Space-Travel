//! Consultas sobre la superficie de un planeta cargado desde OBJ.
//!
//! Se construye una rejilla latitud/longitud en espacio local del planeta:
//! cada celda guarda el radio máximo de la geometría que cae en ella (para
//! colisiones conservadoras de la nave) y la lista de triángulos que la
//! tocan (para lanzar rayos y colocar objetos exactamente sobre el suelo).

use crate::math3d::{PI, Vec3};
use crate::model::Mesh;

const COLUMNS: usize = 96;
const ROWS: usize = 48;

pub struct PlanetSurface {
    max_radius: Vec<f32>,
    cells: Vec<Vec<u32>>,
    triangles: Vec<[Vec3; 3]>,
    /// Radio medio del terreno (mediana aproximada de los vértices).
    pub base_radius: f32,
    /// Radio máximo de toda la geometría.
    pub outer_radius: f32,
}

fn to_uv(d: Vec3) -> (f32, f32) {
    let u = (d.z.atan2(d.x) / (2.0 * PI) + 0.5).rem_euclid(1.0);
    let v = (d.y.clamp(-1.0, 1.0).acos() / PI).clamp(0.0, 0.999_999);
    (u, v)
}

impl PlanetSurface {
    pub fn new(mesh: &Mesh) -> Self {
        let mut max_radius = vec![0.0f32; COLUMNS * ROWS];
        let mut cells: Vec<Vec<u32>> = vec![Vec::new(); COLUMNS * ROWS];
        let mut triangles = Vec::with_capacity(mesh.triangles.len());
        let mut radii: Vec<f32> = Vec::with_capacity(mesh.triangles.len());
        let mut outer: f32 = 0.0;
        for (index, t) in mesh.triangles.iter().enumerate() {
            let p = t.vertices.map(|v| v.position);
            triangles.push(p);
            let r = p.map(|x| x.length());
            let rmax = r[0].max(r[1]).max(r[2]);
            outer = outer.max(rmax);
            radii.push((r[0] + r[1] + r[2]) / 3.0);
            let uv = p.map(|x| to_uv(x.normalized()));
            let v0 = uv.iter().map(|x| x.1).fold(f32::MAX, f32::min);
            let v1 = uv.iter().map(|x| x.1).fold(f32::MIN, f32::max);
            let mut u0 = uv.iter().map(|x| x.0).fold(f32::MAX, f32::min);
            let mut u1 = uv.iter().map(|x| x.0).fold(f32::MIN, f32::max);
            let row0 = (v0 * ROWS as f32) as usize;
            let row1 = ((v1 * ROWS as f32) as usize).min(ROWS - 1);
            let near_pole = row0 == 0 || row1 == ROWS - 1;
            let mut columns: Vec<usize> = Vec::new();
            if near_pole {
                columns.extend(0..COLUMNS);
            } else if u1 - u0 > 0.5 {
                // Cruza la costura de longitud: se recorre por el otro lado.
                let us: Vec<f32> = uv
                    .iter()
                    .map(|x| if x.0 < 0.5 { x.0 + 1.0 } else { x.0 })
                    .collect();
                u0 = us.iter().copied().fold(f32::MAX, f32::min);
                u1 = us.iter().copied().fold(f32::MIN, f32::max);
                let c0 = (u0 * COLUMNS as f32) as usize;
                let c1 = (u1 * COLUMNS as f32) as usize;
                columns.extend((c0..=c1).map(|c| c % COLUMNS));
            } else {
                let c0 = (u0 * COLUMNS as f32) as usize;
                let c1 = ((u1 * COLUMNS as f32) as usize).min(COLUMNS - 1);
                columns.extend(c0..=c1);
            }
            // Margen de una celda para triángulos pequeños entre celdas.
            for row in row0.saturating_sub(1)..=(row1 + 1).min(ROWS - 1) {
                for &col in &columns {
                    for dc in [COLUMNS - 1, 0, 1] {
                        let cell = row * COLUMNS + (col + dc) % COLUMNS;
                        if row >= row0 && row <= row1 && dc == 0 {
                            max_radius[cell] = max_radius[cell].max(rmax);
                        }
                        if cells[cell].last() != Some(&(index as u32)) {
                            cells[cell].push(index as u32);
                        }
                    }
                }
            }
        }
        radii.sort_by(|a, b| a.total_cmp(b));
        let base_radius = radii.get(radii.len() / 2).copied().unwrap_or(1.0);
        // Celdas vacías heredan el radio base.
        for value in &mut max_radius {
            if *value <= 0.0 {
                *value = base_radius;
            }
        }
        Self {
            max_radius,
            cells,
            triangles,
            base_radius,
            outer_radius: outer,
        }
    }

    fn cell_value(&self, col: isize, row: isize) -> f32 {
        let row = row.clamp(0, ROWS as isize - 1) as usize;
        let col = col.rem_euclid(COLUMNS as isize) as usize;
        self.max_radius[row * COLUMNS + col]
    }

    /// Altura máxima (radio local) de la geometría en la dirección `dir`,
    /// tomando el máximo de las celdas vecinas: colisión conservadora.
    pub fn height_at(&self, dir: Vec3) -> f32 {
        let (u, v) = to_uv(dir.normalized());
        let col = (u * COLUMNS as f32) as isize;
        let row = (v * ROWS as f32) as isize;
        let mut h: f32 = 0.0;
        for dr in -1..=1 {
            for dc in -1..=1 {
                h = h.max(self.cell_value(col + dc, row + dr));
            }
        }
        h
    }

    /// Todas las intersecciones (radios) de un rayo desde el centro.
    pub fn ray_hits(&self, dir: Vec3) -> Vec<f32> {
        let d = dir.normalized();
        let (u, v) = to_uv(d);
        let cell = (v * ROWS as f32) as usize * COLUMNS + (u * COLUMNS as f32) as usize;
        let mut hits = Vec::new();
        for &index in &self.cells[cell.min(self.cells.len() - 1)] {
            if let Some(t) = ray_triangle(d, &self.triangles[index as usize]) {
                hits.push(t);
            }
        }
        hits.sort_by(|a, b| a.total_cmp(b));
        hits
    }

    /// Radio del suelo en `dir`, ignorando decoraciones altas (árboles,
    /// casas...) que superen el radio base en más de `tolerance`.
    pub fn ground_at(&self, dir: Vec3, tolerance: f32) -> f32 {
        let hits = self.ray_hits(dir);
        let limit = self.base_radius * (1.0 + tolerance);
        hits.iter()
            .rev()
            .copied()
            .find(|&r| r <= limit)
            .or_else(|| hits.first().copied())
            .unwrap_or(self.base_radius)
    }

    /// ¿Hay una decoración (algo por encima del terreno) en esta dirección?
    pub fn is_clear(&self, dir: Vec3, tolerance: f32) -> bool {
        let hits = self.ray_hits(dir);
        let limit = self.base_radius * (1.0 + tolerance);
        hits.last().is_none_or(|&r| r <= limit)
    }
}

/// Möller–Trumbore para un rayo desde el origen con dirección unitaria.
fn ray_triangle(dir: Vec3, tri: &[Vec3; 3]) -> Option<f32> {
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1.0e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = -tri[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::make_uv_sphere;

    #[test]
    fn sphere_surface_reports_unit_radius() {
        let sphere = make_uv_sphere(48, 24);
        let surface = PlanetSurface::new(&sphere);
        for d in [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.3, 0.8, -0.2),
            Vec3::new(-0.9, -0.1, 0.4),
        ] {
            let ground = surface.ground_at(d, 0.1);
            assert!((ground - 1.0).abs() < 0.02, "suelo {ground}");
            let h = surface.height_at(d);
            assert!((0.98..=1.01).contains(&h), "altura {h}");
        }
        assert!(surface.is_clear(Vec3::new(0.0, 1.0, 0.0), 0.05));
    }

    #[test]
    fn decorations_raise_collision_height_but_not_ground() {
        let mut mesh = make_uv_sphere(48, 24);
        // "Árbol": esfera pequeña sobre el polo +X.
        let mut tree = make_uv_sphere(8, 6);
        for t in &mut tree.triangles {
            for v in &mut t.vertices {
                v.position = v.position * 0.2 + Vec3::new(1.25, 0.0, 0.0);
            }
        }
        mesh.append(&tree);
        let surface = PlanetSurface::new(&mesh);
        let d = Vec3::new(1.0, 0.0, 0.0);
        assert!(surface.height_at(d) > 1.4);
        assert!((surface.ground_at(d, 0.03) - 1.0).abs() < 0.03);
        assert!(!surface.is_clear(d, 0.03));
    }
}
