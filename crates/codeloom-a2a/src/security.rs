//! Seguridad de la tarjeta de un remoto y plan de autenticación del cliente (A8d).
//!
//! Cabe señalar que A2A no prescribe un modelo de autorización: la tarjeta solo **declara** lo que
//! pide (`securitySchemes`, `securityRequirements`). Aquí se interpreta esa declaración, se elige la
//! primera alternativa que este cliente sabe cumplir y se construye lo que hay que añadir a cada
//! petición. Lo que no se sabe cumplir (OAuth2, OpenID Connect, mTLS, clave en cookie…) se rechaza
//! con un motivo legible; jamás se intenta sin credencial.
//!
//! La lectura de `securityRequirements` como «alternativas (O) de conjuntos (Y)» es la semántica de
//! OpenAPI, no una frase de A2A: es lectura nuestra y así consta en la spec.

use std::collections::BTreeMap;
use std::fmt;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::header::{HeaderName, HeaderValue, AUTHORIZATION};
use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_ESQUEMAS: usize = 20;
const MAX_ALTERNATIVAS: usize = 10;
const MAX_POR_ALTERNATIVA: usize = 10;
const MAX_NOMBRE: usize = 200;
/// Tope del JSON opaco que el núcleo guarda junto a la tarjeta.
const MAX_JSON_BYTES: usize = 16 * 1024;

/// Un valor secreto. No se puede mostrar, copiar ni serializar por descuido.
pub struct Secret(String);

impl Secret {
    /// `None` si el valor está vacío: una variable vacía equivale a una variable ausente.
    pub fn new(valor: String) -> Option<Secret> {
        (!valor.trim().is_empty()).then_some(Secret(valor))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("«secreto»")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("«secreto»")
    }
}

/// Dónde viaja una clave de API. La cookie no se admite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyLocation {
    Header,
    Query,
}

/// Tipo de un esquema de la tarjeta, reducido a lo que el cliente sabe hacer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SchemeKind {
    ApiKey {
        location: KeyLocation,
        name: String,
    },
    HttpBearer,
    HttpBasic,
    /// Nombre legible de lo que no se soporta («OAuth2», «mTLS»…).
    Unsupported {
        what: String,
    },
}

/// Lo que la tarjeta pide, ya acotado y saneado.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSecurity {
    schemes: BTreeMap<String, SchemeKind>,
    /// Cada alternativa es un conjunto de nombres de esquema que se han de cumplir a la vez.
    alternatives: Vec<Vec<String>>,
}

/// Quita saltos y caracteres de control (el texto es de un tercero y acaba en mensajes) y acota.
fn limpiar(texto: &str) -> String {
    let plano: String = texto
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let plano = plano.trim();
    if plano.len() <= MAX_NOMBRE {
        return plano.to_string();
    }
    let mut fin = MAX_NOMBRE;
    while !plano.is_char_boundary(fin) {
        fin -= 1;
    }
    format!("{}…", &plano[..fin])
}

fn no_soportado(que: &str) -> SchemeKind {
    SchemeKind::Unsupported {
        what: que.to_string(),
    }
}

