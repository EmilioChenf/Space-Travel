# Space Travel — renderizador por software en Rust

Base del proyecto final de Gráficas por Computadora, escrita en Rust. La escena
3D se transforma, rasteriza, ilumina y prueba contra profundidad completamente
en CPU. Raylib se usa únicamente para la ventana, el teclado, el texto de
depuración y para copiar el framebuffer a la pantalla.

## Lo que ya funciona

- Carga propia de modelos OBJ/MTL para el DeLorean, Oasis y Hielo.
- Pipeline modelo → vista → proyección y rasterización por coordenadas baricéntricas.
- Interpolación con corrección de perspectiva y z-buffer.
- Iluminación CPU que conserva los materiales exportados desde Blender.
- Shader CPU procedural y animado para el sol.
- Planetas Oasis y Hielo con órbitas, rotaciones e iluminación diferenciadas.
- Cámara en tercera persona, vuelo con aceleración/frenado y colisión esférica básica.
- Cuatro propulsores procedurales bajo las ruedas, con llamas animadas e
  iluminación azul sobre la nave.
- Contadores de triángulos y píxeles para facilitar la depuración.

## Compilar en Windows

Necesitas Rust (rustup/Cargo), CMake y un compilador C instalado porque las
dependencias nativas de raylib se compilan durante la primera ejecución.

Desde una terminal en la raíz del repositorio:

```powershell
cargo run --release
```

Cargo descarga y compila los bindings de raylib la primera vez. Las siguientes
compilaciones son incrementales.

## Controles

| Tecla | Acción |
|---|---|
| `W` / `S` | Acelerar / frenar y retroceder |
| `A` / `D` | Girar a izquierda / derecha |
| `↑` / `↓` | Inclinar la nariz |
| `Q` / `E` | Alabeo |
| `Espacio` | Frenar rápidamente sin entrar en reversa |
| `Shift izquierdo` | Aumentar la velocidad máxima |
| `R` | Reiniciar nave |

## Estructura

- `src/math3d.rs`: vectores, matrices, cámara y transformaciones.
- `src/model.rs`: lector OBJ/MTL y geometría procedural reutilizable.
- `src/renderer.rs`: framebuffer, z-buffer, rasterizador y shaders CPU.
- `src/main.rs`: escena, órbita, controles, cámara y presentación con raylib.
- `src/assets.rs`: rutas centralizadas y localización de recursos.
- `recursos/modelos/nave_espacial/delorean/`: `delorean.obj` y `delorean.mtl`.
- `recursos/modelos/planetas/oasis/`: `oasis.obj` y `oasis.mtl`.
- `recursos/modelos/planetas/hielo/`: `Hielo.obj` y `Hielo.mtl`.

## Próximos pasos sugeridos

1. Añadir clipping de triángulos contra el plano cercano.
2. Integrar los planetas restantes con identidades visuales diferentes.
3. Añadir frustum culling y niveles de detalle, usando los contadores actuales
   para demostrar la reducción de trabajo.
4. Completar aquí los integrantes, contribuciones, recursos externos y asistencia
   de IA antes de la entrega.
