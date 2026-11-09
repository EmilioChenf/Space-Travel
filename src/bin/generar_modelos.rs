//! Genera los modelos OBJ/MTL procedurales en `recursos/modelos`.
//!
//! Uso: `cargo run --release --bin generar_modelos`
//!
//! No modifica los modelos originales (DeLorean, Hielo.obj, oasis.obj).

use space_travel::worlds::generate_all;
use std::path::Path;

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("recursos/modelos");
    let report = generate_all(&root)?;
    let mut total = 0;
    for (path, triangles) in &report {
        println!("{triangles:>8} triángulos  recursos/modelos/{path}");
        total += triangles;
    }
    println!(
        "{} modelos generados, {total} triángulos en total.",
        report.len()
    );
    Ok(())
}
