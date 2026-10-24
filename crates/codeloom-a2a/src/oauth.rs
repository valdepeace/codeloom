//! OAuth2 `client_credentials` y OpenID Connect hacia agentes remotos (A8d-2).
//!
//! Cabe señalar que la tarjeta del remoto es un documento de un tercero: de ella salen la dirección del
//! servidor de tokens y los ámbitos, y por eso aquí nada se fía de ellos. La dirección ha de ser HTTPS
//! (o HTTP hacia el propio equipo), sin credenciales incrustadas; no se siguen redirecciones; la
//! respuesta se acota; y el token, igual que la clave del cliente, es un secreto que no aparece en
//! `Debug`, en errores ni en registros.
//!
//! Solo se soporta el flujo `client_credentials`, el único sin persona delante. Los flujos de código,
//! dispositivo, implícito y contraseña se declaran no soportados al leer la tarjeta.

use std::fmt;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderValue, ACCEPT};
use reqwest::{Client, Url};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::remote::leer_acotado;
use crate::security::Secret;

/// Tope de la respuesta del servidor de tokens y del documento de descubrimiento.
const MAX_RESPUESTA: usize = 16 * 1024;
/// Tiempo máximo de cada petición al servidor de tokens.
const TIEMPO_MAXIMO: Duration = Duration::from_secs(15);
/// Tope del `access_token` (también debe caber en una cabecera).
const MAX_TOKEN_BYTES: usize = 8 * 1024;
/// Margen que se resta a la vida del token para no usar uno a punto de caducar.
const MARGEN: u64 = 30;
/// Vida mínima y máxima, en segundos, que se da a un token en la caché.
const VIDA_MINIMA: u64 = 30;
const VIDA_MAXIMA: u64 = 3600;
/// Vida que se supone cuando el servidor no dice `expires_in`.
const VIDA_POR_DEFECTO: u64 = 300;

/// Valida una dirección que sale de la tarjeta: HTTPS, o HTTP solo hacia el propio equipo, sin
/// usuario ni contraseña incrustados. Devuelve el motivo, ya redactado, si no vale.
pub(crate) fn url_permitida(texto: &str) -> Result<Url, String> {
    if texto.len() > 512 {
        return Err("la dirección es demasiado larga".into());
    }
    let url = Url::parse(texto).map_err(|_| "la dirección no es una URL válida".to_string())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("la dirección lleva credenciales incrustadas".into());
    }
    let Some(servidor) = url.host_str() else {
        return Err("la dirección no tiene servidor".into());
    };
    let servidor = servidor.trim_start_matches('[').trim_end_matches(']');
    let local = servidor.eq_ignore_ascii_case("localhost")
        || servidor
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    match url.scheme() {
        "https" => Ok(url),
        "http" if local => Ok(url),
        _ => Err("la dirección no es HTTPS".into()),
    }
}

/// De dónde sale la dirección del servidor de tokens.
#[derive(Debug, Clone)]
pub(crate) enum Origen {
    /// Viene en la tarjeta (`tokenUrl` de OAuth2).
    Directo(Url),
    /// Hay que descubrirla en el documento de OpenID Connect.
    Descubrir(Url),
}

struct Estado {
    endpoint: Option<Url>,
    token: Option<(HeaderValue, Instant)>,
}

/// Obtiene y cachea el token de acceso de un remoto.
pub struct TokenSource {
    http: Client,
    origen: Origen,
    client_id: String,
    client_secret: Secret,
    scopes: Vec<String>,
    estado: Mutex<Estado>,
}

impl fmt::Debug for TokenSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenSource(..)")
    }
}

/// Codifica como pide RFC 6749 §2.3.1 antes de armar la cabecera Basic.
fn codificar(texto: &str) -> String {
    let mut salida = String::new();
    for byte in texto.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                salida.push(byte as char)
            }
            otro => salida.push_str(&format!("%{otro:02X}")),
        }
    }
    salida
}

/// Vida, en la caché, de un token cuyo servidor dijo `expires_in`.
pub(crate) fn vida_del_token(expires_in: Option<&Value>) -> Duration {
    let segundos = match expires_in {
        None => VIDA_POR_DEFECTO,
        // Un número negativo, fraccionario o que no sea número no es de fiar: vida mínima.
        Some(valor) => valor.as_u64().unwrap_or(0),
    };
    Duration::from_secs(
        segundos
            .saturating_sub(MARGEN)
            .clamp(VIDA_MINIMA, VIDA_MAXIMA),
    )
}

