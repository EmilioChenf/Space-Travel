//! Arquitectura reutilizable de minijuegos y los cinco minijuegos.
//!
//! Cada minijuego implementa el trait [`Minigame`] y se ejecuta dentro de
//! la escena continua del sistema solar (sin pantallas de carga): sus
//! objetos viven en coordenadas locales del planeta, giran y orbitan con
//! él, se dibujan con el renderizador por software y se juegan con la nave.
//! [`MinigameRunner`] gestiona el ciclo común: instrucciones → juego →
//! victoria/derrota → reinicio o salida.

use crate::assets::{Assets, PropKind};
use crate::math3d::{Mat4, PI, Rng, Vec3};
use crate::model::Color3;
use crate::procgen::{align_y, scatter_directions};
use crate::renderer::{DrawOptions, ShaderKind, SoftwareRenderer};
use crate::solar::PlanetId;
use crate::surface::PlanetSurface;

/// Información del mundo que recibe un minijuego cada cuadro. `time` es el
/// tiempo transcurrido del minijuego (anima obstáculos y objetivos).
pub struct MinigameContext<'a> {
    pub planet: PlanetId,
    /// Transformación local del planeta → mundo (incluye escala).
    pub planet_model: Mat4,
    pub planet_center: Vec3,
    pub planet_scale: f32,
    pub ship_position: Vec3,
    pub ship_velocity: Vec3,
    pub time: f32,
    pub surface: &'a PlanetSurface,
}

impl MinigameContext<'_> {
    pub fn to_world(&self, local: Vec3) -> Vec3 {
        self.planet_model.transform_point(local)
    }
}

/// Efectos que un minijuego solicita al juego en un cuadro.
#[derive(Default)]
pub struct MinigameEvents {
    pub damage: f32,
    pub message: Option<String>,
    /// Ráfagas de partículas (posición mundo, color).
    pub bursts: Vec<(Vec3, Color3)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    Playing,
    Won,
    Lost(String),
}

pub trait Minigame {
    fn title(&self) -> &'static str;
    fn instructions(&self) -> &'static [&'static str];
    /// Límite de tiempo (None = cronómetro sin límite).
    fn time_limit(&self) -> Option<f32>;
    /// Prepara los objetivos (posiciones reproducibles con semilla).
    fn reset(&mut self, ctx: &MinigameContext);
    fn update(
        &mut self,
        ctx: &MinigameContext,
        elapsed: f32,
        dt: f32,
        events: &mut MinigameEvents,
    ) -> Progress;
    fn render(&self, renderer: &mut SoftwareRenderer, assets: &Assets, ctx: &MinigameContext);
    /// (logrados, total)
    fn progress(&self) -> (u32, u32);
    fn status_lines(&self, ctx: &MinigameContext) -> Vec<String>;
    /// Objetivo actual en coordenadas mundo (para el indicador del HUD).
    fn objective(&self, ctx: &MinigameContext) -> Option<Vec3>;
    /// Penalización de tiempo acumulada (segundos).
    fn penalty(&self) -> f32 {
        0.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunState {
    Intro,
    Playing,
    Won,
    Lost(String),
}

pub struct MinigameRunner {
    pub planet: PlanetId,
    pub game: Box<dyn Minigame>,
    pub state: RunState,
    pub elapsed: f32,
    pub best_time: Option<f32>,
    pub attempts: u32,
}

impl MinigameRunner {
    pub fn new(planet: PlanetId) -> Option<Self> {
        create(planet).map(|game| Self {
            planet,
            game,
            state: RunState::Intro,
            elapsed: 0.0,
            best_time: None,
            attempts: 0,
        })
    }

    pub fn start(&mut self, ctx: &MinigameContext) {
        self.game.reset(ctx);
        self.elapsed = 0.0;
        self.state = RunState::Playing;
        self.attempts += 1;
    }

    /// Tiempo total incluyendo penalizaciones.
    pub fn total_time(&self) -> f32 {
        self.elapsed + self.game.penalty()
    }

    pub fn time_left(&self) -> Option<f32> {
        self.game
            .time_limit()
            .map(|limit| (limit - self.total_time()).max(0.0))
    }

    pub fn update(&mut self, ctx: &MinigameContext, dt: f32, events: &mut MinigameEvents) {
        if self.state != RunState::Playing {
            return;
        }
        self.elapsed += dt;
        match self.game.update(ctx, self.elapsed, dt, events) {
            Progress::Playing => {
                if self.time_left() == Some(0.0) {
                    self.state = RunState::Lost("Se acabo el tiempo".to_owned());
                }
            }
            Progress::Won => {
                let t = self.total_time();
                self.best_time = Some(self.best_time.map_or(t, |b| b.min(t)));
                self.state = RunState::Won;
            }
            Progress::Lost(reason) => self.state = RunState::Lost(reason),
        }
    }

    /// Puntuación: objetivos logrados y bonificación por tiempo restante.
    pub fn score(&self) -> u32 {
        let (done, total) = self.game.progress();
        let base = done * 100;
        let bonus = match (&self.state, self.game.time_limit()) {
            (RunState::Won, Some(limit)) => ((limit - self.total_time()).max(0.0) * 5.0) as u32,
            (RunState::Won, None) => (600.0 - self.total_time()).max(0.0) as u32 * 2,
            _ => 0,
        };
        base + bonus + if total > 0 && done == total { 250 } else { 0 }
    }
}

/// Fábrica: minijuego asociado a cada planeta temático.
pub fn create(planet: PlanetId) -> Option<Box<dyn Minigame>> {
    match planet {
        PlanetId::Hielo => Some(Box::new(CrystalHunt::default())),
        PlanetId::Oasis => Some(Box::new(RelicHunt::default())),
        PlanetId::Fuego => Some(Box::new(VolcanicEscape::default())),
        PlanetId::Dulces => Some(Box::new(CandyRace::default())),
        PlanetId::Escandalosos => Some(Box::new(BearSearch::default())),
        PlanetId::Tierra => None,
    }
}

pub fn minigame_title(planet: PlanetId) -> Option<&'static str> {
    create(planet).map(|g| g.title())
}

