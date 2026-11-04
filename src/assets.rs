use std::path::{Path, PathBuf};

/// Ruta del modelo de la nave relativa a la raíz del proyecto.
pub const DELOREAN_OBJ: &str = "recursos/modelos/nave_espacial/delorean/delorean.obj";

/// Resuelve un recurso desde el directorio actual o desde la raíz conocida por
/// Cargo. Esto permite ejecutar el proyecto con `cargo run` y también desde la
/// mayoría de editores sin duplicar rutas por el código.
pub fn locate(relative_path: impl AsRef<Path>) -> PathBuf {
    let relative_path = relative_path.as_ref();
    let from_working_directory = PathBuf::from(relative_path);
    if from_working_directory.exists() {
        return from_working_directory;
    }

    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_delorean_model() {
        assert!(locate(DELOREAN_OBJ).is_file());
    }
}
