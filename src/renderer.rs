use crate::math3d::{Mat4, Vec2, Vec3, Vec4};
use crate::model::{Color3, Mesh};

#[derive(Clone, Copy)]
pub enum ShaderKind {
    Metal,
    Planet,
    Sun,
    Thruster,
}

pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Clone, Copy, Default)]
pub struct RenderStats {
    pub submitted_triangles: u64,
    pub rasterized_triangles: u64,
    pub shaded_pixels: u64,
}

#[derive(Clone, Copy, Default)]
struct VertexOutput {
    clip: Vec4,
    world_position: Vec3,
    world_normal: Vec3,
    uv: Vec2,
    color: Color3,
}

pub struct SoftwareRenderer {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    depth: Vec<f32>,
    view_projection: Mat4,
    camera_position: Vec3,
    thruster_lights: Vec<Vec3>,
    stats: RenderStats,
}

impl Default for Color3 {
    fn default() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }
}

impl SoftwareRenderer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
            depth: vec![f32::INFINITY; width * height],
            view_projection: Mat4::identity(),
            camera_position: Vec3::ZERO,
            thruster_lights: Vec::with_capacity(4),
            stats: RenderStats::default(),
        }
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    pub fn set_thruster_lights(&mut self, lights: &[Vec3]) {
        self.thruster_lights.clear();
        self.thruster_lights.extend_from_slice(lights);
    }

    pub fn clear(&mut self, color: Color3, time: f32) {
        let packed = pack_rgba(color);
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&packed);
        }
        self.depth.fill(f32::INFINITY);
        self.stats = RenderStats::default();

        let mut state = 0x53A9_F17Bu32;
        for i in 0..260 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = state as usize % self.width;
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let y = state as usize % self.height;
            let flicker = 0.55 + 0.35 * (time * 1.7 + i as f32).sin();
            self.write_color(x, y, Color3::new(flicker, flicker, flicker * 1.15));
        }
    }

    pub fn begin(&mut self, camera: &Camera) {
        self.camera_position = camera.position;
        let view = Mat4::look_at(camera.position, camera.target, camera.up);
        let projection = Mat4::perspective(
            camera.fov_y,
            self.width as f32 / self.height as f32,
            camera.near,
            camera.far,
        );
        self.view_projection = projection * view;
    }

    pub fn draw_mesh(&mut self, mesh: &Mesh, model: Mat4, shader: ShaderKind, time: f32) {
        let mvp = self.view_projection * model;
        for triangle in &mesh.triangles {
            self.stats.submitted_triangles += 1;
            let mut output = [VertexOutput::default(); 3];
            for (i, input) in triangle.vertices.iter().enumerate() {
                output[i] = VertexOutput {
                    clip: mvp
                        * Vec4::new(input.position.x, input.position.y, input.position.z, 1.0),
                    world_position: model.transform_point(input.position),
                    world_normal: model.transform_direction(input.normal).normalized(),
                    uv: input.uv,
                    color: triangle.color,
                };
            }
            // El clipping cercano es una mejora posterior. Por ahora se rechaza
            // el triángulo que cruza el ojo para mantener estable el rasterizador.
            if output.iter().any(|vertex| vertex.clip.w <= 0.01) {
                continue;
            }
            self.rasterize(output, shader, time);
        }
    }

    fn rasterize(&mut self, vertices: [VertexOutput; 3], shader: ShaderKind, time: f32) {
        let mut screen = [Vec2::default(); 3];
        let mut ndc_depth = [0.0; 3];
        let mut inverse_w = [0.0; 3];
        for i in 0..3 {
            inverse_w[i] = 1.0 / vertices[i].clip.w;
            let ndc_x = vertices[i].clip.x * inverse_w[i];
            let ndc_y = vertices[i].clip.y * inverse_w[i];
            ndc_depth[i] = vertices[i].clip.z * inverse_w[i] * 0.5 + 0.5;
            screen[i] = Vec2::new(
                (ndc_x * 0.5 + 0.5) * (self.width - 1) as f32,
                (1.0 - (ndc_y * 0.5 + 0.5)) * (self.height - 1) as f32,
            );
        }
        let signed_area = edge(screen[0], screen[1], screen[2]);
        if signed_area.abs() < 1.0e-6 {
            return;
        }

        let min_x = screen
            .iter()
            .map(|v| v.x)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_x = screen
            .iter()
            .map(|v| v.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min((self.width - 1) as f32) as usize;
        let min_y = screen
            .iter()
            .map(|v| v.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_y = screen
            .iter()
            .map(|v| v.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min((self.height - 1) as f32) as usize;
        if min_x > max_x || min_y > max_y {
            return;
        }
        self.stats.rasterized_triangles += 1;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let sample = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let b0 = edge(screen[1], screen[2], sample) / signed_area;
                let b1 = edge(screen[2], screen[0], sample) / signed_area;
                let b2 = edge(screen[0], screen[1], sample) / signed_area;
                if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                    continue;
                }
                let depth = b0 * ndc_depth[0] + b1 * ndc_depth[1] + b2 * ndc_depth[2];
                let index = y * self.width + x;
                if !(0.0..=1.0).contains(&depth) || depth >= self.depth[index] {
                    continue;
                }
                let denominator = b0 * inverse_w[0] + b1 * inverse_w[1] + b2 * inverse_w[2];
                if denominator <= 1.0e-8 {
                    continue;
                }
                let p = [
                    b0 * inverse_w[0] / denominator,
                    b1 * inverse_w[1] / denominator,
                    b2 * inverse_w[2] / denominator,
                ];
                let position = vertices[0].world_position * p[0]
                    + vertices[1].world_position * p[1]
                    + vertices[2].world_position * p[2];
                let normal = (vertices[0].world_normal * p[0]
                    + vertices[1].world_normal * p[1]
                    + vertices[2].world_normal * p[2])
                    .normalized();
                let uv = vertices[0].uv * p[0] + vertices[1].uv * p[1] + vertices[2].uv * p[2];
                let color =
                    vertices[0].color * p[0] + vertices[1].color * p[1] + vertices[2].color * p[2];
                let shaded = self.shade(shader, position, normal, uv, color, time);
                self.depth[index] = depth;
                self.write_color(x, y, shaded);
                self.stats.shaded_pixels += 1;
            }
        }
    }

    fn shade(
        &self,
        shader: ShaderKind,
        position: Vec3,
        normal: Vec3,
        uv: Vec2,
        base: Color3,
        time: f32,
    ) -> Color3 {
        if matches!(shader, ShaderKind::Sun) {
            let waves = 0.5 + 0.5 * (normal.y * 14.0 + normal.x * 7.0 + time * 2.4).sin();
            let spots = procedural_noise(normal * 5.0 + Vec3::new(0.0, time * 0.15, 0.0));
            return Color3::new(1.0, 0.28 + waves * 0.48, 0.035 + spots * 0.12);
        }
        if matches!(shader, ShaderKind::Thruster) {
            let pulse = 0.82 + 0.18 * (time * 18.0 + position.y * 10.0).sin();
            let hot_core = (1.0 - uv.y).clamp(0.0, 1.0);
            return base * (1.15 * pulse) + Color3::new(0.72, 0.92, 1.0) * (0.38 + hot_core * 0.62);
        }
        let to_sun = (-position).normalized();
        let to_camera = (self.camera_position - position).normalized();
        let diffuse = normal.dot(to_sun).max(0.0);
        let half_vector = (to_sun + to_camera).normalized();
        let shininess = if matches!(shader, ShaderKind::Metal) {
            48.0
        } else {
            10.0
        };
        let specular = normal.dot(half_vector).max(0.0).powf(shininess);
        if matches!(shader, ShaderKind::Planet) {
            let terrain = procedural_noise(normal * 4.5);
            let ice = normal.y.abs().powf(9.0);
            let ocean = Color3::new(0.025, 0.16, 0.34);
            let mut surface = if terrain > 0.53 {
                Color3::new(0.12, 0.48, 0.25)
            } else {
                ocean
            };
            if terrain > 0.68 {
                surface = Color3::new(0.55, 0.43, 0.20);
            }
            surface = surface * (1.0 - ice) + Color3::new(0.82, 0.93, 1.0) * ice;
            let grid = if (uv.x * 80.0).sin() * (uv.y * 40.0).sin() > 0.92 {
                0.08
            } else {
                0.0
            };
            return surface * (0.12 + 0.88 * diffuse + grid)
                + Color3::new(0.8, 0.9, 1.0) * specular * 0.18;
        }
        let brushing = 0.92 + 0.08 * (position.x * 75.0 + position.z * 11.0).sin();
        let mut result = base * (0.16 + 0.84 * diffuse) * brushing
            + Color3::new(1.0, 0.92, 0.72) * specular * 0.7;

        // Las llamas funcionan también como luces puntuales azules sobre la
        // parte baja de la carrocería y las ruedas.
        for light in &self.thruster_lights {
            let to_light = *light - position;
            let distance = to_light.length();
            if distance >= 3.0 || distance <= 1.0e-5 {
                continue;
            }
            let facing = normal.dot(to_light / distance).max(0.0);
            let attenuation = (1.0 - distance / 3.0).powi(2);
            let intensity = attenuation * (0.24 + facing * 1.35);
            result = result + Color3::new(0.04, 0.48, 1.0) * intensity;
        }
        result
    }

    fn write_color(&mut self, x: usize, y: usize, color: Color3) {
        let start = (y * self.width + x) * 4;
        self.pixels[start..start + 4].copy_from_slice(&pack_rgba(color));
    }
}

