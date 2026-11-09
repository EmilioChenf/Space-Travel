//! Estado del juego, simulación y composición de la escena.
//!
//! `Game` no depende de raylib: recibe un [`GameInput`] abstracto, avanza la
//! simulación con delta time, dibuja la escena 3D con el renderizador por
//! software y produce un [`UiFrame`] (HUD/menús) que `ui.rs` pinta con
//! raylib. Esto permite probar estados, transiciones y minijuegos sin ventana.

use crate::assets::Assets;
use crate::camera::{CameraInput, CameraMode, CameraRig};
use crate::launch::{LaunchPhase, LaunchSequence};
use crate::math3d::{Mat4, PI, Vec3, smoothstep};
use crate::meteors::{AsteroidBelt, MeteorShower};
use crate::minigames::{MinigameContext, MinigameEvents, MinigameRunner, RunState, minigame_title};
use crate::model::Color3;
use crate::particles::{Particle, ParticleSystem};
use crate::renderer::{Camera, DrawOptions, PointLight, RenderStats, ShaderKind, SoftwareRenderer};
use crate::ship::{FlightContext, FlightInput, SHIP_RADIUS, Ship, WORLD_UP};
use crate::solar::{
    PLANETS, PlanetId, PropInstance, SUN_RADIUS, config, planet_axis, planet_model,
    planet_position, planet_rotation, scatter_props,
};

/// Semilla global de la generación procedural (decoraciones y meteoritos).
/// Se puede cambiar con la variable de entorno `SPACE_TRAVEL_SEED`.
pub const DEFAULT_SEED: u32 = 20_261_109;

const THRUSTER_OFFSETS: [Vec3; 4] = [
    Vec3::new(1.43, 0.69, 0.99),
    Vec3::new(1.43, 0.69, -0.99),
    Vec3::new(-1.43, 0.69, 0.99),
    Vec3::new(-1.43, 0.69, -0.99),
];

/// Entrada abstracta de un cuadro (la llena `main.rs` desde raylib o las
/// pruebas/capturas con valores sintéticos).
#[derive(Clone, Copy, Debug, Default)]
pub struct GameInput {
    pub flight: FlightInput,
    pub camera: CameraInput,
    pub up: bool,
    pub down: bool,
    pub confirm: bool,
    pub back: bool,
    pub pause: bool,
    pub cycle_camera: bool,
    pub restart: bool,
    pub minigame: bool,
    pub next_target: bool,
    pub toggle_orbits: bool,
    pub toggle_help: bool,
    pub toggle_stats: bool,
    pub skip: bool,
    pub reset_ship: bool,
    pub warp: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    Controls,
    Credits,
    Launch,
    Flight,
    Paused,
    Destroyed,
}

pub const MAIN_MENU: [&str; 4] = ["Iniciar partida", "Controles", "Creditos", "Salir"];
pub const PAUSE_MENU: [&str; 4] = ["Continuar", "Reiniciar", "Volver al menu", "Salir"];

/// Viaje animado hacia un planeta (warp).
#[derive(Clone, Copy, Debug)]
pub struct Warp {
    pub target: PlanetId,
    pub start: Vec3,
    pub start_forward: Vec3,
    pub time: f32,
    pub duration: f32,
}

// ---------------------------------------------------------------------------
// Modelo de interfaz (independiente de raylib)
// ---------------------------------------------------------------------------

