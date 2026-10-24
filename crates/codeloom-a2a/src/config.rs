//! Sección `[a2a]` de `codeloom.toml`: qué papel desempeña este proceso en A2A y con qué cotas.
//!
//! Cabe señalar que, sin la sección, el rol es `off` y no se construye ningún cliente ni se abre
//! ningún puerto. Los remotos concretos no viven aquí sino en perfiles `.md` (A8a, T3); este
//! módulo guarda solo el rol y los límites.

use codeloom_core::error::{CoreError, Result};
use serde::Deserialize;

use crate::serve::config::ServeConfig;

/// Papel del proceso en A2A. Por defecto, apagado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum A2aRole {
    #[default]
    Off,
    /// Expone agentes locales a otros clientes (A8b; exige A8d).
    Serve,
    /// Delega en agentes remotos ya aprobados (A8a).
    Consume,
    Both,
}

/// Cota por defecto de la tarjeta remota: 64 KiB. Una tarjeta real cabe de sobra.
pub const MAX_CARD_BYTES_POR_DEFECTO: usize = 64 * 1024;
/// Cota por defecto del cuerpo de cualquier respuesta remota: 256 KiB.
pub const MAX_RESULT_BYTES_POR_DEFECTO: usize = 256 * 1024;
/// Cada cuánto se refrescan por defecto las tarjetas de los remotos, en segundos (una hora).
pub const CARD_REFRESH_SECS_POR_DEFECTO: u64 = 3600;
/// Suelo del intervalo de refresco: nadie quiere martillear a un remoto.
pub const CARD_REFRESH_SECS_MINIMO: u64 = 60;
/// Tiempo máximo por defecto de una petición remota.
pub const REQUEST_TIMEOUT_SECS_POR_DEFECTO: u64 = 30;
/// Techo de cada cota: lo configurado nunca lo supera (decálogo: nada sin cota).
const TECHO_BYTES: usize = 8 * 1024 * 1024;
const TECHO_SEGUNDOS: u64 = 600;

/// Sección opcional `[a2a]`. Vive aparte de la configuración principal: su ausencia no la afecta.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct A2aConfig {
    pub role: A2aRole,
    /// Tiempo máximo de cada petición HTTP a un remoto, en segundos.
    pub request_timeout_secs: u64,
    /// Tamaño máximo, en bytes, de una tarjeta remota.
    pub max_card_bytes: usize,
    /// Tamaño máximo, en bytes, del cuerpo de una respuesta remota (tarea o resultado).
    pub max_result_bytes: usize,
    /// Cada cuánto se vuelven a descargar las tarjetas de los remotos, en segundos; `0` lo desactiva
    /// y un valor menor que el suelo se sube al suelo.
    pub card_refresh_secs: u64,
    /// Sección `[a2a.serve]`: solo se usa con el rol `serve` o `both` (A8b).
    pub serve: ServeConfig,
}

impl Default for A2aConfig {
    fn default() -> Self {
        A2aConfig {
            role: A2aRole::Off,
            request_timeout_secs: REQUEST_TIMEOUT_SECS_POR_DEFECTO,
            max_card_bytes: MAX_CARD_BYTES_POR_DEFECTO,
            max_result_bytes: MAX_RESULT_BYTES_POR_DEFECTO,
            card_refresh_secs: CARD_REFRESH_SECS_POR_DEFECTO,
            serve: ServeConfig::default(),
        }
    }
}

/// Forma del fichero a efectos de `[a2a]`; el resto de claves se ignora.
#[derive(Deserialize)]
struct FicheroConA2a {
    #[serde(default)]
    a2a: A2aConfig,
}

impl A2aConfig {
    /// Lee la sección `[a2a]` de un texto TOML. Su ausencia da los valores por defecto; una cota
    /// en cero o por encima del techo es un error, no un valor corregido en silencio.
    pub fn from_toml(texto: &str) -> Result<A2aConfig> {
        let config = toml::from_str::<FicheroConA2a>(texto)
            .map(|fichero| fichero.a2a)
            .map_err(|e| CoreError::Config(e.to_string()))?;
        config.validar()?;
        Ok(config)
    }

    /// Lee la sección `[a2a]` del fichero `path`. Un fichero ausente es un error, como en las demás
    /// secciones; quien llama ya cargó la configuración principal del mismo fichero.
    pub fn load(path: &std::path::Path) -> Result<A2aConfig> {
        let texto = std::fs::read_to_string(path)
            .map_err(|e| CoreError::Config(format!("read {}: {e}", path.display())))?;
        Self::from_toml(&texto)
    }