fn edge(a: Vec2, b: Vec2, point: Vec2) -> f32 {
    (point.x - a.x) * (b.y - a.y) - (point.y - a.y) * (b.x - a.x)
}

fn procedural_noise(p: Vec3) -> f32 {
    let value = (p.x * 2.17 + (p.z * 1.73).sin()).sin()
        + (p.y * 3.11 - p.x * 0.71).sin()
        + ((p.x + p.y + p.z) * 4.03).sin() * 0.5;
    value / 2.5 * 0.5 + 0.5
}

fn pack_rgba(color: Color3) -> [u8; 4] {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    [channel(color.r), channel(color.g), channel(color.b), 255]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::make_uv_sphere;

    #[test]
    fn rasterizes_a_sphere_into_the_cpu_framebuffer() {
        let mut renderer = SoftwareRenderer::new(160, 90);
        renderer.clear(Color3::new(0.0, 0.0, 0.0), 0.0);
        renderer.begin(&Camera {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::new(0.0, 1.0, 0.0),
            fov_y: 60.0_f32.to_radians(),
            near: 0.1,
            far: 100.0,
        });
        renderer.draw_mesh(
            &make_uv_sphere(10, 6),
            Mat4::identity(),
            ShaderKind::Sun,
            0.0,
        );
        assert!(renderer.stats().rasterized_triangles > 0);
        assert!(renderer.stats().shaded_pixels > 0);
    }
}
