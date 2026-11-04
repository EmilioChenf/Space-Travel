mod assets;
mod math3d;
mod model;
mod renderer;

use assets::{DELOREAN_OBJ, locate};
use math3d::{Mat4, Vec3};
use model::{Color3, load_obj, make_thruster_flame, make_thruster_nozzle, make_uv_sphere};
use raylib::prelude::*;
use renderer::{Camera, ShaderKind, SoftwareRenderer};
use std::error::Error;

const RENDER_WIDTH: usize = 960;
const RENDER_HEIGHT: usize = 540;
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

fn update_ship(rl: &RaylibHandle, ship: &mut Ship, dt: f32) {
    let key = |positive, negative| {
        (rl.is_key_down(positive) as u8 as f32) - (rl.is_key_down(negative) as u8 as f32)
    };
    let yaw_input = key(KeyboardKey::KEY_D, KeyboardKey::KEY_A);
    let pitch_input = key(KeyboardKey::KEY_UP, KeyboardKey::KEY_DOWN);
    let roll_input = key(KeyboardKey::KEY_E, KeyboardKey::KEY_Q);
    let throttle_input = key(KeyboardKey::KEY_W, KeyboardKey::KEY_S);
    ship.yaw += yaw_input * 1.25 * dt;
    ship.pitch = (ship.pitch + pitch_input * 1.05 * dt).clamp(-1.25, 1.25);
    ship.roll += roll_input * 1.5 * dt;
    let maximum_speed = if rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT) {
        18.0
    } else {
        9.0
    };
    ship.speed += throttle_input * 7.0 * dt;
    if throttle_input == 0.0 {
        ship.speed *= (-0.45 * dt).exp();
    }
    ship.speed = ship.speed.clamp(-3.0, maximum_speed);
    ship.position += ship_forward(ship) * ship.speed * dt;
}

fn keep_outside_sphere(ship: &mut Ship, center: Vec3, radius: f32) {
    let offset = ship.position - center;
    let distance = offset.length();
    if distance < radius && distance > 0.0001 {
        ship.position = center + offset / distance * radius;
        ship.speed = ship.speed.min(0.0);
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
    let sphere = make_uv_sphere(28, 18);
    let thruster_nozzle = make_thruster_nozzle(16);
    let thruster_flame = make_thruster_flame(16);
    let mut renderer = SoftwareRenderer::new(RENDER_WIDTH, RENDER_HEIGHT);
    let mut ship = Ship::default();
    let mut smooth_camera = ship.position - ship_forward(&ship) * 9.0 + Vec3::new(0.0, 3.0, 0.0);

    while !rl.window_should_close() {
        let dt = rl.get_frame_time().min(0.05);
        let time = rl.get_time() as f32;
        if rl.is_key_pressed(KeyboardKey::KEY_R) {
            ship = Ship::default();
        }
        update_ship(&rl, &mut ship, dt);

        let planet_position =
            Vec3::new((time * 0.13).cos() * 18.0, 0.0, (time * 0.13).sin() * 18.0);
        keep_outside_sphere(&mut ship, Vec3::ZERO, 4.2);
        keep_outside_sphere(&mut ship, planet_position, 4.7);

        let rotation = ship_rotation(&ship);
        let forward = ship_forward(&ship);
        let ship_up = rotation
            .transform_direction(Vec3::new(0.0, 1.0, 0.0))
            .normalized();
        let desired_camera = ship.position - forward * 9.0 + ship_up * 3.2;
        smooth_camera = smooth_camera.lerp(desired_camera, 1.0 - (-5.0 * dt).exp());
        let camera = Camera {
            position: smooth_camera,
            target: ship.position + forward * 5.0,
            up: ship_up,
            fov_y: 60.0_f32.to_radians(),
            near: 0.1,
            far: 250.0,
        };

        renderer.clear(Color3::new(0.003, 0.005, 0.018), time);
        renderer.begin(&camera);
        let sun_model = Mat4::rotation_y(time * 0.18) * Mat4::scale(Vec3::new(3.5, 3.5, 3.5));
        renderer.draw_mesh(&sphere, sun_model, ShaderKind::Sun, time);
        let planet_model = Mat4::translation(planet_position)
            * Mat4::rotation_y(time * 0.38)
            * Mat4::scale(Vec3::new(4.0, 4.0, 4.0));
        renderer.draw_mesh(&sphere, planet_model, ShaderKind::Planet, time);
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
            "W/S velocidad  A/D giro  Flechas inclinacion  Q/E alabeo",
            24,
            22,
            18,
            Color::RAYWHITE,
        );
        d.draw_text("Shift impulso  R reiniciar", 24, 46, 18, Color::SKYBLUE);
        d.draw_text(&debug_text, 24, 68, 16, Color::LIGHTGRAY);
        d.draw_fps(window_width - 92, 14);
    }
    Ok(())
}
