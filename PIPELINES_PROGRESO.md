# Progreso por pipelines — Space Travel

Cada pipeline siguió el flujo análisis → implementación → integración →
compilación → pruebas → corrección → validación. Herramientas usadas en
cada uno: `cargo fmt --check`, `cargo clippy --release --all-targets`
(sin advertencias), `cargo test --release` y `cargo build --release`.
Estado final: **80 pruebas en verde**.

**Último pipeline completado:** P19. **Pendiente fuera del código:** grabar
el video (P18) y completar los datos de los integrantes en el `LEEME.md`.

| ID | Pipeline | Estado |
|---|---|---|
| P00 | Auditoría | COMPLETADO |
| P01 | Renderizador | COMPLETADO |
| P02 | Carga OBJ/MTL | COMPLETADO |
| P03 | Modelos faltantes | COMPLETADO |
| P04 | Sistema solar | COMPLETADO |
| P05 | Navegación, física y colisiones | COMPLETADO |
| P06 | Cámaras | COMPLETADO |
| P07 | Despegue y cohete | COMPLETADO |
| P08 | Seis planetas | COMPLETADO |
| P09 | Shaders y efectos | COMPLETADO |
| P10 | Distribución procedural | COMPLETADO |
| P11 | Meteoritos y obstáculos | COMPLETADO |
| P12 | Sistema de minijuegos | COMPLETADO |
| P13 | Cinco minijuegos | COMPLETADO |
| P14 | Menús, HUD y transiciones | COMPLETADO |
| P15 | Optimización | COMPLETADO |
| P16 | Integración y pruebas funcionales | COMPLETADO |
| P17 | Auditoría de rúbrica | COMPLETADO |
| P18 | Documentación y entrega | EN PROCESO (falta el video) |
| P19 | Calidad visual, 60 FPS y escalado multinúcleo | COMPLETADO |

---

### P00 — Auditoría
- **Objetivo:** conocer el estado real del repositorio.
- **Hallazgos:** renderizador funcional de un solo hilo (`main.rs`,
  `renderer.rs`, `model.rs`, `math3d.rs`, `assets.rs`; 1 439 líneas, 11
  pruebas en verde). Cargaba DeLorean (90 466 triángulos), Oasis (165 885) y
  Hielo (9 826) con sus MTL correctos. Solo 2 planetas, sin recorte
  (se descartaban los triángulos que cruzaban el plano cercano), sin
  culling, sin LOD, una sola cámara, sin menús ni minijuegos. Carpetas de
  Fuego, Dulces, Escandalosos, Sol, meteoritos y asteroides vacías
  (`.gitkeep`).
- **Acciones:** se creó la rama de respaldo `respaldo/estado-inicial`.
  Se instalaron las bibliotecas de desarrollo X11/GL para compilar raylib en
  Linux.

### P01 — Renderizador por software
- **Archivos:** `renderer.rs` (reescrito), `shaders.rs` (nuevo), `math3d.rs`.
- **Implementado:** recorte Sutherland–Hodgman contra el plano cercano,
  frustum culling, descarte de caras traseras, binning por bandas,
  raster multihilo, visibility buffer con sombreado diferido, Z-buffer 1/w,
  partículas, triángulos aditivos, postproceso, estadísticas por etapa.
- **Pruebas:** profundidad con 1 y 4 hilos, recorte, culling, interpolación
  con corrección de perspectiva, partículas ocultas, matemáticas.
- **Problema encontrado:** las nubes con descarte dejaban agujeros negros
  porque el visibility buffer ya había elegido su triángulo. **Corrección:**
  prueba alfa durante el pase de visibilidad (`shaders::covers`).

### P02 — Carga y validación de OBJ/MTL
- **Archivos:** `model.rs`, `assets.rs`.
- **Implementado:** objetos separados (`o`/`g`), `Ke` emisivo, tipo
  semántico de material, esfera envolvente, LOD por agrupamiento de vértices.
- **Pruebas:** `every_obj_references_existing_materials` revisa todos los OBJ
  del repositorio (mtllib, usemtl definidos, caras). Carga completa de
  recursos con LOD y superficies.

### P03 — Modelos faltantes
- **Archivos:** `procgen.rs`, `worlds.rs`, `src/bin/generar_modelos.rs`.
- **Generados (31 OBJ/MTL, 144 307 triángulos):** Tierra, Nubes,
  Plataforma, Fuego, roca volcánica, Dulces, piruleta, bastón, gomita, aro
  de caramelo, Escandalosos, pino, árbol redondo, 3 osos, cristal, roca
  helada, palmera, cactus, ruina, reliquia, Sol, 3 meteoritos, 3 asteroides,
  aro de control y cohete (con la cofia en dos mitades).
- **Nota:** se generaron con código, no con Blender (no está disponible en
  el entorno). DeLorean, `Hielo.obj` y `oasis.obj` no se modificaron.
- **Pruebas:** geometría finita con normales unitarias, relieve real,
  explanada de lanzamiento plana, cofia separable, orientación de las caras.

### P04 — Sistema solar
- **Archivos:** `solar.rs`.
- **Implementado:** 6 planetas con radio y velocidad orbital, rotación,
  escala, fase, inclinación orbital y del eje configurables; Sol con shader
  animado; fondo de estrellas; órbitas visibles.
- **Pruebas:** órbitas dependientes solo del tiempo, longitud de arco,
  rotación y separación entre órbitas.

### P05 — Navegación, física y colisiones
- **Archivos:** `ship.rs`, `surface.rs`, `game.rs`.
- **Implementado:** base ortonormal (guiñada, cabeceo, alabeo sin bloqueo de
  cardán), aceleración, freno, desaceleración progresiva, lateral,
  ascenso/descenso, impulso, subpasos fijos, nivelación automática respecto
  a la superficie, límite de velocidad cerca de planetas, arrastre con la
  órbita y la rotación del planeta cercano, colisiones con la rejilla de
  alturas de cada planeta, el Sol y los asteroides, daño.
