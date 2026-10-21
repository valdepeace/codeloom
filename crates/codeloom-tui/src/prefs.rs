//! Preferencias de la interfaz que sobreviven entre sesiones (hoy, solo el tema).
//!
//! Este fichero pertenece a la TUI y no forma parte de `CodeLoomConfig`. Su
//! formato es un TOML mínimo: `theme = "<clave>"`. Al guardar se conservan las
//! demás líneas del fichero tal cual, sin interpretarlas; por eso no se usa un
//! parser TOML completo ni se añade ninguna dependencia nueva.

use std::io;
use std::path::Path;

use batten::widgets::ThemePreset;

const THEME_KEY: &str = "theme";

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

/// Lee el tema guardado. Un fichero inexistente, ilegible, corrupto o con una
/// clave desconocida produce `None`; nunca entra en pánico.
pub fn load_theme(path: &Path) -> Option<ThemePreset> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        if name.trim() != THEME_KEY {
            return None;
        }
        let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
        from_key(value)
    })
}

/// Guarda el tema, conservando el resto de líneas del fichero. Crea el
/// directorio padre si no existe.
pub fn save_theme(path: &Path, preset: ThemePreset) -> io::Result<()> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    let mut content = String::new();
    for line in existing.lines() {
        let is_theme = line
            .split_once('=')
            .is_some_and(|(name, _)| name.trim() == THEME_KEY);
        if !is_theme {
            content.push_str(line);
            content.push('\n');
        }
    }
    content.push_str(&format!("{THEME_KEY} = \"{}\"\n", key(preset)));

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)
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
}