    fn validar(&self) -> Result<()> {
        let segundos = 1..=TECHO_SEGUNDOS;
        if !segundos.contains(&self.request_timeout_secs) {
            return Err(CoreError::Config(format!(
                "a2a.request_timeout_secs debe estar entre 1 y {TECHO_SEGUNDOS}"
            )));
        }
        for (clave, valor) in [
            ("max_card_bytes", self.max_card_bytes),
            ("max_result_bytes", self.max_result_bytes),
        ] {
            if valor == 0 || valor > TECHO_BYTES {
                return Err(CoreError::Config(format!(
                    "a2a.{clave} debe estar entre 1 y {TECHO_BYTES}"
                )));
            }
        }
        self.serve.validar_cotas()?;
        Ok(())
    }

    /// `true` si este proceso puede delegar en remotos.
    /// Intervalo del refresco periódico de tarjetas, o `None` si está desactivado.
    pub fn card_refresh_interval(&self) -> Option<std::time::Duration> {
        (self.card_refresh_secs > 0).then(|| {
            std::time::Duration::from_secs(self.card_refresh_secs.max(CARD_REFRESH_SECS_MINIMO))
        })
    }

    pub fn consumes(&self) -> bool {
        matches!(self.role, A2aRole::Consume | A2aRole::Both)
    }

    /// `true` si este proceso expone agentes (A8b).
    pub fn serves(&self) -> bool {
        matches!(self.role, A2aRole::Serve | A2aRole::Both)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_the_section_the_role_is_off_and_nothing_consumes_or_serves() {
        let config = A2aConfig::from_toml("model = \"x\"\n").unwrap();

        assert_eq!(config.role, A2aRole::Off);
        assert!(!config.consumes());
        assert!(!config.serves());
        assert_eq!(config, A2aConfig::default());
    }

    #[test]
    fn the_card_refresh_interval_is_hourly_off_at_zero_and_has_a_floor() {
        assert_eq!(
            A2aConfig::default().card_refresh_interval(),
            Some(std::time::Duration::from_secs(3600))
        );
        for (texto, esperado) in [("0", None), ("5", Some(60)), ("120", Some(120))] {
            let config = A2aConfig::from_toml(&format!(
                "[a2a]
card_refresh_secs = {texto}
"
            ))
            .unwrap();
            assert_eq!(
                config.card_refresh_interval(),
                esperado.map(std::time::Duration::from_secs),
                "{texto}"
            );
        }
    }

    #[test]
    fn each_role_enables_exactly_its_side() {
        for (texto, consume, sirve) in [
            ("off", false, false),
            ("consume", true, false),
            ("serve", false, true),
            ("both", true, true),
        ] {
            let config = A2aConfig::from_toml(&format!("[a2a]\nrole = \"{texto}\"\n")).unwrap();
            assert_eq!(config.consumes(), consume, "{texto}");
            assert_eq!(config.serves(), sirve, "{texto}");
        }
    }

    #[test]
    fn limits_are_read_and_the_rest_keep_their_defaults() {
        let config = A2aConfig::from_toml(
            "[a2a]\nrole = \"consume\"\nrequest_timeout_secs = 5\nmax_card_bytes = 1024\n",
        )
        .unwrap();

        assert_eq!(config.request_timeout_secs, 5);
        assert_eq!(config.max_card_bytes, 1024);
        assert_eq!(config.max_result_bytes, MAX_RESULT_BYTES_POR_DEFECTO);
    }

    #[test]
    fn a_zero_or_oversized_limit_is_an_error_and_not_silently_fixed() {
        for texto in [
            "[a2a]\nrequest_timeout_secs = 0\n",
            "[a2a]\nrequest_timeout_secs = 601\n",
            "[a2a]\nmax_card_bytes = 0\n",
            "[a2a]\nmax_result_bytes = 8388609\n",
        ] {
            assert!(A2aConfig::from_toml(texto).is_err(), "{texto}");
        }
    }

    #[test]
    fn an_unknown_role_is_an_error() {
        assert!(A2aConfig::from_toml("[a2a]\nrole = \"everything\"\n").is_err());
    }

    #[test]
    fn load_reads_the_section_from_a_file_and_fails_without_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("codeloom.toml");
        std::fs::write(
            &ruta,
            "[a2a]
role = \"consume\"
",
        )
        .unwrap();

        assert!(A2aConfig::load(&ruta).unwrap().consumes());
        assert!(A2aConfig::load(&dir.path().join("no-existe.toml")).is_err());
    }
}