/// Elige direcciones libres sobre la superficie (sin decoraciones encima).
fn surface_spots(ctx: &MinigameContext, seed: u32, count: usize, spacing: f32) -> Vec<(Vec3, f32)> {
    let mut rng = Rng::new(seed);
    let surface = ctx.surface;
    scatter_directions(&mut rng, count, spacing, |d| surface.is_clear(d, 0.06))
        .into_iter()
        .map(|d| (d, surface.ground_at(d, 0.06)))
        .collect()
}

fn nearest(ctx: &MinigameContext, points: impl Iterator<Item = Vec3>) -> Option<(Vec3, f32)> {
    points
        .map(|p| {
            let w = ctx.to_world(p);
            (w, (w - ctx.ship_position).length())
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

fn beacon(
    renderer: &mut SoftwareRenderer,
    base: Vec3,
    up: Vec3,
    height: f32,
    color: Color3,
    time: f32,
) {
    let steps = 14;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let pulse = 0.5 + 0.5 * (time * 4.0 - t * 8.0).sin();
        renderer.draw_particle(
            base + up * (t * height),
            0.35 + pulse * 0.25,
            color * (0.4 + pulse * 0.6),
            1.0 - t * 0.7,
            true,
        );
    }
}

// ---------------------------------------------------------------------------
// Hielo: Cristales perdidos
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct CrystalHunt {
    crystals: Vec<(Vec3, Vec3, bool)>, // posición local, dirección, recogido
}

pub const CRYSTAL_COUNT: usize = 8;
const CRYSTAL_PICKUP: f32 = 3.6;

impl Minigame for CrystalHunt {
    fn title(&self) -> &'static str {
        "Cristales perdidos"
    }
    fn instructions(&self) -> &'static [&'static str] {
        &[
            "Recolecta los 8 cristales de energia repartidos por el planeta Hielo.",
            "Vuela cerca de cada cristal para recogerlo. Sigue el indicador cian.",
            "Tienes 3 minutos antes de que la ventisca los entierre.",
        ]
    }
    fn time_limit(&self) -> Option<f32> {
        Some(180.0)
    }
    fn reset(&mut self, ctx: &MinigameContext) {
        self.crystals = surface_spots(ctx, 0x1CE5, CRYSTAL_COUNT, 0.45)
            .into_iter()
            .map(|(d, ground)| (d * (ground + 0.45), d, false))
            .collect();
    }
    fn update(
        &mut self,
        ctx: &MinigameContext,
        _elapsed: f32,
        _dt: f32,
        events: &mut MinigameEvents,
    ) -> Progress {
        for (local, _, collected) in &mut self.crystals {
            if !*collected {
                let world = ctx.to_world(*local);
                if (world - ctx.ship_position).length() < CRYSTAL_PICKUP {
                    *collected = true;
                    events.bursts.push((world, Color3::new(0.5, 0.9, 1.0)));
                    events.message = Some("Cristal recuperado".to_owned());
                }
            }
        }
        if self.crystals.iter().all(|c| c.2) {
            Progress::Won
        } else {
            Progress::Playing
        }
    }
    fn render(&self, renderer: &mut SoftwareRenderer, assets: &Assets, ctx: &MinigameContext) {
        for (i, (local, dir, collected)) in self.crystals.iter().enumerate() {
            if *collected {
                continue;
            }
            let bob = (ctx.time * 2.0 + i as f32).sin() * 0.08;
            let model = ctx.planet_model
                * Mat4::translation(*local + *dir * bob)
                * align_y(*dir, ctx.time * 1.4 + i as f32)
                * Mat4::uniform_scale(0.45);
            let options = DrawOptions::new(ShaderKind::Glow)
                .tint(Color3::new(0.35, 0.85, 1.0))
                .scalar(i as f32);
            renderer.draw_lod(assets.prop(PropKind::Crystal), model, &options);
            let world = ctx.to_world(*local);
            let up = (world - ctx.planet_center).normalized();
            beacon(
                renderer,
                world,
                up,
                14.0,
                Color3::new(0.3, 0.8, 1.0),
                ctx.time + i as f32,
            );
        }
    }
    fn progress(&self) -> (u32, u32) {
        (
            self.crystals.iter().filter(|c| c.2).count() as u32,
            self.crystals.len() as u32,
        )
    }
    fn status_lines(&self, ctx: &MinigameContext) -> Vec<String> {
        let (done, total) = self.progress();
        let mut lines = vec![format!("Cristales: {done}/{total}")];
        if let Some((_, d)) = nearest(ctx, self.crystals.iter().filter(|c| !c.2).map(|c| c.0)) {
            lines.push(format!("Cristal mas cercano: {d:.0} u"));
        }
        lines
    }
    fn objective(&self, ctx: &MinigameContext) -> Option<Vec3> {
        nearest(ctx, self.crystals.iter().filter(|c| !c.2).map(|c| c.0)).map(|n| n.0)
    }
}

