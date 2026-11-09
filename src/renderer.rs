//! Renderizador 3D por software.
//!
//! Pipeline completo ejecutado en CPU:
//!
//! 1. **Vertex stage** (`draw_mesh`): matriz modelo → vista → proyección,
//!    descarte de objetos por frustum (esfera envolvente), selección de LOD,
//!    descarte de caras traseras y recorte (clipping) de triángulos contra el
//!    plano cercano con Sutherland–Hodgman en espacio de recorte.
//! 2. **Binning**: cada triángulo proyectado se asigna a las bandas
//!    horizontales de pantalla que toca.
//! 3. **Raster stage** (`end_frame`): cada banda se procesa en un hilo. Primero
//!    se rasteriza un *visibility buffer* (prueba de profundidad con Z-buffer
//!    basado en 1/w, guarda el triángulo visible por píxel); después se
//!    sombrea una sola vez cada píxel visible con interpolación de atributos
//!    con corrección de perspectiva y los shaders propios de `shaders.rs`.
//!    Finalmente se dibujan triángulos aditivos (llamas, brillos), partículas
//!    y efectos de postproceso.
//!
//! Raylib nunca participa en este proceso: solo recibe el framebuffer final.

use crate::math3d::{Mat4, Vec2, Vec3, Vec4, apply3};
use crate::model::{Color3, LodMesh, MaterialKind, Mesh};
use crate::shaders::{self, Fragment};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub use crate::shaders::ShaderKind;

/// Cámara en perspectiva definida por posición, objetivo y vector arriba.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    /// Escribe profundidad y color.
    Opaque,
    /// Suma color sin escribir profundidad (fuego, brillos, energía).
    Additive,
}

/// Parámetros de una llamada de dibujo (equivalente a los *uniforms*).
#[derive(Clone, Copy, Debug)]
pub struct DrawOptions {
    pub shader: ShaderKind,
    pub blend: Blend,
    pub cull_backfaces: bool,
    pub tint: Color3,
    /// Vector libre por shader (p. ej. dirección de movimiento del meteorito).
    pub param: Vec3,
    /// Escalar libre por shader (p. ej. intensidad o fase de animación).
    pub scalar: f32,
}

impl DrawOptions {
    pub fn new(shader: ShaderKind) -> Self {
        Self {
            shader,
            blend: Blend::Opaque,
            cull_backfaces: true,
            tint: Color3::new(1.0, 1.0, 1.0),
            param: Vec3::ZERO,
            scalar: 0.0,
        }
    }
    pub fn additive(mut self) -> Self {
        self.blend = Blend::Additive;
        self.cull_backfaces = false;
        self
    }
    pub fn double_sided(mut self) -> Self {
        self.cull_backfaces = false;
        self
    }
    pub fn tint(mut self, tint: Color3) -> Self {
        self.tint = tint;
        self
    }
    pub fn param(mut self, param: Vec3) -> Self {
        self.param = param;
        self
    }
    pub fn scalar(mut self, scalar: f32) -> Self {
        self.scalar = scalar;
        self
    }
}

/// Luz puntual (propulsores, fogatas, lava...).
#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub position: Vec3,
    pub color: Color3,
    pub radius: f32,
}

/// Efectos de postproceso aplicados por banda tras el sombreado.
#[derive(Clone, Copy, Debug, Default)]
pub struct PostFx {
    /// Oscurecimiento a negro (0 = nada, 1 = negro total).
    pub fade: f32,
    /// Destello blanco (0..1).
    pub flash: f32,
    /// Viñeta roja por daño (0..1).
    pub damage: f32,
    /// Distorsión horizontal por calor (0..1).
    pub heat: f32,
    /// Intensidad del resplandor del sol.
    pub sun_glow: f32,
    /// Viñeta oscura cinematográfica (0..1).
    pub vignette: f32,
    /// Tinte de color frío/tormenta de nieve (0..1).
    pub frost: f32,
}

/// Estadísticas por cuadro, usadas por el HUD de rendimiento y las pruebas.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub draw_calls: u64,
    pub culled_objects: u64,
    pub submitted_triangles: u64,
    pub backface_culled: u64,
    pub clipped_triangles: u64,
    pub rasterized_triangles: u64,
    pub shaded_pixels: u64,
    pub particles: u64,
    pub lod_reduced_draws: u64,
    pub vertex_ms: f32,
    pub raster_ms: f32,
}

/// Número de atributos interpolados por vértice:
/// posición mundo (3), normal (3), posición local (3), uv (2), color (3).
const ATTRS: usize = 14;
const BAND_HEIGHT: usize = 16;

#[derive(Clone, Copy)]
struct ClipVertex {
    clip: Vec4,
    attrs: [f32; ATTRS],
}

/// Triángulo ya proyectado y listo para rasterizar.
#[derive(Clone, Copy)]
struct ScreenTri {
    /// Coeficientes (A, B, C) de las tres funciones de arista normalizadas:
    /// la coordenada baricéntrica i en (x, y) es A*x + B*y + C.
    edges: [[f32; 3]; 3],
    inv_w: [f32; 3],
    /// Atributos divididos entre w (para corrección de perspectiva).
    attrs: [[f32; ATTRS]; 3],
    min_x: u16,
    max_x: u16,
    min_y: u16,
    max_y: u16,
    material: MaterialKind,
    draw: u32,
}

#[derive(Clone, Copy)]
struct ScreenParticle {
    x: f32,
    y: f32,
    radius: f32,
    inv_w: f32,
    color: Color3,
    alpha: f32,
    additive: bool,
}

#[derive(Clone, Copy)]
struct DrawCall {
    options: DrawOptions,
}

/// Datos compartidos (solo lectura) por todos los hilos durante el raster.
pub struct Uniforms {
    pub time: f32,
    pub camera_position: Vec3,
    pub sun_position: Vec3,
    pub lights: Vec<PointLight>,
}

pub struct SoftwareRenderer {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    depth: Vec<f32>,
    visibility: Vec<u32>,
    view: Mat4,
    projection: Mat4,
    view_projection: Mat4,
    frustum: [Vec4; 6],
    camera: Camera,
    clear_color: Color3,
    uniforms: Uniforms,
    draws: Vec<DrawCall>,
    opaque: Vec<ScreenTri>,
    additive: Vec<ScreenTri>,
    particles: Vec<ScreenParticle>,
    stars: Vec<(Vec3, f32, Color3)>,
    pub post: PostFx,
    /// Intensidad del fondo de estrellas (0 dentro de una atmósfera).
    pub star_brightness: f32,
    threads: usize,
    stats: RenderStats,
    vertex_time: f32,
}

