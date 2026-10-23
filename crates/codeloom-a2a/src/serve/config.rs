//! Sección `[a2a.serve]`: dónde escucha el servidor, a quién deja pasar y con qué cotas.
//!
//! Cabe señalar que sin autenticación configurada y resuelta (la variable de entorno del secreto
//! existe y no está vacía) el servidor **no arranca**; y que una dirección de escucha que no sea de
//! bucle local es un error de configuración, pues el proceso no habla TLS (§13.4).

use std::net::SocketAddr;

use codeloom_core::error::{CoreError, Result};
use reqwest::header::HeaderName;
use serde::Deserialize;

use crate::security::Secret;

pub const LISTEN_POR_DEFECTO: &str = "127.0.0.1:7077";
pub const AGENTE_POR_DEFECTO: &str = "default";
pub const TAREAS_SIMULTANEAS_POR_DEFECTO: usize = 4;
pub const ESPERA_POR_DEFECTO_SECS: u64 = 30;
pub const TAREA_MAX_POR_DEFECTO_SECS: u64 = 600;
const TECHO_TAREAS: usize = 64;
const TECHO_ESPERA_SECS: u64 = 300;
const TECHO_TAREA_SECS: u64 = 4 * 3600;
const MAX_NOMBRE: usize = 200;

/// Esquema con el que se autentican los clientes. Los valores del TOML son `bearer` y `apikey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServeAuthKind {
    Bearer,
    #[serde(rename = "apikey")]
    ApiKey,
}

/// `auth = { scheme = "bearer", env = "CODELOOM_A2A_TOKEN" }` (o `apikey` con `name`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServeAuthConfig {
    pub scheme: ServeAuthKind,
    /// Nombre de la cabecera de la clave de API (solo con `apikey`).
    #[serde(default)]
    pub name: Option<String>,
    /// Variable de entorno que guarda el secreto. Nunca el secreto en claro.
    pub env: String,
}

/// Esquema ya resuelto, sin el secreto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServeAuthScheme {
    Bearer,
    ApiKey { name: String },
}

/// Autenticación lista para usar: el esquema y el secreto leído del entorno.
#[derive(Debug)]
pub struct ResolvedAuth {
    pub scheme: ServeAuthScheme,
    pub(crate) secret: Secret,
}

/// Sección `[a2a.serve]`. Un campo desconocido es un error: una errata en seguridad no se ignora.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServeConfig {
    pub listen: String,
    pub agent: String,
    pub auth: Option<ServeAuthConfig>,
    pub public_url: Option<String>,
    pub max_concurrent_tasks: usize,
    pub max_wait_secs: u64,
    pub max_task_secs: u64,
}

impl Default for ServeConfig {
    fn default() -> Self {
        ServeConfig {
            listen: LISTEN_POR_DEFECTO.to_string(),
            agent: AGENTE_POR_DEFECTO.to_string(),
            auth: None,
            public_url: None,
            max_concurrent_tasks: TAREAS_SIMULTANEAS_POR_DEFECTO,
            max_wait_secs: ESPERA_POR_DEFECTO_SECS,
            max_task_secs: TAREA_MAX_POR_DEFECTO_SECS,
        }
    }
}

fn error(texto: impl Into<String>) -> CoreError {
    CoreError::Config(texto.into())
}

impl ServeConfig {
    /// Cotas dentro de su rango. Se comprueba siempre, también con el rol apagado, para que una
    /// errata aparezca al editar y no el día que se active.
    pub(crate) fn validar_cotas(&self) -> Result<()> {
        if !(1..=TECHO_TAREAS).contains(&self.max_concurrent_tasks) {
            return Err(error(format!(
                "a2a.serve.max_concurrent_tasks debe estar entre 1 y {TECHO_TAREAS}"
            )));
        }
        if !(1..=TECHO_ESPERA_SECS).contains(&self.max_wait_secs) {
            return Err(error(format!(
                "a2a.serve.max_wait_secs debe estar entre 1 y {TECHO_ESPERA_SECS}"
            )));
        }
        if !(1..=TECHO_TAREA_SECS).contains(&self.max_task_secs) {
            return Err(error(format!(
                "a2a.serve.max_task_secs debe estar entre 1 y {TECHO_TAREA_SECS}"
            )));
        }
        if self.agent.trim().is_empty() || self.agent.len() > MAX_NOMBRE {
            return Err(error("a2a.serve.agent debe ser un nombre no vacío"));
        }
        Ok(())
    }

