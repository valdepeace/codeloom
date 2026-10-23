//! Autenticación de las peticiones entrantes (A8b): `Bearer` o clave de API en cabecera.
//!
//! Cabe señalar que el secreto jamás se compara en claro: se guarda y se compara su SHA-256, con
//! una comparación de tiempo constante, de modo que ni la longitud ni el primer byte distinto se
//! filtran por el tiempo de respuesta. Tampoco se muestra en `Debug`.

use sha2::{Digest, Sha256};

use super::config::{ResolvedAuth, ServeAuthScheme};

type Huella = [u8; 32];

fn huella(texto: &str) -> Huella {
    Sha256::digest(texto.as_bytes()).into()
}

/// Igualdad de dos huellas sin cortocircuito.
fn iguales(a: &Huella, b: &Huella) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Quien decide si una petición lleva la credencial correcta.
pub struct Authenticator {
    scheme: ServeAuthScheme,
    huella: Huella,
}

impl std::fmt::Debug for Authenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Authenticator({:?})", self.scheme)
    }
}

impl Authenticator {
    pub fn new(auth: ResolvedAuth) -> Self {
        // Un secreto leído de un fichero o de `$(cat …)` suele traer un salto de línea al final.
        Authenticator {
            huella: huella(auth.secret.expose().trim()),
            scheme: auth.scheme,
        }
    }

    pub fn scheme(&self) -> &ServeAuthScheme {
        &self.scheme
    }

    /// `true` si la petición lleva la credencial. `cabecera` devuelve el valor de una cabecera
    /// por su nombre (en minúsculas). Una credencial ausente, vacía, de otro esquema o con el
    /// secreto como mero prefijo o ampliación es siempre `false`.
    pub fn check(&self, cabecera: &dyn Fn(&str) -> Option<String>) -> bool {
        let presentada = match &self.scheme {
            ServeAuthScheme::Bearer => cabecera("authorization").and_then(|valor| {
                let (esquema, resto) = valor.split_once(' ')?;
                esquema
                    .eq_ignore_ascii_case("bearer")
                    .then(|| resto.to_string())
            }),
            ServeAuthScheme::ApiKey { name } => cabecera(&name.to_ascii_lowercase()),
        };
        match presentada {
            Some(valor) if !valor.trim().is_empty() => iguales(&huella(valor.trim()), &self.huella),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::config::{ServeAuthConfig, ServeAuthKind, ServeConfig};

    fn autenticador(
        scheme: ServeAuthKind,
        name: Option<&str>,
        secreto: &'static str,
    ) -> Authenticator {
        let config = ServeConfig {
            auth: Some(ServeAuthConfig {
                scheme,
                name: name.map(str::to_string),
                env: "S".into(),
            }),
            ..ServeConfig::default()
        };
        let resuelta = config.validate(&|_| Some(secreto.to_string())).unwrap();
        Authenticator::new(resuelta)
    }

    fn con(pares: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |nombre| {
            pares
                .iter()
                .find(|(k, _)| *k == nombre)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn bearer_accepts_only_the_exact_secret() {
        let auth = autenticador(ServeAuthKind::Bearer, None, "s3cr3to");

        assert!(auth.check(&con(&[("authorization", "Bearer s3cr3to")])));
        assert!(auth.check(&con(&[("authorization", "bearer s3cr3to")])));
        // Ausente, vacío, prefijo, ampliación, otro esquema, sin esquema.
        assert!(!auth.check(&con(&[])));
        assert!(!auth.check(&con(&[("authorization", "")])));
        assert!(!auth.check(&con(&[("authorization", "Bearer ")])));
        assert!(!auth.check(&con(&[("authorization", "Bearer s3cr3")])));
        assert!(!auth.check(&con(&[("authorization", "Bearer s3cr3to-y-mas")])));
        assert!(!auth.check(&con(&[("authorization", "Basic s3cr3to")])));
        assert!(!auth.check(&con(&[("authorization", "s3cr3to")])));
    }

    #[test]
    fn an_api_key_is_read_from_its_own_header_only() {
        let auth = autenticador(ServeAuthKind::ApiKey, Some("X-Api-Key"), "s3cr3to");

        assert!(auth.check(&con(&[("x-api-key", "s3cr3to")])));
        assert!(!auth.check(&con(&[("x-api-key", "otro")])));
        assert!(!auth.check(&con(&[("authorization", "Bearer s3cr3to")])));
        assert!(!auth.check(&con(&[])));
    }

    #[test]
    fn a_trailing_newline_in_the_configured_secret_does_not_lock_everyone_out() {
        let auth = autenticador(ServeAuthKind::Bearer, None, "s3cr3to\n");

        assert!(auth.check(&con(&[("authorization", "Bearer s3cr3to")])));
    }

    #[test]
    fn the_authenticator_never_shows_the_secret() {
        let auth = autenticador(ServeAuthKind::Bearer, None, "super-secreto");

        assert!(!format!("{auth:?}").contains("super-secreto"));
    }

    #[test]
    fn the_comparison_has_no_early_exit() {
        let a = huella("a");
        let b = huella("b");

        assert!(iguales(&a, &a));
        assert!(!iguales(&a, &b));
        // Un único bit distinto, en el último byte, también se detecta.
        let mut c = a;
        c[31] ^= 1;
        assert!(!iguales(&a, &c));
    }
}