// ---------------------------------------------------------------------------
// Oasis: Reliquias del desierto
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct RelicHunt {
    relics: Vec<(Vec3, Vec3, bool)>,
}

pub const RELIC_COUNT: usize = 6;
const RELIC_PICKUP: f32 = 4.0;

impl Minigame for RelicHunt {
    fn title(&self) -> &'static str {
        "Reliquias del desierto"
    }
    fn instructions(&self) -> &'static [&'static str] {
        &[
            "Encuentra las 6 reliquias doradas enterradas en las dunas de Oasis.",
            "Las columnas de luz dorada marcan su ubicacion; el sonar indica la distancia.",
            "Acercate a ras de suelo para recuperarlas antes de 3:30.",
        ]
    }
    fn time_limit(&self) -> Option<f32> {
        Some(210.0)
    }
    fn reset(&mut self, ctx: &MinigameContext) {
        self.relics = surface_spots(ctx, 0x5A4D, RELIC_COUNT, 0.5)
            .into_iter()
            .map(|(d, ground)| (d * (ground - 0.02), d, false))
            .collect();
    }
    fn update(
        &mut self,
        ctx: &MinigameContext,
        _elapsed: f32,
        _dt: f32,
        events: &mut MinigameEvents,
    ) -> Progress {
        for (local, _, found) in &mut self.relics {
            if !*found {
                let world = ctx.to_world(*local);
                if (world - ctx.ship_position).length() < RELIC_PICKUP {
                    *found = true;
                    events.bursts.push((world, Color3::new(1.0, 0.85, 0.3)));
                    events.message = Some("Reliquia recuperada".to_owned());
                }
            }
        }
        if self.relics.iter().all(|r| r.2) {
            Progress::Won
        } else {
            Progress::Playing
        }
    }
    fn render(&self, renderer: &mut SoftwareRenderer, assets: &Assets, ctx: &MinigameContext) {
        for (i, (local, dir, found)) in self.relics.iter().enumerate() {
            if *found {
                continue;
            }
            let model = ctx.planet_model
                * Mat4::translation(*local)
                * align_y(*dir, ctx.time * 0.8 + i as f32)
                * Mat4::uniform_scale(0.55);
            renderer.draw_lod(
                assets.prop(PropKind::Relic),
                model,
                &DrawOptions::new(ShaderKind::Desert),
            );
            let world = ctx.to_world(*local);
            let up = (world - ctx.planet_center).normalized();
            beacon(
                renderer,
                world,
                up,
                18.0,
                Color3::new(1.0, 0.75, 0.2),
                ctx.time + i as f32 * 0.7,
            );
        }
    }
    fn progress(&self) -> (u32, u32) {
        (
            self.relics.iter().filter(|r| r.2).count() as u32,
            self.relics.len() as u32,
        )
    }
    fn status_lines(&self, ctx: &MinigameContext) -> Vec<String> {
        let (done, total) = self.progress();
        let mut lines = vec![format!("Reliquias: {done}/{total}")];
        if let Some((_, d)) = nearest(ctx, self.relics.iter().filter(|r| !r.2).map(|r| r.0)) {
            let hint = if d < 15.0 {
                "MUY CALIENTE"
            } else if d < 40.0 {
                "caliente"
            } else if d < 90.0 {
                "tibio"
            } else {
                "frio"
            };
            lines.push(format!("Sonar: {d:.0} u ({hint})"));
        }
        lines
    }
    fn objective(&self, ctx: &MinigameContext) -> Option<Vec3> {
        nearest(ctx, self.relics.iter().filter(|r| !r.2).map(|r| r.0)).map(|n| n.0)
    }
}

// ---------------------------------------------------------------------------
// Fuego: Escape volcánico
// ---------------------------------------------------------------------------

pub struct VolcanicEscape {
    checkpoints: Vec<(Vec3, Vec3)>, // local, tangente
    next: usize,
    fragments: Vec<(Vec3, f32, f32, f32)>, // dirección base, fase, periodo, tamaño
    orbiters: Vec<(f32, f32)>,             // ángulo inicial, velocidad
    path_axes: (Vec3, Vec3, f32),
    pub energy: f32,
    hit_cooldown: f32,
}

