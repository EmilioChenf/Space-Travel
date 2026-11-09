# Estado de la rúbrica — Space Travel

Estimación honesta con lo implementado y comprobado en este repositorio.
La evidencia es reproducible: pruebas automáticas (`cargo test --release`),
capturas reales de la ventana en `docs/capturas/` y las escenas del modo
`--captura`. Los puntos son una estimación del equipo: la nota la decide el
catedrático.

## Requisitos obligatorios

| Requisito | Estado | Responsable | Evidencia |
|---|---|---|---|
| Renderizador propio por software | Cumplido | `renderer.rs`, `shaders.rs` | raylib solo recibe `renderer.pixels()` (`main.rs`). Pruebas `renderer::tests::*` |
| Sol, nave propia y al menos un planeta | Cumplido | `solar.rs`, `assets.rs` | Sol + DeLorean + 6 planetas. Capturas `sistema.jpg`, `vuelo.jpg` |
| Nave controlable | Cumplido | `ship.rs`, `game.rs` | Pruebas `ship::tests::*`, `game::tests::launch_hands_over_control_and_flight_works` |
| Órbitas y rotación dependientes del tiempo | Cumplido | `solar.rs` | `orbits_depend_only_on_time_and_keep_their_radius`, `rotation_advances_with_time_and_preserves_scale` |
| Acercarse y alejarse sin cambiar de escena | Cumplido | `game.rs` (`carry_with_nearest_planet`, `resolve_collisions`) | `skip_launch_and_warp_to_each_planet_without_scene_change`, `ship_cannot_pass_through_a_planet`; capturas `superficie_tierra.jpg`, `minijuego_*.jpg` |
| Transformaciones de modelo, vista y proyección | Cumplido | `math3d.rs`, `renderer.rs` | `look_at_places_target_in_front_of_camera`, `perspective_maps_near_and_far_to_ndc_limits` |
| Prueba de profundidad | Cumplido | `renderer.rs` (Z-buffer 1/w en visibility buffer) | `depth_test_keeps_the_nearest_surface_regardless_of_order`, `particles_respect_depth` |
| Al menos un shader propio | Cumplido (18) | `shaders.rs` | `shaders_produce_distinct_results_for_the_same_fragment`, `lava_changes_over_time` |
| Código, recursos e instrucciones | Cumplido | repositorio, `LEEME.md` | — |
| **Video de demostración** | **PENDIENTE** | equipo | Guía en `docs/VIDEO_DEMO.md`. Sin video, la guía indica 0 puntos |

## Menú de puntos

| Categoría | Disponibles | Implementado | Responsable | Evidencia | Estimado |
|---|---|---|---|---|---|
| Planetas | 50 (máx. 5 planetas) | Hielo, Oasis, Fuego, Dulces y Escandalosos con geometría, decoraciones, shader y efectos propios (más Tierra y Sol) | `worlds.rs`, `solar.rs`, `shaders.rs` | Capturas `hielo.jpg`, `oasis.jpg`, `fuego.jpg`, `dulces.jpg`, `escandalosos.jpg`; `planets_have_varied_terrain_and_materials` | 50 |
| Elementos y distribución de superficie | 35 | Distribución procedural con semilla sobre la superficie curva (rayos contra la malla, orientación según la normal, giran con el planeta); decoraciones horneadas (volcanes, helados, casas, cabañas, bosques); LOD en decoraciones | `solar.rs::scatter_props`, `surface.rs`, `worlds.rs` | `scatter_is_reproducible_and_spaced`, `decorations_raise_collision_height_but_not_ground`; capturas `minijuego_*.jpg` | 30 |
| Renderizado | 60 | Recorte de plano cercano, culling por frustum, por tamaño y de caras traseras, LOD por agrupamiento de vértices, visibility buffer con sombreado diferido, raster multihilo por bandas, corrección de perspectiva, 18 shaders, prueba alfa, mezcla aditiva, partículas con profundidad, postproceso (resplandor, calor, escarcha, daño), estrellas | `renderer.rs`, `shaders.rs`, `model.rs` | `renderer::tests::*`, `shaders::tests::*`, `decimation_reduces_triangles_and_keeps_bounds`; `F3` en el juego | 50 |
| Navegación | 30 | 3 cámaras con transiciones, warp animado a cada planeta, navegación cercana con límite de velocidad, nivelación respecto a la superficie, colisiones, objetivo con flecha, radar | `camera.rs`, `ship.rs`, `game.rs` | `camera::tests::*`, `ship::tests::*`, `skip_launch_and_warp_to_each_planet_without_scene_change`; capturas `primera_persona.jpg`, `orbital.jpg` | 30 |
| Rendimiento | 15 | Medido: 50,7 FPS de media en 14 escenas a 960x540 con 4 hilos; mínimo 25,6 FPS | `renderer.rs`, `main.rs --benchmark` | Tabla del `LEEME.md` | 10 |
| Presentación | 10 | Menú, controles, créditos, pausa, HUD, cinemática de despegue, transiciones, pantallas de victoria/derrota. Falta el video | `game.rs`, `ui.rs`, `launch.rs` | Capturas `menu.jpg`, `cuenta.jpg`, `separacion.jpg` | 7 |
| **Total** | **200 (máx. 100)** | | | | **177 → 100** |

Aunque se baje mucho la estimación de cada categoría, el total sigue por
encima de 100. **Ningún punto vale sin el video de demostración**, que el
equipo todavía tiene que grabar.

## Funcionalidades adicionales (fuera de rúbrica)

- Cinco minijuegos jugables (`minigames.rs`) con victoria, derrota y reinicio.
- Lluvia de meteoritos periódica con semilla configurable y cinturón de
  110 asteroides con colisiones (`meteors.rs`).
- Cinemática de despegue con cuenta regresiva, ascenso, apertura de la
  cofia y despliegue del DeLorean (`launch.rs`).
