# Guía para grabar el video de demostración

El video es **obligatorio**. Duración sugerida: 4 a 6 minutos. Graba la
ventana a 1280x720 o más con OBS Studio (Captura de ventana), con el juego
compilado en `--release`:

```bash
cargo run --release
```

Antes de grabar, pulsa `F3` una vez para mostrar las estadísticas de
rendimiento cuando hables de la parte técnica.

## Guion

| # | Tiempo | Qué mostrar | Cómo |
|---|---|---|---|
| 1 | 0:00 | Menú principal, Controles y Créditos | Flechas + Enter; Esc para volver |
| 2 | 0:20 | Inicio y despegue: cuenta regresiva, motores, ascenso, transición, apertura de la cofia y salida del DeLorean | "Iniciar partida" y **no** saltar la cinemática |
| 3 | 0:45 | Navegación libre: acelerar, girar, cabeceo, alabeo, lateral, subir/bajar, impulso, freno | `W A S D`, flechas, `Q E`, `R F`, `Shift`, `Espacio` |
| 4 | 1:15 | Órbitas y rotación: vista general con las órbitas visibles | Alejarse con `S`/zoom; `O` alterna órbitas |
| 5 | 1:30 | Acercamiento a un planeta sin pantalla de carga | `T` para elegir objetivo y volar hacia él (o `3` para warp a Oasis) |
| 6 | 1:50 | Vuelo alrededor del planeta a ras de suelo; mostrar que no se atraviesa | Volar bajo con `F`; la velocidad se limita sola |
| 7 | 2:10 | Alejamiento y viaje a otro planeta | `W` + `Shift`; luego warp `4` (Fuego) |
| 8 | 2:30 | Shaders y animaciones: lava, hielo (`2`), dulces (`5`), bosque (`6`), Tierra con nubes (`1`), Sol | Warp a cada uno |
| 9 | 3:10 | Cambio de cámaras: tercera, primera persona y orbital (con ratón y zoom) | `C`, arrastrar el ratón, rueda |
| 10 | 3:30 | Minijuegos: completar uno entero (Cristales perdidos o Tres osos) y mostrar el inicio de los demás | `M`, `Enter`, `R` para reintentar |
| 11 | 4:30 | Lluvia de meteoritos e impactos | Esperar el aviso "LLUVIA DE METEORITOS!" en el HUD |
| 12 | 4:50 | Parte técnica: estadísticas `F3` (culling, LOD, triángulos recortados, tiempos) y explicar el pipeline por software | Panel `F3` |
| 13 | 5:20 | Pausa y salida | `Esc` → Volver al menú → Salir |

## Recomendaciones

- Menciona que raylib solo muestra el framebuffer y que todo el 3D se
  calcula en la CPU.
- Nombra las técnicas de renderizado mientras se ven: recorte, Z-buffer,
  frustum culling, LOD, shaders.
- Sube el video (YouTube no listado o Drive) y pon el enlace en el
  `LEEME.md`.
