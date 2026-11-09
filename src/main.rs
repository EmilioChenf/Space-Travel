//! Punto de entrada de Space Travel.
//!
//! * `cargo run --release` — juego con ventana (raylib solo muestra el
//!   framebuffer generado por el renderizador por software).
//! * `cargo run --release -- --captura <escena> <archivo.ppm> [ancho alto]`
//!   — renderiza una escena sin ventana (evidencia y pruebas visuales).
//! * `cargo run --release -- --benchmark` — mide tiempos por cuadro reales.

use raylib::prelude::*;
use space_travel::assets::Assets;
use space_travel::camera::{CameraInput, CameraMode};
use space_travel::game::{DEFAULT_SEED, Game, GameInput};
use space_travel::minigames::RunState;
use space_travel::renderer::{SoftwareRenderer, save_ppm};
use space_travel::ship::FlightInput;
use space_travel::solar::{PlanetId, config, planet_position};
use space_travel::ui;
use std::error::Error;
use std::path::Path;
use std::time::Instant;

const RENDER_WIDTH: usize = 960;
const RENDER_HEIGHT: usize = 540;

fn seed_from_env() -> u32 {
    std::env::var("SPACE_TRAVEL_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SEED)
}

fn render_size_from_env() -> (usize, usize) {
    std::env::var("SPACE_TRAVEL_RES")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((RENDER_WIDTH, RENDER_HEIGHT))
}

fn read_input(rl: &RaylibHandle) -> GameInput {
    use KeyboardKey::*;
    let down = |k| rl.is_key_down(k);
    let pressed = |k| rl.is_key_pressed(k);
    let axis = |p, n| (down(p) as u8 as f32) - (down(n) as u8 as f32);
    let mut camera = CameraInput {
        orbit_yaw: axis(KEY_L, KEY_J) * 0.03,
        orbit_pitch: axis(KEY_I, KEY_K) * 0.03,
        zoom: rl.get_mouse_wheel_move()
            + axis(KEY_EQUAL, KEY_MINUS) * 0.3
            + axis(KEY_KP_ADD, KEY_KP_SUBTRACT) * 0.3,
    };
    if rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT)
        || rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_RIGHT)
    {
        let delta = rl.get_mouse_delta();
        camera.orbit_yaw += delta.x * 0.006;
        camera.orbit_pitch += delta.y * 0.006;
    }
    let warp_keys = [KEY_ONE, KEY_TWO, KEY_THREE, KEY_FOUR, KEY_FIVE, KEY_SIX];
    GameInput {
        flight: FlightInput {
            yaw: axis(KEY_D, KEY_A),
            pitch: axis(KEY_UP, KEY_DOWN),
            roll: axis(KEY_E, KEY_Q),
            throttle: axis(KEY_W, KEY_S),
            strafe: axis(KEY_RIGHT, KEY_LEFT),
            lift: axis(KEY_R, KEY_F),
            boost: down(KEY_LEFT_SHIFT) || down(KEY_RIGHT_SHIFT),
            brake: down(KEY_SPACE),
        },
        camera,
        up: pressed(KEY_UP) || pressed(KEY_W),
        down: pressed(KEY_DOWN) || pressed(KEY_S),
        confirm: pressed(KEY_ENTER) || pressed(KEY_KP_ENTER),
        back: pressed(KEY_ESCAPE),
        pause: pressed(KEY_P),
        cycle_camera: pressed(KEY_C),
        restart: pressed(KEY_R),
        minigame: pressed(KEY_M),
        next_target: pressed(KEY_T),
        toggle_orbits: pressed(KEY_O),
        toggle_help: pressed(KEY_H),
        toggle_stats: pressed(KEY_F3),
        skip: pressed(KEY_SPACE),
        reset_ship: pressed(KEY_BACKSPACE),
        warp: warp_keys.iter().position(|k| pressed(*k)),
    }
}

