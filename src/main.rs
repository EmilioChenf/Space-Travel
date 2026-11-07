mod assets;
mod math3d;
mod model;
mod renderer;

use assets::{DELOREAN_OBJ, HIELO_OBJ, OASIS_OBJ, locate};
use math3d::{Mat4, PI, Vec3};
use model::{Color3, load_obj, make_thruster_flame, make_thruster_nozzle, make_uv_sphere};
use raylib::prelude::*;
use renderer::{Camera, ShaderKind, SoftwareRenderer};
use std::error::Error;

const RENDER_WIDTH: usize = 960;
const RENDER_HEIGHT: usize = 540;
const WORLD_UP: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const THRUSTER_OFFSETS: [Vec3; 4] = [
    Vec3::new(1.43, 0.69, 0.99),
    Vec3::new(1.43, 0.69, -0.99),
    Vec3::new(-1.43, 0.69, 0.99),
    Vec3::new(-1.43, 0.69, -0.99),
];

struct Ship {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    roll: f32,
    speed: f32,
}

#[derive(Clone, Copy, Default)]
struct FlightInput {
    yaw: f32,
    pitch: f32,
    roll: f32,
    throttle: f32,
    boost: bool,
    brake: bool,
}

impl Default for Ship {
    fn default() -> Self {
        Self {
            position: Vec3::new(28.0, 5.0, 14.0),
            yaw: -2.68,
            pitch: -0.08,
            roll: 0.0,
            speed: 0.0,
        }
    }
}

fn ship_rotation(ship: &Ship) -> Mat4 {
    Mat4::rotation_y(-ship.yaw) * Mat4::rotation_z(ship.pitch) * Mat4::rotation_x(ship.roll)
}

fn ship_forward(ship: &Ship) -> Vec3 {
    ship_rotation(ship)
        .transform_direction(Vec3::new(1.0, 0.0, 0.0))
        .normalized()
}

fn flight_input(rl: &RaylibHandle) -> FlightInput {
    let key = |positive, negative| {
        (rl.is_key_down(positive) as u8 as f32) - (rl.is_key_down(negative) as u8 as f32)
    };
    FlightInput {
        yaw: key(KeyboardKey::KEY_D, KeyboardKey::KEY_A),
        pitch: key(KeyboardKey::KEY_UP, KeyboardKey::KEY_DOWN),
        roll: key(KeyboardKey::KEY_E, KeyboardKey::KEY_Q),
        throttle: key(KeyboardKey::KEY_W, KeyboardKey::KEY_S),
        boost: rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT),
        brake: rl.is_key_down(KeyboardKey::KEY_SPACE),
    }
}

fn approach(current: f32, target: f32, maximum_change: f32) -> f32 {
    current + (target - current).clamp(-maximum_change, maximum_change)
}

fn update_ship(ship: &mut Ship, input: FlightInput, dt: f32) {
    ship.yaw += input.yaw * 1.65 * dt;
    ship.pitch = (ship.pitch + input.pitch * 1.30 * dt).clamp(-1.18, 1.18);

    // El alabeo vuelve suavemente al centro y A/D inclina un poco la nave. La
    // cámara permanece estable, así que girar no rota toda la pantalla.
    let target_roll = if input.roll.abs() > 0.01 {
        input.roll * 0.82
    } else {
        -input.yaw * 0.28
    };
    let roll_response = 1.0 - (-5.5 * dt).exp();
    ship.roll += (target_roll - ship.roll) * roll_response;

    let maximum_speed = if input.boost { 24.0 } else { 12.0 };
    if input.brake {
        ship.speed = approach(ship.speed, 0.0, 24.0 * dt);
    } else if input.throttle.abs() > 0.01 {
        ship.speed += input.throttle * 13.0 * dt;
    } else {
        ship.speed = approach(ship.speed, 0.0, 1.35 * dt);
    }
    ship.speed = ship.speed.clamp(-5.0, maximum_speed);
    ship.position += ship_forward(ship) * ship.speed * dt;
}

fn simulate_ship(ship: &mut Ship, input: FlightInput, frame_dt: f32) {
    // Se subdividen los cuadros lentos para que el control conserve la misma
    // respuesta aun cuando el rasterizador por CPU tenga mucho trabajo.
    let frame_dt = frame_dt.clamp(0.0, 0.20);
    let steps = (frame_dt / (1.0 / 60.0)).ceil().clamp(1.0, 12.0) as usize;
    let step_dt = frame_dt / steps as f32;
    for _ in 0..steps {
        update_ship(ship, input, step_dt);
    }
}

