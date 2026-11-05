# Space Travel — renderizador por software en Rust

Base del proyecto final de Gráficas por Computadora, escrita en Rust. La escena
3D se transforma, rasteriza, ilumina y prueba contra profundidad completamente
en CPU. Raylib se usa únicamente para la ventana, el teclado, el texto de
depuración y para copiar el framebuffer a la pantalla.

## Lo que ya funciona

- Carga propia de modelos OBJ/MTL (incluye el DeLorean en
  `recursos/modelos/nave_espacial/delorean/`).
- Pipeline modelo → vista → proyección y rasterización por coordenadas baricéntricas.
- Interpolación con corrección de perspectiva y z-buffer.
- Shader CPU metálico para la nave.
- Shader CPU procedural y animado para el sol.
- Planeta procedural que orbita al sol y rota según el tiempo transcurrido.
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
| `Shift izquierdo` | Aumentar la velocidad máxima |
| `R` | Reiniciar nave |

## Estructura

- `src/math3d.rs`: vectores, matrices, cámara y transformaciones.
- `src/model.rs`: lector OBJ/MTL y esfera generada proceduralmente.
- `src/renderer.rs`: framebuffer, z-buffer, rasterizador y shaders CPU.
- `src/main.rs`: escena, órbita, controles, cámara y presentación con raylib.
- `src/assets.rs`: rutas centralizadas y localización de recursos.
- `recursos/modelos/nave_espacial/delorean/`: modelo OBJ y material MTL de la nave.

## Próximos pasos sugeridos

1. Añadir clipping de triángulos contra el plano cercano.
2. Crear un segundo planeta con identidad visual y geometría diferente.
3. Colocar objetos propios sobre la superficie curva del planeta.
4. Añadir frustum culling y niveles de detalle, usando los contadores actuales
   para demostrar la reducción de trabajo.
5. Completar aquí los integrantes, contribuciones, recursos externos y asistencia
   de IA antes de la entrega.