fn run_window() -> Result<(), Box<dyn Error>> {
    let (render_width, render_height) = render_size_from_env();
    let (mut rl, thread) = raylib::init()
        .size(1280, 720)
        .title("Space Travel - Software Renderer en Rust")
        .resizable()
        .msaa_4x()
        .build();
    rl.set_target_fps(60);
    rl.set_exit_key(None);

    let image = Image::gen_image_color(render_width as i32, render_height as i32, Color::BLACK);
    let mut framebuffer = rl.load_texture_from_image(&thread, &image)?;
    framebuffer.set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_BILINEAR);

    let assets = Assets::load()?;
    let mut game = Game::new(assets, seed_from_env());
    let mut renderer = SoftwareRenderer::new(render_width, render_height);

    // Captura automática para documentación: SPACE_TRAVEL_AUTOSHOT=cuadros:ruta.png
    let autoshot = std::env::var("SPACE_TRAVEL_AUTOSHOT").ok().and_then(|v| {
        let (frames, path) = v.split_once(':')?;
        Some((frames.parse::<u32>().ok()?, path.to_owned()))
    });
    let autoscene = std::env::var("SPACE_TRAVEL_ESCENA").ok();
    if let Some(scene) = &autoscene {
        setup_scene(&mut game, scene);
    }
    let mut frame_count = 0u32;

    while !rl.window_should_close() && !game.quit {
        let frame_start = Instant::now();
        let dt = rl.get_frame_time();
        let input = if autoscene.is_some() {
            GameInput::default()
        } else {
            read_input(&rl)
        };
        game.update(&input, dt);
        game.render(&mut renderer);
        framebuffer.update_texture(renderer.pixels())?;
        let ui_frame = game.build_ui(&renderer);

        let width = rl.get_screen_width();
        let height = rl.get_screen_height();
        let scale = (width as f32 / render_width as f32).min(height as f32 / render_height as f32);
        let destination = Rectangle::new(
            (width as f32 - render_width as f32 * scale) * 0.5,
            (height as f32 - render_height as f32 * scale) * 0.5,
            render_width as f32 * scale,
            render_height as f32 * scale,
        );
        let take_screenshot = rl.is_key_pressed(KeyboardKey::KEY_F12);
        {
            let mut d = rl.begin_drawing(&thread);
            d.clear_background(Color::BLACK);
            d.draw_texture_pro(
                &framebuffer,
                Rectangle::new(0.0, 0.0, render_width as f32, render_height as f32),
                destination,
                Vector2::zero(),
                0.0,
                Color::WHITE,
            );
            ui::draw(&mut d, &ui_frame, width, height);
        }
        frame_count += 1;
        if take_screenshot {
            let name = format!("captura_{frame_count}.png");
            rl.take_screenshot(&thread, &name);
        }
        if let Some((frames, path)) = &autoshot
            && frame_count == *frames
        {
            rl.take_screenshot(&thread, path);
            break;
        }
        game.record_frame_time(frame_start.elapsed().as_secs_f32());
    }
    Ok(())
}

fn press(f: impl Fn(&mut GameInput)) -> GameInput {
    let mut input = GameInput::default();
    f(&mut input);
    input
}

fn advance(game: &mut Game, seconds: f32, input: &GameInput) {
    let steps = (seconds / 0.05).ceil() as usize;
    for _ in 0..steps {
        game.update(input, 0.05);
    }
}

fn planet_by_name(name: &str) -> Option<(usize, PlanetId)> {
    PlanetId::ALL
        .iter()
        .enumerate()
        .find(|(_, id)| config(**id).name.to_lowercase() == name)
        .map(|(i, id)| (i, *id))
}

/// Prepara una escena reproducible para capturas y pruebas visuales.
fn setup_scene(game: &mut Game, scene: &str) {
    let launch_at = |game: &mut Game, t: f32| {
        game.start_new_game();
        advance(game, t, &GameInput::default());
    };
    let fly = |game: &mut Game| {
        game.start_new_game();
        game.update(&press(|i| i.skip = true), 0.016);
    };
    match scene {
        "menu" => advance(game, 3.0, &GameInput::default()),
        "cuenta" => launch_at(game, 2.2),
        "encendido" => launch_at(game, 4.4),
        "despegue" => launch_at(game, 7.0),
        "ascenso" => launch_at(game, 9.6),
        "separacion" => launch_at(game, 14.0),
        "despliegue" => launch_at(game, 16.4),
        "vuelo" => {
            fly(game);
            advance(game, 2.0, &press(|i| i.flight.throttle = 1.0));
        }
        "sistema" => {
            fly(game);
            game.ship.position = space_travel::math3d::Vec3::new(120.0, 330.0, 560.0);
            let to_sun = (-game.ship.position).normalized();
            game.ship
                .face_towards(to_sun, space_travel::math3d::Vec3::new(0.0, 1.0, 0.0));
            game.rig.snap(&game.ship);
            game.rig.third_distance = 9.0;
            advance(game, 1.0, &GameInput::default());
        }
        "primera_persona" | "orbital" => {
            fly(game);
            game.update(&press(|i| i.warp = Some(0)), 0.016);
            advance(game, 4.0, &GameInput::default());
            let mode = if scene == "orbital" {
                CameraMode::Orbital
            } else {
                CameraMode::FirstPerson
            };
            game.rig.set_mode(mode);
            advance(game, 1.5, &GameInput::default());
        }
        "meteoritos" => {
            fly(game);
            game.meteors.cycle_time =
                space_travel::meteors::SHOWER_CYCLE - space_travel::meteors::SHOWER_DURATION + 0.5;
            advance(game, 6.0, &GameInput::default());
            game.ship.hull = 100.0;
        }
        other => {
            let (base, kind) = other.split_once('_').map_or((other, ""), |(a, b)| (a, b));
            let (planet_name, kind) = if base == "superficie" || base == "minijuego" {
                (kind, base)
            } else {
                (base, kind)
            };
            let Some((index, id)) = planet_by_name(planet_name) else {
                eprintln!("Escena desconocida: {scene}");
                return;
            };
            fly(game);
            game.update(&press(|i| i.warp = Some(index)), 0.016);
            advance(game, 4.6, &GameInput::default());
            match kind {
                "superficie" => {
                    // Vuelo rasante: la nave cerca del suelo mirando al horizonte.
                    let center = planet_position(id, game.sim_time);
                    let up = (game.ship.position - center).normalized();
                    let radius = game.planet_radius_world(id);
                    game.ship.position = center + up * (radius + 5.0);
                    let tangent = up.any_perpendicular();
                    game.ship
                        .face_towards((tangent - up * 0.25).normalized(), up);
                    game.rig.snap(&game.ship);
                    advance(game, 0.6, &GameInput::default());
                }
                "minijuego" => {
                    game.update(&press(|i| i.minigame = true), 0.016);
                    game.update(&press(|i| i.confirm = true), 0.016);
                    if let Some(runner) = &game.minigame {
                        assert_eq!(runner.state, RunState::Playing);
                    }
                    // Acercarse al objetivo para que aparezca en cuadro.
                    let target = game.minigame.as_ref().and_then(|runner| {
                        let ctx = space_travel::minigames::MinigameContext {
                            planet: id,
                            planet_model: space_travel::solar::planet_model(id, game.sim_time),
                            planet_center: planet_position(id, game.sim_time),
                            planet_scale: config(id).scale,
                            ship_position: game.ship.position,
                            ship_velocity: game.ship.velocity,
                            time: runner.elapsed,
                            surface: &game.assets.planets[id.index()].surface,
                        };
                        runner.game.objective(&ctx)
                    });
                    if let Some(target) = target {
                        let center = planet_position(id, game.sim_time);
                        let up = (target - center).normalized();
                        let back = up.any_perpendicular();
                        game.ship.position = target + up * 9.0 + back * 16.0;
                        game.ship
                            .face_towards((target - game.ship.position).normalized(), up);
                        game.rig.snap(&game.ship);
                    }
                    advance(game, 0.5, &GameInput::default());
                }
                _ => {}
            }
        }
    }
}