impl Default for Color3 {
    fn default() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }
}

impl SoftwareRenderer {
    pub fn new(width: usize, height: usize) -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 16);
        let mut rng = crate::math3d::Rng::new(0x5EED_57A2);
        let stars = (0..2200)
            .map(|_| {
                let dir = rng.unit_vector();
                let brightness = rng.next_f32().powf(3.0) * 0.85 + 0.15;
                let tint = rng.next_f32();
                let color = if tint < 0.15 {
                    Color3::new(1.0, 0.78, 0.6)
                } else if tint < 0.35 {
                    Color3::new(0.7, 0.82, 1.0)
                } else {
                    Color3::new(1.0, 1.0, 1.0)
                };
                (dir, brightness, color)
            })
            .collect();
        let camera = Camera {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::new(0.0, 1.0, 0.0),
            fov_y: 1.0,
            near: 0.1,
            far: 100.0,
        };
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
            depth: vec![0.0; width * height],
            visibility: vec![u32::MAX; width * height],
            view: Mat4::identity(),
            projection: Mat4::identity(),
            view_projection: Mat4::identity(),
            frustum: [Vec4::default(); 6],
            camera,
            clear_color: Color3::new(0.0, 0.0, 0.0),
            uniforms: Uniforms {
                time: 0.0,
                camera_position: Vec3::ZERO,
                sun_position: Vec3::ZERO,
                lights: Vec::new(),
            },
            draws: Vec::new(),
            opaque: Vec::new(),
            additive: Vec::new(),
            particles: Vec::new(),
            stars,
            post: PostFx::default(),
            star_brightness: 1.0,
            threads,
            stats: RenderStats::default(),
            vertex_time: 0.0,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn stats(&self) -> RenderStats {
        self.stats
    }
    pub fn camera(&self) -> &Camera {
        &self.camera
    }
    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads.clamp(1, 32);
    }

    pub fn set_sun_position(&mut self, position: Vec3) {
        self.uniforms.sun_position = position;
    }

    pub fn set_lights(&mut self, lights: &[PointLight]) {
        self.uniforms.lights.clear();
        self.uniforms.lights.extend_from_slice(lights);
    }

    pub fn add_light(&mut self, light: PointLight) {
        if self.uniforms.lights.len() < 16 {
            self.uniforms.lights.push(light);
        }
    }

    /// Compatibilidad con la versión anterior: luces azules de propulsores.
    pub fn set_thruster_lights(&mut self, lights: &[Vec3]) {
        self.uniforms.lights.clear();
        for &position in lights {
            self.uniforms.lights.push(PointLight {
                position,
                color: Color3::new(0.06, 0.52, 1.0),
                radius: 4.0,
            });
        }
    }

    /// Inicia un cuadro: configura matrices de vista/proyección y vacía colas.
    pub fn begin_frame(&mut self, camera: &Camera, clear_color: Color3, time: f32) {
        self.camera = *camera;
        self.clear_color = clear_color;
        self.uniforms.time = time;
        self.uniforms.camera_position = camera.position;
        self.view = Mat4::look_at(camera.position, camera.target, camera.up);
        self.projection = Mat4::perspective(
            camera.fov_y,
            self.width as f32 / self.height as f32,
            camera.near,
            camera.far,
        );
        self.view_projection = self.projection * self.view;
        self.frustum = extract_frustum(&self.view_projection);
        self.draws.clear();
        self.opaque.clear();
        self.additive.clear();
        self.particles.clear();
        self.stats = RenderStats::default();
        self.vertex_time = 0.0;
        self.post = PostFx::default();
        self.star_brightness = 1.0;
    }

    /// API antigua: limpia y luego `begin` fija la cámara.
    pub fn clear(&mut self, color: Color3, time: f32) {
        self.clear_color = color;
        self.uniforms.time = time;
    }
    pub fn begin(&mut self, camera: &Camera) {
        let color = self.clear_color;
        let time = self.uniforms.time;
        self.begin_frame(camera, color, time);
    }

    /// ¿La esfera (centro, radio) en coordenadas mundo toca el frustum?
    pub fn sphere_visible(&self, center: Vec3, radius: f32) -> bool {
        self.frustum.iter().all(|plane| {
            plane.x * center.x + plane.y * center.y + plane.z * center.z + plane.w >= -radius
        })
    }

    /// Radio aproximado en píxeles que ocupa una esfera en pantalla.
    pub fn projected_radius(&self, center: Vec3, radius: f32) -> f32 {
        let distance = (center - self.camera.position).length().max(1.0e-3);
        let focal = self.height as f32 * 0.5 / (self.camera.fov_y * 0.5).tan();
        if distance <= radius {
            return f32::MAX;
        }
        radius / distance * focal
    }

    /// Proyecta un punto del mundo a pantalla (píxeles del framebuffer).
    /// Devuelve también si está delante de la cámara.
    pub fn project(&self, point: Vec3) -> Option<(Vec2, f32)> {
        let clip = self.view_projection * Vec4::new(point.x, point.y, point.z, 1.0);
        if clip.w <= 1.0e-4 {
            return None;
        }
        let x = (clip.x / clip.w * 0.5 + 0.5) * self.width as f32;
        let y = (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * self.height as f32;
        Some((Vec2::new(x, y), clip.w))
    }

    /// Dibuja la versión adecuada de una malla con niveles de detalle.
    pub fn draw_lod(&mut self, lod: &LodMesh, model: Mat4, options: &DrawOptions) -> bool {
        let full = lod.full();
        let center = model.transform_point(full.center);
        let radius = full.radius * model.max_scale();
        if !self.sphere_visible(center, radius) {
            self.stats.culled_objects += 1;
            return false;
        }
        let pixels = self.projected_radius(center, radius);
        // Descarte por tamaño: objetos de menos de un píxel no aportan nada.
        if pixels < 0.75 {
            self.stats.culled_objects += 1;
            return false;
        }
        let (level, mesh) = lod.select(pixels);
        if level > 0 {
            self.stats.lod_reduced_draws += 1;
        }
        self.submit(mesh, model, options);
        true
    }

    /// Dibuja una malla completa con descarte previo por frustum.
    pub fn draw_mesh(&mut self, mesh: &Mesh, model: Mat4, options: &DrawOptions) -> bool {
        let center = model.transform_point(mesh.center);
        let radius = mesh.radius * model.max_scale();
        if mesh.radius > 0.0 && !self.sphere_visible(center, radius) {
            self.stats.culled_objects += 1;
            return false;
        }
        self.submit(mesh, model, options);
        true
    }

    fn submit(&mut self, mesh: &Mesh, model: Mat4, options: &DrawOptions) {
        let start = Instant::now();
        self.stats.draw_calls += 1;
        self.stats.submitted_triangles += mesh.triangles.len() as u64;
        let draw_index = self.draws.len() as u32;
        self.draws.push(DrawCall { options: *options });

        let setup = VertexSetup {
            mvp: self.view_projection * model,
            model,
            normal: model.normal_matrix(),
            width: self.width as f32,
            height: self.height as f32,
            cull: options.cull_backfaces,
            draw: draw_index,
            near_w: self.camera.near * 0.999,
            far_w: self.camera.far,
        };
        let target = match options.blend {
            Blend::Opaque => &mut self.opaque,
            Blend::Additive => &mut self.additive,
        };

        let triangles = &mesh.triangles;
        let counters = Counters::default();
        if triangles.len() < 6000 || self.threads == 1 {
            setup.process(triangles, target, &counters);
        } else {
            let chunk = triangles.len().div_ceil(self.threads);
            let results: Vec<Vec<ScreenTri>> = std::thread::scope(|scope| {
                let handles: Vec<_> = triangles
                    .chunks(chunk)
                    .map(|part| {
                        let setup = &setup;
                        let counters = &counters;
                        scope.spawn(move || {
                            let mut out = Vec::with_capacity(part.len() / 2);
                            setup.process(part, &mut out, counters);
                            out
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap_or_default())
                    .collect()
            });
            for mut part in results {
                target.append(&mut part);
            }
        }
        self.stats.backface_culled += counters.backface.load(Ordering::Relaxed);
        self.stats.clipped_triangles += counters.clipped.load(Ordering::Relaxed);
        self.vertex_time += start.elapsed().as_secs_f32();
    }

    /// Partícula esférica con prueba de profundidad (humo, chispas, nieve).
    pub fn draw_particle(
        &mut self,
        position: Vec3,
        world_radius: f32,
        color: Color3,
        alpha: f32,
        additive: bool,
    ) {
        let clip = self.view_projection * Vec4::new(position.x, position.y, position.z, 1.0);
        if clip.w <= self.camera.near {
            return;
        }
        let inv_w = 1.0 / clip.w;
        let x = (clip.x * inv_w * 0.5 + 0.5) * self.width as f32;
        let y = (1.0 - (clip.y * inv_w * 0.5 + 0.5)) * self.height as f32;
        let focal = self.height as f32 * 0.5 / (self.camera.fov_y * 0.5).tan();
        let radius = (world_radius * focal * inv_w).clamp(0.6, 90.0);
        if x + radius < 0.0
            || y + radius < 0.0
            || x - radius > self.width as f32
            || y - radius > self.height as f32
        {
            return;
        }
        self.particles.push(ScreenParticle {
            x,
            y,
            radius,
            inv_w,
            color,
            alpha: alpha.clamp(0.0, 1.0),
            additive,
        });
    }

    /// Segmento 3D dibujado como una cadena de puntos (órbitas, rutas).
    pub fn draw_dotted_line(&mut self, a: Vec3, b: Vec3, steps: usize, size: f32, color: Color3) {
        for i in 0..=steps {
            let t = i as f32 / steps.max(1) as f32;
            self.draw_particle(a.lerp(b, t), size, color, 0.8, true);
        }
    }

    /// Ejecuta el raster paralelo por bandas y el postproceso.
    pub fn end_frame(&mut self) {
        let start = Instant::now();
        self.stats.vertex_ms = self.vertex_time * 1000.0;
        self.stats.particles = self.particles.len() as u64;
        let width = self.width;
        let height = self.height;
        let band_count = height.div_ceil(BAND_HEIGHT);

        // Binning: lista de triángulos por banda.
        let mut opaque_bins: Vec<Vec<u32>> = vec![Vec::new(); band_count];
        for (i, t) in self.opaque.iter().enumerate() {
            for band in (t.min_y as usize / BAND_HEIGHT)..=(t.max_y as usize / BAND_HEIGHT) {
                opaque_bins[band.min(band_count - 1)].push(i as u32);
            }
        }
        let mut additive_bins: Vec<Vec<u32>> = vec![Vec::new(); band_count];
        for (i, t) in self.additive.iter().enumerate() {
            for band in (t.min_y as usize / BAND_HEIGHT)..=(t.max_y as usize / BAND_HEIGHT) {
                additive_bins[band.min(band_count - 1)].push(i as u32);
            }
        }
        self.stats.rasterized_triangles = (self.opaque.len() + self.additive.len()) as u64;

        // Estrellas proyectadas solo con la rotación de la vista (infinito).
        let mut star_points: Vec<(usize, usize, Color3, bool)> = Vec::with_capacity(1500);
        let vp_rot = {
            let mut v = self.view;
            v.m[0][3] = 0.0;
            v.m[1][3] = 0.0;
            v.m[2][3] = 0.0;
            self.projection * v
        };
        let twinkle_time = self.uniforms.time;
        for (i, (dir, brightness, color)) in self.stars.iter().enumerate() {
            let clip = vp_rot * Vec4::new(dir.x, dir.y, dir.z, 0.0);
            if clip.w <= 0.0 {
                continue;
            }
            let x = (clip.x / clip.w * 0.5 + 0.5) * width as f32;
            let y = (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * height as f32;
            if x < 0.0 || y < 0.0 || x >= width as f32 - 1.0 || y >= height as f32 - 1.0 {
                continue;
            }
            let flicker =
                (0.75 + 0.25 * (twinkle_time * 1.7 + i as f32 * 1.37).sin()) * self.star_brightness;
            if flicker <= 0.01 {
                continue;
            }
            star_points.push((
                x as usize,
                y as usize,
                *color * (brightness * flicker),
                *brightness > 0.7,
            ));
        }

        let sun_screen = self.project(self.uniforms.sun_position).and_then(|(p, w)| {
            // Oclusión aproximada del resplandor del sol: se revisa después.
            (w > 0.0).then_some((p, 1.0 / w))
        });

        let shaded = AtomicU64::new(0);
        let context = BandContext {
            width,
            height,
            opaque: &self.opaque,
            additive: &self.additive,
            particles: &self.particles,
            draws: &self.draws,
            uniforms: &self.uniforms,
            stars: &star_points,
            clear: pack_rgba(self.clear_color),
            post: self.post,
            sun_screen,
            shaded: &shaded,
        };

        let jobs: Vec<BandJob> = self
            .pixels
            .chunks_mut(width * BAND_HEIGHT * 4)
            .zip(self.depth.chunks_mut(width * BAND_HEIGHT))
            .zip(self.visibility.chunks_mut(width * BAND_HEIGHT))
            .enumerate()
            .map(|(index, ((pixels, depth), visibility))| BandJob {
                index,
                pixels,
                depth,
                visibility,
                opaque_bin: &opaque_bins[index],
                additive_bin: &additive_bins[index],
            })
            .collect();
        let queue = Mutex::new(jobs);
        let workers = self.threads.min(band_count).max(1);
        if workers == 1 {
            while let Some(job) = queue.lock().ok().and_then(|mut q| q.pop()) {
                context.run(job);
            }
        } else {
            std::thread::scope(|scope| {
                for _ in 0..workers {
                    scope.spawn(|| {
                        while let Some(job) = queue.lock().ok().and_then(|mut q| q.pop()) {
                            context.run(job);
                        }
                    });
                }
            });
        }
        self.stats.shaded_pixels = shaded.load(Ordering::Relaxed);
        self.stats.raster_ms = start.elapsed().as_secs_f32() * 1000.0;
    }

    /// Sombrea un fragmento aislado (usado por pruebas).
    #[cfg(test)]
    fn shade_one(&self, shader: ShaderKind, position: Vec3, normal: Vec3, base: Color3) -> Color3 {
        let fragment = Fragment {
            position,
            normal,
            local: position,
            uv: Vec2::default(),
            color: base,
            material: MaterialKind::Standard,
        };
        shaders::shade(&fragment, &self.uniforms, &DrawOptions::new(shader))
            .unwrap_or(Color3::new(0.0, 0.0, 0.0))
    }
}

#[derive(Default)]
struct Counters {
    backface: AtomicU64,
    clipped: AtomicU64,
}

struct VertexSetup {
    mvp: Mat4,
    model: Mat4,
    normal: [[f32; 3]; 3],
    width: f32,
    height: f32,
    cull: bool,
    draw: u32,
    near_w: f32,
    far_w: f32,
}

impl VertexSetup {
    fn process(
        &self,
        triangles: &[crate::model::Triangle],
        out: &mut Vec<ScreenTri>,
        counters: &Counters,
    ) {
        let mut backface = 0u64;
        let mut clipped = 0u64;
        for triangle in triangles {
            let mut verts = [ClipVertex {
                clip: Vec4::default(),
                attrs: [0.0; ATTRS],
            }; 3];
            for (i, input) in triangle.vertices.iter().enumerate() {
                let p = input.position;
                let world = self.model.transform_point(p);
                let normal = apply3(&self.normal, input.normal).normalized();
                verts[i] = ClipVertex {
                    clip: self.mvp * Vec4::new(p.x, p.y, p.z, 1.0),
                    attrs: [
                        world.x,
                        world.y,
                        world.z,
                        normal.x,
                        normal.y,
                        normal.z,
                        p.x,
                        p.y,
                        p.z,
                        input.uv.x,
                        input.uv.y,
                        triangle.color.r,
                        triangle.color.g,
                        triangle.color.b,
                    ],
                };
            }
            // Rechazo trivial: los tres vértices fuera del mismo plano lateral.
            let outside = |f: &dyn Fn(&Vec4) -> bool| verts.iter().all(|v| f(&v.clip));
            if outside(&|c| c.x > c.w)
                || outside(&|c| c.x < -c.w)
                || outside(&|c| c.y > c.w)
                || outside(&|c| c.y < -c.w)
                || outside(&|c| c.w < self.near_w)
                || outside(&|c| c.w > self.far_w)
            {
                continue;
            }
            if verts.iter().all(|v| v.clip.w >= self.near_w) {
                if !self.emit(&verts[0], &verts[1], &verts[2], triangle.material, out) {
                    backface += 1;
                }
            } else {
                // Recorte Sutherland–Hodgman contra el plano w = near.
                clipped += 1;
                let mut polygon: [ClipVertex; 4] = [verts[0]; 4];
                let mut count = 0;
                for i in 0..3 {
                    let a = &verts[i];
                    let b = &verts[(i + 1) % 3];
                    let da = a.clip.w - self.near_w;
                    let db = b.clip.w - self.near_w;
                    if da >= 0.0 {
                        polygon[count] = *a;
                        count += 1;
                    }
                    if (da >= 0.0) != (db >= 0.0) {
                        let t = da / (da - db);
                        polygon[count] = lerp_vertex(a, b, t);
                        count += 1;
                    }
                }
                for i in 1..count.saturating_sub(1) {
                    if !self.emit(
                        &polygon[0],
                        &polygon[i],
                        &polygon[i + 1],
                        triangle.material,
                        out,
                    ) {
                        backface += 1;
                    }
                }
            }
        }
        counters.backface.fetch_add(backface, Ordering::Relaxed);
        counters.clipped.fetch_add(clipped, Ordering::Relaxed);
    }

    /// Proyecta y prepara un triángulo. Devuelve `false` si fue descartado
    /// por cara trasera.
    fn emit(
        &self,
        a: &ClipVertex,
        b: &ClipVertex,
        c: &ClipVertex,
        material: MaterialKind,
        out: &mut Vec<ScreenTri>,
    ) -> bool {
        let verts = [a, b, c];
        let mut sx = [0.0f32; 3];
        let mut sy = [0.0f32; 3];
        let mut inv_w = [0.0f32; 3];
        for i in 0..3 {
            inv_w[i] = 1.0 / verts[i].clip.w;
            sx[i] = (verts[i].clip.x * inv_w[i] * 0.5 + 0.5) * self.width;
            sy[i] = (1.0 - (verts[i].clip.y * inv_w[i] * 0.5 + 0.5)) * self.height;
        }
        let area = (sx[1] - sx[0]) * (sy[2] - sy[0]) - (sy[1] - sy[0]) * (sx[2] - sx[0]);
        // En pantalla Y crece hacia abajo: los triángulos frontales (CCW en
        // NDC) tienen área negativa aquí.
        if self.cull && area >= 0.0 {
            return false;
        }
        if area.abs() < 1.0e-9 {
            return true;
        }
        let min_x = sx.iter().copied().fold(f32::INFINITY, f32::min);
        let max_x = sx.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min_y = sy.iter().copied().fold(f32::INFINITY, f32::min);
        let max_y = sy.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        // Límites en píxeles cuyo centro (x + 0.5) puede quedar dentro.
        let x0 = (min_x - 0.5).ceil().max(0.0);
        let x1 = (max_x - 0.5).floor().min(self.width - 1.0);
        let y0 = (min_y - 0.5).ceil().max(0.0);
        let y1 = (max_y - 0.5).floor().min(self.height - 1.0);
        if x0 > x1 || y0 > y1 {
            return true; // Triángulo subpíxel: no cubre ningún centro.
        }
        let inv_area = 1.0 / area;
        // b0 corresponde a la arista (v1, v2), etc.
        let edge = |i: usize, j: usize| {
            let a = (sy[i] - sy[j]) * inv_area;
            let b = (sx[j] - sx[i]) * inv_area;
            let c = (sx[i] * sy[j] - sx[j] * sy[i]) * inv_area;
            [a, b, c]
        };
        let edges = [edge(1, 2), edge(2, 0), edge(0, 1)];
        let mut attrs = [[0.0; ATTRS]; 3];
        for (i, out_attrs) in attrs.iter_mut().enumerate() {
            for (value, source) in out_attrs.iter_mut().zip(verts[i].attrs) {
                *value = source * inv_w[i];
            }
        }
        out.push(ScreenTri {
            edges,
            inv_w,
            attrs,
            min_x: x0 as u16,
            max_x: x1 as u16,
            min_y: y0 as u16,
            max_y: y1 as u16,
            material,
            draw: self.draw,
        });
        true
    }
}

fn lerp_vertex(a: &ClipVertex, b: &ClipVertex, t: f32) -> ClipVertex {
    let mut attrs = [0.0; ATTRS];
    for (k, value) in attrs.iter_mut().enumerate() {
        *value = a.attrs[k] + (b.attrs[k] - a.attrs[k]) * t;
    }
    ClipVertex {
        clip: Vec4::new(
            a.clip.x + (b.clip.x - a.clip.x) * t,
            a.clip.y + (b.clip.y - a.clip.y) * t,
            a.clip.z + (b.clip.z - a.clip.z) * t,
            a.clip.w + (b.clip.w - a.clip.w) * t,
        ),
        attrs,
    }
}

/// Planos del frustum (Gribb–Hartmann) en espacio mundo, normalizados.
fn extract_frustum(m: &Mat4) -> [Vec4; 6] {
    let row = |r: usize| Vec4::new(m.m[r][0], m.m[r][1], m.m[r][2], m.m[r][3]);
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    let add = |a: Vec4, b: Vec4| Vec4::new(a.x + b.x, a.y + b.y, a.z + b.z, a.w + b.w);
    let sub = |a: Vec4, b: Vec4| Vec4::new(a.x - b.x, a.y - b.y, a.z - b.z, a.w - b.w);
    let normalize = |p: Vec4| {
        let l = (p.x * p.x + p.y * p.y + p.z * p.z).sqrt().max(1.0e-9);
        Vec4::new(p.x / l, p.y / l, p.z / l, p.w / l)
    };
    [
        normalize(add(r3, r0)),
        normalize(sub(r3, r0)),
        normalize(add(r3, r1)),
        normalize(sub(r3, r1)),
        normalize(add(r3, r2)),
        normalize(sub(r3, r2)),
    ]
}

struct BandJob<'a> {
    index: usize,
    pixels: &'a mut [u8],
    depth: &'a mut [f32],
    visibility: &'a mut [u32],
    opaque_bin: &'a [u32],
    additive_bin: &'a [u32],
}

struct BandContext<'a> {
    width: usize,
    height: usize,
    opaque: &'a [ScreenTri],
    additive: &'a [ScreenTri],
    particles: &'a [ScreenParticle],
    draws: &'a [DrawCall],
    uniforms: &'a Uniforms,
    stars: &'a [(usize, usize, Color3, bool)],
    clear: [u8; 4],
    post: PostFx,
    sun_screen: Option<(Vec2, f32)>,
    shaded: &'a AtomicU64,
}

impl BandContext<'_> {
    fn interpolate(&self, tri: &ScreenTri, px: f32, py: f32) -> Option<(Fragment, f32)> {
        let b = [
            tri.edges[0][0] * px + tri.edges[0][1] * py + tri.edges[0][2],
            tri.edges[1][0] * px + tri.edges[1][1] * py + tri.edges[1][2],
            tri.edges[2][0] * px + tri.edges[2][1] * py + tri.edges[2][2],
        ];
        let z = b[0] * tri.inv_w[0] + b[1] * tri.inv_w[1] + b[2] * tri.inv_w[2];
        if z <= 1.0e-12 {
            return None;
        }
        let w = 1.0 / z;
        let mut a = [0.0f32; ATTRS];
        for (k, value) in a.iter_mut().enumerate() {
            *value = (b[0] * tri.attrs[0][k] + b[1] * tri.attrs[1][k] + b[2] * tri.attrs[2][k]) * w;
        }
        Some((
            Fragment {
                position: Vec3::new(a[0], a[1], a[2]),
                normal: Vec3::new(a[3], a[4], a[5]).normalized(),
                local: Vec3::new(a[6], a[7], a[8]),
                uv: Vec2::new(a[9], a[10]),
                color: Color3::new(a[11], a[12], a[13]),
                material: tri.material,
            },
            z,
        ))
    }

    fn run(&self, job: BandJob) {
        let width = self.width;
        let y_start = job.index * BAND_HEIGHT;
        let rows = job.depth.len() / width;
        let y_end = y_start + rows; // exclusivo

        // 1. Limpieza de la banda y fondo de estrellas.
        for pixel in job.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&self.clear);
        }
        job.depth.fill(0.0);
        job.visibility.fill(u32::MAX);
        for &(x, y, color, big) in self.stars {
            if y >= y_start && y < y_end {
                put_add(job.pixels, (y - y_start) * width + x, color);
                if big && x + 1 < width {
                    put_add(job.pixels, (y - y_start) * width + x + 1, color * 0.45);
                }
            }
        }

        // 2. Visibility buffer: Z-buffer con 1/w (mayor = más cerca).
        for &index in job.opaque_bin {
            let tri = &self.opaque[index as usize];
            let shader = self.draws[tri.draw as usize].options.shader;
            let alpha_tested = shaders::uses_alpha_test(shader);
            let y0 = (tri.min_y as usize).max(y_start);
            let y1 = (tri.max_y as usize).min(y_end - 1);
            if y0 > y1 {
                continue;
            }
            let x0 = tri.min_x as usize;
            let x1 = tri.max_x as usize;
            for y in y0..=y1 {
                let py = y as f32 + 0.5;
                let px0 = x0 as f32 + 0.5;
                let mut b0 = tri.edges[0][0] * px0 + tri.edges[0][1] * py + tri.edges[0][2];
                let mut b1 = tri.edges[1][0] * px0 + tri.edges[1][1] * py + tri.edges[1][2];
                let mut b2 = tri.edges[2][0] * px0 + tri.edges[2][1] * py + tri.edges[2][2];
                let row = (y - y_start) * width;
                for x in x0..=x1 {
                    if b0 >= 0.0 && b1 >= 0.0 && b2 >= 0.0 {
                        let z = b0 * tri.inv_w[0] + b1 * tri.inv_w[1] + b2 * tri.inv_w[2];
                        let i = row + x;
                        if z > job.depth[i]
                            && (!alpha_tested
                                || self.interpolate(tri, x as f32 + 0.5, py).is_some_and(
                                    |(f, _)| shaders::covers(&f, self.uniforms, shader),
                                ))
                        {
                            job.depth[i] = z;
                            job.visibility[i] = index;
                        }
                    }
                    b0 += tri.edges[0][0];
                    b1 += tri.edges[1][0];
                    b2 += tri.edges[2][0];
                }
            }
        }

        // 3. Sombreado diferido: un shader por píxel visible.
        let mut shaded = 0u64;
        for local_y in 0..rows {
            let y = y_start + local_y;
            for x in 0..width {
                let i = local_y * width + x;
                let index = job.visibility[i];
                if index == u32::MAX {
                    continue;
                }
                let tri = &self.opaque[index as usize];
                let draw = &self.draws[tri.draw as usize];
                if let Some((fragment, _)) = self.interpolate(tri, x as f32 + 0.5, y as f32 + 0.5) {
                    match shaders::shade(&fragment, self.uniforms, &draw.options) {
                        Some(color) => put(job.pixels, i, color),
                        None => job.depth[i] = 0.0, // descartado (nubes)
                    }
                    shaded += 1;
                }
            }
        }

        // 4. Triángulos aditivos con prueba de profundidad sin escritura.
        for &index in job.additive_bin {
            let tri = &self.additive[index as usize];
            let draw = &self.draws[tri.draw as usize];
            let y0 = (tri.min_y as usize).max(y_start);
            let y1 = (tri.max_y as usize).min(y_end - 1);
            for y in y0..=y1 {
                for x in tri.min_x as usize..=tri.max_x as usize {
                    let px = x as f32 + 0.5;
                    let py = y as f32 + 0.5;
                    let b0 = tri.edges[0][0] * px + tri.edges[0][1] * py + tri.edges[0][2];
                    let b1 = tri.edges[1][0] * px + tri.edges[1][1] * py + tri.edges[1][2];
                    let b2 = tri.edges[2][0] * px + tri.edges[2][1] * py + tri.edges[2][2];
                    if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                        continue;
                    }
                    let i = (y - y_start) * width + x;
                    if let Some((fragment, z)) = self.interpolate(tri, px, py)
                        && z > job.depth[i]
                        && let Some(color) = shaders::shade(&fragment, self.uniforms, &draw.options)
                    {
                        put_add(job.pixels, i, color);
                        shaded += 1;
                    }
                }
            }
        }

        // 5. Partículas (discos suaves con prueba de profundidad).
        for p in self.particles {
            if p.y + p.radius < y_start as f32 || p.y - p.radius >= y_end as f32 {
                continue;
            }
            let py0 = ((p.y - p.radius).floor().max(y_start as f32)) as usize;
            let py1 = ((p.y + p.radius).ceil().min(y_end as f32 - 1.0)) as usize;
            let px0 = (p.x - p.radius).floor().max(0.0) as usize;
            let px1 = ((p.x + p.radius).ceil().min(width as f32 - 1.0)) as usize;
            let r2 = p.radius * p.radius;
            for y in py0..=py1 {
                for x in px0..=px1 {
                    let dx = x as f32 + 0.5 - p.x;
                    let dy = y as f32 + 0.5 - p.y;
                    let d2 = dx * dx + dy * dy;
                    if d2 > r2 {
                        continue;
                    }
                    let i = (y - y_start) * width + x;
                    if p.inv_w <= job.depth[i] {
                        continue;
                    }
                    let falloff = (1.0 - d2 / r2).powf(1.5) * p.alpha;
                    if p.additive {
                        put_add(job.pixels, i, p.color * falloff);
                    } else {
                        blend(job.pixels, i, p.color, falloff);
                    }
                }
            }
        }

        // 6. Postproceso por banda.
        self.post_process(job.pixels, y_start, rows);
        self.shaded.fetch_add(shaded, Ordering::Relaxed);
    }

    fn post_process(&self, pixels: &mut [u8], y_start: usize, rows: usize) {
        let width = self.width;
        let post = self.post;
        let time = self.uniforms.time;
        let cx = width as f32 * 0.5;
        let cy = self.height as f32 * 0.5;
        let max_r = (cx * cx + cy * cy).sqrt();

        // Distorsión por calor: desplazamiento horizontal por fila.
        if post.heat > 0.001 {
            let mut row_copy = vec![0u8; width * 4];
            for local_y in 0..rows {
                let y = (y_start + local_y) as f32;
                let shift = ((y * 0.09 + time * 6.0).sin() * 2.2
                    + (y * 0.23 - time * 3.7).sin() * 1.1)
                    * post.heat;
                let start = local_y * width * 4;
                row_copy.copy_from_slice(&pixels[start..start + width * 4]);
                for x in 0..width {
                    let sx = (x as f32 + shift).round().clamp(0.0, width as f32 - 1.0) as usize;
                    pixels[start + x * 4..start + x * 4 + 3]
                        .copy_from_slice(&row_copy[sx * 4..sx * 4 + 3]);
                }
            }
        }

        let sun = self.sun_screen.and_then(|(p, inv_w)| {
            if post.sun_glow <= 0.0 {
                return None;
            }
            Some((p, inv_w))
        });
        let needs_pass = post.fade > 0.001
            || post.flash > 0.001
            || post.damage > 0.001
            || post.vignette > 0.001
            || post.frost > 0.001
            || sun.is_some();
        if !needs_pass {
            return;
        }
        for local_y in 0..rows {
            let y = (y_start + local_y) as f32 + 0.5;
            for x in 0..width {
                let i = (local_y * width + x) * 4;
                let mut r = pixels[i] as f32 / 255.0;
                let mut g = pixels[i + 1] as f32 / 255.0;
                let mut b = pixels[i + 2] as f32 / 255.0;
                let dx = x as f32 + 0.5 - cx;
                let dy = y - cy;
                let edge = ((dx * dx + dy * dy).sqrt() / max_r).clamp(0.0, 1.0);
                if let Some((p, _)) = sun {
                    let sx = x as f32 - p.x;
                    let sy = y - p.y;
                    let d2 = (sx * sx + sy * sy) / (self.height as f32 * self.height as f32);
                    let glow =
                        post.sun_glow * (0.55 * (-d2 * 9.0).exp() + 0.45 * (-d2 * 60.0).exp());
                    r += glow;
                    g += glow * 0.78;
                    b += glow * 0.45;
                }
                if post.frost > 0.001 {
                    let f = post.frost * (0.25 + 0.75 * edge * edge);
                    r = r * (1.0 - f * 0.4) + 0.75 * f * 0.4;
                    g = g * (1.0 - f * 0.3) + 0.88 * f * 0.3;
                    b = b * (1.0 - f * 0.2) + f * 0.2;
                }
                if post.damage > 0.001 {
                    let d = post.damage * edge.powf(1.6);
                    r = r * (1.0 - d) + 0.9 * d;
                    g *= 1.0 - d;
                    b *= 1.0 - d;
                }
                if post.vignette > 0.001 {
                    let v = 1.0 - post.vignette * edge.powf(2.2);
                    r *= v;
                    g *= v;
                    b *= v;
                }
                if post.flash > 0.001 {
                    r += (1.0 - r) * post.flash;
                    g += (1.0 - g) * post.flash;
                    b += (1.0 - b) * post.flash;
                }
                if post.fade > 0.001 {
                    let k = 1.0 - post.fade.clamp(0.0, 1.0);
                    r *= k;
                    g *= k;
                    b *= k;
                }
                pixels[i] = to_u8(r);
                pixels[i + 1] = to_u8(g);
                pixels[i + 2] = to_u8(b);
            }
        }
    }
}

