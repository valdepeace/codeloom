//! Rutas de fichero como texto: partirlas en directorio y nombre, y recortarlas sin perder el
//! nombre. Puro; lo comparten la cabecera de las herramientas y el panel de ficheros.

use unicode_segmentation::UnicodeSegmentation;

/// Parte `path` en directorio (con su separador final) y nombre. Admite `/` y `\`.
pub fn split_path(path: &str) -> (&str, &str) {
    match path.rfind(['/', '\\']) {
        Some(at) if at + 1 < path.len() => path.split_at(at + 1),
        _ => ("", path),
    }
}

/// Recorta una ruta a `max` grafemas por la izquierda, de modo que el nombre del fichero se
/// conserve siempre: `…/src/render.rs`. Si ni el nombre cabe, se recorta él por la izquierda.
pub fn truncate_path(path: &str, max: usize) -> String {
    let graphemes: Vec<&str> = path.graphemes(true).collect();
    if graphemes.len() <= max {
        return path.to_string();
    }
    let keep = max.saturating_sub(1);
    let tail: String = graphemes[graphemes.len() - keep..].concat();
    // Se empieza en un separador para no dejar a medias el nombre de un directorio.
    let tail = match tail.find(['/', '\\']) {
        Some(at) if at + 1 < tail.len() => &tail[at..],
        _ => tail.as_str(),
    };
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_path_separates_directory_and_name() {
        assert_eq!(split_path("src/lib.rs"), ("src/", "lib.rs"));
        assert_eq!(split_path(r"src\main.rs"), (r"src\", "main.rs"));
        assert_eq!(split_path("README.md"), ("", "README.md"));
        // Una ruta que acaba en separador no tiene nombre que resaltar: se deja entera.
        assert_eq!(split_path("src/"), ("", "src/"));
    }

    #[test]
    fn truncate_path_keeps_short_paths_and_cuts_a_huge_name_from_the_left() {
        assert_eq!(truncate_path("src/lib.rs", 80), "src/lib.rs");
        assert_eq!(truncate_path("a/b/c/d/lib.rs", 10), "…/d/lib.rs");
        let nombre = "n".repeat(30);
        let cortado = truncate_path(&nombre, 10);
        assert_eq!(cortado.graphemes(true).count(), 10);
        assert!(cortado.starts_with('…'));
    }
}