- **Pruebas:** independencia de FPS, freno, no atravesar Fuego en picada.

### P06 — Cámaras
- **Archivos:** `camera.rs`.
- **Implementado:** tercera persona, primera persona y orbital con zoom y
  transición interpolada.
- **Pruebas:** ciclo de modos, posición detrás de la nave, transición sin
  saltos, distancia orbital.

### P07 — Despegue
- **Archivos:** `launch.rs`, `game.rs`.
- **Implementado:** cuenta regresiva, encendido con humo y fuego, ascenso,
  transición con destello, apertura de la cofia con bisagras, salida y giro
  del DeLorean, entrega del control; se puede saltar.
- **Problemas y correcciones:** la cámara quedaba sobre la capa de nubes
  (cielo gris) → se bajó; el lanzamiento ocurría de noche → se cambió la
  fase orbital de la Tierra; el humo era 16 veces más grande que el cohete
  → se escaló; cielo azul y sin estrellas mientras se está en la atmósfera.

### P08 — Seis planetas
- Integración de modelos, shaders, atmósferas aditivas, nubes y efectos de
  entorno (ventisca, arena, brasas, chispas, luciérnagas). Revisado en las
  capturas de cada planeta.

### P09 — Shaders y efectos
- 18 shaders distintos; distorsión por calor (Oasis, Fuego), tinte de
  escarcha (Hielo), resplandor solar, daño, destello y fundido.
- **Pruebas:** colores distintos para el mismo fragmento, lava animada,
  nubes con descarte parcial.

### P10 — Distribución procedural
- `solar::scatter_props`: semilla, rayos contra la malla para hallar el
  suelo, se descarta agua y decoraciones existentes, orientación con la
  normal radial y transformación local que gira con el planeta.

### P11 — Meteoritos y obstáculos
- **Archivos:** `meteors.rs`.
- Lluvia periódica (70 s, 22 s intensos) con semilla, estelas, impactos,
  daño, límite de 28 meteoritos; cinturón de 110 asteroides con colisiones.
- **Pruebas:** reproducibilidad con semilla, impacto y desaparición, límite.

### P12 — Sistema de minijuegos
- Trait `Minigame` + `MinigameRunner` (instrucciones, juego, victoria,
  derrota, reinicio, salida, cronómetro, penalizaciones, puntuación).

### P13 — Cinco minijuegos
- Cristales perdidos, Reliquias del desierto, Escape volcánico, Carrera de
  caramelos y Encuentra a los tres osos.
- **Pruebas:** cada uno se gana visitando los objetivos; derrota por tiempo
  y reinicio; aros por cruce de plano con fallos y penalización; los
  obstáculos agotan la energía; minijuego completo dentro del juego.

### P14 — Menús, HUD y transiciones
- **Archivos:** `game.rs` (modelo de interfaz), `ui.rs` (raylib 2D).
- Menú principal, controles, créditos, pausa, nave destruida, HUD de nave y
  navegación, radar, marcadores de objetivo, avisos, panel `F3`.
- **Corrección:** textos que se salían del panel y una quinta línea cortada
  en el minijuego de los osos.

### P15 — Optimización
- Culling por frustum y por tamaño, LOD en planetas, nave, rocas y
  decoraciones, decoraciones solo si el planeta ocupa más de 25 px, visibility
  buffer (un shader por píxel), vertex stage y raster multihilo, descarte de
  triángulos subpíxel.
- **Medición inicial:** 50,7 FPS de media en 14 escenas a 960x540 con 4 hilos
  (mínimo 25,6 FPS). La medición actual está en P19.

### P16 — Integración y pruebas funcionales
- Pruebas de estados del juego, warp a los seis planetas, colisiones,
  minijuego completo y render de un cuadro con HUD.
- Pruebas visuales: 21 capturas reales de la ventana con Xvfb en
  `docs/capturas/` y capturas sin ventana con `--captura`.

### P17 — Auditoría de la rúbrica
- `RUBRICA_ESTADO.md` con requisito, puntos, estado, archivo responsable,
  evidencia y estimación.

### P18 — Documentación y entrega
- `LEEME.md` actualizado, guía del video en `docs/VIDEO_DEMO.md`.
- **Pendiente:** grabar y enlazar el video, y completar los datos de los
  integrantes.

### P19 — Calidad visual y escalado multinúcleo
- Resolución dinámica automática entre 896x504 y 1280x720, con inicio en
  1152x648 y objetivo fijo de 60 FPS; no requiere opciones del usuario.
- Baja de nivel tras 8 cuadros pesados y solo sube después de 300 cuadros con
  margen, evitando tirones y oscilaciones visibles. El HUD permanece a la
  resolución completa de la ventana.
- Detección de todos los procesadores lógicos (20 en el equipo medido),
  bandas de 12 filas para mejorar el balance y límite de granularidad para
  no crear hilos de vertex stage con demasiado poco trabajo.
- Binario `release` con LTO completo, una unidad de generación de código e
  instrucciones SIMD de la CPU local. Límite de FPS ajustado al monitor.
- HUD con paneles y barras redondeadas, sombras y brillo; `F3` ahora muestra
  resolución e hilos activos.
- **Medido:** las 14 escenas superan 60 FPS (mínimo 66,9; promedio 86,9)
  usando el mismo controlador dinámico que la ventana interactiva.
- **Pruebas:** 80/80 pruebas pasan en `release`, incluidas dos del controlador
  de resolución.