fn keep_outside_sphere(ship: &mut Ship, center: Vec3, radius: f32) {
    let offset = ship.position - center;
    let distance = offset.length();
    if distance < radius && distance > 0.0001 {
        let outward = offset / distance;
        ship.position = center + outward * radius;
        let velocity = ship_forward(ship) * ship.speed;
        if velocity.dot(outward) < 0.0 {
            ship.speed = 0.0;
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let (mut rl, thread) = raylib::init()
        .size(1280, 720)
        .title("Space Travel - Software Renderer en Rust")
        .resizable()
        .build();
    rl.set_target_fps(60);

    let image = Image::gen_image_color(RENDER_WIDTH as i32, RENDER_HEIGHT as i32, Color::BLACK);
    let mut framebuffer_texture = rl.load_texture_from_image(&thread, &image)?;
    framebuffer_texture.set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_BILINEAR);

    let ship_mesh = load_obj(locate(DELOREAN_OBJ))?;
    let oasis_mesh = load_obj(locate(OASIS_OBJ))?;
    let ice_mesh = load_obj(locate(HIELO_OBJ))?;
    let sphere = make_uv_sphere(28, 18);
    let thruster_nozzle = make_thruster_nozzle(16);
    let thruster_flame = make_thruster_flame(16);
    let mut renderer = SoftwareRenderer::new(RENDER_WIDTH, RENDER_HEIGHT);
    let mut ship = Ship::default();
    let mut smooth_camera = ship.position - ship_forward(&ship) * 9.0 + Vec3::new(0.0, 3.0, 0.0);

    while !rl.window_should_close() {
        let dt = rl.get_frame_time();
        let time = rl.get_time() as f32;
        if rl.is_key_pressed(KeyboardKey::KEY_R) {
            ship = Ship::default();
            smooth_camera = ship.position - ship_forward(&ship) * 9.0 + WORLD_UP * 3.0;
        }
        simulate_ship(&mut ship, flight_input(&rl), dt);

        let oasis_position = Vec3::new((time * 0.13).cos() * 18.0, 0.0, (time * 0.13).sin() * 18.0);
        let ice_angle = time * 0.085 + PI * 0.72;
        let ice_position = Vec3::new(
            ice_angle.cos() * 29.0,
            (ice_angle * 0.65).sin() * 3.5,
            ice_angle.sin() * 29.0,
        );
        keep_outside_sphere(&mut ship, Vec3::ZERO, 4.2);
        keep_outside_sphere(&mut ship, oasis_position, 4.8);
        keep_outside_sphere(&mut ship, ice_position, 4.9);

        let rotation = ship_rotation(&ship);
        let forward = ship_forward(&ship);
        let desired_camera = ship.position - forward * 9.5 + WORLD_UP * 3.4;
        smooth_camera = smooth_camera.lerp(desired_camera, 1.0 - (-6.5 * dt.min(0.20)).exp());
        let camera = Camera {
            position: smooth_camera,
            target: ship.position + forward * 5.0,
            up: WORLD_UP,
            fov_y: 60.0_f32.to_radians(),
            near: 0.1,
            far: 250.0,
        };

        renderer.clear(Color3::new(0.003, 0.005, 0.018), time);
        renderer.begin(&camera);
        let sun_model = Mat4::rotation_y(time * 0.18) * Mat4::scale(Vec3::new(3.5, 3.5, 3.5));
        renderer.draw_mesh(&sphere, sun_model, ShaderKind::Sun, time);
        let oasis_model = Mat4::translation(oasis_position)
            * Mat4::rotation_y(time * 0.22)
            * Mat4::scale(Vec3::new(0.8, 0.8, 0.8));
        renderer.draw_mesh(&oasis_mesh, oasis_model, ShaderKind::Lit, time);
        let ice_model = Mat4::translation(ice_position)
            * Mat4::rotation_y(-time * 0.16)
            * Mat4::rotation_x(0.16)
            * Mat4::scale(Vec3::new(0.86, 0.86, 0.86));
        renderer.draw_mesh(&ice_mesh, ice_model, ShaderKind::Ice, time);
        let ship_model = Mat4::translation(ship.position)
            * rotation
            * Mat4::translation(Vec3::new(0.0, -1.15, 0.0))
            * Mat4::scale(Vec3::new(0.72, 0.72, 0.72));
        let thruster_lights = THRUSTER_OFFSETS
            .map(|offset| ship_model.transform_point(offset + Vec3::new(0.0, -0.28, 0.0)));
        renderer.set_thruster_lights(&thruster_lights);
        renderer.draw_mesh(&ship_mesh, ship_model, ShaderKind::Metal, time);

        let flame_length = 0.82 + ship.speed.abs() / 18.0 * 0.7 + (time * 17.0).sin() * 0.08;
        for offset in THRUSTER_OFFSETS {
            let thruster_model = ship_model * Mat4::translation(offset);
            renderer.draw_mesh(&thruster_nozzle, thruster_model, ShaderKind::Metal, time);
            renderer.draw_mesh(
                &thruster_flame,
                thruster_model * Mat4::scale(Vec3::new(1.0, flame_length, 1.0)),
                ShaderKind::Thruster,
                time,
            );
        }

        framebuffer_texture.update_texture(renderer.pixels())?;
        let window_width = rl.get_screen_width();
        let window_height = rl.get_screen_height();
        let scale = (window_width as f32 / RENDER_WIDTH as f32)
            .min(window_height as f32 / RENDER_HEIGHT as f32);
        let source = Rectangle::new(0.0, 0.0, RENDER_WIDTH as f32, RENDER_HEIGHT as f32);
        let destination = Rectangle::new(
            (window_width as f32 - RENDER_WIDTH as f32 * scale) * 0.5,
            (window_height as f32 - RENDER_HEIGHT as f32 * scale) * 0.5,
            RENDER_WIDTH as f32 * scale,
            RENDER_HEIGHT as f32 * scale,
        );
        let stats = renderer.stats();
        let debug_text = format!(
            "Velocidad {:.1} | tris {}/{} | pixeles {}",
            ship.speed, stats.rasterized_triangles, stats.submitted_triangles, stats.shaded_pixels,
        );

        let mut d = rl.begin_drawing(&thread);
        d.clear_background(Color::BLACK);
        d.draw_texture_pro(
            &framebuffer_texture,
            source,
            destination,
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );
        d.draw_rectangle(12, 12, 580, 80, Color::new(0, 0, 0, 190));
        d.draw_text(
            "W/S acelerar  A/D girar  Flechas inclinar  Q/E alabeo",
            24,
            22,
            18,
            Color::RAYWHITE,
        );
        d.draw_text(
            "Espacio frenar  Shift impulso  R reiniciar",
            24,
            46,
            18,
            Color::SKYBLUE,
        );
        d.draw_text(&debug_text, 24, 68, 16, Color::LIGHTGRAY);
        d.draw_fps(window_width - 92, 14);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_keep_their_response_during_a_slow_frame() {
        let mut ship = Ship::default();
        let initial_position = ship.position;
        simulate_ship(
            &mut ship,
            FlightInput {
                throttle: 1.0,
                ..FlightInput::default()
            },
            0.20,
        );

        assert!(
            ship.speed > 2.5,
            "la aceleración no debe perder tiempo de cuadro"
        );
        assert!((ship.position - initial_position).length() > 0.0);
    }

    #[test]
    fn brake_stops_the_ship_without_reversing_it() {
        let mut ship = Ship {
            speed: 4.0,
            ..Ship::default()
        };
        simulate_ship(
            &mut ship,
            FlightInput {
                brake: true,
                ..FlightInput::default()
            },
            0.2,
        );

        assert!(ship.speed.abs() < 1.0e-5);
    }

    #[test]
    fn collision_only_stops_motion_toward_a_planet() {
        let mut inward = Ship {
            position: Vec3::new(3.0, 0.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            speed: -4.0,
        };
        keep_outside_sphere(&mut inward, Vec3::ZERO, 4.0);
        assert!(inward.speed.abs() < 1.0e-5);

        let mut outward = Ship {
            position: Vec3::new(3.0, 0.0, 0.0),
            speed: 4.0,
            ..inward
        };
        keep_outside_sphere(&mut outward, Vec3::ZERO, 4.0);
        assert_eq!(outward.speed, 4.0);
    }
}