pub type Rgba = [u8; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Elementos 2D en un lienzo virtual de 1280x720.
#[derive(Clone, Debug)]
pub enum UiElement {
    Text {
        x: f32,
        y: f32,
        size: f32,
        color: Rgba,
        text: String,
        align: Align,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Rgba,
    },
    Frame {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Rgba,
    },
    Bar {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        value: f32,
        color: Rgba,
    },
    Circle {
        x: f32,
        y: f32,
        r: f32,
        color: Rgba,
    },
    Ring {
        x: f32,
        y: f32,
        r: f32,
        color: Rgba,
    },
    Arrow {
        x: f32,
        y: f32,
        angle: f32,
        size: f32,
        color: Rgba,
    },
    Line {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        color: Rgba,
    },
}

#[derive(Clone, Debug, Default)]
pub struct UiFrame {
    pub elements: Vec<UiElement>,
}

pub const UI_W: f32 = 1280.0;
pub const UI_H: f32 = 720.0;

const WHITE: Rgba = [240, 244, 255, 255];
const CYAN: Rgba = [90, 220, 255, 255];
const GOLD: Rgba = [255, 205, 80, 255];
const DIM: Rgba = [160, 172, 196, 255];
const PANEL: Rgba = [6, 10, 24, 190];
const RED: Rgba = [255, 80, 70, 255];
const GREEN: Rgba = [110, 255, 160, 255];

impl UiFrame {
    pub fn text(&mut self, x: f32, y: f32, size: f32, color: Rgba, text: impl Into<String>) {
        self.elements.push(UiElement::Text {
            x,
            y,
            size,
            color,
            text: text.into(),
            align: Align::Left,
        });
    }
    pub fn text_aligned(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        color: Rgba,
        text: impl Into<String>,
        align: Align,
    ) {
        self.elements.push(UiElement::Text {
            x,
            y,
            size,
            color,
            text: text.into(),
            align,
        });
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.elements.push(UiElement::Rect { x, y, w, h, color });
    }
    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.rect(x, y, w, h, PANEL);
        self.elements.push(UiElement::Frame {
            x,
            y,
            w,
            h,
            color: [70, 140, 200, 160],
        });
    }
    pub fn bar(&mut self, x: f32, y: f32, w: f32, h: f32, value: f32, color: Rgba) {
        self.elements.push(UiElement::Bar {
            x,
            y,
            w,
            h,
            value: value.clamp(0.0, 1.0),
            color,
        });
    }
    /// Todos los textos concatenados (útil en pruebas).
    pub fn all_text(&self) -> String {
        self.elements
            .iter()
            .filter_map(|e| match e {
                UiElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---------------------------------------------------------------------------
// Juego
// ---------------------------------------------------------------------------

pub struct Game {
    pub assets: Assets,
    pub screen: Screen,
    pub menu_index: usize,
    pub pause_index: usize,
    pub sim_time: f32,
    pub real_time: f32,
    pub ship: Ship,
    pub rig: CameraRig,
    pub launch: LaunchSequence,
    pub props: Vec<Vec<PropInstance>>,
    pub meteors: MeteorShower,
    pub belt: AsteroidBelt,
    pub particles: ParticleSystem,
    pub target: PlanetId,
    pub warp: Option<Warp>,
    pub minigame: Option<MinigameRunner>,
    pub message: Option<(String, f32)>,
    pub show_orbits: bool,
    pub show_help: bool,
    pub show_stats: bool,
    pub quit: bool,
    pub seed: u32,
    damage_flash: f32,
    shake: f32,
    flash: f32,
    camera: Camera,
    pub last_stats: RenderStats,
    pub frame_ms: f32,
    pub fps: f32,
    fps_accum: (f32, u32),
    pub minigames_won: Vec<PlanetId>,
}

impl Game {
    pub fn new(assets: Assets, seed: u32) -> Self {
        let props = PlanetId::ALL
            .iter()
            .map(|id| scatter_props(*id, &assets, seed))
            .collect();
        let ship = Ship::new(Vec3::new(0.0, 60.0, 260.0), Vec3::new(0.0, 0.0, -1.0));
        let rig = CameraRig::new(&ship);
        let camera = Camera {
            position: Vec3::new(0.0, 200.0, 500.0),
            target: Vec3::ZERO,
            up: WORLD_UP,
            fov_y: 1.0,
            near: 0.1,
            far: 3000.0,
        };
        Self {
            assets,
            screen: Screen::MainMenu,
            menu_index: 0,
            pause_index: 0,
            sim_time: 0.0,
            real_time: 0.0,
            ship,
            rig,
            launch: LaunchSequence::new(),
            props,
            meteors: MeteorShower::new(seed),
            belt: AsteroidBelt::new(seed ^ 0xA57E, 110),
            particles: ParticleSystem::new(3500, seed ^ 0x9A27),
            target: PlanetId::Hielo,
            warp: None,
            minigame: None,
            message: None,
            show_orbits: true,
            show_help: true,
            show_stats: false,
            quit: false,
            seed,
            damage_flash: 0.0,
            shake: 0.0,
            flash: 0.0,
            camera,
            last_stats: RenderStats::default(),
            frame_ms: 0.0,
            fps: 0.0,
            fps_accum: (0.0, 0),
            minigames_won: Vec::new(),
        }
    }

    /// Comienza una partida nueva con la cinemática de despegue.
    pub fn start_new_game(&mut self) {
        self.screen = Screen::Launch;
        self.launch = LaunchSequence::new();
        self.sim_time = 0.0;
        self.minigame = None;
        self.warp = None;
        self.meteors.reset();
        self.particles.clear();
        self.message = None;
        self.target = PlanetId::Hielo;
    }

    /// Entrega el control tras la cinemática.
    pub fn enter_flight(&mut self) {
        let (position, forward, up) = self.launch.final_ship_pose(self.sim_time);
        self.ship = Ship::new(position, forward);
        self.ship.face_towards(forward, up);
        self.ship.velocity = forward * 6.0;
        self.rig = CameraRig::new(&self.ship);
        self.rig.update(&self.ship, CameraInput::default(), 0.016);
        self.screen = Screen::Flight;
        self.show_message("Control transferido: buen viaje!");
    }

    /// Reaparece cerca de la Tierra (tras destrucción o Retroceso).
    pub fn respawn(&mut self) {
        let earth = planet_position(PlanetId::Tierra, self.sim_time);
        let away = (earth - Vec3::ZERO).normalized();
        let position = earth + away * 60.0 + WORLD_UP * 12.0;
        self.ship = Ship::new(position, -away);
        self.rig.snap(&self.ship);
        self.warp = None;
        self.minigame = None;
        self.screen = Screen::Flight;
        self.show_message("Nave restaurada en orbita terrestre");
    }

    pub fn show_message(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), 3.0));
    }

    pub fn planet_radius_world(&self, id: PlanetId) -> f32 {
        self.assets.planets[id.index()].surface.outer_radius * config(id).scale
    }

    /// Planeta más cercano: (id, distancia al centro, altitud sobre la superficie).
    pub fn nearest_planet(&self) -> (PlanetId, f32, f32) {
        PlanetId::ALL
            .iter()
            .map(|&id| {
                let center = planet_position(id, self.sim_time);
                let distance = (self.ship.position - center).length();
                let altitude = distance - self.planet_radius_world(id);
                (id, distance, altitude)
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .expect("hay planetas")
    }

    fn minigame_context<'a>(
        assets: &'a Assets,
        planet: PlanetId,
        sim_time: f32,
        ship: &Ship,
        time: f32,
    ) -> MinigameContext<'a> {
        MinigameContext {
            planet,
            planet_model: planet_model(planet, sim_time),
            planet_center: planet_position(planet, sim_time),
            planet_scale: config(planet).scale,
            ship_position: ship.position,
            ship_velocity: ship.velocity,
            time,
            surface: &assets.planets[planet.index()].surface,
        }
    }

    // -----------------------------------------------------------------------
    // Actualización
    // -----------------------------------------------------------------------

    pub fn update(&mut self, input: &GameInput, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        self.real_time += dt;
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }
        if input.toggle_stats {
            self.show_stats = !self.show_stats;
        }
        self.damage_flash = (self.damage_flash - dt * 1.8).max(0.0);
        self.shake = (self.shake - dt * 2.5).max(0.0);
        self.flash = (self.flash - dt * 1.5).max(0.0);
        match self.screen {
            Screen::MainMenu => self.update_menu(input, dt),
            Screen::Controls | Screen::Credits => {
                self.sim_time += dt;
                if input.back || input.confirm {
                    self.screen = Screen::MainMenu;
                }
            }
            Screen::Launch => self.update_launch(input, dt),
            Screen::Flight => self.update_flight(input, dt),
            Screen::Paused => self.update_pause(input),
            Screen::Destroyed => {
                self.sim_time += dt;
                self.particles.update(dt);
                if input.confirm || input.restart {
                    self.respawn();
                }
            }
        }
    }

    fn update_menu(&mut self, input: &GameInput, dt: f32) {
        self.sim_time += dt;
        if input.up {
            self.menu_index = (self.menu_index + MAIN_MENU.len() - 1) % MAIN_MENU.len();
        }
        if input.down {
            self.menu_index = (self.menu_index + 1) % MAIN_MENU.len();
        }
        if input.confirm {
            match self.menu_index {
                0 => self.start_new_game(),
                1 => self.screen = Screen::Controls,
                2 => self.screen = Screen::Credits,
                _ => self.quit = true,
            }
        }
        if input.back {
            self.quit = true;
        }
    }

    fn update_pause(&mut self, input: &GameInput) {
        if input.up {
            self.pause_index = (self.pause_index + PAUSE_MENU.len() - 1) % PAUSE_MENU.len();
        }
        if input.down {
            self.pause_index = (self.pause_index + 1) % PAUSE_MENU.len();
        }
        if input.back || input.pause {
            self.screen = Screen::Flight;
            return;
        }
        if input.confirm {
            match self.pause_index {
                0 => self.screen = Screen::Flight,
                1 => self.start_new_game(),
                2 => {
                    self.screen = Screen::MainMenu;
                    self.minigame = None;
                }
                _ => self.quit = true,
            }
        }
    }

    fn update_launch(&mut self, input: &GameInput, dt: f32) {
        self.sim_time += dt;
        if input.skip || input.confirm {
            self.launch.skip();
        }
        if input.back {
            self.screen = Screen::MainMenu;
            return;
        }
        let before = self.launch.phase();
        self.launch.update(dt);
        let frame = self.launch.frame(self.sim_time);
        // Fuego y humo del escape.
        if frame.thrust > 0.01 {
            let scale = if frame.on_ground { 0.25 } else { 1.8 };
            let count = if frame.on_ground { 10 } else { 6 };
            for _ in 0..count {
                let spread = self.particles.rng.unit_vector() * 0.35;
                let speed = self.particles.rng.range(4.0, 9.0) * scale;
                let life = self.particles.rng.range(0.4, 1.1);
                self.particles.spawn(Particle {
                    position: frame.nozzle,
                    velocity: (frame.exhaust_dir + spread).normalized() * speed,
                    life,
                    max_life: life,
                    size: 0.35 * scale,
                    size_end: 1.1 * scale,
                    color: Color3::new(1.0, 0.75, 0.3),
                    color_end: Color3::new(0.6, 0.1, 0.02),
                    additive: true,
                    drag: 1.2,
                });
            }
            if frame.on_ground {
                for _ in 0..4 {
                    let dir = self.particles.rng.unit_vector();
                    let side = dir.reject(-frame.exhaust_dir).normalized();
                    let life = self.particles.rng.range(1.5, 3.0);
                    let spread = self.particles.rng.range(0.4, 1.6);
                    self.particles.spawn(Particle {
                        position: frame.nozzle - frame.exhaust_dir * 0.02,
                        velocity: (side * spread - frame.exhaust_dir * 0.2) * 0.25,
                        life,
                        max_life: life,
                        size: 0.05,
                        size_end: 0.3,
                        color: Color3::new(0.75, 0.73, 0.7),
                        color_end: Color3::new(0.35, 0.34, 0.33),
                        additive: false,
                        drag: 0.6,
                    });
                }
            }
        }
        if before != LaunchPhase::Separation && self.launch.phase() == LaunchPhase::Separation {
            let center = frame.rocket.transform_point(Vec3::new(
                0.0,
                crate::worlds::ROCKET_FAIRING_BASE,
                0.0,
            ));
            self.particles.burst(
                center,
                60,
                10.0,
                0.4,
                Color3::new(1.0, 0.95, 0.7),
                Color3::new(0.6, 0.3, 0.1),
                1.0,
            );
        }
        self.particles.update(dt);
        self.camera = frame.camera;
        if self.launch.finished() {
            self.enter_flight();
        }
    }

    fn update_flight(&mut self, input: &GameInput, dt: f32) {
        let minigame_menu = self
            .minigame
            .as_ref()
            .is_some_and(|m| m.state != RunState::Playing);
        if input.pause || (input.back && !minigame_menu) {
            self.screen = Screen::Paused;
            self.pause_index = 0;
            return;
        }
        if input.cycle_camera {
            self.rig.cycle();
        }
        if input.toggle_orbits {
            self.show_orbits = !self.show_orbits;
        }
        if input.toggle_help {
            self.show_help = !self.show_help;
        }
        if input.next_target {
            let i = (self.target.index() + 1) % PlanetId::ALL.len();
            self.target = PlanetId::ALL[i];
        }
        if input.reset_ship {
            self.respawn();
            return;
        }
        let playing = self
            .minigame
            .as_ref()
            .is_some_and(|m| m.state == RunState::Playing);
        if let Some(index) = input.warp
            && index < PlanetId::ALL.len()
            && self.warp.is_none()
            && !playing
        {
            let target = PlanetId::ALL[index];
            self.target = target;
            self.warp = Some(Warp {
                target,
                start: self.ship.position,
                start_forward: self.ship.forward,
                time: 0.0,
                duration: 3.2,
            });
            self.minigame = None;
            self.show_message(format!("Warp hacia {}", config(target).name));
        }

        let previous_time = self.sim_time;
        self.sim_time += dt;
        self.carry_with_nearest_planet(previous_time);

        if let Some(warp) = &mut self.warp {
            warp.time += dt;
            let t = smoothstep(0.0, 1.0, (warp.time / warp.duration).min(1.0));
            let center = planet_position(warp.target, self.sim_time);
            let radius = self.assets.planets[warp.target.index()]
                .surface
                .outer_radius
                * config(warp.target).scale;
            let toward_sun = (-center).normalized();
            let arrival = center + toward_sun * (radius * 2.6) + WORLD_UP * (radius * 0.6);
            self.ship.position = warp.start.lerp(arrival, t);
            let look = (center - self.ship.position).normalized();
            let forward = warp
                .start_forward
                .lerp(look, smoothstep(0.0, 0.4, t))
                .normalized();
            self.ship.face_towards(forward, WORLD_UP);
            self.ship.velocity = Vec3::ZERO;
            self.ship.thrust_visual = 1.6;
            // Estelas de warp alrededor de la nave.
            for _ in 0..12 {
                let offset = self.particles.rng.unit_vector() * self.particles.rng.range(4.0, 14.0);
                self.particles.spawn(Particle {
                    position: self.ship.position + forward * 30.0 + offset,
                    velocity: -forward * 260.0,
                    life: 0.25,
                    max_life: 0.25,
                    size: 0.12,
                    size_end: 0.05,
                    color: Color3::new(0.6, 0.85, 1.0),
                    color_end: Color3::new(0.3, 0.4, 1.0),
                    additive: true,
                    drag: 0.0,
                });
            }
            if warp.time >= warp.duration {
                self.warp = None;
                self.flash = 0.8;
                self.rig.snap(&self.ship);
            }
        } else {
            let (id, distance, altitude) = self.nearest_planet();
            let radius = self.planet_radius_world(id);
            let near = altitude < radius * 1.6;
            let ctx = FlightContext {
                speed_factor: (altitude / 70.0).clamp(0.22, 1.0),
                local_up: if near {
                    (self.ship.position - planet_position(id, self.sim_time)).normalized()
                } else {
                    WORLD_UP
                },
                level_strength: if near { 1.0 } else { 0.25 },
            };
            let _ = distance;
            self.ship.simulate(input.flight, ctx, dt);
        }

        self.resolve_collisions();
        self.update_meteors(dt);
        self.update_minigame(input, dt);
        self.spawn_environment_particles(dt);
        self.spawn_thruster_particles();
        self.particles.update(dt);

        let mut camera = self.rig.update(&self.ship, input.camera, dt);
        if let Some(warp) = &self.warp {
            let k = (warp.time / warp.duration * PI).sin();
            camera.fov_y *= 1.0 + 0.45 * k;
        }
        if self.shake > 0.0 {
            let s = self.shake * 0.25;
            let t = self.real_time * 60.0;
            camera.position += Vec3::new(t.sin() * s, (t * 1.3).cos() * s, (t * 0.7).sin() * s);
        }
        self.camera = camera;

        if self.ship.hull <= 0.0 {
            self.particles.burst(
                self.ship.position,
                160,
                18.0,
                0.8,
                Color3::new(1.0, 0.8, 0.4),
                Color3::new(0.5, 0.1, 0.02),
                1.6,
            );
            self.screen = Screen::Destroyed;
            self.minigame = None;
        }
    }

    /// Mientras la nave está cerca de un planeta, se mueve con su órbita y
    /// su rotación: la exploración cercana es estable aunque el planeta
    /// siga orbitando el Sol.
    fn carry_with_nearest_planet(&mut self, previous_time: f32) {
        if self.warp.is_some() {
            return;
        }
        let (id, distance, _) = self.nearest_planet_at(previous_time);
        let radius = self.planet_radius_world(id);
        if distance > radius * 4.0 {
            return;
        }
        let before = planet_position(id, previous_time);
        let after = planet_position(id, self.sim_time);
        let delta = after - before;
        self.ship.position += delta;
        self.particles.shift(delta);
        if distance < radius * 2.2 {
            let angle = config(id).rotation_speed * (self.sim_time - previous_time);
            let axis = planet_axis(id);
            let offset = (self.ship.position - after).rotate_around(axis, angle);
            self.ship.position = after + offset;
            self.ship.forward = self.ship.forward.rotate_around(axis, angle);
            self.ship.up = self.ship.up.rotate_around(axis, angle);
            self.ship.velocity = self.ship.velocity.rotate_around(axis, angle);
        }
    }

    fn nearest_planet_at(&self, time: f32) -> (PlanetId, f32, f32) {
        PlanetId::ALL
            .iter()
            .map(|&id| {
                let center = planet_position(id, time);
                let distance = (self.ship.position - center).length();
                (id, distance, distance - self.planet_radius_world(id))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .expect("hay planetas")
    }

    /// Colisiones con la superficie de cada planeta, el Sol y los asteroides.
    fn resolve_collisions(&mut self) {
        for id in PlanetId::ALL {
            let center = planet_position(id, self.sim_time);
            let outer = self.planet_radius_world(id);
            if (self.ship.position - center).length() > outer + SHIP_RADIUS * 2.0 {
                continue;
            }
            let rotation = planet_rotation(id, self.sim_time);
            let local = rotation
                .inverse_similarity()
                .transform_direction(self.ship.position - center);
            let surface = &self.assets.planets[id.index()].surface;
            let height = surface.height_at(local) * config(id).scale + SHIP_RADIUS * 0.8;
            if let Some(impact) = self.ship.keep_outside(center, height)
                && impact > 9.0
                && self.ship.damage((impact - 9.0) * 1.6)
            {
                self.damage_flash = 0.6;
                self.shake = 0.6;
                self.particles.burst(
                    self.ship.position,
                    30,
                    6.0,
                    0.3,
                    Color3::new(1.0, 0.8, 0.4),
                    Color3::new(0.4, 0.2, 0.1),
                    0.6,
                );
                self.show_message("Impacto contra la superficie!");
            }
        }
        if self
            .ship
            .keep_outside(Vec3::ZERO, SUN_RADIUS * 1.12 + SHIP_RADIUS)
            .is_some()
            && self.ship.damage(18.0)
        {
            self.damage_flash = 0.8;
            self.show_message("Demasiado calor! Alejate del Sol");
        }
        let hits: Vec<(Vec3, f32)> = self
            .belt
            .colliders(self.sim_time)
            .filter(|(p, r)| (*p - self.ship.position).length() < r + SHIP_RADIUS)
            .collect();
        for (p, r) in hits {
            self.ship.keep_outside(p, r + SHIP_RADIUS);
            if self.ship.damage(12.0) {
                self.damage_flash = 0.6;
                self.shake = 0.8;
                self.particles.burst(
                    self.ship.position,
                    25,
                    7.0,
                    0.3,
                    Color3::new(0.9, 0.85, 0.8),
                    Color3::new(0.3, 0.3, 0.3),
                    0.6,
                );
                self.show_message("Choque con un asteroide!");
            }
        }
    }

    fn update_meteors(&mut self, dt: f32) {
        let mut obstacles: Vec<(Vec3, f32)> = PlanetId::ALL
            .iter()
            .map(|&id| {
                (
                    planet_position(id, self.sim_time),
                    self.planet_radius_world(id),
                )
            })
            .collect();
        obstacles.push((Vec3::ZERO, SUN_RADIUS));
        let playing = self
            .minigame
            .as_ref()
            .is_some_and(|m| m.state == RunState::Playing);
        let warping = self.warp.is_some();
        let events = self.meteors.update(
            dt,
            self.ship.position,
            self.ship.forward,
            SHIP_RADIUS,
            &obstacles,
            &mut self.particles,
            playing || warping,
        );
        if events.ship_hits > 0 && !warping && self.ship.damage(events.damage) {
            self.damage_flash = 1.0;
            self.shake = 1.0;
            self.show_message(format!("Impacto de meteorito! -{:.0} casco", events.damage));
        }
    }

    fn update_minigame(&mut self, input: &GameInput, dt: f32) {
        let (id, _, altitude) = self.nearest_planet();
        let radius = self.planet_radius_world(id);
        if self.minigame.is_none() {
            if input.minigame && altitude < radius * 2.5 && self.warp.is_none() {
                self.minigame = MinigameRunner::new(id);
                if self.minigame.is_none() {
                    self.show_message(
                        "La Tierra no tiene minijuego: visita los planetas tematicos",
                    );
                }
            }
            return;
        }
        let Some(runner) = self.minigame.as_mut() else {
            return;
        };
        let planet = runner.planet;
        let ctx = Self::minigame_context(
            &self.assets,
            planet,
            self.sim_time,
            &self.ship,
            runner.elapsed + dt,
        );
        let mut events = MinigameEvents::default();
        match runner.state.clone() {
            RunState::Intro => {
                if input.confirm {
                    runner.start(&ctx);
                } else if input.back || input.minigame {
                    self.minigame = None;
                    return;
                }
            }
            RunState::Playing => {
                runner.update(&ctx, dt, &mut events);
                let center = planet_position(planet, self.sim_time);
                if (self.ship.position - center).length() > self.planet_radius_world(planet) * 7.0 {
                    self.minigame = None;
                    self.show_message("Minijuego cancelado: te alejaste demasiado del planeta");
                    return;
                }
            }
            RunState::Won | RunState::Lost(_) => {
                if input.restart {
                    runner.start(&ctx);
                } else if input.back || input.minigame || input.confirm {
                    if runner.state == RunState::Won && !self.minigames_won.contains(&planet) {
                        self.minigames_won.push(planet);
                    }
                    self.minigame = None;
                    return;
                }
            }
        }
        if let Some(runner) = &self.minigame
            && runner.state == RunState::Won
            && !self.minigames_won.contains(&planet)
        {
            self.minigames_won.push(planet);
        }
        for (position, color) in events.bursts {
            self.particles
                .burst(position, 40, 9.0, 0.35, color, color * 0.3, 0.9);
        }
        if events.damage > 0.0 {
            self.damage_flash = 0.8;
            self.shake = 0.8;
        }
        if let Some(message) = events.message {
            self.show_message(message);
        }
    }

    fn spawn_environment_particles(&mut self, dt: f32) {
        let (id, _, altitude) = self.nearest_planet();
        let radius = self.planet_radius_world(id);
        if altitude > radius * 1.5 {
            return;
        }
        let intensity = 1.0 - (altitude / (radius * 1.5)).clamp(0.0, 1.0);
        let center = planet_position(id, self.sim_time);
        let up = (self.ship.position - center).normalized();
        let count = (intensity * 90.0 * dt).ceil() as usize;
        for _ in 0..count {
            let rng = &mut self.particles.rng;
            let offset = rng.unit_vector() * rng.range(3.0, 22.0);
            let position = self.ship.position + offset + self.ship.forward * 8.0;
            let side = up.any_perpendicular();
            let (velocity, color, color_end, size, additive, life) = match id {
                PlanetId::Hielo => (
                    side * 9.0 - up * 2.0,
                    Color3::new(0.95, 0.97, 1.0),
                    Color3::new(0.8, 0.9, 1.0),
                    0.12,
                    false,
                    1.6,
                ),
                PlanetId::Oasis => (
                    side * 6.0 + up * 0.3,
                    Color3::new(0.95, 0.8, 0.5),
                    Color3::new(0.8, 0.6, 0.35),
                    0.1,
                    false,
                    1.5,
                ),
                PlanetId::Fuego => (
                    up * 3.0 + rng.unit_vector() * 0.6,
                    Color3::new(1.0, 0.5, 0.1),
                    Color3::new(0.6, 0.05, 0.0),
                    0.12,
                    true,
                    1.8,
                ),
                PlanetId::Dulces => (
                    rng.unit_vector() * 0.8,
                    Color3::new(1.0, 0.6, 0.9),
                    Color3::new(0.6, 0.8, 1.0),
                    0.1,
                    true,
                    1.5,
                ),
                PlanetId::Escandalosos => (
                    rng.unit_vector() * 0.5,
                    Color3::new(0.8, 1.0, 0.4),
                    Color3::new(0.3, 0.6, 0.1),
                    0.08,
                    true,
                    2.0,
                ),
                PlanetId::Tierra => (
                    side * 1.0,
                    Color3::new(0.9, 0.95, 1.0),
                    Color3::new(0.6, 0.7, 0.9),
                    0.15,
                    false,
                    1.2,
                ),
            };
            self.particles.spawn(Particle {
                position,
                velocity,
                life,
                max_life: life,
                size,
                size_end: size,
                color,
                color_end,
                additive,
                drag: 0.1,
            });
        }
    }

    fn spawn_thruster_particles(&mut self) {
        if self.ship.thrust_visual < 0.3 || !self.rig.shows_ship() {
            return;
        }
        let model = self.ship.model_matrix();
        let down = -self.ship.up;
        for offset in THRUSTER_OFFSETS {
            let p = model.transform_point(offset + Vec3::new(0.0, -0.9, 0.0));
            let jitter = self.particles.rng.unit_vector() * 0.3;
            self.particles.spawn(Particle {
                position: p,
                velocity: down * 2.0 - self.ship.forward * 3.0 + jitter + self.ship.velocity * 0.85,
                life: 0.35,
                max_life: 0.35,
                size: 0.12,
                size_end: 0.02,
                color: Color3::new(0.4, 0.8, 1.0),
                color_end: Color3::new(0.1, 0.2, 0.8),
                additive: true,
                drag: 0.5,
            });
        }
    }

    // -----------------------------------------------------------------------
    // Render 3D
    // -----------------------------------------------------------------------

    fn menu_camera(&self) -> Camera {
        let a = self.real_time * 0.03 + 0.8;
        Camera {
            position: Vec3::new(
                a.cos() * 560.0,
                230.0 + (a * 0.7).sin() * 60.0,
                a.sin() * 560.0,
            ),
            target: Vec3::new(0.0, -20.0, 0.0),
            up: WORLD_UP,
            fov_y: 52.0_f32.to_radians(),
            near: 0.5,
            far: 3000.0,
        }
    }

    pub fn current_camera(&self) -> Camera {
        match self.screen {
            Screen::MainMenu | Screen::Controls | Screen::Credits => self.menu_camera(),
            _ => self.camera,
        }
    }

    pub fn render(&mut self, renderer: &mut SoftwareRenderer) {
        let camera = self.current_camera();
        // Cielo azul cuando la cámara está dentro de la atmósfera terrestre.
        let earth_center = planet_position(PlanetId::Tierra, self.sim_time);
        let earth_radius = self.assets.planets[PlanetId::Tierra.index()]
            .surface
            .base_radius
            * config(PlanetId::Tierra).scale;
        let altitude = (camera.position - earth_center).length() - earth_radius;
        let sky = 1.0 - smoothstep(0.5, 6.0, altitude);
        let space = Color3::new(0.004, 0.006, 0.02);
        let clear = space * (1.0 - sky) + Color3::new(0.38, 0.62, 0.95) * sky;
        renderer.begin_frame(&camera, clear, self.sim_time);
        renderer.star_brightness = 1.0 - sky;
        renderer.set_sun_position(Vec3::ZERO);
        let time = self.sim_time;

        // Luces de los propulsores.
        let ship_visible =
            matches!(self.screen, Screen::Flight | Screen::Paused) && self.rig.shows_ship();
        let launch_frame =
            (self.screen == Screen::Launch).then(|| self.launch.frame(self.sim_time));
        let deploy_ship = launch_frame.and_then(|f| f.ship);
        if ship_visible || deploy_ship.is_some() {
            let ship_model = self.ship_model_for_render(deploy_ship);
            let lights: Vec<PointLight> = THRUSTER_OFFSETS
                .iter()
                .map(|o| PointLight {
                    position: ship_model.transform_point(*o + Vec3::new(0.0, -0.28, 0.0)),
                    color: Color3::new(0.03, 0.26, 0.55),
                    radius: 1.3,
                })
                .collect();
            renderer.set_lights(&lights);
        } else {
            renderer.set_lights(&[]);
        }

        // Sol.
        let sun_model = Mat4::rotation_y(time * 0.05) * Mat4::uniform_scale(SUN_RADIUS);
        renderer.draw_mesh(
            &self.assets.sun,
            sun_model,
            &DrawOptions::new(ShaderKind::Sun),
        );

        // Planetas, nubes, atmósferas y decoraciones procedurales.
        for planet in PLANETS {
            let id = planet.id;
            let model = planet_model(id, time);
            let asset = &self.assets.planets[id.index()];
            let drawn = renderer.draw_lod(&asset.lod, model, &DrawOptions::new(planet.shader));
            if !drawn {
                continue;
            }
            let center = model.origin();
            let radius = asset.surface.outer_radius * planet.scale;
            let pixels = renderer.projected_radius(center, radius);
            if id == PlanetId::Tierra {
                let clouds = model * Mat4::rotation_y(time * 0.012);
                if sky < 0.5 {
                    renderer.draw_lod(
                        &self.assets.clouds,
                        clouds,
                        &DrawOptions::new(ShaderKind::Cloud).double_sided(),
                    );
                }
                if pixels > 2.0 {
                    renderer.draw_mesh(
                        &self.assets.launch_pad,
                        self.launch_pad_matrix(),
                        &DrawOptions::new(ShaderKind::Rocket),
                    );
                }
            }
            let halo = match id {
                PlanetId::Tierra => Color3::new(0.3, 0.6, 1.0),
                PlanetId::Hielo => Color3::new(0.5, 0.85, 1.0),
                PlanetId::Oasis => Color3::new(1.0, 0.75, 0.45),
                PlanetId::Fuego => Color3::new(1.0, 0.35, 0.08),
                PlanetId::Dulces => Color3::new(1.0, 0.55, 0.85),
                PlanetId::Escandalosos => Color3::new(0.55, 0.9, 0.6),
            };
            if pixels > 3.0 {
                let shell = Mat4::translation(center) * Mat4::uniform_scale(radius * 1.06);
                renderer.draw_mesh(
                    &self.assets.sphere,
                    shell,
                    &DrawOptions::new(ShaderKind::Atmosphere)
                        .additive()
                        .tint(halo),
                );
            }
            // Detalle: las decoraciones solo se envían si el planeta se ve grande.
            if pixels > 25.0 {
                for prop in &self.props[id.index()] {
                    renderer.draw_lod(
                        self.assets.prop(prop.kind),
                        model * prop.local,
                        &DrawOptions::new(prop.shader),
                    );
                }
            }
        }

        self.belt.render(renderer, &self.assets, time);
        if matches!(
            self.screen,
            Screen::Flight | Screen::Paused | Screen::Destroyed
        ) {
            self.meteors.render(renderer, &self.assets);
        }

        // Cohete durante la cinemática.
        if let Some(frame) = launch_frame {
            let rocket = &self.assets.rocket;
            let options = DrawOptions::new(ShaderKind::Rocket);
            renderer.draw_mesh(&rocket.body, frame.rocket, &options);
            renderer.draw_mesh(
                &rocket.fairing_left,
                frame.fairing_left,
                &options.double_sided(),
            );
            renderer.draw_mesh(
                &rocket.fairing_right,
                frame.fairing_right,
                &options.double_sided(),
            );
            if frame.thrust > 0.01 {
                let flicker = 1.0 + (self.real_time * 31.0).sin() * 0.08;
                let flame = frame.rocket
                    * Mat4::translation(Vec3::new(0.0, -0.25, 0.0))
                    * Mat4::scale(Vec3::new(3.0, 7.5 * frame.thrust * flicker, 3.0));
                renderer.draw_mesh(
                    &self.assets.thruster_flame,
                    flame,
                    &DrawOptions::new(ShaderKind::Exhaust)
                        .additive()
                        .scalar(frame.thrust),
                );
            }
        }

        if ship_visible || deploy_ship.is_some() {
            self.draw_ship(renderer, deploy_ship);
        }

        if let Some(runner) = &self.minigame
            && runner.state != RunState::Intro
        {
            let ctx = Self::minigame_context(
                &self.assets,
                runner.planet,
                time,
                &self.ship,
                runner.elapsed,
            );
            runner.game.render(renderer, &self.assets, &ctx);
        }

        if self.show_orbits && self.screen != Screen::Launch {
            for planet in PLANETS {
                let color = Color3::new(0.12, 0.2, 0.32);
                let steps = 180;
                for i in 0..steps {
                    let a = i as f32 / steps as f32 * 2.0 * PI;
                    let flat = Vec3::new(a.cos(), 0.0, a.sin()) * planet.orbit_radius;
                    let p = Mat4::rotation_x(planet.inclination).transform_point(flat);
                    renderer.draw_particle(p, 0.55, color, 1.0, true);
                }
            }
        }

        self.particles.render(renderer);

        // Postproceso.
        let mut post = crate::renderer::PostFx {
            sun_glow: 0.32,
            vignette: 0.35,
            damage: self.damage_flash,
            flash: self.flash,
            ..Default::default()
        };
        if let Some(frame) = launch_frame {
            post.fade = frame.fade;
            post.flash = post.flash.max(frame.fade * 0.0).max(frame.flash);
            if frame.fade > 0.0 {
                post.flash = frame.fade;
            }
        }
        if matches!(self.screen, Screen::Flight | Screen::Paused) {
            let (id, _, altitude) = self.nearest_planet();
            let radius = self.planet_radius_world(id);
            let near = 1.0 - (altitude / (radius * 1.2)).clamp(0.0, 1.0);
            match id {
                PlanetId::Oasis => post.heat = near * 0.7,
                PlanetId::Fuego => post.heat = near,
                PlanetId::Hielo => post.frost = near * 0.55,
                _ => {}
            }
        }
        if self.screen == Screen::Paused || self.screen == Screen::Destroyed {
            post.fade = post.fade.max(0.35);
        }
        renderer.post = post;
        renderer.end_frame();
        self.last_stats = renderer.stats();
    }

    fn launch_pad_matrix(&self) -> Mat4 {
        let earth = planet_model(PlanetId::Tierra, self.sim_time);
        let r = crate::worlds::EARTH_RADIUS
            * (1.0 + crate::worlds::earth_height(crate::worlds::LAUNCH_DIR))
            - 0.005;
        earth
            * Mat4::translation(crate::worlds::LAUNCH_DIR * r)
            * crate::procgen::align_y(crate::worlds::LAUNCH_DIR, 0.0)
            * Mat4::uniform_scale(crate::launch::GROUND_SCALE)
    }

    fn ship_model_for_render(&self, deploy: Option<(Vec3, Vec3, Vec3)>) -> Mat4 {
        match deploy {
            Some((position, forward, up)) => {
                let mut ship = Ship::new(position, forward);
                ship.face_towards(forward, up);
                ship.model_matrix()
            }
            None => self.ship.model_matrix(),
        }
    }

    fn draw_ship(&self, renderer: &mut SoftwareRenderer, deploy: Option<(Vec3, Vec3, Vec3)>) {
        let model = self.ship_model_for_render(deploy);
        renderer.draw_lod(
            &self.assets.ship,
            model,
            &DrawOptions::new(ShaderKind::Metal).double_sided(),
        );
        let thrust = if deploy.is_some() {
            1.0
        } else {
            self.ship.thrust_visual
        };
        let flame_length = 0.6 + thrust * 0.9 + (self.real_time * 17.0).sin() * 0.08;
        for offset in THRUSTER_OFFSETS {
            let thruster = model * Mat4::translation(offset);
            renderer.draw_mesh(
                &self.assets.thruster_nozzle,
                thruster,
                &DrawOptions::new(ShaderKind::Metal).double_sided(),
            );
            renderer.draw_mesh(
                &self.assets.thruster_flame,
                thruster * Mat4::scale(Vec3::new(1.0, flame_length, 1.0)),
                &DrawOptions::new(ShaderKind::Thruster).additive(),
            );
        }
    }

    // -----------------------------------------------------------------------
    // Interfaz
    // -----------------------------------------------------------------------

    pub fn record_frame_time(&mut self, seconds: f32) {
        self.frame_ms = seconds * 1000.0;
        self.fps_accum.0 += seconds;
        self.fps_accum.1 += 1;
        if self.fps_accum.0 >= 0.5 {
            self.fps = self.fps_accum.1 as f32 / self.fps_accum.0;
            self.fps_accum = (0.0, 0);
        }
    }

    /// Construye el HUD/menús para el estado actual. `renderer` permite
    /// proyectar posiciones 3D (marcadores) al lienzo de la interfaz.
    pub fn build_ui(&self, renderer: &SoftwareRenderer) -> UiFrame {
        let mut ui = UiFrame::default();
        match self.screen {
            Screen::MainMenu => self.ui_main_menu(&mut ui),
            Screen::Controls => self.ui_controls(&mut ui),
            Screen::Credits => self.ui_credits(&mut ui),
            Screen::Launch => self.ui_launch(&mut ui),
            Screen::Flight => self.ui_flight(&mut ui, renderer),
            Screen::Paused => {
                self.ui_flight(&mut ui, renderer);
                self.ui_menu_box(&mut ui, "PAUSA", &PAUSE_MENU, self.pause_index);
            }
            Screen::Destroyed => {
                ui.text_aligned(
                    UI_W * 0.5,
                    280.0,
                    54.0,
                    RED,
                    "NAVE DESTRUIDA",
                    Align::Center,
                );
                ui.text_aligned(
                    UI_W * 0.5,
                    350.0,
                    22.0,
                    WHITE,
                    "El casco llego a 0%. Pulsa Enter para reaparecer cerca de la Tierra.",
                    Align::Center,
                );
            }
        }
        if self.show_stats {
            self.ui_stats(&mut ui);
        }
        ui
    }

    fn ui_menu_box(&self, ui: &mut UiFrame, title: &str, items: &[&str], selected: usize) {
        let w = 420.0;
        let h = 90.0 + items.len() as f32 * 52.0;
        let x = (UI_W - w) * 0.5;
        let y = (UI_H - h) * 0.5 + 40.0;
        ui.panel(x, y, w, h);
        ui.text_aligned(UI_W * 0.5, y + 20.0, 32.0, CYAN, title, Align::Center);
        for (i, item) in items.iter().enumerate() {
            let iy = y + 74.0 + i as f32 * 52.0;
            if i == selected {
                ui.rect(x + 30.0, iy - 8.0, w - 60.0, 42.0, [40, 110, 170, 200]);
                ui.text_aligned(
                    UI_W * 0.5,
                    iy,
                    26.0,
                    GOLD,
                    format!("> {item} <"),
                    Align::Center,
                );
            } else {
                ui.text_aligned(UI_W * 0.5, iy, 26.0, WHITE, *item, Align::Center);
            }
        }
    }

    fn ui_main_menu(&self, ui: &mut UiFrame) {
        ui.text_aligned(
            UI_W * 0.5,
            70.0,
            84.0,
            [255, 210, 90, 255],
            "SPACE TRAVEL",
            Align::Center,
        );
        ui.text_aligned(
            UI_W * 0.5,
            160.0,
            22.0,
            CYAN,
            "Un viaje por seis mundos - renderizador 3D por software en Rust",
            Align::Center,
        );
        self.ui_menu_box(ui, "MENU PRINCIPAL", &MAIN_MENU, self.menu_index);
        ui.text_aligned(
            UI_W * 0.5,
            680.0,
            18.0,
            DIM,
            "Flechas: elegir   Enter: aceptar   Esc: salir",
            Align::Center,
        );
    }

    fn ui_controls(&self, ui: &mut UiFrame) {
        ui.panel(160.0, 60.0, 960.0, 600.0);
        ui.text_aligned(UI_W * 0.5, 80.0, 40.0, CYAN, "CONTROLES", Align::Center);
        let rows = [
            ("W / S", "Acelerar / frenar y retroceder"),
            ("A / D", "Guinada (girar izquierda / derecha)"),
            ("Flecha arriba / abajo", "Cabeceo (subir / bajar el morro)"),
            ("Q / E", "Alabeo"),
            ("Flecha izq. / der.", "Desplazamiento lateral"),
            ("R / F", "Ascender / descender"),
            ("Shift", "Impulso"),
            ("Espacio", "Freno"),
            (
                "1 - 6",
                "Warp animado a Tierra, Hielo, Oasis, Fuego, Dulces, Escandalosos",
            ),
            ("T", "Cambiar planeta objetivo de navegacion"),
            ("C", "Cambiar camara (tercera, primera persona, orbital)"),
            (
                "Raton (arrastrar) / I J K L",
                "Rotar camara orbital;  rueda o + / - : zoom",
            ),
            ("M", "Iniciar / salir del minijuego del planeta cercano"),
            ("Enter / R (minijuego)", "Empezar / reintentar"),
            (
                "O / H / F3",
                "Orbitas / ayuda / estadisticas de rendimiento",
            ),
            ("Retroceso", "Reaparecer en orbita terrestre"),
            ("Esc / P", "Pausa"),
        ];
        for (i, (key, action)) in rows.iter().enumerate() {
            let y = 140.0 + i as f32 * 28.0;
            ui.text(200.0, y, 20.0, GOLD, *key);
            ui.text(520.0, y, 20.0, WHITE, *action);
        }
        ui.text_aligned(
            UI_W * 0.5,
            625.0,
            18.0,
            DIM,
            "Enter o Esc para volver",
            Align::Center,
        );
    }

    fn ui_credits(&self, ui: &mut UiFrame) {
        ui.panel(200.0, 90.0, 880.0, 540.0);
        ui.text_aligned(UI_W * 0.5, 110.0, 40.0, CYAN, "CREDITOS", Align::Center);
        let lines = [
            ("Proyecto final", "Graficas por Computadora"),
            (
                "Equipo",
                "EmilioChenf, ESJ0, zoomjosue (segun el repositorio)",
            ),
            ("Lenguaje", "Rust 2024"),
            ("Renderizado 3D", "Pipeline propio por software en CPU"),
            (
                "Ventana y entrada",
                "raylib 6 (solo ventana, teclado y texto 2D)",
            ),
            (
                "Modelos originales",
                "DeLorean, Hielo y Oasis (Blender, equipo)",
            ),
            (
                "Modelos generados",
                "Tierra, Fuego, Dulces, Escandalosos, Sol, cohete...",
            ),
            ("", "(procedurales: src/procgen.rs y src/worlds.rs)"),
            (
                "Asistencia de IA",
                "Claude (Anthropic) en codigo y documentacion",
            ),
        ];
        for (i, (a, b)) in lines.iter().enumerate() {
            let y = 190.0 + i as f32 * 40.0;
            ui.text_aligned(620.0, y, 22.0, GOLD, *a, Align::Right);
            ui.text(650.0, y, 22.0, WHITE, *b);
        }
        ui.text_aligned(
            UI_W * 0.5,
            590.0,
            18.0,
            DIM,
            "Enter o Esc para volver",
            Align::Center,
        );
    }

    fn ui_launch(&self, ui: &mut UiFrame) {
        let phase = self.launch.phase();
        ui.panel(20.0, 20.0, 380.0, 70.0);
        ui.text(36.0, 32.0, 24.0, GOLD, "MISION: SPACE TRAVEL");
        ui.text(36.0, 62.0, 18.0, WHITE, phase.label());
        let t = self.launch.time;
        if t < crate::launch::T_LIFTOFF {
            let n = (crate::launch::T_LIFTOFF - t).ceil() as u32;
            ui.text_aligned(
                UI_W * 0.5,
                250.0,
                140.0,
                [255, 230, 120, 255],
                n.to_string(),
                Align::Center,
            );
            if t > crate::launch::T_IGNITION {
                ui.text_aligned(
                    UI_W * 0.5,
                    410.0,
                    28.0,
                    [255, 150, 60, 255],
                    "Encendido de motores",
                    Align::Center,
                );
            }
        } else if t < crate::launch::T_LIFTOFF + 2.0 {
            ui.text_aligned(
                UI_W * 0.5,
                280.0,
                64.0,
                [255, 200, 90, 255],
                "DESPEGUE!",
                Align::Center,
            );
        }
        if phase == LaunchPhase::Separation {
            ui.text_aligned(
                UI_W * 0.5,
                120.0,
                30.0,
                CYAN,
                "Separacion de la cofia",
                Align::Center,
            );
        }
        if phase == LaunchPhase::Deploy {
            ui.text_aligned(
                UI_W * 0.5,
                120.0,
                30.0,
                CYAN,
                "Despliegue del DeLorean",
                Align::Center,
            );
        }
        ui.text_aligned(
            UI_W - 30.0,
            680.0,
            18.0,
            DIM,
            "Espacio / Enter: saltar cinematica",
            Align::Right,
        );
    }

    fn project_ui(renderer: &SoftwareRenderer, point: Vec3) -> Option<(f32, f32, bool)> {
        let sx = UI_W / renderer.width() as f32;
        let sy = UI_H / renderer.height() as f32;
        let (p, w) = renderer.project(point)?;
        let x = p.x * sx;
        let y = p.y * sy;
        let inside = w > 0.0 && (0.0..=UI_W).contains(&x) && (0.0..=UI_H).contains(&y);
        Some((x, y, inside))
    }

    /// Marcador de objetivo: círculo si está en pantalla, flecha en el borde si no.
    fn ui_marker(
        &self,
        ui: &mut UiFrame,
        renderer: &SoftwareRenderer,
        world: Vec3,
        label: &str,
        color: Rgba,
    ) {
        let camera = renderer.camera();
        let to = world - camera.position;
        let forward = (camera.target - camera.position).normalized();
        let in_front = to.dot(forward) > 0.0;
        let projected = Self::project_ui(renderer, world);
        if let Some((x, y, true)) = projected
            && in_front
        {
            ui.elements.push(UiElement::Ring {
                x,
                y,
                r: 16.0,
                color,
            });
            ui.text_aligned(x, y + 20.0, 16.0, color, label, Align::Center);
            return;
        }
        // Dirección en espacio de cámara para la flecha del borde.
        let right = forward.cross(camera.up).normalized();
        let up = right.cross(forward);
        let mut dx = to.dot(right);
        let mut dy = -to.dot(up);
        if !in_front && dx.abs() + dy.abs() < 1.0e-3 {
            dy = 1.0;
        }
        let len = (dx * dx + dy * dy).sqrt().max(1.0e-5);
        dx /= len;
        dy /= len;
        let cx = UI_W * 0.5;
        let cy = UI_H * 0.5;
        let k = ((cx - 60.0) / dx.abs().max(1.0e-4)).min((cy - 60.0) / dy.abs().max(1.0e-4));
        let x = cx + dx * k;
        let y = cy + dy * k;
        ui.elements.push(UiElement::Arrow {
            x,
            y,
            angle: dy.atan2(dx),
            size: 18.0,
            color,
        });
        ui.text_aligned(
            x - dx * 34.0,
            y - dy * 34.0 - 8.0,
            16.0,
            color,
            label,
            Align::Center,
        );
    }

    fn ui_flight(&self, ui: &mut UiFrame, renderer: &SoftwareRenderer) {
        let (near_id, _, altitude) = self.nearest_planet();
        let near = config(near_id);
        let speed = self.ship.speed();

        // Etiquetas de planetas visibles.
        for planet in PLANETS {
            let center = planet_position(planet.id, self.sim_time);
            let distance = (center - self.ship.position).length();
            if let Some((x, y, true)) = Self::project_ui(renderer, center)
                && distance > self.planet_radius_world(planet.id) * 3.0
            {
                ui.text_aligned(
                    x,
                    y - 26.0,
                    16.0,
                    DIM,
                    format!("{} {:.0}u", planet.name, distance),
                    Align::Center,
                );
            }
        }
        // Objetivo de navegación.
        let target_pos = planet_position(self.target, self.sim_time);
        let target_distance = (target_pos - self.ship.position).length();
        if self.minigame.is_none() && target_distance > self.planet_radius_world(self.target) * 2.0
        {
            self.ui_marker(
                ui,
                renderer,
                target_pos,
                &format!("{} {:.0}u", config(self.target).name, target_distance),
                CYAN,
            );
        }

        // Panel de estado de la nave.
        ui.panel(16.0, 16.0, 360.0, 168.0);
        ui.text(30.0, 26.0, 22.0, GOLD, "DELOREAN DMC-12  |  NAVE");
        ui.text(
            30.0,
            56.0,
            20.0,
            WHITE,
            format!("Velocidad: {:5.1} u/s", speed),
        );
        let max_speed = crate::ship::BOOST_SPEED;
        ui.bar(30.0, 82.0, 330.0, 10.0, speed / max_speed, CYAN);
        let hull_color = if self.ship.hull > 50.0 {
            GREEN
        } else if self.ship.hull > 25.0 {
            GOLD
        } else {
            RED
        };
        ui.text(
            30.0,
            100.0,
            20.0,
            WHITE,
            format!("Casco: {:3.0}%", self.ship.hull),
        );
        ui.bar(30.0, 126.0, 330.0, 10.0, self.ship.hull / 100.0, hull_color);
        let status = if self.warp.is_some() {
            "WARP ACTIVO"
        } else if self.ship.invulnerable > 0.0 {
            "Impacto recibido"
        } else if altitude < self.planet_radius_world(near_id) * 1.6 {
            "Cerca de planeta (vel. limitada)"
        } else {
            "Crucero"
        };
        ui.text(30.0, 146.0, 18.0, DIM, format!("Estado: {status}"));

        // Panel de navegación.
        ui.panel(UI_W - 376.0, 16.0, 360.0, 140.0);
        ui.text(UI_W - 362.0, 26.0, 22.0, GOLD, "NAVEGACION");
        ui.text(
            UI_W - 362.0,
            56.0,
            19.0,
            WHITE,
            format!("Cercano: {} ({:.0} u)", near.name, altitude.max(0.0)),
        );
        ui.text(
            UI_W - 362.0,
            82.0,
            19.0,
            CYAN,
            format!(
                "Objetivo [T]: {} ({:.0} u)",
                config(self.target).name,
                target_distance
            ),
        );
        ui.text(
            UI_W - 362.0,
            108.0,
            19.0,
            WHITE,
            format!("Camara [C]: {}", self.rig.mode.label()),
        );
        let storm = if self.meteors.storm_active() {
            "LLUVIA DE METEORITOS!".to_owned()
        } else {
            format!(
                "Lluvia de meteoritos en {:.0} s",
                self.meteors.seconds_to_storm()
            )
        };
        ui.text(
            UI_W - 362.0,
            132.0,
            17.0,
            if self.meteors.storm_active() {
                RED
            } else {
                DIM
            },
            storm,
        );

        // Radar (vista superior del sistema solar).
        let rx = UI_W - 106.0;
        let ry = UI_H - 106.0;
        let scale = 90.0 / 500.0;
        ui.elements.push(UiElement::Circle {
            x: rx,
            y: ry,
            r: 94.0,
            color: [4, 8, 20, 200],
        });
        ui.elements.push(UiElement::Ring {
            x: rx,
            y: ry,
            r: 94.0,
            color: [70, 140, 200, 180],
        });
        ui.elements.push(UiElement::Circle {
            x: rx,
            y: ry,
            r: 5.0,
            color: [255, 200, 60, 255],
        });
        for planet in PLANETS {
            let p = planet_position(planet.id, self.sim_time);
            let color = if planet.id == self.target {
                CYAN
            } else {
                [200, 200, 220, 255]
            };
            ui.elements.push(UiElement::Circle {
                x: rx + p.x * scale,
                y: ry + p.z * scale,
                r: 3.5,
                color,
            });
        }
        let s = self.ship.position;
        ui.elements.push(UiElement::Circle {
            x: rx + (s.x * scale).clamp(-94.0, 94.0),
            y: ry + (s.z * scale).clamp(-94.0, 94.0),
            r: 3.0,
            color: GOLD,
        });

        // Ayuda de controles.
        if self.show_help {
            ui.panel(16.0, UI_H - 132.0, 560.0, 116.0);
            ui.text(
                28.0,
                UI_H - 122.0,
                17.0,
                WHITE,
                "W/S acelerar  A/D girar  Flechas cabeceo/lateral  Q/E alabeo",
            );
            ui.text(
                28.0,
                UI_H - 98.0,
                17.0,
                WHITE,
                "R/F subir/bajar  Shift impulso  Espacio freno  C camara",
            );
            ui.text(
                28.0,
                UI_H - 74.0,
                17.0,
                CYAN,
                "1-6 warp a planeta  T objetivo  M minijuego  O orbitas",
            );
            ui.text(
                28.0,
                UI_H - 50.0,
                17.0,
                DIM,
                "H ocultar ayuda  F3 rendimiento  Esc pausa",
            );
        }

        // Aviso de minijuego disponible.
        let radius = self.planet_radius_world(near_id);
        if self.minigame.is_none()
            && altitude < radius * 2.5
            && let Some(title) = minigame_title(near_id)
        {
            ui.rect(UI_W * 0.5 - 260.0, 196.0, 520.0, 40.0, [10, 40, 60, 200]);
            ui.text_aligned(
                UI_W * 0.5,
                205.0,
                22.0,
                GOLD,
                format!("Pulsa M: minijuego \"{title}\""),
                Align::Center,
            );
        }

        if let Some(runner) = &self.minigame {
            self.ui_minigame(ui, renderer, runner);
        }

        if let Some((text, t)) = &self.message {
            let alpha = (t.min(1.0) * 255.0) as u8;
            ui.text_aligned(
                UI_W * 0.5,
                250.0,
                26.0,
                [255, 240, 200, alpha],
                text.clone(),
                Align::Center,
            );
        }

        if self.rig.mode == CameraMode::FirstPerson {
            // Retícula de cabina.
            ui.elements.push(UiElement::Ring {
                x: UI_W * 0.5,
                y: UI_H * 0.5,
                r: 10.0,
                color: [120, 255, 200, 200],
            });
            ui.elements.push(UiElement::Line {
                x0: UI_W * 0.5 - 30.0,
                y0: UI_H * 0.5,
                x1: UI_W * 0.5 - 14.0,
                y1: UI_H * 0.5,
                color: [120, 255, 200, 200],
            });
            ui.elements.push(UiElement::Line {
                x0: UI_W * 0.5 + 14.0,
                y0: UI_H * 0.5,
                x1: UI_W * 0.5 + 30.0,
                y1: UI_H * 0.5,
                color: [120, 255, 200, 200],
            });
        }
    }

    fn ui_minigame(&self, ui: &mut UiFrame, renderer: &SoftwareRenderer, runner: &MinigameRunner) {
        let ctx = Self::minigame_context(
            &self.assets,
            runner.planet,
            self.sim_time,
            &self.ship,
            runner.elapsed,
        );
        let title = runner.game.title();
        match &runner.state {
            RunState::Intro => {
                ui.panel(UI_W * 0.5 - 380.0, 250.0, 760.0, 220.0);
                ui.text_aligned(
                    UI_W * 0.5,
                    266.0,
                    34.0,
                    GOLD,
                    format!("MINIJUEGO: {title}"),
                    Align::Center,
                );
                for (i, line) in runner.game.instructions().iter().enumerate() {
                    ui.text_aligned(
                        UI_W * 0.5,
                        320.0 + i as f32 * 30.0,
                        20.0,
                        WHITE,
                        *line,
                        Align::Center,
                    );
                }
                ui.text_aligned(
                    UI_W * 0.5,
                    425.0,
                    22.0,
                    CYAN,
                    "Enter: comenzar     M / Esc: salir",
                    Align::Center,
                );
            }
            RunState::Playing => {
                ui.panel(UI_W * 0.5 - 250.0, 16.0, 500.0, 54.0 + 24.0 * 5.0);
                ui.text_aligned(UI_W * 0.5, 24.0, 24.0, GOLD, title, Align::Center);
                let timer = match runner.time_left() {
                    Some(left) => format!(
                        "Tiempo restante: {:02}:{:02}",
                        (left / 60.0) as u32,
                        (left % 60.0) as u32
                    ),
                    None => format!(
                        "Cronometro: {:02}:{:02}",
                        (runner.total_time() / 60.0) as u32,
                        (runner.total_time() % 60.0) as u32
                    ),
                };
                let low = runner.time_left().is_some_and(|l| l < 20.0);
                ui.text_aligned(
                    UI_W * 0.5,
                    54.0,
                    20.0,
                    if low { RED } else { WHITE },
                    timer,
                    Align::Center,
                );
                for (i, line) in runner.game.status_lines(&ctx).iter().take(5).enumerate() {
                    ui.text_aligned(
                        UI_W * 0.5,
                        80.0 + i as f32 * 24.0,
                        18.0,
                        CYAN,
                        line.clone(),
                        Align::Center,
                    );
                }
                let (done, total) = runner.game.progress();
                ui.bar(
                    UI_W * 0.5 - 230.0,
                    54.0 + 24.0 * 5.0 + 8.0,
                    460.0,
                    8.0,
                    done as f32 / total.max(1) as f32,
                    GREEN,
                );
                if let Some(target) = runner.game.objective(&ctx) {
                    let d = (target - self.ship.position).length();
                    self.ui_marker(ui, renderer, target, &format!("{d:.0}u"), GOLD);
                }
            }
            RunState::Won | RunState::Lost(_) => {
                let won = runner.state == RunState::Won;
                ui.panel(UI_W * 0.5 - 330.0, 240.0, 660.0, 230.0);
                let (headline, color) = if won {
                    ("VICTORIA!", GREEN)
                } else {
                    ("DERROTA", RED)
                };
                ui.text_aligned(UI_W * 0.5, 258.0, 52.0, color, headline, Align::Center);
                if let RunState::Lost(reason) = &runner.state {
                    ui.text_aligned(
                        UI_W * 0.5,
                        320.0,
                        22.0,
                        WHITE,
                        reason.clone(),
                        Align::Center,
                    );
                }
                let (done, total) = runner.game.progress();
                ui.text_aligned(
                    UI_W * 0.5,
                    352.0,
                    22.0,
                    WHITE,
                    format!(
                        "{title}: {done}/{total}  |  Tiempo {:.1} s  |  Puntos {}",
                        runner.total_time(),
                        runner.score()
                    ),
                    Align::Center,
                );
                if let Some(best) = runner.best_time {
                    ui.text_aligned(
                        UI_W * 0.5,
                        384.0,
                        20.0,
                        GOLD,
                        format!("Mejor tiempo: {best:.1} s"),
                        Align::Center,
                    );
                }
                ui.text_aligned(
                    UI_W * 0.5,
                    425.0,
                    22.0,
                    CYAN,
                    "R: reintentar     Enter / M: volver al sistema solar",
                    Align::Center,
                );
            }
        }
    }

    fn ui_stats(&self, ui: &mut UiFrame) {
        let s = self.last_stats;
        ui.panel(390.0, 16.0, 500.0, 214.0);
        let lines = [
            format!("FPS {:.0}  |  cuadro {:.1} ms", self.fps, self.frame_ms),
            format!(
                "Vertex stage {:.1} ms  |  Raster + shading {:.1} ms",
                s.vertex_ms, s.raster_ms
            ),
            format!(
                "Draw calls {}  |  objetos descartados (frustum) {}",
                s.draw_calls, s.culled_objects
            ),
            format!(
                "Triangulos enviados {}  |  rasterizados {}",
                s.submitted_triangles, s.rasterized_triangles
            ),
            format!(
                "Caras traseras descartadas {}  |  recortados {}",
                s.backface_culled, s.clipped_triangles
            ),
            format!(
                "Dibujos con LOD reducido {}  |  particulas {}",
                s.lod_reduced_draws, s.particles
            ),
            format!("Pixeles sombreados {}", s.shaded_pixels),
            format!(
                "Meteoritos {}  |  semilla {}",
                self.meteors.meteors.len(),
                self.seed
            ),
        ];
        for (i, line) in lines.iter().enumerate() {
            ui.text(
                404.0,
                26.0 + i as f32 * 25.0,
                17.0,
                if i == 0 { GOLD } else { WHITE },
                line.clone(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        Game::new(Assets::load().expect("recursos"), DEFAULT_SEED)
    }

    fn press(f: impl Fn(&mut GameInput)) -> GameInput {
        let mut input = GameInput::default();
        f(&mut input);
        input
    }

    #[test]
    fn menu_navigation_reaches_every_screen() {
        let mut g = game();
        assert_eq!(g.screen, Screen::MainMenu);
        g.update(&press(|i| i.down = true), 0.016);
        g.update(&press(|i| i.confirm = true), 0.016);
        assert_eq!(g.screen, Screen::Controls);
        g.update(&press(|i| i.back = true), 0.016);
        g.update(&press(|i| i.down = true), 0.016);
        g.update(&press(|i| i.confirm = true), 0.016);
        assert_eq!(g.screen, Screen::Credits);
        g.update(&press(|i| i.back = true), 0.016);
        g.update(&press(|i| i.down = true), 0.016);
        g.update(&press(|i| i.confirm = true), 0.016);
        assert!(g.quit, "Salir debe cerrar el juego");
    }

    #[test]
    fn launch_hands_over_control_and_flight_works() {
        let mut g = game();
        g.update(&press(|i| i.confirm = true), 0.016);
        assert_eq!(g.screen, Screen::Launch);
        for _ in 0..(crate::launch::T_END / 0.05) as usize + 5 {
            g.update(&GameInput::default(), 0.05);
        }
        assert_eq!(g.screen, Screen::Flight);
        let start = g.ship.position;
        let throttle = press(|i| i.flight.throttle = 1.0);
        for _ in 0..60 {
            g.update(&throttle, 0.05);
        }
        assert!((g.ship.position - start).length() > 10.0);
        // Pausa y reanudación.
        g.update(&press(|i| i.pause = true), 0.016);
        assert_eq!(g.screen, Screen::Paused);
        let frozen = g.sim_time;
        g.update(&GameInput::default(), 0.5);
        assert_eq!(g.sim_time, frozen, "la simulación se congela en pausa");
        g.update(&press(|i| i.back = true), 0.016);
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn skip_launch_and_warp_to_each_planet_without_scene_change() {
        let mut g = game();
        g.start_new_game();
        g.update(&press(|i| i.skip = true), 0.016);
        assert_eq!(g.screen, Screen::Flight);
        for (index, id) in PlanetId::ALL.iter().enumerate() {
            g.update(&press(|i| i.warp = Some(index)), 0.016);
            for _ in 0..80 {
                g.update(&GameInput::default(), 0.05);
            }
            assert!(g.warp.is_none());
            assert_eq!(g.screen, Screen::Flight);
            let (nearest, _, altitude) = g.nearest_planet();
            assert_eq!(nearest, *id);
            assert!(altitude < g.planet_radius_world(*id) * 3.0);
        }
    }

    #[test]
    fn ship_cannot_pass_through_a_planet() {
        let mut g = game();
        g.start_new_game();
        g.update(&press(|i| i.skip = true), 0.016);
        let center = planet_position(PlanetId::Fuego, g.sim_time);
        g.ship = Ship::new(
            center + Vec3::new(0.0, 0.0, 60.0),
            Vec3::new(0.0, 0.0, -1.0),
        );
        let dive = press(|i| {
            i.flight.throttle = 1.0;
            i.flight.boost = true;
        });
        let mut min_distance = f32::MAX;
        for _ in 0..400 {
            g.update(&dive, 0.05);
            let c = planet_position(PlanetId::Fuego, g.sim_time);
            min_distance = min_distance.min((g.ship.position - c).length());
        }
        let base = g.assets.planets[PlanetId::Fuego.index()]
            .surface
            .base_radius
            * config(PlanetId::Fuego).scale;
        assert!(
            min_distance > base,
            "la nave no debe atravesar la superficie ({min_distance} < {base})"
        );
    }

    #[test]
    fn minigame_can_be_started_played_and_exited() {
        let mut g = game();
        g.start_new_game();
        g.update(&press(|i| i.skip = true), 0.016);
        g.update(&press(|i| i.warp = Some(1)), 0.016); // Hielo
        for _ in 0..80 {
            g.update(&GameInput::default(), 0.05);
        }
        g.update(&press(|i| i.minigame = true), 0.016);
        assert!(
            g.minigame
                .as_ref()
                .is_some_and(|m| m.state == RunState::Intro)
        );
        g.update(&press(|i| i.confirm = true), 0.016);
        assert!(
            g.minigame
                .as_ref()
                .is_some_and(|m| m.state == RunState::Playing)
        );
        // Llevar la nave a cada cristal.
        for _ in 0..40 {
            let target = {
                let runner = g.minigame.as_ref().unwrap();
                if runner.state != RunState::Playing {
                    break;
                }
                let ctx = Game::minigame_context(
                    &g.assets,
                    runner.planet,
                    g.sim_time,
                    &g.ship,
                    runner.elapsed,
                );
                runner.game.objective(&ctx).unwrap()
            };
            g.ship.position = target;
            g.ship.velocity = Vec3::ZERO;
            g.update(&GameInput::default(), 0.016);
        }
        assert_eq!(g.minigame.as_ref().unwrap().state, RunState::Won);
        let ui = g.build_ui(&SoftwareRenderer::new(320, 180));
        assert!(ui.all_text().contains("VICTORIA"));
        g.update(&press(|i| i.confirm = true), 0.016);
        assert!(g.minigame.is_none());
        assert!(g.minigames_won.contains(&PlanetId::Hielo));
    }

    #[test]
    fn rendering_a_flight_frame_produces_pixels_and_stats() {
        let mut g = game();
        g.start_new_game();
        g.update(&press(|i| i.skip = true), 0.016);
        g.update(&GameInput::default(), 0.016);
        let mut renderer = SoftwareRenderer::new(320, 180);
        g.render(&mut renderer);
        let stats = renderer.stats();
        assert!(stats.draw_calls > 5);
        assert!(stats.shaded_pixels > 1000);
        let ui = g.build_ui(&renderer);
        assert!(ui.all_text().contains("Velocidad"));
        assert!(ui.all_text().contains("Camara"));
    }
}
