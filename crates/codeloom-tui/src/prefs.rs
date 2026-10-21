//! Preferencias de la interfaz que sobreviven entre sesiones (el tema y la
//! captura del ratón).
//!
//! Este fichero pertenece a la TUI y no forma parte de `CodeLoomConfig`. Su
//! formato es un TOML mínimo: `theme = "<clave>"` y `mouse = true|false`. Al
//! guardar se conservan las demás líneas del fichero tal cual, sin
//! interpretarlas; por eso no se usa un parser TOML completo ni se añade
//! ninguna dependencia nueva.

use std::io;
use std::path::Path;

use batten::widgets::ThemePreset;

const THEME_KEY: &str = "theme";
const MOUSE_KEY: &str = "mouse";

/// Identificador estable y en minúsculas de cada preset, tal como se escribe en el fichero.
fn key(preset: ThemePreset) -> &'static str {
    match preset {
        ThemePreset::Batten => "batten",
        ThemePreset::Martty => "martty",
        ThemePreset::Mindfork => "mindfork",
        ThemePreset::Pawan => "pawan",
        ThemePreset::Crush => "crush",
        ThemePreset::MastraCode => "mastracode",
    }
}

fn from_key(value: &str) -> Option<ThemePreset> {
    ThemePreset::ALL
        .into_iter()
        .find(|preset| key(*preset) == value)
}

/// Indica si una línea asigna la clave `name` (`name = ...`).
fn is_assignment_of(line: &str, name: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(left, _)| left.trim() == name)
}

/// Lee el valor crudo (sin espacios) de la primera línea que asigna `name` y
/// lo interpreta con `parse`. Un fichero inexistente, ilegible o con un valor
/// no reconocido produce `None`; nunca entra en pánico.
fn load_value<T>(path: &Path, name: &str, parse: impl Fn(&str) -> Option<T>) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find_map(|line| {
        let (left, value) = line.split_once('=')?;
        if left.trim() != name {
            return None;
        }
        parse(value.trim())
    })
}

/// Escribe `name = value`, sustituyendo cualquier línea previa que asigne
/// `name` y conservando el resto de líneas. Crea el directorio padre si no
/// existe. `value` ya debe venir formateado (con comillas si es texto).
fn save_value(path: &Path, name: &str, value: &str) -> io::Result<()> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    let mut content = String::new();
    for line in existing.lines() {
        if !is_assignment_of(line, name) {
            content.push_str(line);
            content.push('\n');
        }
    }
    content.push_str(&format!("{name} = {value}\n"));

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)
}

/// Lee el tema guardado. Un fichero inexistente, ilegible, corrupto o con una
/// clave desconocida produce `None`; nunca entra en pánico.
pub fn load_theme(path: &Path) -> Option<ThemePreset> {
    load_value(path, THEME_KEY, |value| {
        from_key(value.strip_prefix('"')?.strip_suffix('"')?)
    })
}

/// Guarda el tema, conservando el resto de líneas del fichero.
pub fn save_theme(path: &Path, preset: ThemePreset) -> io::Result<()> {
    save_value(path, THEME_KEY, &format!("\"{}\"", key(preset)))
}

/// Lee la captura del ratón guardada (`mouse = true|false`). Cualquier otro
/// valor, o la ausencia de la clave, produce `None`.
pub fn load_mouse(path: &Path) -> Option<bool> {
    load_value(path, MOUSE_KEY, |value| match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

/// Guarda la captura del ratón, conservando el resto de líneas del fichero.
pub fn save_mouse(path: &Path, on: bool) -> io::Result<()> {
    save_value(path, MOUSE_KEY, if on { "true" } else { "false" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_round_trips_for_every_preset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        for preset in ThemePreset::ALL {
            save_theme(&path, preset).unwrap();
            assert_eq!(load_theme(&path), Some(preset), "preset {preset:?}");
        }
    }

    #[test]
    fn missing_or_corrupt_file_loads_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_theme(&dir.path().join("no-existe.toml")), None);

        let corrupt = dir.path().join("corrupt.toml");
        std::fs::write(&corrupt, [0xff, 0xfe, 0x00, b'=', b'"']).unwrap();
        assert_eq!(load_theme(&corrupt), None);

        let garbage = dir.path().join("garbage.toml");
        std::fs::write(&garbage, "theme = \nno es toml \"\"\"").unwrap();
        assert_eq!(load_theme(&garbage), None);
    }

    #[test]
    fn unknown_key_loads_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        std::fs::write(&path, "theme = \"solarized\"\n").unwrap();
        assert_eq!(load_theme(&path), None);
    }

    #[test]
    fn save_creates_the_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("anidado").join("ui.toml");
        save_theme(&path, ThemePreset::Crush).unwrap();
        assert_eq!(load_theme(&path), Some(ThemePreset::Crush));
    }

    #[test]
    fn save_keeps_other_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        std::fs::write(
            &path,
            "# preferencias\nmodo = \"denso\"\ntheme = \"pawan\"\n",
        )
        .unwrap();
        save_theme(&path, ThemePreset::Crush).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# preferencias\n"));
        assert!(text.contains("modo = \"denso\"\n"));
        assert_eq!(text.matches("theme").count(), 1);
        assert_eq!(load_theme(&path), Some(ThemePreset::Crush));
    }

    #[test]
    fn mouse_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        for on in [true, false, true] {
            save_mouse(&path, on).unwrap();
            assert_eq!(load_mouse(&path), Some(on), "mouse {on}");
        }
    }

    #[test]
    fn mouse_and_theme_keep_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        save_theme(&path, ThemePreset::Pawan).unwrap();
        save_mouse(&path, false).unwrap();
        assert_eq!(load_theme(&path), Some(ThemePreset::Pawan));
        assert_eq!(load_mouse(&path), Some(false));

        save_theme(&path, ThemePreset::Crush).unwrap();
        assert_eq!(load_mouse(&path), Some(false));
        assert_eq!(load_theme(&path), Some(ThemePreset::Crush));

        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("mouse").count(), 1);
        assert_eq!(text.matches("theme").count(), 1);
    }

    #[test]
    fn mouse_missing_or_garbage_loads_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_mouse(&dir.path().join("no-existe.toml")), None);

        for (name, content) in [
            ("vacio.toml", "mouse = \n"),
            ("texto.toml", "mouse = \"true\"\n"),
            ("mayus.toml", "mouse = True\n"),
            ("numero.toml", "mouse = 1\n"),
            ("otra.toml", "mouse_mode = true\n"),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, content).unwrap();
            assert_eq!(load_mouse(&path), None, "contenido {content:?}");
        }
    }
}