    /// Dirección de escucha: una IP literal **de bucle local**. Un nombre de anfitrión o una
    /// dirección no local se rechazan: el proceso no habla TLS, y quien quiera exponerlo ha de
    /// ponerle delante un proxy que sí lo haga y declarar `public_url`.
    pub fn listen_addr(&self) -> Result<SocketAddr> {
        let direccion: SocketAddr = self.listen.parse().map_err(|_| {
            error(format!(
                "a2a.serve.listen «{}» debe ser una IP literal con puerto, p. ej. 127.0.0.1:7077",
                self.listen
            ))
        })?;
        if !direccion.ip().is_loopback() {
            return Err(error(format!(
                "a2a.serve.listen «{}» no es de bucle local: CodeLoom no habla TLS; \
                 ponle delante un proxy TLS y declara a2a.serve.public_url",
                self.listen
            )));
        }
        Ok(direccion)
    }

    /// URL pública de la interfaz que anuncia la tarjeta: `public_url` o la dirección de escucha.
    pub fn interface_url(&self) -> Result<String> {
        match &self.public_url {
            Some(url) => Ok(url.trim_end_matches('/').to_string()),
            None => Ok(format!("http://{}/a2a", self.listen_addr()?)),
        }
    }

    /// Valida todo lo necesario para arrancar y resuelve el secreto con `env`. Falla si falta la
    /// autenticación o su variable, si la dirección no es de bucle local o si `public_url` no es
    /// admisible.
    pub fn validate(&self, env: &dyn Fn(&str) -> Option<String>) -> Result<ResolvedAuth> {
        self.validar_cotas()?;
        self.listen_addr()?;
        if let Some(url) = &self.public_url {
            codeloom_core::config::validate_base_url(url).map_err(|motivo| {
                error(format!("a2a.serve.public_url no es admisible: {motivo}"))
            })?;
        }
        let Some(auth) = &self.auth else {
            return Err(error(
                "a2a.serve necesita `auth`: sin autenticación el servidor no arranca",
            ));
        };
        let scheme = match (auth.scheme, &auth.name) {
            (ServeAuthKind::Bearer, _) => ServeAuthScheme::Bearer,
            (ServeAuthKind::ApiKey, Some(nombre))
                if HeaderName::from_bytes(nombre.as_bytes()).is_ok() =>
            {
                ServeAuthScheme::ApiKey {
                    name: nombre.clone(),
                }
            }
            (ServeAuthKind::ApiKey, _) => {
                return Err(error(
                    "a2a.serve.auth con `apikey` necesita `name`, un nombre de cabecera válido",
                ))
            }
        };
        let Some(secret) = env(&auth.env).and_then(Secret::new) else {
            return Err(error(format!(
                "falta la variable de entorno {} (secreto de a2a.serve.auth): el servidor no arranca sin ella",
                auth.env
            )));
        };
        Ok(ResolvedAuth { scheme, secret })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::A2aConfig;

    fn con_auth() -> ServeConfig {
        ServeConfig {
            auth: Some(ServeAuthConfig {
                scheme: ServeAuthKind::Bearer,
                name: None,
                env: "TOKEN_LOOM".into(),
            }),
            ..ServeConfig::default()
        }
    }

    fn entorno(valor: Option<&'static str>) -> impl Fn(&str) -> Option<String> {
        move |variable| {
            if variable == "TOKEN_LOOM" {
                valor.map(str::to_string)
            } else {
                None
            }
        }
    }

    #[test]
    fn the_defaults_listen_on_loopback_with_small_limits_and_no_auth() {
        let config = A2aConfig::from_toml("[a2a]\nrole = \"serve\"\n").unwrap();

        assert_eq!(config.serve, ServeConfig::default());
        assert_eq!(config.serve.listen, "127.0.0.1:7077");
        assert_eq!(config.serve.max_concurrent_tasks, 4);
        assert!(config.serve.auth.is_none());
    }

    #[test]
    fn both_auth_schemes_parse() {
        let bearer = A2aConfig::from_toml(
            "[a2a.serve]\nauth = { scheme = \"bearer\", env = \"TOKEN_LOOM\" }\n",
        )
        .unwrap();
        assert_eq!(bearer.serve.auth.unwrap().scheme, ServeAuthKind::Bearer);

        let clave = A2aConfig::from_toml(
            "[a2a.serve]\nauth = { scheme = \"apikey\", name = \"X-Api-Key\", env = \"K\" }\n",
        )
        .unwrap();
        let auth = clave.serve.auth.unwrap();
        assert_eq!(auth.scheme, ServeAuthKind::ApiKey);
        assert_eq!(auth.name.as_deref(), Some("X-Api-Key"));
    }

    #[test]
    fn an_unknown_key_in_the_serve_section_is_an_error_not_ignored() {
        for texto in [
            "[a2a.serve]\nlisten_on = \"127.0.0.1:1\"\n",
            "[a2a.serve]\nauth = { scheme = \"bearer\", env = \"T\", token = \"secreto\" }\n",
            "[a2a.serve]\nauth = { scheme = \"digest\", env = \"T\" }\n",
        ] {
            assert!(A2aConfig::from_toml(texto).is_err(), "{texto}");
        }
    }

    #[test]
    fn a_listen_address_that_is_not_loopback_is_a_configuration_error() {
        for malo in [
            "0.0.0.0:7077",
            "192.0.2.1:7077",
            "[::]:7077",
            "localhost:7077",
            "loom.example:7077",
            "7077",
            "",
        ] {
            let config = ServeConfig {
                listen: malo.into(),
                ..con_auth()
            };
            assert!(config.listen_addr().is_err(), "{malo}");
            assert!(config.validate(&entorno(Some("s"))).is_err(), "{malo}");
        }
        for bueno in ["127.0.0.1:0", "127.0.0.1:7077", "[::1]:7077"] {
            let config = ServeConfig {
                listen: bueno.into(),
                ..con_auth()
            };
            assert!(config.listen_addr().is_ok(), "{bueno}");
        }
    }

    #[test]
    fn without_auth_or_without_its_secret_the_server_does_not_start() {
        let sin_auth = ServeConfig::default();
        let error = sin_auth.validate(&entorno(Some("s"))).unwrap_err();
        assert!(error.to_string().contains("auth"), "{error}");

        for valor in [None, Some(""), Some("   ")] {
            let error = con_auth().validate(&entorno(valor)).unwrap_err();
            assert!(error.to_string().contains("TOKEN_LOOM"), "{error}");
        }
    }

    #[test]
    fn an_api_key_scheme_needs_a_valid_header_name() {
        for nombre in [None, Some("con espacio"), Some("")] {
            let config = ServeConfig {
                auth: Some(ServeAuthConfig {
                    scheme: ServeAuthKind::ApiKey,
                    name: nombre.map(str::to_string),
                    env: "TOKEN_LOOM".into(),
                }),
                ..ServeConfig::default()
            };
            assert!(config.validate(&entorno(Some("s"))).is_err(), "{nombre:?}");
        }
    }

    #[test]
    fn a_resolved_auth_never_shows_its_secret() {
        let resuelta = con_auth()
            .validate(&entorno(Some("super-secreto")))
            .unwrap();

        assert_eq!(resuelta.scheme, ServeAuthScheme::Bearer);
        assert!(!format!("{resuelta:?}").contains("super-secreto"));
    }

    #[test]
    fn limits_out_of_range_are_errors() {
        for texto in [
            "[a2a.serve]\nmax_concurrent_tasks = 0\n",
            "[a2a.serve]\nmax_concurrent_tasks = 65\n",
            "[a2a.serve]\nmax_wait_secs = 0\n",
            "[a2a.serve]\nmax_wait_secs = 301\n",
            "[a2a.serve]\nmax_task_secs = 0\n",
            "[a2a.serve]\nmax_task_secs = 14401\n",
            "[a2a.serve]\nagent = \"\"\n",
        ] {
            assert!(A2aConfig::from_toml(texto).is_err(), "{texto}");
        }
    }

    #[test]
    fn the_public_url_must_be_https_or_loopback() {
        let con = |url: &str| ServeConfig {
            public_url: Some(url.into()),
            ..con_auth()
        };
        assert!(con("https://loom.example/a2a")
            .validate(&entorno(Some("s")))
            .is_ok());
        assert!(con("http://127.0.0.1:9/a2a")
            .validate(&entorno(Some("s")))
            .is_ok());
        assert!(con("http://loom.example/a2a")
            .validate(&entorno(Some("s")))
            .is_err());
    }

    #[test]
    fn the_interface_url_is_the_public_one_or_the_listen_address() {
        assert_eq!(
            con_auth().interface_url().unwrap(),
            "http://127.0.0.1:7077/a2a"
        );
        let publica = ServeConfig {
            public_url: Some("https://loom.example/a2a/".into()),
            ..con_auth()
        };
        assert_eq!(publica.interface_url().unwrap(), "https://loom.example/a2a");
    }
}
