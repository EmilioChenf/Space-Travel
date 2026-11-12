//! Graba el video de demostración con el juego real.
//!
//! Juega el recorrido completo con un guion de entradas (las mismas teclas
//! que usaría una persona, generadas por un piloto automático sencillo) a un
//! paso de tiempo fijo de 1/30 s, renderiza cada cuadro con el pipeline por
//! software y el HUD de raylib, y envía los cuadros a ffmpeg.
//!
//! * `cargo run --release --bin grabar_video -- video.mp4` — graba (requiere
//!   una pantalla; en un servidor se puede usar `xvfb-run`).
//! * `cargo run --release --bin grabar_video -- --simular` — recorre el guion
//!   sin ventana y muestra el progreso, para ajustar el guion rápidamente.
//!
//! Como el tiempo de simulación avanza 1/30 s por cuadro, el video se ve a
//! velocidad real aunque cada cuadro tarde más en calcularse.

use raylib::prelude::*;
use space_travel::assets::Assets;
use space_travel::game::{DEFAULT_SEED, Game, GameInput, Screen};
use space_travel::math3d::Vec3;
use space_travel::renderer::SoftwareRenderer;
use space_travel::solar::{PlanetId, planet_position, planet_rotation};
use space_travel::ui;
use std::error::Error;
use std::io::Write;
use std::process::{Child, Command, Stdio};

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;
const FPS: u32 = 30;
const DT: f32 = 1.0 / FPS as f32;

struct Window {
    rl: RaylibHandle,
    thread: RaylibThread,
    framebuffer: Texture2D,
    renderer: SoftwareRenderer,
    ffmpeg: Child,
}

struct Recorder {
    game: Game,
    window: Option<Window>,
    frames: u64,
    skip_seconds: f32,
}

/// Resultado del guion para un cuadro.
struct Step {
    input: GameInput,
    caption: String,
    done: bool,
}

fn step(input: GameInput, caption: &str) -> Step {
    Step {
        input,
        caption: caption.to_owned(),
        done: false,
    }
}

impl Recorder {
    /// Ejecuta un segmento del guion hasta `max_seconds` o hasta que el guion
    /// diga que terminó. `updates` > 1 acelera el tiempo de la simulación.
    fn segment(
        &mut self,
        name: &str,
        max_seconds: f32,
        updates: u32,
        mut script: impl FnMut(&Game, f32) -> Step,
    ) -> Result<(), Box<dyn Error>> {
        let frames = (max_seconds * FPS as f32).ceil() as u32;
        let mut t = 0.0;
        // En el tramo acelerado se apagan los meteoritos: en unos segundos de
        // video caerían varios minutos de lluvia sobre la nave detenida.
        self.game.meteors.enabled = updates == 1;
        if updates > 1 {
            self.game.meteors.meteors.clear();
        }
        let mut min_sun = f32::MAX;
        let mut min_alt = f32::MAX;
        for frame in 0..frames {
            min_sun = min_sun.min(self.game.ship.position.length());
            min_alt = min_alt.min(self.game.nearest_planet().2);
            let s = script(&self.game, t);
            if s.done {
                break;
            }
            // Las teclas "pulsadas" solo cuentan en la primera actualización.
            self.game.update(&s.input, DT);
            let mut held = s.input;
            held.warp = None;
            held.confirm = false;
            held.minigame = false;
            held.cycle_camera = false;
            held.toggle_orbits = false;
            held.toggle_stats = false;
            held.toggle_help = false;
            held.skip = false;
            for _ in 1..updates {
                self.game.update(&held, DT);
            }
            self.emit(&s.caption)?;
            t += DT;
            if self.window.is_none() && frame % (FPS * 2) == 0 {
                let (id, _, altitude) = self.game.nearest_planet();
                println!(
                    "{name:>12} t={t:5.1} screen={:?} cerca={id:?} alt={altitude:7.1} sol={:6.1} vel={:5.1} casco={:5.1}",
                    self.game.screen,
                    self.game.ship.position.length(),
                    self.game.ship.speed(),
                    self.game.ship.hull
                );
            }
        }
        println!(
            "{name:>12} terminado: {:.1} s de video | casco {:.0} | min sol {min_sun:.0} | min alt {min_alt:.1}",
            self.frames as f32 / FPS as f32,
            self.game.ship.hull
        );
        Ok(())
    }

