//! Space Travel — biblioteca del juego.
//!
//! Todo el renderizado 3D ocurre en `renderer` + `shaders` (CPU). Raylib
//! solo se usa en `ui` y `main` para la ventana, la entrada y mostrar el
//! framebuffer y el texto del HUD.

pub mod assets;
pub mod camera;
pub mod game;
pub mod launch;
pub mod math3d;
pub mod meteors;
pub mod minigames;
pub mod model;
pub mod particles;
pub mod procgen;
pub mod renderer;
pub mod shaders;
pub mod ship;
pub mod solar;
pub mod surface;
pub mod ui;
pub mod worlds;