impl Default for VolcanicEscape {
    fn default() -> Self {
        Self {
            checkpoints: Vec::new(),
            next: 0,
            fragments: Vec::new(),
            orbiters: Vec::new(),
            path_axes: (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 6.5),
            energy: 100.0,
            hit_cooldown: 0.0,
        }
    }
}

pub const CHECKPOINT_COUNT: usize = 8;
const CHECKPOINT_RADIUS: f32 = 4.8;

impl VolcanicEscape {
    fn path_point(&self, angle: f32) -> Vec3 {
        let (u, v, r) = self.path_axes;
        (u * angle.cos() + v * angle.sin()) * r
    }

    fn fragment_local(&self, i: usize, time: f32, ctx: &MinigameContext) -> (Vec3, f32) {
        let (dir, phase, period, size) = self.fragments[i];
        let tau = (time + phase).rem_euclid(period);
        let v0 = 2.4;
        let g = 2.0 * v0 / period;
        let h = (v0 * tau - 0.5 * g * tau * tau).max(0.0);
        let ground = ctx.surface.height_at(dir);
        (dir * (ground * 0.98 + h), size)
    }

    fn orbiter_local(&self, i: usize, time: f32) -> Vec3 {
        let (start, speed) = self.orbiters[i];
        self.path_point(start + time * speed)
    }
}

impl Minigame for VolcanicEscape {
    fn title(&self) -> &'static str {
        "Escape volcanico"
    }
    fn instructions(&self) -> &'static [&'static str] {
        &[
            "Atraviesa los 8 puntos de control en orden alrededor del planeta Fuego.",
            "Esquiva los fragmentos volcanicos y los meteoritos en orbita.",
            "Cada impacto resta 20 de energia. Sin energia o sin tiempo, pierdes.",
        ]
    }
    fn time_limit(&self) -> Option<f32> {
        Some(150.0)
    }
    fn reset(&mut self, ctx: &MinigameContext) {
        let mut rng = Rng::new(0xF00D);
        let axis = Vec3::new(0.3, 1.0, 0.2).normalized();
        let u = axis.any_perpendicular();
        let v = axis.cross(u);
        let radius = ctx.surface.outer_radius + 1.3;
        self.path_axes = (u, v, radius);
        // El primer punto de control queda del lado de la nave.
        let local_ship = ctx
            .planet_model
            .inverse_similarity()
            .transform_point(ctx.ship_position);
        let start = local_ship.dot(v).atan2(local_ship.dot(u));
        self.checkpoints = (0..CHECKPOINT_COUNT)
            .map(|i| {
                let a = start + 0.35 + i as f32 / CHECKPOINT_COUNT as f32 * 2.0 * PI * 0.92;
                let p = self.path_point(a);
                let tangent = (u * -a.sin() + v * a.cos()).normalized();
                (p, tangent)
            })
            .collect();
        self.next = 0;
        self.energy = 100.0;
        self.hit_cooldown = 0.0;
        self.fragments = (0..18)
            .map(|i| {
                let a = start + 0.35 + (i as f32 + 0.5) / 18.0 * 2.0 * PI * 0.92;
                let d = (self.path_point(a).normalized() + rng.unit_vector() * 0.08).normalized();
                (
                    d,
                    rng.range(0.0, 4.0),
                    rng.range(2.6, 4.2),
                    rng.range(0.25, 0.45),
                )
            })
            .collect();
        self.orbiters = (0..6)
            .map(|i| (i as f32 * 1.05, -0.12 - rng.range(0.0, 0.06)))
            .collect();
    }
    fn update(
        &mut self,
        ctx: &MinigameContext,
        elapsed: f32,
        dt: f32,
        events: &mut MinigameEvents,
    ) -> Progress {
        self.hit_cooldown = (self.hit_cooldown - dt).max(0.0);
        if let Some((local, _)) = self.checkpoints.get(self.next) {
            let world = ctx.to_world(*local);
            if (world - ctx.ship_position).length() < CHECKPOINT_RADIUS {
                events.bursts.push((world, Color3::new(0.3, 1.0, 0.6)));
                events.message = Some(format!(
                    "Punto de control {}/{}",
                    self.next + 1,
                    self.checkpoints.len()
                ));
                self.next += 1;
            }
        }
        if self.next >= self.checkpoints.len() {
            return Progress::Won;
        }
        if self.hit_cooldown <= 0.0 {
            let mut hit = None;
            for i in 0..self.fragments.len() {
                let (local, size) = self.fragment_local(i, elapsed, ctx);
                let w = ctx.to_world(local);
                if (w - ctx.ship_position).length() < 1.2 + size * ctx.planet_scale {
                    hit = Some(w);
                }
            }
            for i in 0..self.orbiters.len() {
                let w = ctx.to_world(self.orbiter_local(i, elapsed));
                if (w - ctx.ship_position).length() < 1.2 + 0.45 * ctx.planet_scale {
                    hit = Some(w);
                }
            }
            if let Some(w) = hit {
                self.energy = (self.energy - 20.0).max(0.0);
                self.hit_cooldown = 1.0;
                events.damage += 20.0;
                events.bursts.push((w, Color3::new(1.0, 0.45, 0.1)));
                events.message = Some("Impacto! -20 energia".to_owned());
            }
        }
        if self.energy <= 0.0 {
            Progress::Lost("La nave se quedo sin energia".to_owned())
        } else {
            Progress::Playing
        }
    }
    fn render(&self, renderer: &mut SoftwareRenderer, assets: &Assets, ctx: &MinigameContext) {
        for (i, (local, tangent)) in self.checkpoints.iter().enumerate().skip(self.next) {
            let active = i == self.next;
            let tint = if active {
                Color3::new(0.3, 1.0, 0.6)
            } else {
                Color3::new(0.9, 0.5, 0.2)
            };
            let model = ctx.planet_model
                * Mat4::translation(*local)
                * align_y(*tangent, 0.0)
                * Mat4::uniform_scale(if active { 1.05 } else { 0.85 });
            renderer.draw_mesh(
                &assets.checkpoint,
                model,
                &DrawOptions::new(ShaderKind::Glow)
                    .tint(tint)
                    .scalar(i as f32)
                    .double_sided(),
            );
        }
        for i in 0..self.fragments.len() {
            let (local, size) = self.fragment_local(i, ctx.time, ctx);
            let model = ctx.planet_model
                * Mat4::translation(local)
                * Mat4::rotation_axis(self.fragments[i].0, ctx.time * 2.0 + i as f32)
                * Mat4::uniform_scale(size);
            let options = DrawOptions::new(ShaderKind::Meteor).param(self.fragments[i].0);
            renderer.draw_lod(&assets.meteors[i % 3], model, &options);
        }
        for i in 0..self.orbiters.len() {
            let local = self.orbiter_local(i, ctx.time);
            let model = ctx.planet_model
                * Mat4::translation(local)
                * Mat4::rotation_y(ctx.time * 3.0)
                * Mat4::uniform_scale(0.45);
            renderer.draw_lod(
                &assets.meteors[(i + 1) % 3],
                model,
                &DrawOptions::new(ShaderKind::Meteor).param(local),
            );
        }
    }
    fn progress(&self) -> (u32, u32) {
        (self.next as u32, self.checkpoints.len() as u32)
    }
    fn status_lines(&self, ctx: &MinigameContext) -> Vec<String> {
        let mut lines = vec![
            format!(
                "Puntos de control: {}/{}",
                self.next,
                self.checkpoints.len()
            ),
            format!("Energia: {:.0}%", self.energy),
        ];
        if let Some(target) = self.objective(ctx) {
            lines.push(format!(
                "Siguiente control: {:.0} u",
                (target - ctx.ship_position).length()
            ));
        }
        lines
    }
    fn objective(&self, ctx: &MinigameContext) -> Option<Vec3> {
        self.checkpoints
            .get(self.next)
            .map(|(p, _)| ctx.to_world(*p))
    }
}