#[inline]
fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[inline]
fn put(pixels: &mut [u8], index: usize, color: Color3) {
    let i = index * 4;
    pixels[i] = to_u8(color.r);
    pixels[i + 1] = to_u8(color.g);
    pixels[i + 2] = to_u8(color.b);
    pixels[i + 3] = 255;
}

#[inline]
fn put_add(pixels: &mut [u8], index: usize, color: Color3) {
    let i = index * 4;
    pixels[i] = pixels[i].saturating_add(to_u8(color.r));
    pixels[i + 1] = pixels[i + 1].saturating_add(to_u8(color.g));
    pixels[i + 2] = pixels[i + 2].saturating_add(to_u8(color.b));
}

#[inline]
fn blend(pixels: &mut [u8], index: usize, color: Color3, alpha: f32) {
    let i = index * 4;
    let mix =
        |dst: u8, src: f32| to_u8(dst as f32 / 255.0 * (1.0 - alpha) + src.clamp(0.0, 1.0) * alpha);
    pixels[i] = mix(pixels[i], color.r);
    pixels[i + 1] = mix(pixels[i + 1], color.g);
    pixels[i + 2] = mix(pixels[i + 2], color.b);
}

pub fn pack_rgba(color: Color3) -> [u8; 4] {
    [to_u8(color.r), to_u8(color.g), to_u8(color.b), 255]
}

