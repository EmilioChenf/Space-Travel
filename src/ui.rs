//! Dibujo del HUD y los menús con raylib (solo 2D). El contenido lo genera
//! `Game::build_ui`; aquí únicamente se escala del lienzo virtual 1280x720
//! al tamaño real de la ventana.

use crate::game::{Align, UI_H, UI_W, UiElement, UiFrame};
use raylib::prelude::*;

/// La fuente por defecto de raylib solo tiene ASCII: se quitan acentos.
pub fn ascii(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'Á' => 'A',
            'É' => 'E',
            'Í' => 'I',
            'Ó' => 'O',
            'Ú' => 'U',
            'ñ' => 'n',
            'Ñ' => 'N',
            '¡' | '¿' => ' ',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

fn color(c: [u8; 4]) -> Color {
    Color::new(c[0], c[1], c[2], c[3])
}

pub fn draw(d: &mut RaylibDrawHandle, ui: &UiFrame, width: i32, height: i32) {
    let sx = width as f32 / UI_W;
    let sy = height as f32 / UI_H;
    let s = sx.min(sy);
    let ox = (width as f32 - UI_W * s) * 0.5;
    let oy = (height as f32 - UI_H * s) * 0.5;
    let px = |x: f32| ox + x * s;
    let py = |y: f32| oy + y * s;
    for element in &ui.elements {
        match element {
            UiElement::Text {
                x,
                y,
                size,
                color: c,
                text,
                align,
            } => {
                let text = ascii(text);
                let font = (size * s).round().max(8.0) as i32;
                let w = d.measure_text(&text, font) as f32;
                let x = match align {
                    Align::Left => px(*x),
                    Align::Center => px(*x) - w * 0.5,
                    Align::Right => px(*x) - w,
                };
                // Sombra para legibilidad sobre la escena.
                d.draw_text(
                    &text,
                    x as i32 + 2,
                    py(*y) as i32 + 2,
                    font,
                    Color::new(0, 0, 0, c[3] / 2),
                );
                d.draw_text(&text, x as i32, py(*y) as i32, font, color(*c));
            }
            UiElement::Rect {
                x,
                y,
                w,
                h,
                color: c,
            } => {
                let rect = Rectangle::new(px(*x), py(*y), w * s, h * s);
                // Sombra y esquinas suaves: conservan el estilo arcade pero
                // separan mejor menús y avisos de una escena luminosa.
                d.draw_rectangle_rounded(
                    Rectangle::new(rect.x + 3.0 * s, rect.y + 4.0 * s, rect.width, rect.height),
                    0.16,
                    8,
                    Color::new(0, 0, 0, c[3] / 3),
                );
                d.draw_rectangle_rounded(rect, 0.16, 8, color(*c));
            }
            UiElement::Frame {
                x,
                y,
                w,
                h,
                color: c,
            } => {
                d.draw_rectangle_rounded_lines_ex(
                    Rectangle::new(px(*x), py(*y), w * s, h * s),
                    0.08,
                    8,
                    (1.5 * s).max(1.0),
                    color(*c),
                );
            }
            UiElement::Bar {
                x,
                y,
                w,
                h,
                value,
                color: c,
            } => {
                let background = Rectangle::new(px(*x), py(*y), w * s, h * s);
                d.draw_rectangle_rounded(background, 0.7, 8, Color::new(22, 30, 48, 220));
                let filled = Rectangle::new(px(*x), py(*y), w * s * value, h * s);
                if filled.width > 0.5 {
                    d.draw_rectangle_rounded(filled, 0.7, 8, color(*c));
                    d.draw_rectangle_rounded(
                        Rectangle::new(filled.x, filled.y, filled.width, filled.height * 0.35),
                        0.7,
                        8,
                        Color::new(255, 255, 255, 42),
                    );
                }
            }
            UiElement::Circle { x, y, r, color: c } => {
                d.draw_circle_v(Vector2::new(px(*x), py(*y)), r * s, color(*c));
            }
            UiElement::Ring { x, y, r, color: c } => {
                d.draw_ring(
                    Vector2::new(px(*x), py(*y)),
                    r * s - 1.5,
                    r * s + 1.0,
                    0.0,
                    360.0,
                    32,
                    color(*c),
                );
            }
            UiElement::Line {
                x0,
                y0,
                x1,
                y1,
                color: c,
            } => {
                d.draw_line_ex(
                    Vector2::new(px(*x0), py(*y0)),
                    Vector2::new(px(*x1), py(*y1)),
                    2.0,
                    color(*c),
                );
            }
            UiElement::Arrow {
                x,
                y,
                angle,
                size,
                color: c,
            } => {
                let (sin, cos) = angle.sin_cos();
                let tip = Vector2::new(px(*x) + cos * size * s, py(*y) + sin * size * s);
                let back = |side: f32| {
                    let a = angle + side * 2.5;
                    Vector2::new(
                        px(*x) + a.cos() * size * s * 0.8,
                        py(*y) + a.sin() * size * s * 0.8,
                    )
                };
                // raylib espera los vértices en sentido antihorario.
                d.draw_triangle(tip, back(-1.0), back(1.0), color(*c));
                d.draw_triangle(tip, back(1.0), back(-1.0), color(*c));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_are_removed_for_the_default_font() {
        assert_eq!(
            ascii("Órbita, cámara y ñandú ¡ya!"),
            "Orbita, camara y nandu  ya!"
        );
    }
}