// ---------------------------------------------------------------------------
// Dulces: Carrera de caramelos
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct CandyRace {
    rings: Vec<(Vec3, Vec3)>, // local, normal (tangente de la ruta)
    next: usize,
    previous_side: Option<f32>,
    pub misses: u32,
    passed: u32,
}

pub const RING_COUNT: usize = 12;
const RING_RADIUS_LOCAL: f32 = 0.95;
const MISS_PENALTY: f32 = 5.0;
const MAX_MISSES: u32 = 4;

impl Minigame for CandyRace {
    fn title(&self) -> &'static str {
        "Carrera de caramelos"
    }
    fn instructions(&self) -> &'static [&'static str] {
        &[
            "Recorre la ruta atravesando los 12 aros de caramelo en orden.",
            "Pasar por fuera de un aro cuenta como fallo y suma 5 s de penalizacion.",
            "Con mas de 4 fallos o mas de 4 minutos, pierdes. Gana el mejor tiempo!",
        ]
    }
    fn time_limit(&self) -> Option<f32> {
        Some(240.0)
    }
    fn reset(&mut self, ctx: &MinigameContext) {
        let axis = Vec3::new(-0.2, 1.0, 0.35).normalized();
        let u = axis.any_perpendicular();
        let v = axis.cross(u);
        let local_ship = ctx
            .planet_model
            .inverse_similarity()
            .transform_point(ctx.ship_position);
        let start = local_ship.dot(v).atan2(local_ship.dot(u));
        let surface = ctx.surface;
        self.rings = (0..RING_COUNT)
            .map(|i| {
                let a = start + 0.3 + i as f32 / RING_COUNT as f32 * 2.0 * PI * 0.95;
                let dir = (u * a.cos() + v * a.sin() + axis * (a * 2.0).sin() * 0.25).normalized();
                let altitude = surface.height_at(dir) + 0.7 + 0.45 * (a * 3.0).sin().abs();
                (dir * altitude, Vec3::ZERO)
            })
            .collect();
        // Normales: dirección hacia el siguiente aro.
        let n = self.rings.len();
        for i in 0..n {
            let next = self.rings[(i + 1).min(n - 1)].0;
            let prev = self.rings[i.saturating_sub(1)].0;
            let tangent = if i + 1 < n {
                next - self.rings[i].0
            } else {
                self.rings[i].0 - prev
            };
            self.rings[i].1 = tangent.normalized();
        }
        self.next = 0;
        self.previous_side = None;
        self.misses = 0;
        self.passed = 0;
    }
    fn update(
        &mut self,
        ctx: &MinigameContext,
        _elapsed: f32,
        _dt: f32,
        events: &mut MinigameEvents,
    ) -> Progress {
        let Some(&(local, normal_local)) = self.rings.get(self.next) else {
            return Progress::Won;
        };
        let center = ctx.to_world(local);
        let normal = (ctx.to_world(local + normal_local) - center).normalized();
        let offset = ctx.ship_position - center;
        let side = offset.dot(normal);
        let radial = (offset - normal * side).length();
        let ring_radius = RING_RADIUS_LOCAL * ctx.planet_scale;
        if let Some(previous) = self.previous_side
            && previous < 0.0
            && side >= 0.0
            && offset.length() < ring_radius * 6.0
        {
            if radial <= ring_radius {
                self.passed += 1;
                events.bursts.push((center, Color3::new(1.0, 0.4, 0.7)));
                events.message = Some(format!("Aro {}/{}", self.next + 1, self.rings.len()));
            } else {
                self.misses += 1;
                events.message = Some(format!("Fallaste el aro! +{MISS_PENALTY:.0} s"));
            }
            self.next += 1;
            self.previous_side = None;
        } else {
            self.previous_side = Some(side);
        }
        if self.misses > MAX_MISSES {
            return Progress::Lost("Demasiados aros fallados".to_owned());
        }
        if self.next >= self.rings.len() {
            Progress::Won
        } else {
            Progress::Playing
        }
    }
    fn render(&self, renderer: &mut SoftwareRenderer, assets: &Assets, ctx: &MinigameContext) {
        for (i, (local, normal)) in self.rings.iter().enumerate().skip(self.next) {
            let active = i == self.next;
            let model = ctx.planet_model
                * Mat4::translation(*local)
                * align_y(*normal, ctx.time * 0.5)
                * Mat4::uniform_scale(RING_RADIUS_LOCAL);
            let tint = if active {
                Color3::new(1.2, 1.2, 1.2)
            } else {
                Color3::new(0.75, 0.75, 0.75)
            };
            renderer.draw_lod(
                assets.prop(PropKind::CandyRing),
                model,
                &DrawOptions::new(ShaderKind::Candy)
                    .tint(tint)
                    .double_sided(),
            );
            if active {
                let world = ctx.to_world(*local);
                renderer.draw_particle(
                    world,
                    0.6 + 0.3 * (ctx.time * 6.0).sin(),
                    Color3::new(1.0, 0.6, 0.9),
                    1.0,
                    true,
                );
            }
        }
        // Ruta reconocible: puntos entre aros.
        for i in self.next..self.rings.len().saturating_sub(1) {
            let a = ctx.to_world(self.rings[i].0);
            let b = ctx.to_world(self.rings[i + 1].0);
            renderer.draw_dotted_line(a, b, 10, 0.18, Color3::new(1.0, 0.5, 0.8));
        }
    }
    fn progress(&self) -> (u32, u32) {
        (self.passed, self.rings.len() as u32)
    }
    fn status_lines(&self, ctx: &MinigameContext) -> Vec<String> {
        let mut lines = vec![
            format!(
                "Aros: {}/{}  (siguiente {}/{})",
                self.passed,
                self.rings.len(),
                (self.next + 1).min(self.rings.len()),
                self.rings.len()
            ),
            format!(
                "Fallos: {}/{}  Penalizacion: +{:.0} s",
                self.misses,
                MAX_MISSES,
                self.penalty()
            ),
        ];
        if let Some(t) = self.objective(ctx) {
            lines.push(format!(
                "Siguiente aro: {:.0} u",
                (t - ctx.ship_position).length()
            ));
        }
        lines
    }
    fn objective(&self, ctx: &MinigameContext) -> Option<Vec3> {
        self.rings.get(self.next).map(|(p, _)| ctx.to_world(*p))
    }
    fn penalty(&self) -> f32 {
        self.misses as f32 * MISS_PENALTY
    }
}