fn leer_esquema(valor: &Value) -> SchemeKind {
    let Some(objeto) = valor.as_object() else {
        return no_soportado("esquema mal formado");
    };
    let Some((miembro, cuerpo)) = objeto.iter().next() else {
        return no_soportado("esquema vacío");
    };
    let texto = |clave: &str| cuerpo.get(clave).and_then(Value::as_str).unwrap_or("");
    match miembro.as_str() {
        "apiKeySecurityScheme" => {
            let nombre = limpiar(texto("name"));
            let ubicacion = texto("location").to_ascii_lowercase();
            match ubicacion.as_str() {
                "header" if HeaderName::from_bytes(nombre.as_bytes()).is_ok() => {
                    SchemeKind::ApiKey {
                        location: KeyLocation::Header,
                        name: nombre,
                    }
                }
                "query" if !nombre.is_empty() => SchemeKind::ApiKey {
                    location: KeyLocation::Query,
                    name: nombre,
                },
                "cookie" => no_soportado("clave de API en cookie"),
                _ => no_soportado("clave de API con ubicación o nombre inválidos"),
            }
        }
        "httpAuthSecurityScheme" => match texto("scheme").to_ascii_lowercase().as_str() {
            "bearer" => SchemeKind::HttpBearer,
            "basic" => SchemeKind::HttpBasic,
            otro => no_soportado(&format!("HTTP {}", limpiar(otro))),
        },
        "oauth2SecurityScheme" => no_soportado("OAuth2"),
        "openIdConnectSecurityScheme" => no_soportado("OpenID Connect"),
        "mtlsSecurityScheme" => no_soportado("mTLS"),
        _ => no_soportado("tipo de esquema desconocido"),
    }
}

impl CardSecurity {
    /// Lee `securitySchemes` y `securityRequirements` de la tarjeta. Tolerante: lo que no entiende lo
    /// marca como no soportado en lugar de callarlo. Acotado en número y en tamaño de nombre.
    pub fn from_card_json(tarjeta: &Value) -> CardSecurity {
        let schemes = tarjeta
            .get("securitySchemes")
            .and_then(Value::as_object)
            .map(|mapa| {
                mapa.iter()
                    .take(MAX_ESQUEMAS)
                    .map(|(nombre, esquema)| (limpiar(nombre), leer_esquema(esquema)))
                    .collect()
            })
            .unwrap_or_default();
        let alternatives = tarjeta
            .get("securityRequirements")
            .and_then(Value::as_array)
            .map(|lista| {
                lista
                    .iter()
                    .take(MAX_ALTERNATIVAS)
                    .map(|requisito| {
                        requisito
                            .get("schemes")
                            .and_then(Value::as_object)
                            .map(|mapa| {
                                mapa.keys()
                                    .take(MAX_POR_ALTERNATIVA)
                                    .map(|nombre| limpiar(nombre))
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();
        CardSecurity {
            schemes,
            alternatives,
        }
    }

    /// `true` si la tarjeta no pide nada: remoto abierto.
    pub fn is_empty(&self) -> bool {
        self.alternatives.is_empty()
    }

    /// Forma opaca que guarda el núcleo junto a la tarjeta. `None` si no pide nada o si pasaría del tope.
    pub fn to_json(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        serde_json::to_string(self)
            .ok()
            .filter(|texto| texto.len() <= MAX_JSON_BYTES)
    }

    pub fn from_json(texto: &str) -> Option<CardSecurity> {
        if texto.len() > MAX_JSON_BYTES {
            return None;
        }
        serde_json::from_str(texto).ok()
    }
}

/// Por qué no se puede construir un plan de autenticación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthProblem {
    Unsupported { scheme: String, kind: String },
    MissingCredential { scheme: String, env: Option<String> },
    InvalidCredential { scheme: String, env: String },
}

impl fmt::Display for AuthProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthProblem::Unsupported { scheme, kind } => write!(
                f,
                "el esquema «{scheme}» ({kind}) exige autenticación que aún no está soportada"
            ),
            AuthProblem::MissingCredential {
                scheme,
                env: Some(env),
            } => write!(
                f,
                "falta la variable de entorno {env} (credencial del esquema «{scheme}»)"
            ),
            AuthProblem::MissingCredential { scheme, env: None } => write!(
                f,
                "el esquema «{scheme}» necesita una credencial: declárala en `a2a.credentials` del perfil"
            ),
            AuthProblem::InvalidCredential { scheme, env } => write!(
                f,
                "la credencial del esquema «{scheme}» (variable {env}) no es válida para ese esquema"
            ),
        }
    }
}

enum Paso {
    Cabecera(HeaderName, HeaderValue),
    Consulta(String, Secret),
}

/// Lo que hay que añadir a cada petición para cumplir la alternativa elegida.
pub struct AuthPlan {
    pasos: Vec<Paso>,
}

impl fmt::Debug for AuthPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AuthPlan({} pasos)", self.pasos.len())
    }
}