fn run_capture(
    scene: &str,
    path: &Path,
    width: usize,
    height: usize,
) -> Result<(), Box<dyn Error>> {
    let assets = Assets::load()?;
    let mut game = Game::new(assets, seed_from_env());
    setup_scene(&mut game, scene);
    let mut renderer = SoftwareRenderer::new(width, height);
    let start = Instant::now();
    game.render(&mut renderer);
    let elapsed = start.elapsed().as_secs_f32() * 1000.0;
    save_ppm(path, width, height, renderer.pixels())?;
    let s = renderer.stats();
    println!(
        "{scene}: {:?} | {elapsed:.1} ms | draws {} | tris {} -> {} | backface {} | clip {} | culled {} | lod {} | px {}",
        game.screen,
        s.draw_calls,
        s.submitted_triangles,
        s.rasterized_triangles,
        s.backface_culled,
        s.clipped_triangles,
        s.culled_objects,
        s.lod_reduced_draws,
        s.shaded_pixels
    );
    // Texto del HUD para documentar lo que se vería en pantalla.
    if std::env::var("SPACE_TRAVEL_HUD").is_ok() {
        println!("{}", game.build_ui(&renderer).all_text());
    }
    Ok(())
}

fn run_benchmark() -> Result<(), Box<dyn Error>> {
    let (width, height) = render_size_from_env();
    let mut renderer = SoftwareRenderer::new(width, height);
    println!(
        "Benchmark a {width}x{height}, {} hilos disponibles",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    );
    let scenes = [
        "despegue",
        "separacion",
        "sistema",
        "vuelo",
        "tierra",
        "hielo",
        "oasis",
        "fuego",
        "dulces",
        "escandalosos",
        "superficie_oasis",
        "superficie_hielo",
        "minijuego_dulces",
        "meteoritos",
    ];
    let mut total = 0.0;
    for scene in scenes {
        let mut game = Game::new(Assets::load()?, seed_from_env());
        setup_scene(&mut game, scene);
        let frames = 30;
        let start = Instant::now();
        let mut tris = 0;
        for _ in 0..frames {
            game.update(&GameInput::default(), 1.0 / 60.0);
            game.render(&mut renderer);
            tris += renderer.stats().rasterized_triangles;
        }
        let ms = start.elapsed().as_secs_f32() * 1000.0 / frames as f32;
        total += ms;
        println!(
            "{scene:>18}: {ms:6.1} ms/cuadro ({:5.1} FPS)  tris rasterizados/cuadro {}",
            1000.0 / ms,
            tris / frames
        );
    }
    println!(
        "Promedio: {:.1} ms/cuadro ({:.1} FPS)",
        total / scenes.len() as f32,
        1000.0 * scenes.len() as f32 / total
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--captura") => {
            let scene = args.get(2).ok_or("falta la escena")?;
            let path = args.get(3).ok_or("falta el archivo de salida")?;
            let width = args
                .get(4)
                .and_then(|v| v.parse().ok())
                .unwrap_or(RENDER_WIDTH);
            let height = args
                .get(5)
                .and_then(|v| v.parse().ok())
                .unwrap_or(RENDER_HEIGHT);
            run_capture(scene, Path::new(path), width, height)
        }
        Some("--benchmark") => run_benchmark(),
        _ => run_window(),
    }
}