// ---------------------------------------------------------------------------
// Escandalosos: Encuentra a los tres osos
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct BearSearch {
    bears: Vec<(Vec3, Vec3, PropKind, Option<f32>)>, // local, dir, modelo, instante encontrado
}

const BEAR_FOUND_DISTANCE: f32 = 7.0;
const BEAR_NAMES: [(&str, &str); 3] = [
    ("Oso pardo", "le encantan los arboles altos"),
    ("Oso panda", "busca un lugar tranquilo para comer"),
    ("Oso polar", "prefiere estar lejos del ecuador"),
];

impl Minigame for BearSearch {
    fn title(&self) -> &'static str {
        "Encuentra a los tres osos"
    }
    fn instructions(&self) -> &'static [&'static str] {
        &[
            "Tres osos amistosos se escondieron en el planeta Escandalosos.",
            "Vuela bajo y acercate a cada uno para saludarlo. Usa las pistas del HUD.",
            "El cronometro mide tu mejor tiempo (sin limite).",
        ]
    }
    fn time_limit(&self) -> Option<f32> {
        None
    }
    fn reset(&mut self, ctx: &MinigameContext) {
        let kinds = [
            PropKind::BearBrown,
            PropKind::BearPanda,
            PropKind::BearPolar,
        ];
        let spots = surface_spots(ctx, 0xBEA4, 3, 0.9);
        self.bears = spots
            .into_iter()
            .zip(kinds)
            .map(|((d, ground), kind)| (d * (ground - 0.01), d, kind, None))
            .collect();
    }
    fn update(
        &mut self,
        ctx: &MinigameContext,
        elapsed: f32,
        _dt: f32,
        events: &mut MinigameEvents,
    ) -> Progress {
        for (i, (local, _, _, found)) in self.bears.iter_mut().enumerate() {
            if found.is_none() {
                let world = ctx.to_world(*local);
                if (world - ctx.ship_position).length() < BEAR_FOUND_DISTANCE {
                    *found = Some(elapsed);
                    events.bursts.push((world, Color3::new(1.0, 0.9, 0.4)));
                    events.message = Some(format!("Encontraste al {}!", BEAR_NAMES[i].0));
                }
            }
        }
        if self.bears.iter().all(|b| b.3.is_some()) {
            Progress::Won
        } else {
            Progress::Playing
        }
    }
    fn render(&self, renderer: &mut SoftwareRenderer, assets: &Assets, ctx: &MinigameContext) {
        for (i, (local, dir, kind, found)) in self.bears.iter().enumerate() {
            // Animación: balanceo en reposo; saltos de alegría al ser encontrado.
            let t = ctx.time + i as f32 * 1.3;
            let (hop, squash, spin) = match found {
                Some(_) => {
                    let h = (t * 5.0).sin().abs() * 0.35;
                    (h, 1.0 + h * 0.3, t * 2.5)
                }
                None => (0.0, 1.0 + (t * 2.0).sin() * 0.04, (t * 0.7).sin() * 0.6),
            };
            let sway = Mat4::rotation_z((t * 1.6).sin() * 0.08);
            let model = ctx.planet_model
                * Mat4::translation(*local + *dir * hop)
                * align_y(*dir, spin)
                * sway
                * Mat4::scale(Vec3::new(0.6, 0.6 * squash, 0.6));
            renderer.draw_lod(
                assets.prop(*kind),
                model,
                &DrawOptions::new(ShaderKind::Forest),
            );
            let world = ctx.to_world(*local);
            let distance = (world - ctx.ship_position).length();
            if found.is_none() && distance < 70.0 {
                // Marcador de proximidad: chispas que aparecen al acercarse.
                let up = (world - ctx.planet_center).normalized();
                for k in 0..6 {
                    let a = t * 2.0 + k as f32;
                    let p = world
                        + up * (5.0 + (a * 1.3).sin())
                        + up.any_perpendicular().rotate_around(up, a) * 1.5;
                    renderer.draw_particle(p, 0.3, Color3::new(1.0, 0.9, 0.3), 1.0, true);
                }
            }
        }
    }
    fn progress(&self) -> (u32, u32) {
        (
            self.bears.iter().filter(|b| b.3.is_some()).count() as u32,
            self.bears.len() as u32,
        )
    }
    fn status_lines(&self, ctx: &MinigameContext) -> Vec<String> {
        let (done, total) = self.progress();
        let mut lines = vec![format!("Osos encontrados: {done}/{total}")];
        for (i, (local, _, _, found)) in self.bears.iter().enumerate() {
            let (name, hint) = BEAR_NAMES[i];
            match found {
                Some(t) => lines.push(format!("  {name}: encontrado en {t:.0} s")),
                None => {
                    let d = (ctx.to_world(*local) - ctx.ship_position).length();
                    let near = if d < 70.0 { "cerca!" } else { hint };
                    lines.push(format!("  {name}: {d:.0} u - {near}"));
                }
            }
        }
        lines
    }
    fn objective(&self, ctx: &MinigameContext) -> Option<Vec3> {
        nearest(
            ctx,
            self.bears.iter().filter(|b| b.3.is_none()).map(|b| b.0),
        )
        .map(|n| n.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::make_uv_sphere;

    fn sphere_surface() -> PlanetSurface {
        let mut sphere = make_uv_sphere(64, 32);
        for t in &mut sphere.triangles {
            for v in &mut t.vertices {
                v.position *= 5.0;
            }
        }
        PlanetSurface::new(&sphere)
    }

    fn ctx<'a>(
        surface: &'a PlanetSurface,
        planet: PlanetId,
        ship: Vec3,
        time: f32,
    ) -> MinigameContext<'a> {
        MinigameContext {
            planet,
            planet_model: Mat4::uniform_scale(4.0),
            planet_center: Vec3::ZERO,
            planet_scale: 4.0,
            ship_position: ship,
            ship_velocity: Vec3::ZERO,
            time,
            surface,
        }
    }

    /// Visita cada objetivo con la nave hasta ganar.
    fn play_by_visiting(planet: PlanetId) -> MinigameRunner {
        let surface = sphere_surface();
        let mut runner = MinigameRunner::new(planet).unwrap();
        runner.start(&ctx(&surface, planet, Vec3::new(0.0, 30.0, 0.0), 0.0));
        for step in 0..400 {
            let c0 = ctx(
                &surface,
                planet,
                Vec3::new(0.0, 30.0, 0.0),
                step as f32 * 0.05,
            );
            let Some(target) = runner.game.objective(&c0) else {
                break;
            };
            let c = ctx(&surface, planet, target, step as f32 * 0.05);
            let mut events = MinigameEvents::default();
            runner.update(&c, 0.05, &mut events);
            if runner.state != RunState::Playing {
                break;
            }
        }
        runner
    }

    #[test]
    fn every_themed_planet_has_a_minigame() {
        for planet in PlanetId::ALL {
            assert_eq!(
                MinigameRunner::new(planet).is_some(),
                planet != PlanetId::Tierra
            );
        }
    }

    #[test]
    fn collection_games_are_won_by_visiting_targets() {
        for planet in [
            PlanetId::Hielo,
            PlanetId::Oasis,
            PlanetId::Escandalosos,
            PlanetId::Fuego,
        ] {
            let runner = play_by_visiting(planet);
            assert_eq!(runner.state, RunState::Won, "{planet:?}");
            let (done, total) = runner.game.progress();
            assert_eq!(done, total);
            assert!(total >= 3);
            assert!(runner.score() > 0);
        }
    }

    #[test]
    fn timers_cause_defeat() {
        let surface = sphere_surface();
        let mut runner = MinigameRunner::new(PlanetId::Hielo).unwrap();
        let far = Vec3::new(0.0, 500.0, 0.0);
        runner.start(&ctx(&surface, PlanetId::Hielo, far, 0.0));
        for _ in 0..200 {
            runner.update(
                &ctx(&surface, PlanetId::Hielo, far, 0.0),
                1.0,
                &mut MinigameEvents::default(),
            );
        }
        assert!(matches!(runner.state, RunState::Lost(_)));
        // Reiniciar vuelve a jugar con los objetivos completos.
        runner.start(&ctx(&surface, PlanetId::Hielo, far, 0.0));
        assert_eq!(runner.state, RunState::Playing);
        assert_eq!(runner.game.progress(), (0, CRYSTAL_COUNT as u32));
    }

    #[test]
    fn candy_race_counts_passes_and_misses_by_crossing_ring_planes() {
        let surface = sphere_surface();
        let start = Vec3::new(0.0, 40.0, 0.0);
        // Cruce directo por el centro siguiendo la tangente local.
        let surface2 = surface;
        let mut race = CandyRace::default();
        let c = ctx(&surface2, PlanetId::Dulces, start, 0.0);
        race.reset(&c);
        let (local, normal) = race.rings[0];
        let center = local * 4.0;
        let mut events = MinigameEvents::default();
        for k in -5..=5 {
            let p = center + normal * (k as f32 * 0.5);
            race.update(
                &ctx(&surface2, PlanetId::Dulces, p, 0.0),
                0.0,
                0.016,
                &mut events,
            );
        }
        assert_eq!(race.passed, 1);
        assert_eq!(race.misses, 0);
        // El segundo aro se cruza por fuera: cuenta como fallo.
        let (local, normal) = race.rings[1];
        let side = normal.any_perpendicular();
        let center = local * 4.0 + side * 8.0;
        for k in -5..=5 {
            let p = center + normal * (k as f32 * 0.5);
            race.update(
                &ctx(&surface2, PlanetId::Dulces, p, 0.0),
                0.0,
                0.016,
                &mut events,
            );
        }
        assert_eq!(race.passed, 1);
        assert_eq!(race.misses, 1);
        assert!((race.penalty() - MISS_PENALTY).abs() < 1.0e-5);
    }

    #[test]
    fn volcanic_obstacles_drain_energy() {
        let surface = sphere_surface();
        let mut game = VolcanicEscape::default();
        let c = ctx(&surface, PlanetId::Fuego, Vec3::new(0.0, 60.0, 0.0), 0.0);
        game.reset(&c);
        let mut lost = false;
        for step in 0..2000 {
            let t = step as f32 * 0.05;
            let probe = ctx(&surface, PlanetId::Fuego, Vec3::ZERO, t);
            let target = probe.to_world(game.orbiter_local(0, t));
            let c = ctx(&surface, PlanetId::Fuego, target, t);
            let mut events = MinigameEvents::default();
            if let Progress::Lost(_) = game.update(&c, t, 0.05, &mut events) {
                lost = true;
                break;
            }
        }
        assert!(lost, "chocar repetidamente debe agotar la energia");
        assert_eq!(game.energy, 0.0);
    }
}