    fn emit(&mut self, caption: &str) -> Result<(), Box<dyn Error>> {
        self.frames += 1;
        let Some(w) = &mut self.window else {
            return Ok(());
        };
        // Vista previa: DESDE=<segundos> solo graba a partir de ese punto.
        if (self.frames as f32) < self.skip_seconds * FPS as f32 {
            return Ok(());
        }
        self.game.render(&mut w.renderer);
        w.framebuffer.update_texture(w.renderer.pixels())?;
        let ui_frame = self.game.build_ui(&w.renderer);
        {
            let mut d = w.rl.begin_drawing(&w.thread);
            d.clear_background(Color::BLACK);
            d.draw_texture(&w.framebuffer, 0, 0, Color::WHITE);
            ui::draw(&mut d, &ui_frame, WIDTH as i32, HEIGHT as i32);
            if !caption.is_empty() {
                let text = ui::ascii(caption);
                let size = 22;
                let tw = d.measure_text(&text, size);
                let x = (WIDTH as i32 - tw) / 2;
                let y = 636;
                d.draw_rectangle_rounded(
                    Rectangle::new(x as f32 - 18.0, y as f32 - 10.0, tw as f32 + 36.0, 42.0),
                    0.4,
                    8,
                    Color::new(4, 8, 20, 205),
                );
                d.draw_text(&text, x, y, size, Color::new(255, 225, 120, 255));
            }
        }
        let image = w.rl.load_image_from_screen(&w.thread);
        let bytes = image.get_image_data_u8(false);
        w.ffmpeg
            .stdin
            .as_mut()
            .ok_or("ffmpeg sin entrada")?
            .write_all(&bytes)?;
        if self.frames.is_multiple_of(FPS as u64 * 5) {
            println!("{:.0} s grabados", self.frames as f32 / FPS as f32);
        }
        Ok(())
    }
}

/// Convierte una dirección deseada en entradas de guiñada y cabeceo, como
/// lo haría quien juega con `A/D` y las flechas.
fn steer(game: &Game, direction: Vec3) -> (f32, f32) {
    let ship = &game.ship;
    let d = direction.normalized();
    let side = d.dot(ship.right());
    let mut yaw = (side * 3.0).clamp(-1.0, 1.0);
    let pitch = (d.dot(ship.up) * 3.0).clamp(-1.0, 1.0);
    if d.dot(ship.forward) < 0.0 {
        yaw = if side >= 0.0 { 1.0 } else { -1.0 };
    }
    (yaw, pitch)
}

fn fly_towards(game: &Game, direction: Vec3, throttle: f32, boost: bool) -> GameInput {
    let (yaw, pitch) = steer(game, direction);
    let mut input = GameInput::default();
    input.flight.yaw = yaw;
    input.flight.pitch = pitch;
    input.flight.throttle = throttle;
    input.flight.boost = boost;
    input
}

/// Piloto de órbita baja: mantiene la altitud y avanza en un plano fijo
/// respecto al planeta (que gira). Devuelve las entradas y el ángulo
/// recorrido sobre la superficie en este cuadro.
struct Orbiter {
    planet: PlanetId,
    altitude: f32,
    normal_local: Option<Vec3>,
    last_local: Option<Vec3>,
    angle: f32,
}

impl Orbiter {
    fn new(planet: PlanetId, altitude: f32) -> Self {
        Self {
            planet,
            altitude,
            normal_local: None,
            last_local: None,
            angle: 0.0,
        }
    }

