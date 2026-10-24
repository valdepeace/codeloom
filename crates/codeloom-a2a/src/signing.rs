//! Firmas de la tarjeta de agente (A2A §8.4): JWS (RFC 7515) sobre la tarjeta canonizada con JCS
//! (RFC 8785), verificadas **solo** contra claves de confianza que el usuario aporta en local.
//!
//! Cabe señalar la decisión de fondo (D17): la confianza no puede salir del documento que se
//! verifica, de modo que jamás se descarga una clave de la URL `jku` que indique la propia tarjeta.
//! Sin `trusted_jwks` configurado, una tarjeta firmada se trata exactamente como una sin firmar.
//!
//! Algoritmos admitidos: `ES256`, `ES384`, `RS256` y `EdDSA`. Todo lo demás (`none`, `HS*`, `PS*`…)
//! se rechaza, que para eso se fija la lista y no se confía en la que anuncia la firma.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::{rsa, signature};
use serde_json::Value;

/// Tope de claves de un JWKS de confianza.
const MAX_CLAVES: usize = 64;
/// Tope de firmas que se examinan de una tarjeta.
const MAX_FIRMAS: usize = 8;
/// Tope de longitud (en caracteres) de los campos `protected` y `signature` de una firma.
const MAX_PROTEGIDO: usize = 4096;
const MAX_FIRMA: usize = 2048;
/// Profundidad máxima de anidamiento de la tarjeta al canonizarla.
const MAX_PROFUNDIDAD: usize = 64;
/// El mayor entero que JCS (números de ECMAScript) representa sin pérdida.
const MAX_ENTERO_SEGURO: u64 = 9_007_199_254_740_991;

/// Firmas que la tarjeta trae y la carga útil canonizada contra la que se verificarían.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardSignatures {
    /// Pares `(protected, signature)`, ambos en base64url, tal como vienen en la tarjeta.
    pub signatures: Vec<(String, String)>,
    /// Forma canónica de la tarjeta sin `signatures`. Solo se calcula si hay firmas; si la tarjeta
    /// no admite forma canónica, el motivo.
    pub payload: Result<String, String>,
}

impl Default for CardSignatures {
    fn default() -> Self {
        CardSignatures {
            signatures: Vec::new(),
            payload: Ok(String::new()),
        }
    }
}