impl TokenSource {
    /// `credencial` es `client_id:client_secret`; devuelve `None` si no tiene esa forma.
    pub(crate) fn new(origen: Origen, credencial: &Secret, scopes: Vec<String>) -> Option<Self> {
        let (id, secreto) = credencial.expose().split_once(':')?;
        if id.is_empty() || secreto.is_empty() {
            return None;
        }
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIEMPO_MAXIMO)
            .build()
            .ok()?;
        Some(TokenSource {
            http,
            origen,
            client_id: id.to_string(),
            client_secret: Secret::new(secreto.to_string())?,
            scopes,
            estado: Mutex::new(Estado {
                endpoint: None,
                token: None,
            }),
        })
    }

    /// Cabecera `Authorization` con un token vigente, pidiéndolo si hace falta.
    pub async fn bearer(&self) -> Result<HeaderValue, String> {
        let mut estado = self.estado.lock().await;
        if let Some((valor, caduca)) = &estado.token {
            if Instant::now() < *caduca {
                return Ok(valor.clone());
            }
        }
        let endpoint = match &estado.endpoint {
            Some(url) => url.clone(),
            None => {
                let url = self.resolver_endpoint().await?;
                estado.endpoint = Some(url.clone());
                url
            }
        };
        let (valor, vida) = self.pedir(&endpoint).await?;
        estado.token = Some((valor.clone(), Instant::now() + vida));
        Ok(valor)
    }

    /// Olvida el token (el remoto lo rechazó): el siguiente uso pide uno nuevo.
    pub async fn invalidate(&self) {
        self.estado.lock().await.token = None;
    }

    async fn resolver_endpoint(&self) -> Result<Url, String> {
        let descubrimiento = match &self.origen {
            Origen::Directo(url) => return Ok(url.clone()),
            Origen::Descubrir(url) => url.clone(),
        };
        let respuesta = self
            .http
            .get(descubrimiento)
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| {
                format!(
                    "no se pudo leer el descubrimiento de OpenID Connect: {}",
                    e.without_url()
                )
            })?;
        if !respuesta.status().is_success() {
            return Err(format!(
                "el descubrimiento de OpenID Connect contestó HTTP {}",
                respuesta.status().as_u16()
            ));
        }
        let bytes = leer_acotado(respuesta, MAX_RESPUESTA)
            .await
            .map_err(|e| format!("descubrimiento de OpenID Connect: {e}"))?;
        let documento: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "el descubrimiento de OpenID Connect no es JSON".to_string())?;
        if let Some(tipos) = documento
            .get("grant_types_supported")
            .and_then(Value::as_array)
        {
            if !tipos
                .iter()
                .any(|t| t.as_str() == Some("client_credentials"))
            {
                return Err(
                    "el proveedor de OpenID Connect no admite el flujo client_credentials".into(),
                );
            }
        }
        let endpoint = documento
            .get("token_endpoint")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "el descubrimiento de OpenID Connect no trae token_endpoint".to_string()
            })?;
        url_permitida(endpoint).map_err(|motivo| format!("token_endpoint: {motivo}"))
    }

    async fn pedir(&self, endpoint: &Url) -> Result<(HeaderValue, Duration), String> {
        let ambito = self.scopes.join(" ");
        let mut formulario = vec![("grant_type", "client_credentials")];
        if !ambito.is_empty() {
            formulario.push(("scope", ambito.as_str()));
        }
        let respuesta = self
            .http
            .post(endpoint.clone())
            .basic_auth(
                codificar(&self.client_id),
                Some(codificar(self.client_secret.expose())),
            )
            .header(ACCEPT, "application/json")
            .form(&formulario)
            .send()
            .await
            .map_err(|e| format!("no se pudo pedir el token: {}", e.without_url()))?;
        let estado = respuesta.status();
        // El cuerpo de un rechazo no se repite: podría citar la propia credencial.
        if !estado.is_success() {
            return Err(format!(
                "el servidor de tokens contestó HTTP {}",
                estado.as_u16()
            ));
        }
        let bytes = leer_acotado(respuesta, MAX_RESPUESTA)
            .await
            .map_err(|e| format!("respuesta del servidor de tokens: {e}"))?;
        let cuerpo: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "la respuesta del servidor de tokens no es JSON".to_string())?;
        let tipo = cuerpo
            .get("token_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !tipo.eq_ignore_ascii_case("bearer") {
            return Err("el servidor de tokens no entrega un token de tipo Bearer".into());
        }
        let token = cuerpo
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty() && t.len() <= MAX_TOKEN_BYTES)
            .ok_or_else(|| {
                "el servidor de tokens no entregó un access_token utilizable".to_string()
            })?;
        let mut valor = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| "el access_token no cabe en una cabecera".to_string())?;
        valor.set_sensitive(true);
        Ok((valor, vida_del_token(cuerpo.get("expires_in"))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn credencial(texto: &str) -> Secret {
        Secret::new(texto.to_string()).unwrap()
    }

    fn fuente(servidor: &MockServer, ruta: &str, scopes: &[&str]) -> TokenSource {
        let url = Url::parse(&format!("{}{ruta}", servidor.uri())).unwrap();
        TokenSource::new(
            Origen::Directo(url),
            &credencial("mi-cliente:mi-secreto"),
            scopes.iter().map(|s| s.to_string()).collect(),
        )
        .unwrap()
    }

    fn token_ok(token: &str, expires: Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(
            json!({"access_token": token, "token_type": "Bearer", "expires_in": expires}),
        )
    }

    #[test]
    fn only_https_or_local_http_without_embedded_credentials_is_allowed() {
        for bueno in [
            "https://auth.example/token",
            "http://127.0.0.1:9000/token",
            "http://localhost/token",
            "http://[::1]:80/token",
        ] {
            assert!(url_permitida(bueno).is_ok(), "{bueno}");
        }
        for malo in [
            "http://auth.example/token",
            "http://10.0.0.5/token",
            "ftp://auth.example/token",
            "https://usuario:clave@auth.example/token",
            "no es url",
            "file:///etc/passwd",
        ] {
            assert!(url_permitida(malo).is_err(), "{malo}");
        }
        assert!(url_permitida(&format!("https://a.example/{}", "x".repeat(600))).is_err());
    }

    #[test]
    fn the_token_lifetime_is_bounded_and_odd_values_get_the_minimum() {
        let vida = |v: Option<Value>| vida_del_token(v.as_ref()).as_secs();
        assert_eq!(vida(None), 270);
        assert_eq!(vida(Some(json!(3600))), 3570);
        assert_eq!(vida(Some(json!(1_000_000))), 3600);
        assert_eq!(vida(Some(json!(0))), 30);
        assert_eq!(vida(Some(json!(-5))), 30);
        assert_eq!(vida(Some(json!(10.5))), 30);
        assert_eq!(vida(Some(json!("mucho"))), 30);
    }

    #[test]
    fn a_credential_without_the_id_secret_shape_is_refused() {
        let origen = || Origen::Directo(Url::parse("https://a.example/t").unwrap());
        for malo in ["sin-dos-puntos", ":solo-secreto", "solo-id:"] {
            assert!(
                TokenSource::new(origen(), &credencial(malo), vec![]).is_none(),
                "{malo}"
            );
        }
        assert!(TokenSource::new(origen(), &credencial("a:b:c"), vec![]).is_some());
    }

    #[tokio::test]
    async fn the_token_is_requested_with_basic_auth_and_cached() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(header(
                "authorization",
                "Basic bWktY2xpZW50ZTptaS1zZWNyZXRv",
            ))
            .and(body_string_contains("grant_type=client_credentials"))
            .and(body_string_contains("scope=leer+escribir"))
            .respond_with(token_ok("tok-1", json!(3600)))
            .expect(1)
            .mount(&servidor)
            .await;
        let fuente = fuente(&servidor, "/token", &["leer", "escribir"]);

        let primero = fuente.bearer().await.unwrap();
        let segundo = fuente.bearer().await.unwrap();

        assert_eq!(primero.to_str().unwrap(), "Bearer tok-1");
        assert_eq!(segundo, primero);
        assert!(primero.is_sensitive());
        servidor.verify().await;
    }

    #[tokio::test]
    async fn invalidating_forces_a_new_request() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(token_ok("tok", json!(3600)))
            .expect(2)
            .mount(&servidor)
            .await;
        let fuente = fuente(&servidor, "/token", &[]);

        fuente.bearer().await.unwrap();
        fuente.invalidate().await;
        fuente.bearer().await.unwrap();

        servidor.verify().await;
    }

    #[tokio::test]
    async fn a_bad_token_answer_is_refused_without_echoing_anything_secret() {
        let casos = [
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "x", "token_type": "mac"})),
            ResponseTemplate::new(200).set_body_json(json!({"token_type": "Bearer"})),
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "", "token_type": "Bearer"})),
            ResponseTemplate::new(200).set_body_string("no json mi-secreto"),
            ResponseTemplate::new(401).set_body_string("credencial mi-secreto invalida"),
            ResponseTemplate::new(500),
            ResponseTemplate::new(200).set_body_string("x".repeat(MAX_RESPUESTA + 10)),
            ResponseTemplate::new(200).set_body_json(
                json!({"access_token": "x".repeat(MAX_TOKEN_BYTES + 1), "token_type": "Bearer"}),
            ),
        ];
        for caso in casos {
            let servidor = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(caso)
                .mount(&servidor)
                .await;

            let error = fuente(&servidor, "/token", &[]).bearer().await.unwrap_err();

            assert!(
                !error.contains("mi-secreto") && !error.contains("mi-cliente"),
                "{error}"
            );
        }
    }

    #[tokio::test]
    async fn a_redirect_from_the_token_server_is_not_followed() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/otro", servidor.uri())),
            )
            .mount(&servidor)
            .await;
        Mock::given(path("/otro"))
            .respond_with(token_ok("robado", json!(3600)))
            .expect(0)
            .mount(&servidor)
            .await;

        let error = fuente(&servidor, "/token", &[]).bearer().await.unwrap_err();

        assert!(error.contains("302"), "{error}");
        servidor.verify().await;
    }

    #[tokio::test]
    async fn the_secret_never_shows_in_debug() {
        let servidor = MockServer::start().await;
        let texto = format!("{:?}", fuente(&servidor, "/token", &[]));
        assert!(
            !texto.contains("mi-secreto") && !texto.contains("mi-cliente"),
            "{texto}"
        );
    }

    fn por_descubrimiento(servidor: &MockServer) -> TokenSource {
        let url = Url::parse(&format!(
            "{}/.well-known/openid-configuration",
            servidor.uri()
        ))
        .unwrap();
        TokenSource::new(Origen::Descubrir(url), &credencial("c:s"), vec![]).unwrap()
    }

    async fn descubrimiento(servidor: &MockServer, cuerpo: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(cuerpo)
            .mount(servidor)
            .await;
    }

    #[tokio::test]
    async fn openid_connect_discovers_the_token_endpoint_once_and_uses_it() {
        let servidor = MockServer::start().await;
        descubrimiento(
            &servidor,
            ResponseTemplate::new(200).set_body_json(json!({
                "token_endpoint": format!("{}/oauth/token", servidor.uri()),
                "grant_types_supported": ["authorization_code", "client_credentials"]
            })),
        )
        .await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(token_ok("tok-oidc", json!(3600)))
            .mount(&servidor)
            .await;
        let fuente = por_descubrimiento(&servidor);

        assert_eq!(
            fuente.bearer().await.unwrap().to_str().unwrap(),
            "Bearer tok-oidc"
        );
        fuente.invalidate().await;
        fuente.bearer().await.unwrap();

        let descubrimientos = servidor
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|p| p.method.as_str() == "GET")
            .count();
        assert_eq!(descubrimientos, 1);
    }

    #[tokio::test]
    async fn a_bad_discovery_document_is_refused() {
        let casos = [
            json!({"issuer": "x"}),
            json!({"token_endpoint": "http://evil.example/t"}),
            json!({"token_endpoint": "https://u:p@auth.example/t"}),
            json!({"token_endpoint": "https://auth.example/t", "grant_types_supported": ["password"]}),
        ];
        for caso in casos {
            let servidor = MockServer::start().await;
            descubrimiento(
                &servidor,
                ResponseTemplate::new(200).set_body_json(caso.clone()),
            )
            .await;

            assert!(
                por_descubrimiento(&servidor).bearer().await.is_err(),
                "{caso}"
            );
        }
        let servidor = MockServer::start().await;
        descubrimiento(
            &servidor,
            ResponseTemplate::new(200).set_body_string("x".repeat(MAX_RESPUESTA + 5)),
        )
        .await;
        assert!(por_descubrimiento(&servidor).bearer().await.is_err());
    }
}