    fn input(&mut self, game: &Game, boost: bool) -> GameInput {
        let center = planet_position(self.planet, game.sim_time);
        let rotation = planet_rotation(self.planet, game.sim_time);
        let inverse = rotation.inverse_similarity();
        let offset = game.ship.position - center;
        let distance = offset.length();
        let radial = offset / distance;
        let local = inverse.transform_direction(radial).normalized();
        let normal_local = *self.normal_local.get_or_insert_with(|| {
            let n = radial.cross(game.ship.forward).normalized();
            inverse.transform_direction(n).normalized()
        });
        if let Some(last) = self.last_local {
            self.angle += last.dot(local).clamp(-1.0, 1.0).acos();
        }
        self.last_local = Some(local);
        let normal = rotation.transform_direction(normal_local).normalized();
        let tangent = normal.cross(radial).normalized();
        let target = game.planet_radius_world(self.planet) + self.altitude;
        let correction = ((target - distance) * 0.07).clamp(-0.45, 0.45);
        fly_towards(game, tangent + radial * correction, 1.0, boost)
    }
}

fn run(output: Option<&str>) -> Result<(), Box<dyn Error>> {
    let seed = std::env::var("SPACE_TRAVEL_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SEED);
    let window = match output {
        Some(path) => {
            let (mut rl, thread) = raylib::init()
                .size(WIDTH as i32, HEIGHT as i32)
                .title("Space Travel - grabacion del video")
                .build();
            rl.set_target_fps(0);
            let image = Image::gen_image_color(WIDTH as i32, HEIGHT as i32, Color::BLACK);
            let framebuffer = rl.load_texture_from_image(&thread, &image)?;
            let ffmpeg = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-loglevel",
                    "error",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgba",
                ])
                .args(["-s", &format!("{WIDTH}x{HEIGHT}"), "-r", &FPS.to_string()])
                .args([
                    "-i", "-", "-c:v", "libx264", "-preset", "medium", "-crf", "20",
                ])
                .args(["-pix_fmt", "yuv420p", "-movflags", "+faststart", path])
                .stdin(Stdio::piped())
                .spawn()?;
            Some(Window {
                rl,
                thread,
                framebuffer,
                renderer: SoftwareRenderer::new(WIDTH, HEIGHT),
                ffmpeg,
            })
        }
        None => None,
    };
    let mut rec = Recorder {
        game: Game::new(Assets::load()?, seed),
        window,
        frames: 0,
        skip_seconds: std::env::var("DESDE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0),
    };

    // 1. Menú principal.
    rec.segment("menu", 5.0, 1, |_, t| {
        let input = GameInput {
            confirm: t >= 5.0 - DT * 1.5,
            ..GameInput::default()
        };
        step(
            input,
            "Space Travel: todo el 3D se calcula en la CPU con nuestro renderizador",
        )
    })?;

    // 2. Cinemática de despegue completa.
    rec.segment("despegue", 26.0, 1, |game, t| {
        let caption = if t < 3.5 {
            "Cuenta regresiva en la plataforma de la Tierra"
        } else if t < 10.6 {
            "Despegue: humo y fuego son particulas y triangulos aditivos"
        } else if t < 14.6 {
            "En el espacio se abre la cofia del cohete"
        } else {
            "Sale el DeLorean, la nave del jugador"
        };
        Step {
            input: GameInput::default(),
            caption: caption.to_owned(),
            done: game.screen == Screen::Flight,
        }
    })?;

    // 3. Controles de vuelo.
    let mut first = true;
    rec.segment("controles", 14.0, 1, |_, t| {
        // H oculta el panel de ayuda: los subtítulos explican las teclas.
        let mut input = GameInput {
            toggle_help: std::mem::take(&mut first),
            ..GameInput::default()
        };
        let caption = if t < 3.0 {
            input.flight.throttle = 1.0;
            "W: acelerar (aceleracion progresiva, independiente de los FPS)"
        } else if t < 5.5 {
            input.flight.throttle = 1.0;
            input.flight.yaw = 1.0;
            "A / D: girar"
        } else if t < 8.0 {
            input.flight.throttle = 1.0;
            input.flight.pitch = 1.0;
            "Flechas arriba / abajo: cabeceo"
        } else if t < 10.0 {
            input.flight.throttle = 0.6;
            input.flight.roll = 1.0;
            "Q / E: alabeo"
        } else if t < 12.0 {
            input.flight.throttle = 1.0;
            input.flight.boost = true;
            "Shift: impulso"
        } else {
            input.flight.brake = true;
            "Espacio: freno exponencial"
        };
        step(input, caption)
    })?;

    // 4. Subir para ver el sistema solar completo.
    let lookout = Vec3::new(40.0, 230.0, 330.0);
    // Apuntar por debajo del Sol deja el Sol arriba de la nave en pantalla.
    let below_sun = Vec3::new(0.0, -110.0, 0.0);
    rec.segment("mirador", 16.0, 1, |game, _| {
        let to = lookout - game.ship.position;
        Step {
            input: fly_towards(game, to, 1.0, true),
            caption: "Vuelo espacial libre hacia un punto alto del sistema".to_owned(),
            done: to.length() < 35.0,
        }
    })?;
    let mut first = true;
    rec.segment("frenar", 3.5, 1, |game, _| {
        let mut input = fly_towards(game, below_sun - game.ship.position, 0.0, false);
        input.flight.brake = true;
        input.toggle_orbits = std::mem::take(&mut first);
        step(input, "Sol, nave y 6 planetas. O muestra las orbitas")
    })?;

    // 5. Tiempo acelerado para que se note la órbita y la rotación.
    rec.segment("acelerado", 9.0, 24, |game, _| {
        let mut input = fly_towards(game, below_sun - game.ship.position, 0.0, false);
        input.flight.brake = true;
        step(
            input,
            "Tiempo x24 solo en el video: orbitas y rotacion dependen del tiempo",
        )
    })?;

    // 6. Acercamiento real a Fuego.
    let fuego = PlanetId::Fuego;
    let mut first = true;
    rec.segment("acercamiento", 45.0, 1, |game, _| {
        let center = planet_position(fuego, game.sim_time);
        let altitude = (game.ship.position - center).length() - game.planet_radius_world(fuego);
        let mut input = fly_towards(game, center - game.ship.position, 1.0, altitude > 50.0);
        input.toggle_orbits = std::mem::take(&mut first);
        let caption = if altitude > 70.0 {
            "Acercamiento continuo a Fuego, sin pantalla de carga"
        } else {
            "Cerca del planeta la nave frena sola y se nivela con la superficie"
        };
        Step {
            input,
            caption: caption.to_owned(),
            done: altitude < 40.0,
        }
    })?;

    // 7. Vuelta completa alrededor de Fuego.
    let mut orbiter = Orbiter::new(fuego, 9.0);
    rec.segment("vuelta", 60.0, 1, |game, t| {
        let mut input = orbiter.input(game, true);
        // C dos veces: cámara orbital, elevada y alejada para ver el planeta.
        input.cycle_camera = t < DT * 0.5 || (0.3..0.3 + DT).contains(&t);
        if (0.5..1.5).contains(&t) {
            input.camera.orbit_pitch = 0.02;
        }
        if (0.5..1.3).contains(&t) {
            input.camera.zoom = -0.4;
        }
        let caption = if t < 2.5 {
            "Camara orbital (C) para ver la vuelta alrededor del planeta"
        } else if orbiter.angle < 2.2 {
            "Vuelta completa a Fuego: volcanes y rios de lava hechos con ruido"
        } else if orbiter.angle < 4.4 {
            "Shader de lava animado: el ruido se desplaza con el tiempo"
        } else {
            "Las rocas se distribuyen con semilla y giran con el planeta"
        };
        Step {
            input,
            caption: caption.to_owned(),
            done: orbiter.angle >= 2.0 * std::f32::consts::PI,
        }
    })?;

    // 8. Alejamiento.
    rec.segment("alejamiento", 8.0, 1, |game, t| {
        let away = game.ship.position - planet_position(fuego, game.sim_time);
        let mut input = fly_towards(game, away, 1.0, true);
        // La cámara orbital sigue mostrando el planeta; al final vuelve a
        // tercera persona.
        input.cycle_camera = (7.8..7.8 + DT).contains(&t);
        step(
            input,
            "Alejamiento: al salir de la zona cercana vuelve la velocidad maxima",
        )
    })?;

    // 9. Recorrido por los demás planetas con warp.
    let tour = [
        (
            PlanetId::Hielo,
            "Hielo: escarcha, grietas y destellos que titilan; ventisca en pantalla",
        ),
        (
            PlanetId::Escandalosos,
            "Escandalosos: estilo caricatura con contorno y bosques con semilla",
        ),
        (
            PlanetId::Dulces,
            "Dulces: cel shading, chispas de azucar y lagos de chocolate con ondas",
        ),
        (
            PlanetId::Oasis,
            "Oasis: ondas de arena, causticas animadas y distorsion por calor",
        ),
        (
            PlanetId::Tierra,
            "Tierra: oceanos con brillo, nubes con prueba alfa y luces nocturnas",
        ),
    ];
    let order: Vec<usize> = std::env::var("ORDEN")
        .ok()
        .map(|v| {
            v.chars()
                .filter_map(|c| c.to_digit(10))
                .map(|d| d as usize)
                .collect()
        })
        .unwrap_or_else(|| vec![0, 1, 3, 2, 4]);
    for (planet, caption) in order.into_iter().map(|i| tour[i]) {
        let key = planet.index();
        // Ganar altura antes del warp para que la ruta no cruce el planeta.
        rec.segment("subir", 2.5, 1, |game, _| {
            let (near, _, _) = game.nearest_planet();
            let away = game.ship.position - planet_position(near, game.sim_time);
            step(
                fly_towards(game, away, 1.0, true),
                "Alejamiento del planeta",
            )
        })?;
        let mut first = true;
        rec.segment("warp", 4.0, 1, |game, t| {
            let mut input = GameInput::default();
            if std::mem::take(&mut first) {
                input.warp = Some(key);
            }
            Step {
                input,
                caption: format!("Warp animado (tecla {}): misma escena, sin carga", key + 1),
                done: t > 0.5 && game.warp.is_none(),
            }
        })?;
        let cameras = planet == PlanetId::Escandalosos;
        let mut orbiter = Orbiter::new(planet, 8.0);
        let seconds = if cameras { 14.0 } else { 8.0 };
        rec.segment("planeta", seconds, 1, |game, t| {
            let mut input = orbiter.input(game, false);
            let mut caption = caption;
            if cameras {
                // Tercera persona -> primera persona -> orbital -> tercera.
                let switch = [4.0, 8.0, 12.5];
                input.cycle_camera = switch.iter().any(|s| t >= *s && t < s + DT);
                if (4.0..12.5).contains(&t) {
                    caption = "C: camaras primera persona y orbital, con transicion suave";
                }
            }
            step(input, caption)
        })?;
    }

    // 10. Contadores del renderer: LOD y culling.
    let tierra = PlanetId::Tierra;
    let mut first = true;
    rec.segment("lejos", 9.0, 1, |game, _| {
        let away = game.ship.position - planet_position(tierra, game.sim_time);
        let mut input = fly_towards(game, away, 1.0, true);
        input.toggle_stats = std::mem::take(&mut first);
        step(
            input,
            "F3: al alejarse bajan los triangulos (LOD) y suben los objetos descartados",
        )
    })?;
    rec.segment("cerca", 14.0, 1, |game, _| {
        let center = planet_position(tierra, game.sim_time);
        let altitude = (game.ship.position - center).length() - game.planet_radius_world(tierra);
        Step {
            input: fly_towards(game, center - game.ship.position, 1.0, altitude > 50.0),
            caption: "Al acercarse vuelve la malla completa del planeta".to_owned(),
            done: altitude < 18.0,
        }
    })?;
    let mut first = true;
    rec.segment("final", 5.0, 1, |game, _| {
        let center = planet_position(tierra, game.sim_time);
        let mut input = fly_towards(game, center - game.ship.position, 0.0, false);
        input.flight.brake = true;
        input.toggle_stats = std::mem::take(&mut first);
        step(
            input,
            "Space Travel - Proyecto final de Graficas por Computadora",
        )
    })?;

    if let Some(mut w) = rec.window.take() {
        drop(w.ffmpeg.stdin.take());
        w.ffmpeg.wait()?;
    }
    println!(
        "Listo: {} cuadros ({:.1} s)",
        rec.frames,
        rec.frames as f32 / FPS as f32
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--simular") => run(None),
        Some(path) => run(Some(path)),
        None => Err("uso: grabar_video <salida.mp4> | --simular".into()),
    }
}