impl CardSignatures {
    /// Extrae las firmas de la tarjeta tal como llegó. Una entrada malformada se descarta.
    pub fn from_card_json(tarjeta: &Value) -> CardSignatures {
        let firmas: Vec<(String, String)> = tarjeta
            .get("signatures")
            .and_then(Value::as_array)
            .map(|lista| {
                lista
                    .iter()
                    .take(MAX_FIRMAS)
                    .filter_map(|f| {
                        let protegido = f.get("protected")?.as_str()?;
                        let firma = f.get("signature")?.as_str()?;
                        (protegido.len() <= MAX_PROTEGIDO && firma.len() <= MAX_FIRMA)
                            .then(|| (protegido.to_string(), firma.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let payload = if firmas.is_empty() {
            Ok(String::new())
        } else {
            canonical_payload(tarjeta)
        };
        CardSignatures {
            signatures: firmas,
            payload,
        }
    }

    pub fn is_signed(&self) -> bool {
        !self.signatures.is_empty()
    }
}

/// Forma canónica de la tarjeta que se firma: sin el campo `signatures`, sin las cadenas, listas ni
/// objetos vacíos (los valores por defecto de Protocol Buffers que la serialización JSON omite) y
/// con la canonización JCS.
pub fn canonical_payload(tarjeta: &Value) -> Result<String, String> {
    let Some(objeto) = tarjeta.as_object() else {
        return Err("la tarjeta no es un objeto JSON".into());
    };
    let sin_firmas: serde_json::Map<String, Value> = objeto
        .iter()
        .filter(|(clave, _)| clave.as_str() != "signatures")
        .map(|(clave, valor)| (clave.clone(), valor.clone()))
        .collect();
    let limpia = limpiar(&Value::Object(sin_firmas), 1)?
        .ok_or_else(|| "la tarjeta queda vacía al quitarle los valores por defecto".to_string())?;
    let mut salida = String::new();
    canonizar(&limpia, &mut salida, 1)?;
    Ok(salida)
}

/// Quita recursivamente cadenas, listas y objetos vacíos. `false` y `0` se conservan: en una
/// tarjeta ya serializada son valores presentes.
fn limpiar(valor: &Value, profundidad: usize) -> Result<Option<Value>, String> {
    if matches!(valor, Value::Object(_) | Value::Array(_)) && profundidad > MAX_PROFUNDIDAD {
        return Err(format!(
            "la tarjeta anida más de {MAX_PROFUNDIDAD} niveles y no se canoniza"
        ));
    }
    Ok(match valor {
        Value::Object(mapa) => {
            let mut limpio = serde_json::Map::new();
            for (clave, hijo) in mapa {
                if let Some(hijo) = limpiar(hijo, profundidad + 1)? {
                    limpio.insert(clave.clone(), hijo);
                }
            }
            (!limpio.is_empty()).then_some(Value::Object(limpio))
        }
        Value::Array(lista) => {
            let mut limpia = Vec::new();
            for hijo in lista {
                if let Some(hijo) = limpiar(hijo, profundidad + 1)? {
                    limpia.push(hijo);
                }
            }
            (!limpia.is_empty()).then_some(Value::Array(limpia))
        }
        Value::String(texto) if texto.is_empty() => None,
        otro => Some(otro.clone()),
    })
}

/// Serialización canónica RFC 8785. Los números no enteros (o enteros fuera del rango seguro) no
/// tienen aquí forma canónica comprobada y se rechazan: más vale no verificar que verificar mal.
fn canonizar(valor: &Value, salida: &mut String, profundidad: usize) -> Result<(), String> {
    if profundidad > MAX_PROFUNDIDAD {
        return Err(format!(
            "la tarjeta anida más de {MAX_PROFUNDIDAD} niveles y no se canoniza"
        ));
    }
    match valor {
        Value::Null => salida.push_str("null"),
        Value::Bool(b) => salida.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let entero = n
                .as_i64()
                .filter(|i| i.unsigned_abs() <= MAX_ENTERO_SEGURO)
                .map(|i| i.to_string())
                .or_else(|| {
                    n.as_u64()
                        .filter(|u| *u <= MAX_ENTERO_SEGURO)
                        .map(|u| u.to_string())
                });
            match entero {
                Some(texto) => salida.push_str(&texto),
                None => {
                    return Err(
                        "la tarjeta lleva un número que no es un entero seguro: sin forma canónica"
                            .into(),
                    )
                }
            }
        }
        Value::String(texto) => salida.push_str(&escapar(texto)),
        Value::Array(lista) => {
            salida.push('[');
            for (i, hijo) in lista.iter().enumerate() {
                if i > 0 {
                    salida.push(',');
                }
                canonizar(hijo, salida, profundidad + 1)?;
            }
            salida.push(']');
        }
        Value::Object(mapa) => {
            // Las claves se ordenan por unidades de código UTF-16, no por bytes UTF-8.
            let mut claves: Vec<&String> = mapa.keys().collect();
            claves.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            salida.push('{');
            for (i, clave) in claves.into_iter().enumerate() {
                if i > 0 {
                    salida.push(',');
                }
                salida.push_str(&escapar(clave));
                salida.push(':');
                canonizar(&mapa[clave], salida, profundidad + 1)?;
            }
            salida.push('}');
        }
    }
    Ok(())
}

/// Cadena JSON con el escapado mínimo de `JSON.stringify`, que es el que fija JCS.
fn escapar(texto: &str) -> String {
    let mut fuera = String::with_capacity(texto.len() + 2);
    fuera.push('"');
    for c in texto.chars() {
        match c {
            '"' => fuera.push_str("\\\""),
            '\\' => fuera.push_str("\\\\"),
            '\u{8}' => fuera.push_str("\\b"),
            '\u{c}' => fuera.push_str("\\f"),
            '\n' => fuera.push_str("\\n"),
            '\r' => fuera.push_str("\\r"),
            '\t' => fuera.push_str("\\t"),
            c if (c as u32) < 0x20 => fuera.push_str(&format!("\\u{:04x}", c as u32)),
            c => fuera.push(c),
        }
    }
    fuera.push('"');
    fuera
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Material {
    P256(Vec<u8>),
    P384(Vec<u8>),
    Rsa { n: Vec<u8>, e: Vec<u8> },
    Ed25519(Vec<u8>),
}

impl Material {
    fn algoritmo(&self) -> &'static str {
        match self {
            Material::P256(_) => "ES256",
            Material::P384(_) => "ES384",
            Material::Rsa { .. } => "RS256",
            Material::Ed25519(_) => "EdDSA",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClaveDeConfianza {
    kid: String,
    material: Material,
}

/// Claves públicas en las que el usuario confía para las firmas de las tarjetas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedKeys {
    claves: Vec<ClaveDeConfianza>,
}

fn base64url(campo: &Value, nombre: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(campo.get(nombre)?.as_str()?.as_bytes())
        .ok()
}

fn clave_de_jwk(jwk: &Value) -> Option<ClaveDeConfianza> {
    let kid = jwk.get("kid")?.as_str()?.to_string();
    if kid.is_empty() {
        return None;
    }
    if jwk.get("use").is_some_and(|u| u.as_str() != Some("sig")) {
        return None;
    }
    let material = match (
        jwk.get("kty")?.as_str()?,
        jwk.get("crv").and_then(Value::as_str),
    ) {
        ("EC", Some(curva @ ("P-256" | "P-384"))) => {
            let largo = if curva == "P-256" { 32 } else { 48 };
            let x = base64url(jwk, "x").filter(|x| x.len() == largo)?;
            let y = base64url(jwk, "y").filter(|y| y.len() == largo)?;
            let mut punto = vec![0x04];
            punto.extend(x);
            punto.extend(y);
            if curva == "P-256" {
                Material::P256(punto)
            } else {
                Material::P384(punto)
            }
        }
        ("OKP", Some("Ed25519")) => {
            Material::Ed25519(base64url(jwk, "x").filter(|x| x.len() == 32)?)
        }
        ("RSA", _) => {
            let n = base64url(jwk, "n").filter(|n| !n.is_empty())?;
            let e = base64url(jwk, "e").filter(|e| !e.is_empty())?;
            Material::Rsa { n, e }
        }
        _ => return None,
    };
    // Si el JWK declara su algoritmo, ha de ser el que corresponde a su material.
    if jwk
        .get("alg")
        .and_then(Value::as_str)
        .is_some_and(|alg| alg != material.algoritmo())
    {
        return None;
    }
    Some(ClaveDeConfianza { kid, material })
}

impl TrustedKeys {
    /// Lee un JWKS (RFC 7517). Las claves inutilizables (sin `kid`, de un tipo que no admitimos,
    /// mal formadas) se descartan; si no queda ninguna, es un error: un conjunto de confianza vacío
    /// rechazaría todo sin que nadie supiera por qué.
    pub fn from_jwks(bytes: &[u8]) -> Result<TrustedKeys, String> {
        let valor: Value =
            serde_json::from_slice(bytes).map_err(|_| "no es un JSON válido".to_string())?;
        let lista = valor
            .get("keys")
            .and_then(Value::as_array)
            .ok_or_else(|| "no es un JWKS: falta el arreglo «keys»".to_string())?;
        let claves: Vec<ClaveDeConfianza> = lista
            .iter()
            .take(MAX_CLAVES)
            .filter_map(clave_de_jwk)
            .collect();
        if claves.is_empty() {
            return Err(
                "no contiene ninguna clave utilizable (con «kid», de tipo EC P-256/P-384, RSA u OKP Ed25519)"
                    .into(),
            );
        }
        Ok(TrustedKeys { claves })
    }

    /// Lee el JWKS de un fichero, acotado.
    pub fn from_file(ruta: &str) -> Result<TrustedKeys, String> {
        let bytes = crate::remote::leer_pem(ruta)?;
        TrustedKeys::from_jwks(&bytes)
    }

    /// Verifica que **alguna** firma de la tarjeta sea válida con una de las claves de confianza.
    /// Una tarjeta sin firmas, sin forma canónica o con ninguna firma válida se rechaza.
    pub fn verify(&self, tarjeta: &CardSignatures) -> Result<(), String> {
        if !tarjeta.is_signed() {
            return Err("la tarjeta no está firmada y se exigen firmas (trusted_jwks)".into());
        }
        let payload = tarjeta
            .payload
            .as_ref()
            .map_err(|motivo| format!("la tarjeta no se puede verificar: {motivo}"))?;
        let carga = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let validas = tarjeta
            .signatures
            .iter()
            .any(|(protegido, firma)| self.firma_valida(protegido, firma, &carga));
        if validas {
            Ok(())
        } else {
            Err(
                "ninguna firma de la tarjeta es válida con las claves de confianza (trusted_jwks)"
                    .into(),
            )
        }
    }

    fn firma_valida(&self, protegido: &str, firma: &str, carga: &str) -> bool {
        let Some(cabecera) = URL_SAFE_NO_PAD
            .decode(protegido.as_bytes())
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            return false;
        };
        // `crit` exige entender extensiones que no entendemos; `b64: false` cambia la entrada firmada.
        if cabecera.get("crit").is_some() || cabecera.get("b64").is_some() {
            return false;
        }
        let (Some(alg), Some(kid)) = (
            cabecera.get("alg").and_then(Value::as_str),
            cabecera.get("kid").and_then(Value::as_str),
        ) else {
            return false;
        };
        if !matches!(alg, "ES256" | "ES384" | "RS256" | "EdDSA") {
            return false;
        }
        let Ok(firma) = URL_SAFE_NO_PAD.decode(firma.as_bytes()) else {
            return false;
        };
        let entrada = format!("{protegido}.{carga}");
        self.claves
            .iter()
            .filter(|clave| clave.kid == kid && clave.material.algoritmo() == alg)
            .any(|clave| verificar(&clave.material, entrada.as_bytes(), &firma))
    }
}

fn verificar(material: &Material, mensaje: &[u8], firma: &[u8]) -> bool {
    match material {
        Material::P256(punto) => {
            signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, punto)
                .verify(mensaje, firma)
                .is_ok()
        }
        Material::P384(punto) => {
            signature::UnparsedPublicKey::new(&signature::ECDSA_P384_SHA384_FIXED, punto)
                .verify(mensaje, firma)
                .is_ok()
        }
        Material::Ed25519(punto) => signature::UnparsedPublicKey::new(&signature::ED25519, punto)
            .verify(mensaje, firma)
            .is_ok(),
        Material::Rsa { n, e } => rsa::PublicKeyComponents {
            n: n.as_slice(),
            e: e.as_slice(),
        }
        .verify(&signature::RSA_PKCS1_2048_8192_SHA256, mensaje, firma)
        .is_ok(),
    }
}

/// Ayudas para firmar tarjetas en las pruebas de este módulo y de la sincronización.
#[cfg(test)]
pub(crate) mod pruebas {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use serde_json::json;

    /// Un par de claves Ed25519 desechable y el JWKS público que lo acompaña.
    pub fn par(kid: &str) -> (Ed25519KeyPair, String) {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let par = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let jwks = json!({ "keys": [{
            "kty": "OKP", "crv": "Ed25519", "kid": kid,
            "x": URL_SAFE_NO_PAD.encode(par.public_key().as_ref()),
        }] });
        (par, jwks.to_string())
    }

    /// Firma con una cabecera protegida a medida; devuelve la entrada `signatures`.
    pub fn firma_con_cabecera(tarjeta: &Value, par: &Ed25519KeyPair, cabecera: Value) -> Value {
        let protegido = URL_SAFE_NO_PAD.encode(cabecera.to_string());
        let carga = URL_SAFE_NO_PAD.encode(canonical_payload(tarjeta).unwrap());
        let firma = par.sign(format!("{protegido}.{carga}").as_bytes());
        json!({ "protected": protegido, "signature": URL_SAFE_NO_PAD.encode(firma.as_ref()) })
    }

    /// La tarjeta con una firma EdDSA válida con `kid`.
    pub fn firmada(mut tarjeta: Value, par: &Ed25519KeyPair, kid: &str) -> Value {
        let firma = firma_con_cabecera(
            &tarjeta,
            par,
            json!({ "alg": "EdDSA", "typ": "JOSE", "kid": kid }),
        );
        tarjeta["signatures"] = json!([firma]);
        tarjeta
    }
}

#[cfg(test)]
mod tests {
    use super::pruebas::{firma_con_cabecera, firmada, par};
    use super::*;
    use serde_json::json;

    const TARJETA_FIRMADA: &str = include_str!("../tests/fixtures/signed-card.sample.json");
    const JWKS: &str = include_str!("../tests/fixtures/trusted-jwks.sample.json");

    fn tarjeta_firmada() -> Value {
        serde_json::from_str(TARJETA_FIRMADA).unwrap()
    }

    /// El JWKS de confianza reducido a la clave con ese `kid`.
    fn solo(kid: &str) -> TrustedKeys {
        let mut jwks: Value = serde_json::from_str(JWKS).unwrap();
        jwks["keys"]
            .as_array_mut()
            .unwrap()
            .retain(|k| k["kid"] == kid);
        TrustedKeys::from_jwks(jwks.to_string().as_bytes()).unwrap()
    }

    fn simple() -> Value {
        json!({ "name": "A", "description": "D", "version": "1", "skills": [{ "id": "s" }] })
    }

    // ---- Interoperabilidad: tarjeta firmada por el SDK oficial de Python (a2a-sdk 1.2.3) ----

    #[test]
    fn a_card_signed_by_the_official_python_sdk_verifies_with_each_algorithm() {
        let tarjeta = CardSignatures::from_card_json(&tarjeta_firmada());
        assert_eq!(tarjeta.signatures.len(), 3);

        for kid in ["clave-es256", "clave-rs256", "clave-eddsa"] {
            assert_eq!(solo(kid).verify(&tarjeta), Ok(()), "{kid}");
        }
        assert_eq!(
            TrustedKeys::from_jwks(JWKS.as_bytes())
                .unwrap()
                .verify(&tarjeta),
            Ok(())
        );
    }

    #[test]
    fn changing_any_signed_field_of_the_official_card_breaks_every_signature() {
        let mut valor = tarjeta_firmada();
        valor["description"] = json!("Tarjeta manipulada");

        let tarjeta = CardSignatures::from_card_json(&valor);

        assert!(TrustedKeys::from_jwks(JWKS.as_bytes())
            .unwrap()
            .verify(&tarjeta)
            .unwrap_err()
            .contains("ninguna firma"));
    }

    #[test]
    fn a_key_with_the_right_kid_but_other_material_does_not_verify() {
        let (_, otro) = par("clave-eddsa");
        let tarjeta = CardSignatures::from_card_json(&tarjeta_firmada());

        assert!(TrustedKeys::from_jwks(otro.as_bytes())
            .unwrap()
            .verify(&tarjeta)
            .is_err());
    }

    #[test]
    fn a_signature_is_only_checked_against_keys_with_its_kid_and_algorithm() {
        let tarjeta = CardSignatures::from_card_json(&tarjeta_firmada());
        let mut jwks: Value = serde_json::from_str(JWKS).unwrap();
        for clave in jwks["keys"].as_array_mut().unwrap() {
            clave["kid"] = json!("otro");
        }

        let claves = TrustedKeys::from_jwks(jwks.to_string().as_bytes()).unwrap();

        assert!(claves.verify(&tarjeta).is_err());
    }

    // ---- Lo que jamás se acepta ----

    #[test]
    fn an_unsigned_card_is_rejected_when_keys_are_required() {
        let tarjeta = CardSignatures::from_card_json(&simple());

        let error = solo("clave-es256").verify(&tarjeta).unwrap_err();

        assert!(error.contains("no está firmada"), "{error}");
    }

    #[test]
    fn unsafe_algorithms_and_headers_are_never_accepted_even_with_a_valid_signature_value() {
        let (clave, jwks) = par("k");
        let claves = TrustedKeys::from_jwks(jwks.as_bytes()).unwrap();
        let base = simple();
        let con = |cabecera: Value| {
            let mut valor = base.clone();
            valor["signatures"] = json!([firma_con_cabecera(&base, &clave, cabecera)]);
            claves.verify(&CardSignatures::from_card_json(&valor))
        };

        // Control: la misma firma con una cabecera honesta sí valida.
        assert_eq!(con(json!({ "alg": "EdDSA", "kid": "k" })), Ok(()));
        // Algoritmo `none`, simétrico (confusión de algoritmo) o no admitido.
        for alg in ["none", "HS256", "PS256", "ES512"] {
            assert!(con(json!({ "alg": alg, "kid": "k" })).is_err(), "{alg}");
        }
        // El algoritmo ha de corresponder al tipo de clave.
        assert!(con(json!({ "alg": "ES256", "kid": "k" })).is_err());
        // Extensiones críticas, `b64` y la ausencia de `kid`.
        assert!(con(json!({ "alg": "EdDSA", "kid": "k", "crit": ["exp"] })).is_err());
        assert!(con(json!({ "alg": "EdDSA", "kid": "k", "b64": false })).is_err());
        assert!(con(json!({ "alg": "EdDSA" })).is_err());
    }

    #[test]
    fn a_malformed_signature_is_ignored_and_never_panics() {
        let (_, jwks) = par("k");
        let claves = TrustedKeys::from_jwks(jwks.as_bytes()).unwrap();
        let mut valor = simple();
        valor["signatures"] = json!([
            { "protected": "%%%", "signature": "!!" },
            { "protected": "e30", "signature": "" },
            { "protected": 3, "signature": "x" },
            "no soy un objeto",
            { "protected": "a".repeat(MAX_PROTEGIDO + 1), "signature": "x" },
        ]);

        let tarjeta = CardSignatures::from_card_json(&valor);

        assert!(claves.verify(&tarjeta).is_err());
    }

    #[test]
    fn one_valid_signature_among_invalid_ones_is_enough() {
        let (clave, jwks) = par("k");
        let mut valor = firmada(simple(), &clave, "k");
        let buena = valor["signatures"][0].clone();
        valor["signatures"] = json!([{ "protected": "e30", "signature": "AA" }, buena]);

        let tarjeta = CardSignatures::from_card_json(&valor);

        assert_eq!(
            TrustedKeys::from_jwks(jwks.as_bytes())
                .unwrap()
                .verify(&tarjeta),
            Ok(())
        );
    }

    #[test]
    fn only_the_first_signatures_are_examined() {
        let (clave, _) = par("k");
        let mut valor = firmada(simple(), &clave, "k");
        let buena = valor["signatures"][0].clone();
        let mut lista = vec![json!({ "protected": "e30", "signature": "AA" }); MAX_FIRMAS];
        lista.push(buena);
        valor["signatures"] = Value::Array(lista);

        let tarjeta = CardSignatures::from_card_json(&valor);

        assert_eq!(tarjeta.signatures.len(), MAX_FIRMAS);
    }

    // ---- JWKS de confianza ----

    #[test]
    fn unusable_keys_are_dropped_and_an_empty_set_is_an_error() {
        let jwks = json!({ "keys": [
            { "kty": "oct", "kid": "simetrica", "k": "AAAA" },
            { "kty": "OKP", "crv": "Ed25519", "x": "AAAA" },
            { "kty": "OKP", "crv": "Ed25519", "kid": "corta", "x": "AAAA" },
            { "kty": "OKP", "crv": "Ed25519", "kid": "cifrado", "use": "enc",
              "x": "A".repeat(43) },
            { "kty": "EC", "crv": "P-521", "kid": "p521", "x": "AA", "y": "AA" },
        ] });

        let error = TrustedKeys::from_jwks(jwks.to_string().as_bytes()).unwrap_err();

        assert!(error.contains("ninguna clave utilizable"), "{error}");
        assert!(TrustedKeys::from_jwks(b"no es json").is_err());
        assert!(TrustedKeys::from_jwks(b"{}").unwrap_err().contains("keys"));
    }

    #[test]
    fn a_jwk_that_declares_another_algorithm_than_its_material_is_dropped() {
        let (_, jwks) = par("k");
        let mut valor: Value = serde_json::from_str(&jwks).unwrap();
        valor["keys"][0]["alg"] = json!("ES256");

        assert!(TrustedKeys::from_jwks(valor.to_string().as_bytes()).is_err());
    }

    #[test]
    fn a_missing_or_oversized_jwks_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let grande = dir.path().join("grande.json");
        std::fs::write(&grande, vec![b' '; crate::remote::MAX_PEM_BYTES + 1]).unwrap();

        assert!(TrustedKeys::from_file("/no/existe.json").is_err());
        assert!(TrustedKeys::from_file(&grande.display().to_string()).is_err());
    }

    // ---- Canonización (RFC 8785) ----

    fn canonica(valor: Value) -> Result<String, String> {
        canonical_payload(&valor)
    }

    #[test]
    fn keys_are_ordered_by_utf16_code_units_not_by_utf8_bytes() {
        // U+10000 se codifica en UTF-16 como D800 DC00 y U+FFFF como FFFF: en UTF-16 va primero la
        // primera; en UTF-8 (bytes) iría al revés.
        let valor: Value =
            serde_json::from_str("{\"\u{ffff}\":1,\"\u{10000}\":2,\"a\":3}").unwrap();

        assert_eq!(
            canonica(valor).unwrap(),
            "{\"a\":3,\"\u{10000}\":2,\"\u{ffff}\":1}"
        );
    }

    #[test]
    fn strings_are_escaped_like_json_stringify() {
        let valor = json!({ "t": "a\"b\\c\n\r\t\u{8}\u{c}\u{1}\u{1f}\u{7f}ñ🎯" });

        assert_eq!(
            canonica(valor).unwrap(),
            "{\"t\":\"a\\\"b\\\\c\\n\\r\\t\\b\\f\\u0001\\u001f\u{7f}ñ🎯\"}"
        );
    }

    #[test]
    fn empty_values_are_removed_but_false_and_zero_are_kept() {
        let valor = json!({
            "a": "", "b": [], "c": {}, "d": [[], {}, ""], "e": { "f": "" },
            "g": false, "h": 0, "i": null, "j": [0, "", false],
            "signatures": [{ "protected": "x", "signature": "y" }],
        });

        assert_eq!(
            canonica(valor).unwrap(),
            "{\"g\":false,\"h\":0,\"i\":null,\"j\":[0,false]}"
        );
    }

    #[test]
    fn the_spec_example_canonicalizes_to_the_documented_form() {
        // A2A §8.4.1: ejemplo con `extensions: []` omitido.
        let valor = json!({
            "name": "Example Agent",
            "description": "An example agent that echoes messages.",
            "supportedInterfaces": [{ "url": "https://example.com/a2a/v1", "protocolBinding": "JSONRPC", "protocolVersion": "1.0" }],
            "version": "1.0.0",
            "capabilities": { "streaming": false, "pushNotifications": false, "extensions": [] },
            "defaultInputModes": ["text/plain"],
            "defaultOutputModes": ["text/plain"],
            "skills": [{ "id": "echo", "name": "Echo", "description": "Echoes the input.", "tags": ["echo"] }],
        });

        assert_eq!(
            canonica(valor).unwrap(),
            "{\"capabilities\":{\"pushNotifications\":false,\"streaming\":false},\"defaultInputModes\":[\"text/plain\"],\"defaultOutputModes\":[\"text/plain\"],\"description\":\"An example agent that echoes messages.\",\"name\":\"Example Agent\",\"skills\":[{\"description\":\"Echoes the input.\",\"id\":\"echo\",\"name\":\"Echo\",\"tags\":[\"echo\"]}],\"supportedInterfaces\":[{\"protocolBinding\":\"JSONRPC\",\"protocolVersion\":\"1.0\",\"url\":\"https://example.com/a2a/v1\"}],\"version\":\"1.0.0\"}"
        );
    }

    #[test]
    fn numbers_without_a_checked_canonical_form_and_deep_nesting_are_refused() {
        assert!(canonica(json!({ "n": 1.5 }))
            .unwrap_err()
            .contains("entero"));
        assert!(canonica(json!({ "n": 9_007_199_254_740_992_u64 })).is_err());
        assert_eq!(canonica(json!({ "n": -42 })).unwrap(), "{\"n\":-42}");

        let mut hondo = json!("x");
        for _ in 0..(MAX_PROFUNDIDAD + 5) {
            hondo = json!({ "a": hondo });
        }
        assert!(canonica(hondo).unwrap_err().contains("anida"));
        assert!(canonica(json!("no soy un objeto")).is_err());
        assert!(canonica(json!({ "a": "" })).is_err());
    }
}