/// Guarda el framebuffer en formato PPM (P6). Se usa para capturas sin
/// ventana y para la evidencia de pruebas visuales.
pub fn save_ppm(
    path: &std::path::Path,
    width: usize,
    height: usize,
    rgba: &[u8],
) -> std::io::Result<()> {
    let mut data = format!("P6\n{width} {height}\n255\n").into_bytes();
    data.reserve(width * height * 3);
    for pixel in rgba.chunks_exact(4) {
        data.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Triangle, Vertex, make_uv_sphere};

    fn camera_at(z: f32) -> Camera {
        Camera {
            position: Vec3::new(0.0, 0.0, z),
            target: Vec3::ZERO,
            up: Vec3::new(0.0, 1.0, 0.0),
            fov_y: 60.0_f32.to_radians(),
            near: 0.1,
            far: 100.0,
        }
    }

    fn quad(z: f32, color: Color3) -> Mesh {
        let v = |x: f32, y: f32| Vertex {
            position: Vec3::new(x, y, z),
            normal: Vec3::new(0.0, 0.0, 1.0),
            uv: Vec2::default(),
        };
        Mesh::from_triangles(vec![
            Triangle::new([v(-1.0, -1.0), v(1.0, -1.0), v(1.0, 1.0)], color),
            Triangle::new([v(-1.0, -1.0), v(1.0, 1.0), v(-1.0, 1.0)], color),
        ])
    }

    #[test]
    fn rasterizes_a_sphere_into_the_cpu_framebuffer() {
        let mut renderer = SoftwareRenderer::new(160, 90);
        renderer.begin_frame(&camera_at(5.0), Color3::new(0.0, 0.0, 0.0), 0.0);
        renderer.draw_mesh(
            &make_uv_sphere(10, 6),
            Mat4::identity(),
            &DrawOptions::new(ShaderKind::Sun),
        );
        renderer.end_frame();
        assert!(renderer.stats().rasterized_triangles > 0);
        assert!(renderer.stats().shaded_pixels > 0);
        // El centro debe quedar pintado por el sol (canal rojo alto).
        let center = (45 * 160 + 80) * 4;
        assert!(renderer.pixels()[center] > 200);
    }

    #[test]
    fn depth_test_keeps_the_nearest_surface_regardless_of_order() {
        for threads in [1, 4] {
            let mut renderer = SoftwareRenderer::new(64, 64);
            renderer.set_threads(threads);
            renderer.begin_frame(&camera_at(5.0), Color3::new(0.0, 0.0, 0.0), 0.0);
            let near = quad(1.0, Color3::new(0.0, 1.0, 0.0));
            let far = quad(-1.0, Color3::new(1.0, 0.0, 0.0));
            let options = DrawOptions::new(ShaderKind::Unlit);
            renderer.draw_mesh(&near, Mat4::identity(), &options);
            renderer.draw_mesh(&far, Mat4::identity(), &options);
            renderer.end_frame();
            let center = (32 * 64 + 32) * 4;
            let px = &renderer.pixels()[center..center + 3];
            assert!(
                px[1] > 200 && px[0] < 30,
                "debe verse el cuadro cercano: {px:?}"
            );
        }
    }

    #[test]
    fn backfaces_are_culled_and_double_sided_draws_are_kept() {
        let mut renderer = SoftwareRenderer::new(64, 64);
        renderer.begin_frame(&camera_at(-5.0), Color3::new(0.0, 0.0, 0.0), 0.0);
        let q = quad(0.0, Color3::new(1.0, 1.0, 1.0));
        renderer.draw_mesh(&q, Mat4::identity(), &DrawOptions::new(ShaderKind::Unlit));
        assert_eq!(renderer.stats().backface_culled, 2);
        renderer.draw_mesh(
            &q,
            Mat4::identity(),
            &DrawOptions::new(ShaderKind::Unlit).double_sided(),
        );
        renderer.end_frame();
        assert_eq!(renderer.stats().rasterized_triangles, 2);
    }

    #[test]
    fn triangles_crossing_the_near_plane_are_clipped_not_dropped() {
        let mut renderer = SoftwareRenderer::new(64, 64);
        renderer.begin_frame(
            &Camera {
                position: Vec3::new(0.0, 1.0, 0.0),
                target: Vec3::new(0.0, 1.0, -1.0),
                up: Vec3::new(0.0, 1.0, 0.0),
                fov_y: 1.2,
                near: 0.1,
                far: 100.0,
            },
            Color3::new(0.0, 0.0, 0.0),
            0.0,
        );
        // Suelo enorme que pasa por detrás de la cámara.
        let v = |x: f32, z: f32| Vertex {
            position: Vec3::new(x, 0.0, z),
            normal: Vec3::new(0.0, 1.0, 0.0),
            uv: Vec2::default(),
        };
        let floor = Mesh::from_triangles(vec![Triangle::new(
            [v(-50.0, 20.0), v(50.0, 20.0), v(0.0, -50.0)],
            Color3::new(1.0, 1.0, 1.0),
        )]);
        renderer.draw_mesh(
            &floor,
            Mat4::identity(),
            &DrawOptions::new(ShaderKind::Unlit).double_sided(),
        );
        renderer.end_frame();
        assert!(renderer.stats().clipped_triangles >= 1);
        // La mitad inferior de la pantalla debe estar pintada por el suelo.
        let bottom = (60 * 64 + 32) * 4;
        assert!(renderer.pixels()[bottom] > 200);
    }

    #[test]
    fn frustum_culling_skips_objects_behind_the_camera() {
        let mut renderer = SoftwareRenderer::new(64, 64);
        renderer.begin_frame(&camera_at(5.0), Color3::new(0.0, 0.0, 0.0), 0.0);
        let drawn = renderer.draw_mesh(
            &make_uv_sphere(8, 6),
            Mat4::translation(Vec3::new(0.0, 0.0, 20.0)),
            &DrawOptions::new(ShaderKind::Unlit),
        );
        assert!(!drawn);
        assert_eq!(renderer.stats().culled_objects, 1);
        assert_eq!(renderer.stats().submitted_triangles, 0);
    }

    #[test]
    fn perspective_correct_interpolation_keeps_attributes_in_range() {
        let mut renderer = SoftwareRenderer::new(80, 80);
        renderer.stars.clear();
        renderer.begin_frame(&camera_at(3.0), Color3::new(0.0, 0.0, 0.0), 0.0);
        let mut mesh = quad(0.0, Color3::new(0.5, 0.5, 0.5));
        mesh.triangles[0].vertices[0].position.z = -6.0; // muy inclinado
        renderer.draw_mesh(
            &mesh,
            Mat4::identity(),
            &DrawOptions::new(ShaderKind::Unlit).double_sided(),
        );
        renderer.end_frame();
        for px in renderer.pixels().chunks_exact(4) {
            assert!(
                px[0] <= 128,
                "el color interpolado no debe exceder el original"
            );
        }
    }

    #[test]
    fn planets_and_ship_remain_visible_without_direct_sunlight() {
        let renderer = SoftwareRenderer::new(16, 16);
        let position = Vec3::new(5.0, 0.0, 0.0);
        let normal = Vec3::new(1.0, 0.0, 0.0);
        let base = Color3::new(0.4, 0.4, 0.4);
        let oasis = renderer.shade_one(ShaderKind::Lit, position, normal, base);
        let ship = renderer.shade_one(ShaderKind::Metal, position, normal, base);
        let ice = renderer.shade_one(ShaderKind::Ice, position, normal, base);
        assert!(oasis.r > 0.12 && oasis.g > 0.12 && oasis.b > 0.12);
        assert!(ice.r > 0.15 && ice.g > 0.15 && ice.b > 0.15);
        assert!(ship.r > 0.15 && ship.g > 0.15 && ship.b > 0.15);
    }

    #[test]
    fn particles_respect_depth() {
        let mut renderer = SoftwareRenderer::new(64, 64);
        renderer.begin_frame(&camera_at(5.0), Color3::new(0.0, 0.0, 0.0), 0.0);
        renderer.draw_mesh(
            &quad(1.0, Color3::new(0.0, 0.0, 1.0)),
            Mat4::identity(),
            &DrawOptions::new(ShaderKind::Unlit),
        );
        renderer.draw_particle(
            Vec3::new(0.0, 0.0, -2.0),
            0.5,
            Color3::new(1.0, 0.0, 0.0),
            1.0,
            false,
        );
        renderer.end_frame();
        let center = (32 * 64 + 32) * 4;
        assert!(
            renderer.pixels()[center] < 20,
            "la partícula oculta no debe verse"
        );
    }
}