fn valor_sensible(texto: &str) -> Option<HeaderValue> {
    let mut valor = HeaderValue::from_str(texto).ok()?;
    valor.set_sensitive(true);
    Some(valor)
}

impl AuthPlan {
    /// Sin autenticación: el remoto es abierto.
    pub fn open() -> AuthPlan {
        AuthPlan { pasos: Vec::new() }
    }

    /// `true` si no añade nada a las peticiones.
    pub fn is_open(&self) -> bool {
        self.pasos.is_empty()
    }

    /// Elige la primera alternativa cuyos esquemas se soportan todos y tienen credencial. Si
    /// ninguna sirve, devuelve el problema de la primera. `credentials` mapea esquema → nombre de
    /// variable; `env` lee una variable (inyectable para las pruebas).
    pub fn choose(
        seguridad: &CardSecurity,
        credentials: &BTreeMap<String, String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<AuthPlan, AuthProblem> {
        if seguridad.alternatives.is_empty() {
            return Ok(AuthPlan::open());
        }
        let mut primero = None;
        for alternativa in &seguridad.alternatives {
            match Self::cumplir(seguridad, alternativa, credentials, env) {
                Ok(plan) => return Ok(plan),
                Err(problema) => {
                    primero.get_or_insert(problema);
                }
            }
        }
        Err(primero.expect("hay al menos una alternativa"))
    }

    fn cumplir(
        seguridad: &CardSecurity,
        alternativa: &[String],
        credentials: &BTreeMap<String, String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<AuthPlan, AuthProblem> {
        let mut pasos = Vec::new();
        for nombre in alternativa {
            let tipo = seguridad
                .schemes
                .get(nombre)
                .cloned()
                .unwrap_or_else(|| no_soportado("esquema no definido en la tarjeta"));
            if let SchemeKind::Unsupported { what } = &tipo {
                return Err(AuthProblem::Unsupported {
                    scheme: nombre.clone(),
                    kind: what.clone(),
                });
            }
            let Some(variable) = credentials.get(nombre) else {
                return Err(AuthProblem::MissingCredential {
                    scheme: nombre.clone(),
                    env: None,
                });
            };
            let Some(secreto) = env(variable).and_then(Secret::new) else {
                return Err(AuthProblem::MissingCredential {
                    scheme: nombre.clone(),
                    env: Some(variable.clone()),
                });
            };
            let invalida = || AuthProblem::InvalidCredential {
                scheme: nombre.clone(),
                env: variable.clone(),
            };
            let paso = match tipo {
                SchemeKind::ApiKey {
                    location: KeyLocation::Header,
                    name,
                } => {
                    let cabecera =
                        HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalida())?;
                    Paso::Cabecera(
                        cabecera,
                        valor_sensible(secreto.expose().trim()).ok_or_else(invalida)?,
                    )
                }
                SchemeKind::ApiKey {
                    location: KeyLocation::Query,
                    name,
                } => Paso::Consulta(name, secreto),
                SchemeKind::HttpBearer => Paso::Cabecera(
                    AUTHORIZATION,
                    valor_sensible(&format!("Bearer {}", secreto.expose().trim()))
                        .ok_or_else(invalida)?,
                ),
                SchemeKind::HttpBasic => {
                    if !secreto.expose().contains(':') {
                        return Err(invalida());
                    }
                    Paso::Cabecera(
                        AUTHORIZATION,
                        valor_sensible(&format!("Basic {}", STANDARD.encode(secreto.expose())))
                            .ok_or_else(invalida)?,
                    )
                }
                SchemeKind::Unsupported { .. } => unreachable!("descartado arriba"),
            };
            pasos.push(paso);
        }
        Ok(AuthPlan { pasos })
    }

    /// Plan para un remoto ya descargado: `security` es el JSON opaco que guardó la tarjeta,
    /// `requires_auth` si la tarjeta declaró requisitos. Si los declaró y no se pueden leer (por
    /// ejemplo, por pasarse del tope), el remoto no se usa: jamás se intenta sin credencial.
    /// El error ya está redactado para mostrarlo.
    pub fn for_card(
        security: Option<&str>,
        requires_auth: bool,
        credentials: &BTreeMap<String, String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<AuthPlan, String> {
        let seguridad = match security {
            Some(texto) => CardSecurity::from_json(texto)
                .ok_or_else(|| "la seguridad de la tarjeta no se pudo leer".to_string())?,
            None if requires_auth => {
                return Err(
                    "la seguridad de la tarjeta es demasiado grande para interpretarla".to_string(),
                )
            }
            None => CardSecurity::default(),
        };
        AuthPlan::choose(&seguridad, credentials, env).map_err(|problema| problema.to_string())
    }

    /// Añade la autenticación a una petición.
    pub fn apply(&self, mut peticion: RequestBuilder) -> RequestBuilder {
        for paso in &self.pasos {
            peticion = match paso {
                Paso::Cabecera(nombre, valor) => peticion.header(nombre.clone(), valor.clone()),
                Paso::Consulta(nombre, secreto) => {
                    peticion.query(&[(nombre.as_str(), secreto.expose())])
                }
            };
        }
        peticion
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tarjeta(esquemas: Value, requisitos: Value) -> Value {
        json!({ "securitySchemes": esquemas, "securityRequirements": requisitos })
    }

    fn api_key_header() -> Value {
        json!({ "apiKeySecurityScheme": { "location": "header", "name": "X-Api-Key" } })
    }

    fn requisito(nombres: &[&str]) -> Value {
        let mapa: serde_json::Map<String, Value> = nombres
            .iter()
            .map(|n| (n.to_string(), json!({ "list": [] })))
            .collect();
        json!({ "schemes": mapa })
    }

    fn entorno<'a>(pares: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |nombre| {
            pares
                .iter()
                .find(|(k, _)| *k == nombre)
                .map(|(_, v)| v.to_string())
        }
    }

    fn credenciales(pares: &[(&str, &str)]) -> BTreeMap<String, String> {
        pares
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn construir(plan: &AuthPlan) -> reqwest::Request {
        let cliente = reqwest::Client::new();
        plan.apply(cliente.post("http://127.0.0.1:1/rpc"))
            .build()
            .unwrap()
    }

    #[test]
    fn parses_the_literal_sample_card() {
        let valor: Value =
            serde_json::from_str(include_str!("../tests/fixtures/agent-card.sample.json")).unwrap();
        let seguridad = CardSecurity::from_card_json(&valor);

        assert_eq!(
            seguridad.schemes["google"],
            SchemeKind::Unsupported {
                what: "OpenID Connect".into()
            }
        );
        assert_eq!(seguridad.alternatives, vec![vec!["google".to_string()]]);
    }

    #[test]
    fn parses_api_key_header_and_query() {
        let valor = tarjeta(
            json!({
                "h": api_key_header(),
                "q": { "apiKeySecurityScheme": { "location": "QUERY", "name": "key" } },
            }),
            json!([]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);

        assert_eq!(
            seguridad.schemes["h"],
            SchemeKind::ApiKey {
                location: KeyLocation::Header,
                name: "X-Api-Key".into()
            }
        );
        assert_eq!(
            seguridad.schemes["q"],
            SchemeKind::ApiKey {
                location: KeyLocation::Query,
                name: "key".into()
            }
        );
    }

    #[test]
    fn parses_http_bearer_and_basic_case_insensitively() {
        let valor = tarjeta(
            json!({
                "b": { "httpAuthSecurityScheme": { "scheme": "BEARER" } },
                "c": { "httpAuthSecurityScheme": { "scheme": "basic" } },
            }),
            json!([]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);

        assert_eq!(seguridad.schemes["b"], SchemeKind::HttpBearer);
        assert_eq!(seguridad.schemes["c"], SchemeKind::HttpBasic);
    }

    #[test]
    fn cookie_api_key_digest_oauth2_and_mtls_are_unsupported_with_a_readable_name() {
        let valor = tarjeta(
            json!({
                "ck": { "apiKeySecurityScheme": { "location": "cookie", "name": "sid" } },
                "dg": { "httpAuthSecurityScheme": { "scheme": "Digest" } },
                "oa": { "oauth2SecurityScheme": { "flows": {} } },
                "tl": { "mtlsSecurityScheme": {} },
            }),
            json!([]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);
        let que = |n: &str| match &seguridad.schemes[n] {
            SchemeKind::Unsupported { what } => what.clone(),
            otro => panic!("{otro:?}"),
        };

        assert_eq!(que("ck"), "clave de API en cookie");
        assert_eq!(que("dg"), "HTTP digest");
        assert_eq!(que("oa"), "OAuth2");
        assert_eq!(que("tl"), "mTLS");
    }

    #[test]
    fn an_unknown_scheme_object_is_unsupported_not_ignored() {
        let valor = tarjeta(json!({ "x": { "magiaSecurityScheme": {} } }), json!([]));
        let seguridad = CardSecurity::from_card_json(&valor);

        assert!(matches!(
            seguridad.schemes["x"],
            SchemeKind::Unsupported { .. }
        ));
    }

    #[test]
    fn choose_takes_the_first_alternative_whose_schemes_are_all_supported_and_have_a_credential() {
        let valor = tarjeta(
            json!({
                "oa": { "oauth2SecurityScheme": { "flows": {} } },
                "k": api_key_header(),
            }),
            json!([requisito(&["oa"]), requisito(&["k"])]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);

        let plan = AuthPlan::choose(
            &seguridad,
            &credenciales(&[("k", "MI_CLAVE")]),
            &entorno(&[("MI_CLAVE", "abc")]),
        )
        .unwrap();

        let peticion = construir(&plan);
        assert_eq!(peticion.headers()["x-api-key"], "abc");
    }

    #[test]
    fn choose_requires_every_scheme_of_an_alternative() {
        let valor = tarjeta(
            json!({ "k": api_key_header(), "b": { "httpAuthSecurityScheme": { "scheme": "Bearer" } } }),
            json!([requisito(&["b", "k"])]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);

        let error = AuthPlan::choose(
            &seguridad,
            &credenciales(&[("k", "CLAVE"), ("b", "TOKEN")]),
            &entorno(&[("CLAVE", "abc")]),
        )
        .unwrap_err();

        assert_eq!(
            error,
            AuthProblem::MissingCredential {
                scheme: "b".into(),
                env: Some("TOKEN".into())
            }
        );
        assert!(error.to_string().contains("TOKEN"));
    }

    #[test]
    fn an_empty_env_value_counts_as_missing() {
        let valor = tarjeta(json!({ "k": api_key_header() }), json!([requisito(&["k"])]));
        let seguridad = CardSecurity::from_card_json(&valor);

        let error = AuthPlan::choose(
            &seguridad,
            &credenciales(&[("k", "VACIA")]),
            &entorno(&[("VACIA", "   ")]),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AuthProblem::MissingCredential { env: Some(_), .. }
        ));
    }

    #[test]
    fn a_scheme_without_a_declared_credential_says_where_to_declare_it() {
        let valor = tarjeta(json!({ "k": api_key_header() }), json!([requisito(&["k"])]));
        let seguridad = CardSecurity::from_card_json(&valor);

        let error = AuthPlan::choose(&seguridad, &credenciales(&[]), &entorno(&[])).unwrap_err();

        assert!(error.to_string().contains("a2a.credentials"), "{error}");
    }

    #[test]
    fn choose_reports_the_first_alternative_when_none_works() {
        let valor = tarjeta(
            json!({
                "oa": { "oauth2SecurityScheme": { "flows": {} } },
                "tl": { "mtlsSecurityScheme": {} },
            }),
            json!([requisito(&["oa"]), requisito(&["tl"])]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);

        let error = AuthPlan::choose(&seguridad, &credenciales(&[]), &entorno(&[])).unwrap_err();

        assert_eq!(
            error,
            AuthProblem::Unsupported {
                scheme: "oa".into(),
                kind: "OAuth2".into()
            }
        );
    }

    #[test]
    fn an_open_card_chooses_the_open_plan() {
        let seguridad = CardSecurity::from_card_json(&json!({ "name": "x" }));
        let plan = AuthPlan::choose(&seguridad, &credenciales(&[]), &entorno(&[])).unwrap();

        assert!(plan.is_open());
        assert!(construir(&plan).headers().get("authorization").is_none());
        assert!(seguridad.is_empty());
        assert!(seguridad.to_json().is_none());
    }

    #[test]
    fn an_empty_requirement_means_no_authentication_is_needed() {
        let valor = tarjeta(json!({ "k": api_key_header() }), json!([requisito(&[])]));
        let seguridad = CardSecurity::from_card_json(&valor);

        let plan = AuthPlan::choose(&seguridad, &credenciales(&[]), &entorno(&[])).unwrap();

        assert!(plan.is_open());
    }

    #[test]
    fn a_requirement_naming_an_undefined_scheme_is_unsupported() {
        let valor = tarjeta(json!({}), json!([requisito(&["fantasma"])]));
        let seguridad = CardSecurity::from_card_json(&valor);

        let error = AuthPlan::choose(&seguridad, &credenciales(&[]), &entorno(&[])).unwrap_err();

        assert!(matches!(error, AuthProblem::Unsupported { .. }));
    }

    #[test]
    fn secret_debug_and_display_never_show_the_value() {
        let secreto = Secret::new("super-secreto".into()).unwrap();

        assert!(!format!("{secreto:?}").contains("super-secreto"));
        assert!(!format!("{secreto}").contains("super-secreto"));
        assert!(Secret::new("  ".into()).is_none());
    }

    #[test]
    fn the_plan_never_shows_its_credential_in_debug() {
        let valor = tarjeta(json!({ "k": api_key_header() }), json!([requisito(&["k"])]));
        let seguridad = CardSecurity::from_card_json(&valor);
        let plan = AuthPlan::choose(
            &seguridad,
            &credenciales(&[("k", "V")]),
            &entorno(&[("V", "super-secreto")]),
        )
        .unwrap();

        assert!(!format!("{plan:?}").contains("super-secreto"));
        assert!(construir(&plan).headers()["x-api-key"].is_sensitive());
    }

    #[test]
    fn apply_sets_header_bearer_basic_and_query() {
        let valor = tarjeta(
            json!({
                "b": { "httpAuthSecurityScheme": { "scheme": "Bearer" } },
                "c": { "httpAuthSecurityScheme": { "scheme": "Basic" } },
                "q": { "apiKeySecurityScheme": { "location": "query", "name": "key" } },
            }),
            json!([requisito(&["b"]), requisito(&["c"]), requisito(&["q"])]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);
        let con = |esquema: &str, valor: &'static str| {
            let solo = CardSecurity {
                schemes: seguridad.schemes.clone(),
                alternatives: vec![vec![esquema.to_string()]],
            };
            AuthPlan::choose(
                &solo,
                &credenciales(&[(esquema, "V")]),
                &entorno(&[("V", valor)]),
            )
            .unwrap()
        };

        let bearer = construir(&con("b", "tok"));
        assert_eq!(bearer.headers()["authorization"], "Bearer tok");

        let basic = construir(&con("c", "usuario:clave"));
        assert_eq!(
            basic.headers()["authorization"],
            format!("Basic {}", STANDARD.encode("usuario:clave")).as_str()
        );

        let consulta = construir(&con("q", "s3"));
        assert_eq!(consulta.url().query(), Some("key=s3"));
    }

    #[test]
    fn for_card_refuses_a_card_whose_security_could_not_be_stored() {
        let error = AuthPlan::for_card(None, true, &credenciales(&[]), &entorno(&[])).unwrap_err();
        assert!(error.contains("demasiado grande"), "{error}");

        let error = AuthPlan::for_card(Some("no es json"), true, &credenciales(&[]), &entorno(&[]))
            .unwrap_err();
        assert!(error.contains("no se pudo leer"), "{error}");

        assert!(
            AuthPlan::for_card(None, false, &credenciales(&[]), &entorno(&[]))
                .unwrap()
                .is_open()
        );
    }

    #[test]
    fn basic_without_a_colon_is_an_invalid_credential() {
        let valor = tarjeta(
            json!({ "c": { "httpAuthSecurityScheme": { "scheme": "Basic" } } }),
            json!([requisito(&["c"])]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);

        let error = AuthPlan::choose(
            &seguridad,
            &credenciales(&[("c", "V")]),
            &entorno(&[("V", "sin-dos-puntos")]),
        )
        .unwrap_err();

        assert!(matches!(error, AuthProblem::InvalidCredential { .. }));
    }

    #[test]
    fn a_header_value_with_control_characters_is_an_invalid_credential() {
        let valor = tarjeta(json!({ "k": api_key_header() }), json!([requisito(&["k"])]));
        let seguridad = CardSecurity::from_card_json(&valor);

        let error = AuthPlan::choose(
            &seguridad,
            &credenciales(&[("k", "V")]),
            &entorno(&[("V", "a\nb")]),
        )
        .unwrap_err();

        assert!(matches!(error, AuthProblem::InvalidCredential { .. }));
    }

    #[test]
    fn json_roundtrip_keeps_the_security_and_bounds_it() {
        let valor = tarjeta(json!({ "k": api_key_header() }), json!([requisito(&["k"])]));
        let seguridad = CardSecurity::from_card_json(&valor);

        let texto = seguridad.to_json().unwrap();
        assert_eq!(CardSecurity::from_json(&texto), Some(seguridad));
        assert!(CardSecurity::from_json(&"x".repeat(MAX_JSON_BYTES + 1)).is_none());
        assert!(CardSecurity::from_json("no es json").is_none());
    }

    #[test]
    fn names_from_a_third_party_are_cleaned_and_bounded() {
        let nombre = format!("a\nb{}", "z".repeat(500));
        let valor = tarjeta(
            json!({ nombre.clone(): api_key_header() }),
            json!([requisito(&[&nombre])]),
        );
        let seguridad = CardSecurity::from_card_json(&valor);
        let limpio = seguridad.schemes.keys().next().unwrap();

        assert!(!limpio.contains('\n'));
        assert!(limpio.len() <= MAX_NOMBRE + '…'.len_utf8());
        assert_eq!(&seguridad.alternatives[0][0], limpio);
    }

    #[test]
    fn the_number_of_schemes_and_alternatives_is_capped() {
        let mut esquemas = serde_json::Map::new();
        for i in 0..50 {
            esquemas.insert(format!("e{i:02}"), api_key_header());
        }
        let requisitos: Vec<Value> = (0..50).map(|i| requisito(&[&format!("e{i:02}")])).collect();
        let seguridad = CardSecurity::from_card_json(&tarjeta(
            Value::Object(esquemas),
            Value::Array(requisitos),
        ));

        assert_eq!(seguridad.schemes.len(), MAX_ESQUEMAS);
        assert_eq!(seguridad.alternatives.len(), MAX_ALTERNATIVAS);
    }
}
